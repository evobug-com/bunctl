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

    registry.register(app_id.clone(), handle.clone());

    let retrieved = registry.get(&app_id);
    assert!(retrieved.is_some());
    let retrieved_handle = retrieved.unwrap();
    assert_eq!(retrieved_handle.pid, 1234);
    assert_eq!(retrieved_handle.app_id, app_id);
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
    assert_eq!(found_app.unwrap(), app_id);

    // Non-existent PID
    assert!(registry.get_by_pid(9999).is_none());
}

#[test]
fn test_registry_list() {
    let registry = ProcessRegistry::new();

    let app1 = AppId::new("app1").unwrap();
    let app2 = AppId::new("app2").unwrap();

    let handle1 = ProcessHandle {
        pid: 1111,
        app_id: app1.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };

    let handle2 = ProcessHandle {
        pid: 2222,
        app_id: app2.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };

    registry.register(app1.clone(), handle1);
    registry.register(app2.clone(), handle2);

    let list = registry.list();
    assert_eq!(list.len(), 2);

    // Check both entries exist
    assert!(list.iter().any(|(id, pid)| id == &app1 && *pid == 1111));
    assert!(list.iter().any(|(id, pid)| id == &app2 && *pid == 2222));
}

#[test]
fn test_registry_count() {
    let registry = ProcessRegistry::new();
    assert_eq!(registry.count(), 0);

    let app1 = AppId::new("app1").unwrap();
    let handle1 = ProcessHandle {
        pid: 1111,
        app_id: app1.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };
    registry.register(app1.clone(), handle1);
    assert_eq!(registry.count(), 1);

    let app2 = AppId::new("app2").unwrap();
    let handle2 = ProcessHandle {
        pid: 2222,
        app_id: app2.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };
    registry.register(app2.clone(), handle2);
    assert_eq!(registry.count(), 2);

    registry.unregister(&app1);
    assert_eq!(registry.count(), 1);
}

#[test]
fn test_registry_clear() {
    let registry = ProcessRegistry::new();

    // Add multiple processes
    for i in 0..5 {
        let app_id = AppId::new(format!("app{}", i)).unwrap();
        let handle = ProcessHandle {
            pid: 1000 + i,
            app_id: app_id.clone(),
            inner: None,
            stdout: None,
            stderr: None,
        };
        registry.register(app_id, handle);
    }

    assert_eq!(registry.count(), 5);

    registry.clear();
    assert_eq!(registry.count(), 0);
    assert!(registry.list().is_empty());
}

#[test]
fn test_registry_replace_handle() {
    let registry = ProcessRegistry::new();
    let app_id = AppId::new("test-app").unwrap();

    // Register first handle
    let handle1 = ProcessHandle {
        pid: 1234,
        app_id: app_id.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };
    registry.register(app_id.clone(), handle1);

    // Replace with new handle
    let handle2 = ProcessHandle {
        pid: 5678,
        app_id: app_id.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };
    registry.register(app_id.clone(), handle2);

    // Check that old PID mapping is removed and new one exists
    assert!(registry.get_by_pid(1234).is_none());
    assert!(registry.get_by_pid(5678).is_some());

    let retrieved = registry.get(&app_id).unwrap();
    assert_eq!(retrieved.pid, 5678);
}

#[test]
fn test_registry_concurrent_access() {
    let registry = Arc::new(ProcessRegistry::new());
    let counter = Arc::new(AtomicU32::new(0));

    let mut threads = vec![];

    // Spawn multiple threads that register processes
    for i in 0..10 {
        let reg = registry.clone();
        let cnt = counter.clone();

        let handle = thread::spawn(move || {
            for j in 0..100 {
                let app_id = AppId::new(format!("app-{}-{}", i, j)).unwrap();
                let pid = cnt.fetch_add(1, Ordering::SeqCst);

                let handle = ProcessHandle {
                    pid,
                    app_id: app_id.clone(),
                    inner: None,
                    stdout: None,
                    stderr: None,
                };

                reg.register(app_id.clone(), handle);

                // Verify we can retrieve it
                assert!(reg.get(&app_id).is_some());

                // Sometimes unregister it
                if j % 3 == 0 {
                    reg.unregister(&app_id);
                }
            }
        });

        threads.push(handle);
    }

    // Wait for all threads
    for thread in threads {
        thread.join().unwrap();
    }

    // Verify the registry is in a consistent state
    let list = registry.list();
    for (app_id, pid) in list {
        // Each entry should be retrievable
        let handle = registry.get(&app_id);
        assert!(handle.is_some());
        assert_eq!(handle.unwrap().pid, pid);

        // PID lookup should work
        let found_app = registry.get_by_pid(pid);
        assert!(found_app.is_some());
        assert_eq!(found_app.unwrap(), app_id);
    }
}

#[test]
fn test_registry_clone() {
    let registry1 = ProcessRegistry::new();
    let registry2 = registry1.clone();

    let app_id = AppId::new("test-app").unwrap();
    let handle = ProcessHandle {
        pid: 1234,
        app_id: app_id.clone(),
        inner: None,
        stdout: None,
        stderr: None,
    };

    // Register in registry1
    registry1.register(app_id.clone(), handle);

    // Should be visible in registry2 (they share the same Arc)
    assert!(registry2.get(&app_id).is_some());
    assert_eq!(registry2.count(), 1);

    // Unregister from registry2
    registry2.unregister(&app_id);

    // Should be gone from registry1 too
    assert!(registry1.get(&app_id).is_none());
    assert_eq!(registry1.count(), 0);
}
