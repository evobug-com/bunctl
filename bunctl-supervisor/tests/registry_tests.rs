use bunctl_core::{AppId, ProcessHandle};
use bunctl_supervisor::ProcessRegistry;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

#[test]
fn test_registry_register_and_get() {
    let registry = ProcessRegistry::new();
    let app_id = AppId::new("test-app").unwrap();

    // Create a mock process handle
    let handle = ProcessHandle {
        pid: 1234,
        app_id: app_id.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };

    registry.register(app_id.clone(), handle);

    let retrieved = registry.get(&app_id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().pid, 1234);
}

#[test]
fn test_registry_unregister() {
    let registry = ProcessRegistry::new();
    let app_id = AppId::new("test-app").unwrap();

    let handle = ProcessHandle {
        pid: 1234,
        app_id: app_id.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };

    registry.register(app_id.clone(), handle);
    assert!(registry.get(&app_id).is_some());

    let unregistered = registry.unregister(&app_id);
    assert!(unregistered.is_some());
    assert_eq!(unregistered.unwrap().pid, 1234);

    assert!(registry.get(&app_id).is_none());
}

#[test]
fn test_registry_get_by_pid() {
    let registry = ProcessRegistry::new();
    let app_id = AppId::new("test-app").unwrap();

    let handle = ProcessHandle {
        pid: 5678,
        app_id: app_id.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };

    registry.register(app_id.clone(), handle);

    let found_app = registry.get_by_pid(5678);
    assert!(found_app.is_some());
    assert_eq!(found_app.unwrap().as_str(), "test-app");

    assert!(registry.get_by_pid(9999).is_none());
}

#[test]
fn test_registry_list() {
    let registry = ProcessRegistry::new();

    let app1 = AppId::new("app1").unwrap();
    let app2 = AppId::new("app2").unwrap();
    let app3 = AppId::new("app3").unwrap();

    registry.register(
        app1.clone(),
        ProcessHandle {
            pid: 100,
            app_id: app1.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    registry.register(
        app2.clone(),
        ProcessHandle {
            pid: 200,
            app_id: app2.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    registry.register(
        app3.clone(),
        ProcessHandle {
            pid: 300,
            app_id: app3.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    let list = registry.list();
    assert_eq!(list.len(), 3);

    let pids: Vec<u32> = list.iter().map(|(_, pid)| *pid).collect();
    assert!(pids.contains(&100));
    assert!(pids.contains(&200));
    assert!(pids.contains(&300));
}

#[test]
fn test_registry_count() {
    let registry = ProcessRegistry::new();

    assert_eq!(registry.count(), 0);

    let app1 = AppId::new("app1").unwrap();
    registry.register(
        app1.clone(),
        ProcessHandle {
            pid: 100,
            app_id: app1,
            inner: None,
            stdout: None,
            stderr: None,
        },
    );
    assert_eq!(registry.count(), 1);

    let app2 = AppId::new("app2").unwrap();
    registry.register(
        app2.clone(),
        ProcessHandle {
            pid: 200,
            app_id: app2,
            inner: None,
            stdout: None,
            stderr: None,
        },
    );
    assert_eq!(registry.count(), 2);

    let app3 = AppId::new("app3").unwrap();
    registry.unregister(&app3); // Unregister non-existent
    assert_eq!(registry.count(), 2);

    registry.unregister(&AppId::new("app1").unwrap());
    assert_eq!(registry.count(), 1);
}

#[test]
fn test_registry_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(ProcessRegistry::new());
    let mut handles = vec![];

    // Spawn multiple threads that register processes
    for i in 0..10 {
        let registry_clone = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            let app_id = AppId::new(format!("app{}", i)).unwrap();
            registry_clone.register(
                app_id.clone(),
                ProcessHandle {
                    pid: i as u32,
                    app_id,
                    inner: None,
                    stdout: None,
                    stderr: None,
                },
            );
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(registry.count(), 10);
}

#[test]
fn test_registry_overwrite() {
    let registry = ProcessRegistry::new();
    let app_id = AppId::new("test-app").unwrap();

    // Register first handle
    registry.register(
        app_id.clone(),
        ProcessHandle {
            pid: 1111,
            app_id: app_id.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    assert_eq!(registry.get(&app_id).unwrap().pid, 1111);

    // Register second handle with same app_id
    registry.register(
        app_id.clone(),
        ProcessHandle {
            pid: 2222,
            app_id: app_id.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    // Should have overwritten the first one
    assert_eq!(registry.get(&app_id).unwrap().pid, 2222);
    assert_eq!(registry.count(), 1);

    // Old PID should not be found
    assert!(registry.get_by_pid(1111).is_none());
    assert!(registry.get_by_pid(2222).is_some());
}

#[test]
fn test_registry_empty_operations() {
    let registry = ProcessRegistry::new();

    // Operations on empty registry
    assert_eq!(registry.count(), 0);
    assert!(registry.list().is_empty());
    assert!(registry.get(&AppId::new("nonexistent").unwrap()).is_none());
    assert!(registry.get_by_pid(9999).is_none());
    assert!(
        registry
            .unregister(&AppId::new("nonexistent").unwrap())
            .is_none()
    );
}

#[test]
fn test_registry_pid_collision() {
    let registry = ProcessRegistry::new();
    let app1 = AppId::new("app1").unwrap();
    let app2 = AppId::new("app2").unwrap();

    // Register two apps with different PIDs
    registry.register(
        app1.clone(),
        ProcessHandle {
            pid: 1000,
            app_id: app1.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    registry.register(
        app2.clone(),
        ProcessHandle {
            pid: 2000,
            app_id: app2.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    // Verify both are registered
    assert_eq!(registry.count(), 2);
    assert_eq!(registry.get_by_pid(1000).unwrap(), app1);
    assert_eq!(registry.get_by_pid(2000).unwrap(), app2);

    // Update app1 with new PID
    registry.register(
        app1.clone(),
        ProcessHandle {
            pid: 3000,
            app_id: app1.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    // Old PID should be removed, new PID should be registered
    assert!(registry.get_by_pid(1000).is_none());
    assert_eq!(registry.get_by_pid(3000).unwrap(), app1);
    assert_eq!(registry.get_by_pid(2000).unwrap(), app2);
    assert_eq!(registry.count(), 2);
}

#[test]
fn test_registry_stress_concurrent_modifications() {
    let registry = Arc::new(ProcessRegistry::new());
    let iterations = 1000;
    let thread_count = 10;
    let mut handles = vec![];

    // Spawn threads that perform various operations
    for thread_id in 0..thread_count {
        let registry_clone = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            for i in 0..iterations {
                let app_id = AppId::new(format!("app_{}_{}", thread_id, i % 10)).unwrap();

                // Register
                registry_clone.register(
                    app_id.clone(),
                    ProcessHandle {
                        pid: (thread_id * 10000 + i) as u32,
                        app_id: app_id.clone(),
                        inner: None,
                        stdout: None,
                        stderr: None,
                    },
                );

                // Read operations
                let _ = registry_clone.get(&app_id);
                let _ = registry_clone.list();
                let _ = registry_clone.count();

                // Sometimes unregister
                if i % 3 == 0 {
                    registry_clone.unregister(&app_id);
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify registry is in consistent state
    let list = registry.list();
    let count = registry.count();
    assert_eq!(list.len(), count);

    // Verify pid_to_app consistency
    for (app_id, pid) in list {
        assert_eq!(registry.get_by_pid(pid), Some(app_id.clone()));
        assert_eq!(registry.get(&app_id).unwrap().pid, pid);
    }
}

#[test]
fn test_registry_large_scale() {
    let registry = ProcessRegistry::new();
    let app_count = 10000;

    // Register many processes
    for i in 0..app_count {
        let app_id = AppId::new(format!("app_{}", i)).unwrap();
        registry.register(
            app_id.clone(),
            ProcessHandle {
                pid: i as u32,
                app_id,
                inner: None,
                stdout: None,
                stderr: None,
            },
        );
    }

    assert_eq!(registry.count(), app_count);

    // Verify all can be retrieved
    for i in 0..app_count {
        let app_id = AppId::new(format!("app_{}", i)).unwrap();
        assert!(registry.get(&app_id).is_some());
        assert!(registry.get_by_pid(i as u32).is_some());
    }

    // Unregister half
    for i in 0..app_count / 2 {
        let app_id = AppId::new(format!("app_{}", i)).unwrap();
        assert!(registry.unregister(&app_id).is_some());
    }

    assert_eq!(registry.count(), app_count - app_count / 2);

    // Verify correct ones remain
    for i in 0..app_count {
        let app_id = AppId::new(format!("app_{}", i)).unwrap();
        if i < app_count / 2 {
            assert!(registry.get(&app_id).is_none());
        } else {
            assert!(registry.get(&app_id).is_some());
        }
    }
}

#[test]
fn test_registry_thread_safety_with_atomics() {
    let registry = Arc::new(ProcessRegistry::new());
    let counter = Arc::new(AtomicU32::new(0));
    let thread_count = 20;
    let ops_per_thread = 500;
    let mut handles = vec![];

    for _ in 0..thread_count {
        let registry_clone = Arc::clone(&registry);
        let counter_clone = Arc::clone(&counter);

        let handle = thread::spawn(move || {
            for _ in 0..ops_per_thread {
                let pid = counter_clone.fetch_add(1, Ordering::SeqCst);
                let app_id = AppId::new(format!("app_{}", pid)).unwrap();

                registry_clone.register(
                    app_id.clone(),
                    ProcessHandle {
                        pid,
                        app_id: app_id.clone(),
                        inner: None,
                        stdout: None,
                        stderr: None,
                    },
                );

                // Immediately verify it was registered
                assert!(registry_clone.get(&app_id).is_some());
                assert_eq!(registry_clone.get_by_pid(pid), Some(app_id));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final count
    assert_eq!(registry.count(), (thread_count * ops_per_thread) as usize);
}

#[test]
fn test_registry_pid_reuse() {
    let registry = ProcessRegistry::new();
    let app1 = AppId::new("app1").unwrap();
    let app2 = AppId::new("app2").unwrap();
    let reused_pid = 5000;

    // Register app1 with a PID
    registry.register(
        app1.clone(),
        ProcessHandle {
            pid: reused_pid,
            app_id: app1.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    assert_eq!(registry.get_by_pid(reused_pid), Some(app1.clone()));

    // Unregister app1
    registry.unregister(&app1);
    assert!(registry.get_by_pid(reused_pid).is_none());

    // Register app2 with the same PID (simulating PID reuse)
    registry.register(
        app2.clone(),
        ProcessHandle {
            pid: reused_pid,
            app_id: app2.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        },
    );

    // Should now map to app2
    assert_eq!(registry.get_by_pid(reused_pid), Some(app2.clone()));
    assert!(registry.get(&app1).is_none());
    assert!(registry.get(&app2).is_some());
}
