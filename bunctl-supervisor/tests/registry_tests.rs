use bunctl_core::{AppId, ProcessHandle};
use bunctl_supervisor::ProcessRegistry;

#[test]
fn test_registry_register_and_get() {
    let registry = ProcessRegistry::new();
    let app_id = AppId::new("test-app").unwrap();

    // Create a mock process handle
    let handle = ProcessHandle {
        pid: 1234,
        app_id: app_id.clone(),
        inner: None,
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
        },
    );

    registry.register(
        app2.clone(),
        ProcessHandle {
            pid: 200,
            app_id: app2.clone(),
            inner: None,
        },
    );

    registry.register(
        app3.clone(),
        ProcessHandle {
            pid: 300,
            app_id: app3.clone(),
            inner: None,
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
            let app_id = AppId::new(&format!("app{}", i)).unwrap();
            registry_clone.register(
                app_id.clone(),
                ProcessHandle {
                    pid: i as u32,
                    app_id,
                    inner: None,
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
        },
    );

    // Should have overwritten the first one
    assert_eq!(registry.get(&app_id).unwrap().pid, 2222);
    assert_eq!(registry.count(), 1);

    // Old PID should not be found
    assert!(registry.get_by_pid(1111).is_none());
    assert!(registry.get_by_pid(2222).is_some());
}
