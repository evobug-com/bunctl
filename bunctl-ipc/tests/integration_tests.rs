use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer, SubscriptionType};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
#[cfg(unix)]
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

#[cfg(windows)]
fn get_test_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("test_{}", name))
}

#[cfg(unix)]
fn get_test_path(name: &str) -> std::path::PathBuf {
    let temp_dir = TempDir::new().unwrap();
    temp_dir.path().join(format!("test_{}.sock", name))
}

#[tokio::test]
async fn test_basic_client_server_communication() {
    let path = get_test_path("basic_comm");

    // Start server in background
    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Receive message
        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::Start { name, config } => {
                assert_eq!(name, "test-app");
                assert_eq!(config, "test-config");

                // Send response
                let response = IpcResponse::Success {
                    message: "Started successfully".to_string(),
                };
                conn.send(&response).await.unwrap();
            }
            _ => panic!("Unexpected message type"),
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Connect client and send message
    let mut client = IpcClient::connect(&path).await.unwrap();
    let msg = IpcMessage::Start {
        name: "test-app".to_string(),
        config: "test-config".to_string(),
    };
    client.send(&msg).await.unwrap();

    // Receive response
    let response = client.recv().await.unwrap();
    match response {
        IpcResponse::Success { message } => {
            assert_eq!(message, "Started successfully");
        }
        _ => panic!("Unexpected response type"),
    }

    // Wait for server to finish
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_multiple_messages_exchange() {
    let path = get_test_path("multi_msg");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Handle multiple messages
        for i in 0..5 {
            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::Status { name } => {
                    assert_eq!(name, Some(format!("app{}", i)));

                    let response = IpcResponse::Data {
                        data: json!({
                            "app": format!("app{}", i),
                            "status": "running",
                            "pid": 1000 + i
                        }),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // Send and receive multiple messages
    for i in 0..5 {
        let msg = IpcMessage::Status {
            name: Some(format!("app{}", i)),
        };
        client.send(&msg).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Data { data } => {
                assert_eq!(data["app"], format!("app{}", i));
                assert_eq!(data["status"], "running");
                assert_eq!(data["pid"], 1000 + i);
            }
            _ => panic!("Unexpected response type"),
        }
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_concurrent_clients() {
    let path = get_test_path("concurrent");

    let server_path = path.clone();
    let client_count = Arc::new(Mutex::new(0));
    let client_count_clone = client_count.clone();

    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();

        // Handle multiple concurrent connections
        for _ in 0..3 {
            let mut conn = server.accept().await.unwrap();
            let client_count = client_count_clone.clone();

            tokio::spawn(async move {
                let msg = conn.recv().await.unwrap();
                match msg {
                    IpcMessage::List => {
                        let mut count = client_count.lock().await;
                        *count += 1;

                        let response = IpcResponse::Data {
                            data: json!({
                                "client_id": *count,
                                "apps": []
                            }),
                        };
                        conn.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                }
            });
        }

        // Keep server alive for the test
        sleep(Duration::from_secs(1)).await;
    });

    sleep(Duration::from_millis(100)).await;

    // Spawn multiple clients with slight delays on Windows to avoid pipe busy errors
    let mut handles = vec![];
    for i in 0..3 {
        let client_path = path.clone();
        let handle = tokio::spawn(async move {
            // On Windows, add a small delay between connections to avoid "All pipe instances are busy"
            #[cfg(windows)]
            if i > 0 {
                sleep(Duration::from_millis(50)).await;
            }

            // Retry connection a few times on Windows
            let mut client = None;
            for retry in 0..5 {
                match IpcClient::connect(&client_path).await {
                    Ok(c) => {
                        client = Some(c);
                        break;
                    }
                    Err(e) => {
                        if retry < 4 {
                            sleep(Duration::from_millis(100)).await;
                        } else {
                            panic!("Failed to connect after 5 retries: {:?}", e);
                        }
                    }
                }
            }

            let mut client = client.unwrap();
            client.send(&IpcMessage::List).await.unwrap();

            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Data { data } => {
                    assert!(data["client_id"].as_u64().unwrap() > 0);
                    assert!(data["client_id"].as_u64().unwrap() <= 3);
                }
                _ => panic!("Unexpected response type"),
            }
        });
        handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Final count should be 3
    assert_eq!(*client_count.lock().await, 3);

    server_handle.abort();
}

#[tokio::test]
async fn test_large_message_handling() {
    let path = get_test_path("large_msg");

    // Create a large config string (1MB)
    let large_config = "x".repeat(1024 * 1024);
    let large_config_clone = large_config.clone();

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::Start { name, config } => {
                assert_eq!(name, "large-app");
                assert_eq!(config.len(), 1024 * 1024);
                assert_eq!(config, large_config_clone);

                // Send large response
                let response = IpcResponse::Data {
                    data: json!({
                        "result": "y".repeat(1024 * 1024)
                    }),
                };
                conn.send(&response).await.unwrap();
            }
            _ => panic!("Unexpected message type"),
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    let msg = IpcMessage::Start {
        name: "large-app".to_string(),
        config: large_config,
    };
    client.send(&msg).await.unwrap();

    let response = client.recv().await.unwrap();
    match response {
        IpcResponse::Data { data } => {
            let result = data["result"].as_str().unwrap();
            assert_eq!(result.len(), 1024 * 1024);
            assert!(result.chars().all(|c| c == 'y'));
        }
        _ => panic!("Unexpected response type"),
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_timeout_handling() {
    let path = get_test_path("timeout");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Receive message but don't send response (simulate hang)
        let _msg = conn.recv().await.unwrap();

        // Keep connection open but don't respond
        sleep(Duration::from_secs(5)).await;
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // Send message
    let msg = IpcMessage::List;
    client.send(&msg).await.unwrap();

    // Try to receive with timeout
    let result = timeout(Duration::from_secs(1), client.recv()).await;
    assert!(result.is_err(), "Should timeout waiting for response");

    server_handle.abort();
}

#[tokio::test]
async fn test_subscription_events() {
    let path = get_test_path("subscription");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Receive subscription message
        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::Subscribe { subscription } => {
                match subscription {
                    SubscriptionType::StatusEvents { app_name } => {
                        assert_eq!(app_name, Some("test-app".to_string()));
                    }
                    _ => panic!("Wrong subscription type"),
                }

                // Send confirmation
                let response = IpcResponse::Success {
                    message: "Subscribed".to_string(),
                };
                conn.send(&response).await.unwrap();

                // Send some events
                for i in 0..3 {
                    let event = IpcResponse::Event {
                        event_type: "status-change".to_string(),
                        data: json!({
                            "app": "test-app",
                            "status": format!("state-{}", i)
                        }),
                    };
                    conn.send(&event).await.unwrap();
                    sleep(Duration::from_millis(100)).await;
                }
            }
            _ => panic!("Unexpected message type"),
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // Subscribe to events
    let msg = IpcMessage::Subscribe {
        subscription: SubscriptionType::StatusEvents {
            app_name: Some("test-app".to_string()),
        },
    };
    client.send(&msg).await.unwrap();

    // Receive confirmation
    let response = client.recv().await.unwrap();
    match response {
        IpcResponse::Success { message } => {
            assert_eq!(message, "Subscribed");
        }
        _ => panic!("Unexpected response type"),
    }

    // Receive events
    for i in 0..3 {
        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Event { event_type, data } => {
                assert_eq!(event_type, "status-change");
                assert_eq!(data["app"], "test-app");
                assert_eq!(data["status"], format!("state-{}", i));
            }
            _ => panic!("Unexpected response type"),
        }
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_error_response_handling() {
    let path = get_test_path("error_resp");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::Start { name, .. } => {
                // Simulate error response
                let response = if name == "invalid-app" {
                    IpcResponse::Error {
                        message: "Invalid app configuration".to_string(),
                    }
                } else {
                    IpcResponse::Success {
                        message: "Started".to_string(),
                    }
                };
                conn.send(&response).await.unwrap();
            }
            _ => panic!("Unexpected message type"),
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // Send invalid request
    let msg = IpcMessage::Start {
        name: "invalid-app".to_string(),
        config: "{}".to_string(),
    };
    client.send(&msg).await.unwrap();

    let response = client.recv().await.unwrap();
    match response {
        IpcResponse::Error { message } => {
            assert_eq!(message, "Invalid app configuration");
        }
        _ => panic!("Expected error response"),
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_message_ordering() {
    let path = get_test_path("ordering");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Receive and respond to messages in order
        for i in 0..10 {
            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::Logs { lines, .. } => {
                    assert_eq!(lines, i);

                    let response = IpcResponse::Data {
                        data: json!({
                            "sequence": i,
                            "lines": lines
                        }),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // Send messages in sequence
    for i in 0..10 {
        let msg = IpcMessage::Logs {
            name: None,
            lines: i,
        };
        client.send(&msg).await.unwrap();
    }

    // Verify responses arrive in order
    for i in 0..10 {
        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Data { data } => {
                assert_eq!(data["sequence"], i);
                assert_eq!(data["lines"], i);
            }
            _ => panic!("Unexpected response type"),
        }
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_reconnection_after_disconnect() {
    let path = get_test_path("reconnect");

    // First connection
    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();

        // Accept first connection
        let mut conn = server.accept().await.unwrap();
        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::List => {
                let response = IpcResponse::Success {
                    message: "First connection".to_string(),
                };
                conn.send(&response).await.unwrap();
            }
            _ => panic!("Unexpected message type"),
        }

        // Connection drops here
        drop(conn);

        // Accept second connection
        let mut conn = server.accept().await.unwrap();
        let msg = conn.recv().await.unwrap();
        match msg {
            IpcMessage::List => {
                let response = IpcResponse::Success {
                    message: "Second connection".to_string(),
                };
                conn.send(&response).await.unwrap();
            }
            _ => panic!("Unexpected message type"),
        }
    });

    sleep(Duration::from_millis(100)).await;

    // First client connection
    {
        let mut client = IpcClient::connect(&path).await.unwrap();
        client.send(&IpcMessage::List).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, "First connection");
            }
            _ => panic!("Unexpected response type"),
        }
        // Client drops here
    }

    sleep(Duration::from_millis(100)).await;

    // Second client connection (reconnection)
    {
        let mut client = IpcClient::connect(&path).await.unwrap();
        client.send(&IpcMessage::List).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, "Second connection");
            }
            _ => panic!("Unexpected response type"),
        }
    }

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_rapid_message_exchange() {
    let path = get_test_path("rapid");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Handle rapid messages
        for i in 0..100 {
            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::Delete { name } => {
                    assert_eq!(name, format!("app-{}", i));

                    let response = IpcResponse::Success {
                        message: format!("Deleted app-{}", i),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    // Send rapid messages
    let start = std::time::Instant::now();
    for i in 0..100 {
        let msg = IpcMessage::Delete {
            name: format!("app-{}", i),
        };
        client.send(&msg).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, format!("Deleted app-{}", i));
            }
            _ => panic!("Unexpected response type"),
        }
    }
    let duration = start.elapsed();

    // Should complete 100 round-trips reasonably quickly
    assert!(
        duration.as_secs() < 5,
        "Rapid message exchange took too long: {:?}",
        duration
    );

    server_handle.await.unwrap();
}
