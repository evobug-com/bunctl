use bunctl_ipc::{IpcClient, IpcMessage, IpcResponse, IpcServer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use tokio::time::sleep;

#[cfg(unix)]
use tempfile::TempDir;

#[cfg(windows)]
fn get_test_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("stress_{}", name))
}

#[cfg(unix)]
fn get_test_path(name: &str) -> std::path::PathBuf {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join(format!("stress_{}.sock", name));
    std::mem::forget(temp_dir);
    path
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_high_throughput() {
    let path = get_test_path("throughput");
    let message_count = 1000;

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        for i in 0..message_count {
            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::Status { name } => {
                    assert_eq!(name, Some(format!("msg-{}", i)));

                    let response = IpcResponse::Success {
                        message: format!("Processed {}", i),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    let start = Instant::now();

    for i in 0..message_count {
        let msg = IpcMessage::Status {
            name: Some(format!("msg-{}", i)),
        };
        client.send(&msg).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, format!("Processed {}", i));
            }
            _ => panic!("Unexpected response type"),
        }
    }

    let duration = start.elapsed();
    let messages_per_sec = message_count as f64 / duration.as_secs_f64();

    println!(
        "Throughput test: {} messages in {:?} ({:.0} msg/sec)",
        message_count, duration, messages_per_sec
    );

    // Should handle at least 100 messages per second
    assert!(
        messages_per_sec > 100.0,
        "Throughput too low: {:.0} msg/sec",
        messages_per_sec
    );

    server_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_stress() {
    let path = get_test_path("concurrent_stress");
    let client_count = 10;
    let messages_per_client = 50;

    let total_messages = Arc::new(AtomicU64::new(0));
    let total_clone = total_messages.clone();

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();

        let mut handles = vec![];
        for _ in 0..client_count {
            let mut conn = server.accept().await.unwrap();
            let total = total_clone.clone();

            let handle = tokio::spawn(async move {
                for _ in 0..messages_per_client {
                    let msg = conn.recv().await.unwrap();
                    match msg {
                        IpcMessage::Logs { lines, .. } => {
                            total.fetch_add(1, Ordering::SeqCst);

                            let response = IpcResponse::Data {
                                data: serde_json::json!({
                                    "processed": lines
                                }),
                            };
                            conn.send(&response).await.unwrap();
                        }
                        _ => panic!("Unexpected message type"),
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
    });

    sleep(Duration::from_millis(200)).await;

    let barrier = Arc::new(Barrier::new(client_count));
    let mut client_handles = vec![];

    let start = Instant::now();

    for client_id in 0..client_count {
        let client_path = path.clone();
        let barrier_clone = barrier.clone();

        let handle = tokio::spawn(async move {
            // Retry connection with exponential backoff for Windows named pipes
            let mut client = None;
            for retry in 0..10 {
                match IpcClient::connect(&client_path).await {
                    Ok(c) => {
                        client = Some(c);
                        break;
                    }
                    Err(_) if retry < 9 => {
                        tokio::time::sleep(Duration::from_millis(50 * (retry + 1) as u64)).await;
                    }
                    Err(e) => {
                        panic!("Failed to connect after 10 retries: {:?}", e);
                    }
                }
            }
            let mut client = client.unwrap();

            // Synchronize start
            barrier_clone.wait().await;

            for i in 0..messages_per_client {
                let msg = IpcMessage::Logs {
                    name: Some(format!("client-{}", client_id)),
                    lines: i,
                };
                client.send(&msg).await.unwrap();

                let response = client.recv().await.unwrap();
                match response {
                    IpcResponse::Data { data } => {
                        assert_eq!(data["processed"], i);
                    }
                    _ => panic!("Unexpected response type"),
                }
            }
        });
        client_handles.push(handle);
    }

    for handle in client_handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    let total_count = total_messages.load(Ordering::SeqCst);

    assert_eq!(
        total_count,
        (client_count * messages_per_client) as u64,
        "Not all messages were processed"
    );

    println!(
        "Concurrent stress test: {} clients, {} total messages in {:?}",
        client_count, total_count, duration
    );

    server_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_memory_stress() {
    let path = get_test_path("memory");
    let iterations = 100;

    // Test with progressively larger messages
    let sizes = vec![1_000, 10_000, 100_000, 500_000];

    for size in sizes {
        let server_path = path.clone();
        let size_clone = size;

        let server_handle = tokio::spawn(async move {
            let mut server = IpcServer::bind(&server_path).await.unwrap();
            let mut conn = server.accept().await.unwrap();

            for _ in 0..iterations {
                let msg = conn.recv().await.unwrap();
                match msg {
                    IpcMessage::Start { config, .. } => {
                        assert_eq!(config.len(), size_clone);

                        let response = IpcResponse::Success {
                            message: format!("Received {} bytes", size_clone),
                        };
                        conn.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                }
            }
        });

        sleep(Duration::from_millis(100)).await;

        let mut client = IpcClient::connect(&path).await.unwrap();

        let data = "x".repeat(size);
        let start = Instant::now();

        for _ in 0..iterations {
            let msg = IpcMessage::Start {
                name: "test".to_string(),
                config: data.clone(),
            };
            client.send(&msg).await.unwrap();

            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Success { message } => {
                    assert_eq!(message, format!("Received {} bytes", size));
                }
                _ => panic!("Unexpected response type"),
            }
        }

        let duration = start.elapsed();
        println!(
            "Memory stress test: {} iterations of {} bytes in {:?}",
            iterations, size, duration
        );

        server_handle.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rapid_reconnection() {
    let path = get_test_path("rapid_reconnect");
    let reconnect_count = 20;

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();

        for i in 0..reconnect_count {
            let mut conn = server.accept().await.unwrap();

            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::Status { .. } => {
                    let response = IpcResponse::Data {
                        data: serde_json::json!({
                            "connection": i
                        }),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }

            // Connection drops here
        }
    });

    sleep(Duration::from_millis(100)).await;

    let start = Instant::now();

    for i in 0..reconnect_count {
        let mut client = IpcClient::connect(&path).await.unwrap();

        let msg = IpcMessage::Status {
            name: Some(format!("reconnect-{}", i)),
        };
        client.send(&msg).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Data { data } => {
                assert_eq!(data["connection"], i);
            }
            _ => panic!("Unexpected response type"),
        }

        // Client drops here
    }

    let duration = start.elapsed();
    println!(
        "Rapid reconnection test: {} connections in {:?}",
        reconnect_count, duration
    );

    // Should handle rapid reconnections
    assert!(
        duration.as_secs() < 10,
        "Reconnections took too long: {:?}",
        duration
    );

    server_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_burst_messages() {
    let path = get_test_path("burst");
    let burst_size = 100;
    let burst_count = 5;

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        for burst in 0..burst_count {
            // Receive burst
            for i in 0..burst_size {
                let msg = conn.recv().await.unwrap();
                match msg {
                    IpcMessage::Delete { name } => {
                        assert_eq!(name, format!("burst-{}-msg-{}", burst, i));
                    }
                    _ => panic!("Unexpected message type"),
                }
            }

            // Send burst of responses
            for i in 0..burst_size {
                let response = IpcResponse::Success {
                    message: format!("Deleted burst-{}-msg-{}", burst, i),
                };
                conn.send(&response).await.unwrap();
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    for burst in 0..burst_count {
        let start = Instant::now();

        // Send burst
        for i in 0..burst_size {
            let msg = IpcMessage::Delete {
                name: format!("burst-{}-msg-{}", burst, i),
            };
            client.send(&msg).await.unwrap();
        }

        // Receive burst of responses
        for i in 0..burst_size {
            let response = client.recv().await.unwrap();
            match response {
                IpcResponse::Success { message } => {
                    assert_eq!(message, format!("Deleted burst-{}-msg-{}", burst, i));
                }
                _ => panic!("Unexpected response type"),
            }
        }

        let duration = start.elapsed();
        println!(
            "Burst {} ({} messages) completed in {:?}",
            burst, burst_size, duration
        );
    }

    server_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mixed_size_messages() {
    let path = get_test_path("mixed_size");

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        // Handle messages of varying sizes
        for i in 0..50 {
            let msg = conn.recv().await.unwrap();
            match msg {
                IpcMessage::Start { name, config } => {
                    let expected_size = if i % 3 == 0 {
                        10
                    } else if i % 3 == 1 {
                        1000
                    } else {
                        100000
                    };

                    assert_eq!(config.len(), expected_size);
                    assert_eq!(name, format!("msg-{}", i));

                    let response = IpcResponse::Success {
                        message: format!("Processed {} bytes", config.len()),
                    };
                    conn.send(&response).await.unwrap();
                }
                _ => panic!("Unexpected message type"),
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    let start = Instant::now();

    for i in 0..50 {
        let size = if i % 3 == 0 {
            10
        } else if i % 3 == 1 {
            1000
        } else {
            100000
        };

        let msg = IpcMessage::Start {
            name: format!("msg-{}", i),
            config: "x".repeat(size),
        };
        client.send(&msg).await.unwrap();

        let response = client.recv().await.unwrap();
        match response {
            IpcResponse::Success { message } => {
                assert_eq!(message, format!("Processed {} bytes", size));
            }
            _ => panic!("Unexpected response type"),
        }
    }

    let duration = start.elapsed();
    println!("Mixed size messages test completed in {:?}", duration);

    server_handle.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sustained_load() {
    let path = get_test_path("sustained");
    let duration_secs = 3;
    let message_count = Arc::new(AtomicU64::new(0));
    let message_count_clone = message_count.clone();

    let server_path = path.clone();
    let server_handle = tokio::spawn(async move {
        let mut server = IpcServer::bind(&server_path).await.unwrap();
        let mut conn = server.accept().await.unwrap();

        let start = Instant::now();
        while start.elapsed().as_secs() < duration_secs {
            match tokio::time::timeout(Duration::from_millis(100), conn.recv()).await {
                Ok(Ok(msg)) => match msg {
                    IpcMessage::Restart { .. } => {
                        message_count_clone.fetch_add(1, Ordering::SeqCst);

                        let response = IpcResponse::Success {
                            message: "Restarted".to_string(),
                        };
                        conn.send(&response).await.unwrap();
                    }
                    _ => panic!("Unexpected message type"),
                },
                Ok(Err(_)) => break, // Connection error
                Err(_) => continue,  // Timeout, keep waiting
            }
        }
    });

    sleep(Duration::from_millis(100)).await;

    let mut client = IpcClient::connect(&path).await.unwrap();

    let start = Instant::now();
    let mut client_count = 0u64;

    while start.elapsed().as_secs() < duration_secs {
        let msg = IpcMessage::Restart {
            name: format!("app-{}", client_count),
        };

        if client.send(&msg).await.is_err() {
            break;
        }

        match tokio::time::timeout(Duration::from_millis(100), client.recv()).await {
            Ok(Ok(response)) => match response {
                IpcResponse::Success { .. } => {
                    client_count += 1;
                }
                _ => panic!("Unexpected response type"),
            },
            _ => break,
        }
    }

    let total = message_count.load(Ordering::SeqCst);
    let rate = total as f64 / duration_secs as f64;

    println!(
        "Sustained load test: {} messages over {} seconds ({:.0} msg/sec)",
        total, duration_secs, rate
    );

    assert!(total > 50, "Should process significant message volume");
    assert_eq!(total, client_count, "Message counts should match");

    server_handle.abort();
}
