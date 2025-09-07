#[cfg(test)]
mod tests {
    use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer, SubscriptionType};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio::time::timeout;

    fn get_test_socket_path() -> std::path::PathBuf {
        #[cfg(target_os = "linux")]
        {
            let temp_file = tempfile::NamedTempFile::new().unwrap();
            temp_file.path().to_path_buf()
        }
        #[cfg(windows)]
        {
            let unique_name = format!("test_{}_{}", std::process::id(), rand::random::<u32>());
            std::path::PathBuf::from(unique_name)
        }
    }

    #[tokio::test]
    async fn test_basic_client_server_communication() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();

        // Start server in background
        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut connection = server.accept().await.unwrap();

            // Receive message
            let msg = connection.recv().await.unwrap();
            match msg {
                IpcMessage::Start { name, config } => {
                    assert_eq!(name, "test-app");
                    assert_eq!(config, "test-config");

                    // Send response
                    let response = IpcResponse::Success {
                        message: "App started".to_string(),
                    };
                    connection.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect client
        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        // Send message
        let msg = IpcMessage::Start {
            name: "test-app".to_string(),
            config: "test-config".to_string(),
        };
        client.send(&msg).await.unwrap();

        // Receive response
        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, "App started");
            }
            _ => panic!("Unexpected response type"),
        }

        // Wait for server to finish
        timeout(Duration::from_secs(1), server_handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut connection = server.accept().await.unwrap();

            // Handle multiple messages
            for i in 0..3 {
                let msg = connection.recv().await.unwrap();
                match msg {
                    IpcMessage::Status { name } => {
                        assert_eq!(name, Some(format!("app-{}", i)));

                        let response = IpcResponse::Data {
                            data: serde_json::json!({
                                "status": "running",
                                "pid": 1000 + i
                            }),
                        };
                        connection.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        // Send and receive multiple messages
        for i in 0..3 {
            let msg = IpcMessage::Status {
                name: Some(format!("app-{}", i)),
            };
            client.send(&msg).await.unwrap();

            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Data { data } => {
                    assert_eq!(data["status"], "running");
                    assert_eq!(data["pid"], 1000 + i);
                }
                _ => panic!("Unexpected response type"),
            }
        }

        timeout(Duration::from_secs(1), server_handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_large_message() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();

        // Create a large config string (1MB)
        let large_config = "x".repeat(1024 * 1024);
        let expected_config = large_config.clone();

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut connection = server.accept().await.unwrap();

            let msg = connection.recv().await.unwrap();
            match msg {
                IpcMessage::Start { name, config } => {
                    assert_eq!(name, "large-app");
                    assert_eq!(config.len(), 1024 * 1024);
                    assert_eq!(config, expected_config);

                    let response = IpcResponse::Success {
                        message: "Large message received".to_string(),
                    };
                    connection.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        let msg = IpcMessage::Start {
            name: "large-app".to_string(),
            config: large_config,
        };
        client.send(&msg).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, "Large message received");
            }
            _ => panic!("Unexpected response type"),
        }

        timeout(Duration::from_secs(2), server_handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_message_size_limit() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut _connection = server.accept().await.unwrap();
            // Server just waits, client should fail with oversized message
            tokio::time::sleep(Duration::from_secs(1)).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        // Try to send a message that exceeds the maximum size (>10MB)
        let oversized_config = "x".repeat(11 * 1024 * 1024);
        let msg = IpcMessage::Start {
            name: "oversized-app".to_string(),
            config: oversized_config,
        };

        let result = client.send(&msg).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceeds maximum allowed size")
        );

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_timeout_handling() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut connection = server.accept().await.unwrap();

            // Receive message but don't send response (simulate hang)
            let _msg = connection.recv().await.unwrap();
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        // Set a short timeout
        client.set_timeout(Duration::from_millis(500));

        let msg = IpcMessage::List;
        client.send(&msg).await.unwrap();

        // Should timeout waiting for response
        let result = client.recv().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_concurrent_clients() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();
        let client_count = 3;
        let counter = Arc::new(Mutex::new(0));
        let server_counter = counter.clone();

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();

            // Handle multiple clients sequentially
            for _ in 0..client_count {
                let mut connection = server.accept().await.unwrap();
                let counter = server_counter.clone();

                tokio::spawn(async move {
                    let msg = connection.recv().await.unwrap();
                    match msg {
                        IpcMessage::Status { name } => {
                            let mut count = counter.lock().await;
                            *count += 1;
                            let current = *count;

                            let response = IpcResponse::Data {
                                data: serde_json::json!({
                                    "client": name,
                                    "order": current
                                }),
                            };
                            connection.send(&response).await.unwrap();
                        }
                        _ => panic!("Unexpected message type"),
                    }
                });
            }

            // Wait for all clients to be processed
            tokio::time::sleep(Duration::from_secs(1)).await;
        });

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Connect multiple clients
        let mut handles = vec![];
        for i in 0..client_count {
            let socket_path = socket_path.clone();
            let handle = tokio::spawn(async move {
                let mut client = IpcClient::connect(&socket_path).await.unwrap();

                let msg = IpcMessage::Status {
                    name: Some(format!("client-{}", i)),
                };
                client.send(&msg).await.unwrap();

                let response = client.recv().await.unwrap();
                match response {
                    IpcResponse::Data { data } => {
                        assert_eq!(data["client"], format!("client-{}", i));
                        data["order"].as_u64().unwrap()
                    }
                    _ => panic!("Unexpected response type"),
                }
            });
            handles.push(handle);

            // On Windows, add small delay to allow server to create next pipe instance
            #[cfg(windows)]
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Wait for all clients
        for handle in handles {
            handle.await.unwrap();
        }

        let final_count = *counter.lock().await;
        assert_eq!(final_count, client_count);

        timeout(Duration::from_secs(2), server_handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_all_message_types() {
        let socket_path = get_test_socket_path();
        let server_path = socket_path.clone();

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut connection = server.accept().await.unwrap();

            // Test each message type
            let messages = vec![
                IpcMessage::Start {
                    name: "app1".to_string(),
                    config: "{}".to_string(),
                },
                IpcMessage::Stop {
                    name: "app1".to_string(),
                },
                IpcMessage::Restart {
                    name: "app1".to_string(),
                },
                IpcMessage::Delete {
                    name: "app1".to_string(),
                },
                IpcMessage::Status { name: None },
                IpcMessage::List,
                IpcMessage::Logs {
                    name: None,
                    lines: 100,
                },
                IpcMessage::Subscribe {
                    subscription: SubscriptionType::AllEvents { app_name: None },
                },
                IpcMessage::Unsubscribe,
            ];

            for expected in messages {
                let received = connection.recv().await.unwrap();

                // Verify message matches
                let matches = match (&received, &expected) {
                    (IpcMessage::Start { .. }, IpcMessage::Start { .. }) => true,
                    (IpcMessage::Stop { .. }, IpcMessage::Stop { .. }) => true,
                    (IpcMessage::Restart { .. }, IpcMessage::Restart { .. }) => true,
                    (IpcMessage::Delete { .. }, IpcMessage::Delete { .. }) => true,
                    (IpcMessage::Status { .. }, IpcMessage::Status { .. }) => true,
                    (IpcMessage::List, IpcMessage::List) => true,
                    (IpcMessage::Logs { .. }, IpcMessage::Logs { .. }) => true,
                    (IpcMessage::Subscribe { .. }, IpcMessage::Subscribe { .. }) => true,
                    (IpcMessage::Unsubscribe, IpcMessage::Unsubscribe) => true,
                    _ => false,
                };
                assert!(matches, "Message type mismatch");

                let response = IpcResponse::Success {
                    message: "OK".to_string(),
                };
                connection.send(&response).await.unwrap();
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();

        // Send all message types
        let messages = vec![
            IpcMessage::Start {
                name: "app1".to_string(),
                config: "{}".to_string(),
            },
            IpcMessage::Stop {
                name: "app1".to_string(),
            },
            IpcMessage::Restart {
                name: "app1".to_string(),
            },
            IpcMessage::Delete {
                name: "app1".to_string(),
            },
            IpcMessage::Status { name: None },
            IpcMessage::List,
            IpcMessage::Logs {
                name: None,
                lines: 100,
            },
            IpcMessage::Subscribe {
                subscription: SubscriptionType::AllEvents { app_name: None },
            },
            IpcMessage::Unsubscribe,
        ];

        for msg in messages {
            client.send(&msg).await.unwrap();
            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Success { message } => assert_eq!(message, "OK"),
                _ => panic!("Unexpected response"),
            }
        }

        timeout(Duration::from_secs(1), server_handle)
            .await
            .unwrap()
            .unwrap();
    }
}
