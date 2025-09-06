use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, timeout};

#[cfg(unix)]
use tempfile::TempDir;

#[cfg(windows)]
fn get_test_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("error_test_{}", name))
}

#[cfg(unix)]
fn get_test_path(name: &str) -> std::path::PathBuf {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join(format!("error_test_{}.sock", name));
    // Leak the temp_dir to keep it alive for the test
    std::mem::forget(temp_dir);
    path
}

#[tokio::test]
async fn test_connection_refused() {
    let path = get_test_path("refused");

    // Try to connect without server running
    let result = IpcClient::connect(&path).await;
    assert!(
        result.is_err(),
        "Should fail to connect when server is not running"
    );
}

#[tokio::test]
async fn test_broken_pipe_on_send() {
    let path = get_test_path("broken_send");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let conn = server.accept().await.unwrap();

        // Accept connection but immediately drop it
        drop(conn);
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // First send might succeed (buffered)
    let _ = client.send(&IpcMessage::List).await;

    // Subsequent operations should fail
    let result = client.recv().await;
    assert!(
        result.is_err(),
        "Should fail to receive after connection dropped"
    );

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_broken_pipe_on_recv() {
    let path = get_test_path("broken_recv");

    let server_path = path.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Receive one message
        let _msg = conn.recv().await.unwrap();

        // Signal that we received the message
        tx.send(()).await.unwrap();

        // Drop connection without sending response
        drop(conn);
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();
    client.send(&IpcMessage::List).await.unwrap();

    // Wait for server to receive message
    rx.recv().await.unwrap();

    // Try to receive response (should fail)
    let result = client.recv().await;
    assert!(
        result.is_err(),
        "Should fail to receive after server dropped connection"
    );

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_invalid_message_format() {
    let path = get_test_path("invalid_format");

    #[cfg(unix)]
    {
        use tokio::net::{UnixListener, UnixStream};

        let server_path = path.clone();
        let server_handle = tokio::spawn(async move {
            let listener = UnixListener::bind(&server_path).unwrap();
            let (mut stream, _) = listener.accept().await.unwrap();

            // Send invalid data (not proper length prefix)
            stream.write_all(b"invalid data").await.unwrap();
            stream.flush().await.unwrap();
        });

        sleep(Duration::from_millis(100)).await;

        let mut stream = UnixStream::connect(&path).await.unwrap();

        // Try to read as IpcMessage (should fail)
        let mut len_bytes = [0u8; 4];
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            stream.read_exact(&mut len_bytes),
        )
        .await;

        // Either timeout or invalid data
        assert!(result.is_err() || len_bytes != [0u8; 4]);

        server_handle.await.unwrap();
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.file_name().unwrap().to_string_lossy()
        );

        let pipe_name_clone = pipe_name.clone();
        let server_handle = tokio::spawn(async move {
            let mut server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_name_clone)
                .unwrap();

            server.connect().await.unwrap();

            // Send invalid data
            server.write_all(b"invalid data").await.unwrap();
            server.flush().await.unwrap();
        });

        sleep(Duration::from_millis(100)).await;

        let mut client = ClientOptions::new().open(&pipe_name).unwrap();

        // Try to read as IpcMessage (should fail)
        let mut len_bytes = [0u8; 4];
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            client.read_exact(&mut len_bytes),
        )
        .await;

        // Either timeout or invalid data
        assert!(result.is_err() || len_bytes != [0u8; 4]);

        server_handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_partial_message_send() {
    let path = get_test_path("partial_send");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Try to receive message (might fail or get partial data)
        let result = timeout(Duration::from_millis(500), conn.recv()).await;

        if let Ok(Ok(_msg)) = result {
            // If we somehow got a message, send response
            let response = IpcResponse::Error {
                message: "Unexpected message received".to_string(),
            };
            let _ = conn.send(&response).await;
        }
    });

    sleep(Duration::from_millis(100)).await;

    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let mut stream = UnixStream::connect(&path).await.unwrap();

        // Send partial length header
        let _ = stream.write_all(&[0u8, 0u8]).await;

        // Close connection
        drop(stream);
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.file_name().unwrap().to_string_lossy()
        );

        let mut client = ClientOptions::new().open(&pipe_name).unwrap();

        // Send partial length header
        let _ = client.write_all(&[0u8, 0u8]).await;

        // Close connection
        drop(client);
    }

    // Server should handle partial message gracefully
    let result = timeout(Duration::from_secs(1), server_handle).await;
    assert!(
        result.is_ok(),
        "Server should handle partial messages gracefully"
    );
}

#[tokio::test]
async fn test_zero_length_message() {
    let path = get_test_path("zero_length");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Should fail to parse zero-length message
        let result = conn.recv().await;
        assert!(result.is_err(), "Zero-length message should fail to parse");
    });

    sleep(Duration::from_millis(100)).await;

    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let mut stream = UnixStream::connect(&path).await.unwrap();

        // Send zero length
        let len_bytes = 0u32.to_le_bytes();
        stream.write_all(&len_bytes).await.unwrap();
        stream.flush().await.unwrap();
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.file_name().unwrap().to_string_lossy()
        );

        let mut client = ClientOptions::new().open(&pipe_name).unwrap();

        // Send zero length
        let len_bytes = 0u32.to_le_bytes();
        client.write_all(&len_bytes).await.unwrap();
        client.flush().await.unwrap();
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_malformed_json() {
    let path = get_test_path("malformed_json");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Should fail to deserialize malformed JSON
        let result = conn.recv().await;
        assert!(result.is_err(), "Malformed JSON should fail to deserialize");
    });

    sleep(Duration::from_millis(100)).await;

    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let mut stream = UnixStream::connect(&path).await.unwrap();

        // Send valid length but invalid JSON
        let bad_json = b"{ this is not valid json }";
        let len_bytes = (bad_json.len() as u32).to_le_bytes();
        stream.write_all(&len_bytes).await.unwrap();
        stream.write_all(bad_json).await.unwrap();
        stream.flush().await.unwrap();
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.file_name().unwrap().to_string_lossy()
        );

        let mut client = ClientOptions::new().open(&pipe_name).unwrap();

        // Send valid length but invalid JSON
        let bad_json = b"{ this is not valid json }";
        let len_bytes = (bad_json.len() as u32).to_le_bytes();
        client.write_all(&len_bytes).await.unwrap();
        client.write_all(bad_json).await.unwrap();
        client.flush().await.unwrap();
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_message_too_large() {
    let path = get_test_path("too_large");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Try to receive extremely large message
        let result = timeout(Duration::from_secs(2), conn.recv()).await;

        // Should either timeout or fail with allocation error
        match result {
            Ok(Ok(_)) => panic!("Should not successfully receive 100MB message"),
            Ok(Err(_)) | Err(_) => {} // Expected: error or timeout
        }
    });

    sleep(Duration::from_millis(100)).await;

    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let mut stream = UnixStream::connect(&path).await.unwrap();

        // Send header indicating 100MB message
        let huge_size = 100_000_000u32;
        let len_bytes = huge_size.to_le_bytes();
        let _ = stream.write_all(&len_bytes).await;

        // Start sending data but don't complete
        let chunk = vec![b'x'; 1024];
        for _ in 0..100 {
            if stream.write_all(&chunk).await.is_err() {
                break;
            }
        }
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.file_name().unwrap().to_string_lossy()
        );

        let mut client = ClientOptions::new().open(&pipe_name).unwrap();

        // Send header indicating 100MB message
        let huge_size = 100_000_000u32;
        let len_bytes = huge_size.to_le_bytes();
        let _ = client.write_all(&len_bytes).await;

        // Start sending data but don't complete
        let chunk = vec![b'x'; 1024];
        for _ in 0..100 {
            if client.write_all(&chunk).await.is_err() {
                break;
            }
        }
    }

    let _ = timeout(Duration::from_secs(3), server_handle).await;
}

#[tokio::test]
async fn test_concurrent_send_recv_error() {
    let path = get_test_path("concurrent_error");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Receive first message
        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::List => {
                // Send response
                let response = IpcResponse::Success {
                    message: "First".to_string(),
                };
                conn.send(&response).await.unwrap();
            }
            _ => panic!("Unexpected message type"),
        }

        // Simulate error: drop connection during second exchange
        let _ = conn.recv().await;
        // Don't send response, just drop
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // First exchange should succeed
    client.send(&IpcMessage::List).await.unwrap();
    let response = client.recv().await.unwrap();
    match response {
        IpcResponse::Success { message } => {
            assert_eq!(message, "First");
        }
        _ => panic!("Unexpected response type"),
    }

    // Second exchange should fail
    client.send(&IpcMessage::List).await.unwrap();
    let result = timeout(Duration::from_millis(500), client.recv()).await;
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Should fail on dropped connection"
    );

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_invalid_enum_variant() {
    let path = get_test_path("invalid_variant");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Should fail to deserialize unknown enum variant
        let result = conn.recv().await;
        assert!(
            result.is_err(),
            "Unknown enum variant should fail to deserialize"
        );
    });

    sleep(Duration::from_millis(100)).await;

    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let mut stream = UnixStream::connect(&path).await.unwrap();

        // Send JSON with unknown enum variant
        let bad_json = br#"{"UnknownVariant": {"field": "value"}}"#;
        let len_bytes = (bad_json.len() as u32).to_le_bytes();
        stream.write_all(&len_bytes).await.unwrap();
        stream.write_all(bad_json).await.unwrap();
        stream.flush().await.unwrap();
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        let pipe_name = format!(
            r"\\.\pipe\bunctl_{}",
            path.file_name().unwrap().to_string_lossy()
        );

        let mut client = ClientOptions::new().open(&pipe_name).unwrap();

        // Send JSON with unknown enum variant
        let bad_json = br#"{"UnknownVariant": {"field": "value"}}"#;
        let len_bytes = (bad_json.len() as u32).to_le_bytes();
        client.write_all(&len_bytes).await.unwrap();
        client.write_all(bad_json).await.unwrap();
        client.flush().await.unwrap();
    }

    server_handle.await.unwrap();
}
