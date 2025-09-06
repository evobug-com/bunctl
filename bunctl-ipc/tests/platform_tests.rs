#[cfg(windows)]
mod windows_tests {
    use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_windows_named_pipe_path_handling() {
        // Test various path formats
        let test_cases = vec![
            "test_app",
            "test-app-123",
            "app_with_underscore",
            "app.with.dots",
        ];

        for name in test_cases {
            let path = std::path::PathBuf::from(name);

            let server_path = path.clone();
            let server_handle = tokio::spawn(async move {
                let mut server = IpcServer::bind(&server_path).await.unwrap();
                let mut conn = server.accept().await.unwrap();

                let msg = conn.recv().await.unwrap();
                match msg {
                    IpcMessage::List => {
                        let response = IpcResponse::Success {
                            message: format!("Pipe: {}", server_path.display()),
                        };
                        conn.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                }
            });

            sleep(Duration::from_millis(100)).await;

            let mut client = IpcClient::connect(&path).await.unwrap();
            client.send(&IpcMessage::List).await.unwrap();

            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Success { message } => {
                    assert!(message.contains(name));
                }
                _ => panic!("Unexpected response type"),
            }

            server_handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_windows_multiple_pipe_instances() {
        // Windows named pipes support multiple instances
        let path = std::path::PathBuf::from("multi_instance_test");

        let server_path = path.clone();
        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();

            // Handle multiple connections sequentially
            for i in 0..3 {
                let mut conn = server.accept().await.unwrap();

                tokio::spawn(async move {
                    let msg = conn.recv().await.unwrap();
                    match msg {
                        IpcMessage::Status { .. } => {
                            let response = IpcResponse::Data {
                                data: serde_json::json!({
                                    "connection": i,
                                    "status": "connected"
                                }),
                            };
                            conn.send(&response).await.unwrap();
                        }
                        _ => panic!("Unexpected message type"),
                    }
                });
            }

            sleep(Duration::from_secs(2)).await;
        });

        sleep(Duration::from_millis(200)).await;

        // Connect multiple clients with staggered timing to avoid pipe busy errors
        let mut handles = vec![];
        for i in 0..3 {
            // Add delay between client connections to ensure server has time to create new pipe instance
            if i > 0 {
                sleep(Duration::from_millis(100)).await;
            }

            let client_path = path.clone();
            let handle = tokio::spawn(async move {
                // Retry connection with backoff
                let mut client = None;
                for retry in 0..10 {
                    match IpcClient::connect(&client_path).await {
                        Ok(c) => {
                            client = Some(c);
                            break;
                        }
                        Err(_) if retry < 9 => {
                            sleep(Duration::from_millis(50 * (retry + 1) as u64)).await;
                        }
                        Err(e) => {
                            panic!("Failed to connect after 10 retries: {:?}", e);
                        }
                    }
                }

                let mut client = client.unwrap();
                let msg = IpcMessage::Status {
                    name: Some(format!("client-{}", i)),
                };
                client.send(&msg).await.unwrap();

                let response = client.recv().await.unwrap();
                match response {
                    IpcResponse::Data { data } => {
                        assert_eq!(data["status"], "connected");
                    }
                    _ => panic!("Unexpected response type"),
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_windows_pipe_buffer_sizes() {
        let path = std::path::PathBuf::from("buffer_test");

        // Test various message sizes
        let test_sizes = vec![
            10,      // Small
            1024,    // 1KB
            65536,   // 64KB
            262144,  // 256KB
            1048576, // 1MB
        ];

        for size in test_sizes {
            let data = "x".repeat(size);
            let data_clone = data.clone();

            let server_path = path.clone();
            let server_handle = tokio::spawn(async move {
                let mut server = IpcServer::bind(&server_path).await.unwrap();
                let mut conn = server.accept().await.unwrap();

                let msg = conn.recv().await.unwrap();
                match msg {
                    IpcMessage::Start { config, .. } => {
                        assert_eq!(config.len(), size);
                        assert_eq!(config, data_clone);

                        let response = IpcResponse::Success {
                            message: format!("Received {} bytes", size),
                        };
                        conn.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                }
            });

            sleep(Duration::from_millis(100)).await;

            let mut client = IpcClient::connect(&path).await.unwrap();
            let msg = IpcMessage::Start {
                name: "test".to_string(),
                config: data,
            };
            client.send(&msg).await.unwrap();

            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Success { message } => {
                    assert_eq!(message, format!("Received {} bytes", size));
                }
                _ => panic!("Unexpected response type"),
            }

            server_handle.await.unwrap();
        }
    }
}

#[cfg(unix)]
mod unix_tests {
    use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_unix_socket_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        // First server creates socket
        {
            let server = IpcServer::bind(&socket_path).await.unwrap();
            assert!(socket_path.exists(), "Socket file should exist");
            drop(server);
        }

        // Socket should be cleaned up and recreated
        {
            let server = IpcServer::bind(&socket_path).await.unwrap();
            assert!(socket_path.exists(), "Socket file should exist again");
            drop(server);
        }
    }

    #[tokio::test]
    async fn test_unix_socket_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("perm_test.sock");

        let server_path = socket_path.clone();
        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();

            // Check socket file permissions
            let metadata = fs::metadata(&server_path).unwrap();
            let permissions = metadata.permissions();
            let mode = permissions.mode();

            // Socket should be accessible by owner
            assert!(
                mode & 0o600 != 0,
                "Socket should have owner read/write permissions"
            );

            let mut conn = server.accept().await.unwrap();
            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::List => {
                    let response = IpcResponse::Success {
                        message: format!("Permissions: {:o}", mode),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        });

        sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();
        client.send(&IpcMessage::List).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert!(message.contains("Permissions"));
            }
            _ => panic!("Unexpected response type"),
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_unix_socket_path_length() {
        // Unix domain sockets have path length limitations (typically 108 chars)
        let temp_dir = TempDir::new().unwrap();

        // Test with normal length path
        let normal_path = temp_dir.path().join("normal.sock");
        let server = IpcServer::bind(&normal_path).await;
        assert!(server.is_ok(), "Normal path should work");

        // Test with very long filename (but within total path limits)
        let long_name = format!("{}.sock", "a".repeat(50));
        let long_path = temp_dir.path().join(long_name);
        let server = IpcServer::bind(&long_path).await;
        assert!(
            server.is_ok(),
            "Long filename should work if total path is within limits"
        );
    }

    #[tokio::test]
    async fn test_unix_socket_concurrent_connections() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("concurrent.sock");

        let server_path = socket_path.clone();
        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();

            // Handle multiple connections
            let mut handles = vec![];
            for i in 0..5 {
                let mut conn = server.accept().await.unwrap();

                let handle = tokio::spawn(async move {
                    let msg = conn.recv().await.unwrap();
                    match msg {
                        IpcMessage::Status { name } => {
                            // Simulate some processing
                            sleep(Duration::from_millis(50)).await;

                            let response = IpcResponse::Data {
                                data: serde_json::json!({
                                    "client": name,
                                    "handler": i
                                }),
                            };
                            conn.send(&response).await.unwrap();
                        }
                        _ => panic!("Unexpected message type"),
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.await.unwrap();
            }
        });

        sleep(Duration::from_millis(100)).await;

        // Spawn concurrent clients
        let mut client_handles = vec![];
        for i in 0..5 {
            let client_path = socket_path.clone();
            let handle = tokio::spawn(async move {
                let mut client = IpcClient::connect(&client_path).await.unwrap();
                let msg = IpcMessage::Status {
                    name: Some(format!("client-{}", i)),
                };
                client.send(&msg).await.unwrap();

                let response = client.recv().await.unwrap();
                match response {
                    IpcResponse::Data { data } => {
                        assert_eq!(data["client"], format!("client-{}", i));
                        assert!(data["handler"].as_u64().unwrap() < 5);
                    }
                    _ => panic!("Unexpected response type"),
                }
            });
            client_handles.push(handle);
        }

        for handle in client_handles {
            handle.await.unwrap();
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_unix_socket_abstract_namespace() {
        // Note: Abstract namespace is Linux-specific
        // This test demonstrates handling of special socket paths
        let temp_dir = TempDir::new().unwrap();

        // Test with paths containing special characters
        let special_paths = vec![
            "socket-with-dash.sock",
            "socket_with_underscore.sock",
            "socket.with.dots.sock",
            "socket123.sock",
        ];

        for name in special_paths {
            let socket_path = temp_dir.path().join(name);

            let server_path = socket_path.clone();
            let server_handle = tokio::spawn(async move {
                let mut server = IpcServer::bind(&server_path).await.unwrap();
                let mut conn = server.accept().await.unwrap();

                let msg = conn.recv().await.unwrap();
                match msg {
                    IpcMessage::List => {
                        let response = IpcResponse::Success {
                            message: format!("Socket: {}", server_path.display()),
                        };
                        conn.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                }
            });

            sleep(Duration::from_millis(100)).await;

            let mut client = IpcClient::connect(&socket_path).await.unwrap();
            client.send(&IpcMessage::List).await.unwrap();

            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Success { message } => {
                    assert!(message.contains(name));
                }
                _ => panic!("Unexpected response type"),
            }

            server_handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_unix_socket_buffer_behavior() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("buffer.sock");

        let server_path = socket_path.clone();
        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut conn = server.accept().await.unwrap();

            // Receive multiple messages before sending responses
            let mut messages = vec![];
            for _ in 0..5 {
                let msg = conn.recv().await.unwrap();
                messages.push(msg);
            }

            // Send all responses
            for (i, msg) in messages.into_iter().enumerate() {
                match msg {
                    IpcMessage::Logs { lines, .. } => {
                        let response = IpcResponse::Data {
                            data: serde_json::json!({
                                "index": i,
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

        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        // Send all messages first
        for i in 0..5 {
            let msg = IpcMessage::Logs {
                name: None,
                lines: i * 10,
            };
            client.send(&msg).await.unwrap();
        }

        // Then receive all responses
        for i in 0..5 {
            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Data { data } => {
                    assert_eq!(data["index"], i);
                    assert_eq!(data["lines"], i * 10);
                }
                _ => panic!("Unexpected response type"),
            }
        }

        server_handle.await.unwrap();
    }
}
