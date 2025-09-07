use bunctl_core::{App, AppConfig, AppId, AppState};
use std::time::{Duration, Instant};

// ============================================================================
// App State Management Tests
// ============================================================================

#[test]
fn test_app_id_sanitization() {
    // Valid names
    assert_eq!(AppId::new("valid-app").unwrap().as_str(), "valid-app");
    assert_eq!(AppId::new("valid_app").unwrap().as_str(), "valid_app");
    assert_eq!(AppId::new("valid.app").unwrap().as_str(), "valid.app");
    assert_eq!(AppId::new("app123").unwrap().as_str(), "app123");

    // Names with spaces
    assert_eq!(AppId::new("My App").unwrap().as_str(), "my-app");
    assert_eq!(
        AppId::new("  spaced  app  ").unwrap().as_str(),
        "spaced--app"
    );

    // Names with special characters
    assert_eq!(AppId::new("app@host").unwrap().as_str(), "app-host");
    assert_eq!(AppId::new("app!@#$%").unwrap().as_str(), "app"); // Special chars at end get trimmed
    assert_eq!(AppId::new("app/path").unwrap().as_str(), "app-path");

    // Case conversion
    assert_eq!(AppId::new("UPPERCASE").unwrap().as_str(), "uppercase");
    assert_eq!(AppId::new("MixedCase").unwrap().as_str(), "mixedcase");

    // Edge cases
    assert_eq!(AppId::new("---app---").unwrap().as_str(), "app");
    assert_eq!(AppId::new("_app_").unwrap().as_str(), "_app_");
    assert_eq!(AppId::new(".app.").unwrap().as_str(), ".app.");
}

#[test]
fn test_app_id_validation() {
    // Valid IDs
    assert!(AppId::new("app").is_ok());
    assert!(AppId::new("a").is_ok());
    assert!(AppId::new("1").is_ok());
    assert!(AppId::new("app-123").is_ok());

    // Invalid IDs
    assert!(AppId::new("").is_err());
    assert!(AppId::new("   ").is_err());
    assert!(AppId::new("---").is_err());
    assert!(AppId::new("@@@").is_err());
}

#[test]
fn test_app_id_display() {
    let id = AppId::new("test-app").unwrap();
    assert_eq!(format!("{}", id), "test-app");
    assert_eq!(id.to_string(), "test-app");
}

#[test]
fn test_app_id_equality() {
    let id1 = AppId::new("test-app").unwrap();
    let id2 = AppId::new("test-app").unwrap();
    let id3 = AppId::new("Test App").unwrap(); // Should sanitize to same
    let id4 = AppId::new("different-app").unwrap();

    assert_eq!(id1, id2);
    assert_eq!(id1, id3);
    assert_ne!(id1, id4);
}

#[test]
fn test_app_state_transitions() {
    let state = AppState::Stopped;
    assert!(state.is_stopped());
    assert!(!state.is_running());

    let state = AppState::Starting;
    assert!(!state.is_stopped());
    assert!(!state.is_running());

    let state = AppState::Running;
    assert!(!state.is_stopped());
    assert!(state.is_running());

    let state = AppState::Stopping;
    assert!(!state.is_stopped());
    assert!(!state.is_running());

    let state = AppState::Crashed;
    assert!(!state.is_stopped());
    assert!(!state.is_running());

    let state = AppState::Backoff {
        attempt: 3,
        next_retry: Instant::now() + Duration::from_secs(5),
    };
    assert!(!state.is_stopped());
    assert!(!state.is_running());
}

#[test]
fn test_app_creation() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig {
        name: "test-app".to_string(),
        command: "bun".to_string(),
        args: vec!["app.ts".to_string()],
        ..Default::default()
    };

    let app = App::new(id.clone(), config.clone());

    assert_eq!(app.id, id);
    assert_eq!(app.config.read().name, "test-app");
    assert_eq!(app.get_state(), AppState::Stopped);
    assert_eq!(app.get_pid(), None);
    assert_eq!(app.uptime(), None);
    assert_eq!(*app.restart_count.read(), 0);
    assert_eq!(*app.last_exit_code.read(), None);
}

#[test]
fn test_app_state_management() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = App::new(id, config);

    // Test state transitions
    app.set_state(AppState::Starting);
    assert_eq!(app.get_state(), AppState::Starting);

    app.set_state(AppState::Running);
    assert_eq!(app.get_state(), AppState::Running);

    app.set_state(AppState::Stopping);
    assert_eq!(app.get_state(), AppState::Stopping);

    app.set_state(AppState::Stopped);
    assert_eq!(app.get_state(), AppState::Stopped);

    app.set_state(AppState::Crashed);
    assert_eq!(app.get_state(), AppState::Crashed);
}

#[test]
fn test_app_pid_management() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = App::new(id, config);

    assert_eq!(app.get_pid(), None);
    assert_eq!(app.uptime(), None);

    // Set PID
    app.set_pid(Some(12345));
    assert_eq!(app.get_pid(), Some(12345));

    // Should have start time now
    std::thread::sleep(Duration::from_millis(10));
    let uptime = app.uptime().unwrap();
    assert!(uptime >= Duration::from_millis(10));
    assert!(uptime < Duration::from_secs(1));

    // Clear PID
    app.set_pid(None);
    assert_eq!(app.get_pid(), None);
    assert_eq!(app.uptime(), None);
}

#[test]
fn test_app_restart_count() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = App::new(id, config);

    assert_eq!(*app.restart_count.read(), 0);

    app.increment_restart_count();
    assert_eq!(*app.restart_count.read(), 1);

    app.increment_restart_count();
    app.increment_restart_count();
    assert_eq!(*app.restart_count.read(), 3);

    app.reset_restart_count();
    assert_eq!(*app.restart_count.read(), 0);
}

#[test]
fn test_app_exit_code() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = App::new(id, config);

    assert_eq!(*app.last_exit_code.read(), None);

    *app.last_exit_code.write() = Some(0);
    assert_eq!(*app.last_exit_code.read(), Some(0));

    *app.last_exit_code.write() = Some(1);
    assert_eq!(*app.last_exit_code.read(), Some(1));

    *app.last_exit_code.write() = Some(-1);
    assert_eq!(*app.last_exit_code.read(), Some(-1));
}

#[test]
fn test_app_backoff_management() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig {
        name: "test-app".to_string(),
        backoff: bunctl_core::config::BackoffConfig {
            base_delay_ms: 100,
            max_delay_ms: 5000,
            multiplier: 2.0,
            jitter: 0.0,
            max_attempts: Some(3),
            exhausted_action: bunctl_core::config::ExhaustedAction::Stop,
        },
        ..Default::default()
    };

    let app = App::new(id, config.clone());

    // Initially no backoff
    assert!(!app.is_backoff_exhausted());

    // Get or create backoff
    let mut backoff = app.get_or_create_backoff(&config);
    assert_eq!(backoff.attempt(), 0);

    // Use some attempts
    backoff.next_delay();
    backoff.next_delay();
    app.update_backoff(backoff.clone());

    // Should have same backoff on next get
    let backoff2 = app.get_or_create_backoff(&config);
    assert_eq!(backoff2.attempt(), 2);

    // Exhaust backoff
    let mut backoff3 = backoff2.clone();
    backoff3.next_delay();
    app.update_backoff(backoff3);
    assert!(app.is_backoff_exhausted());

    // Reset backoff
    app.reset_backoff();
    assert!(!app.is_backoff_exhausted());

    // Should create new backoff after reset
    let backoff4 = app.get_or_create_backoff(&config);
    assert_eq!(backoff4.attempt(), 0);
}

#[test]
fn test_app_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = Arc::new(App::new(id, config));

    let mut handles = vec![];

    // Spawn multiple threads accessing the app concurrently
    for i in 0..10 {
        let app_clone = app.clone();
        let handle = thread::spawn(move || {
            // Each thread performs various operations
            app_clone.set_state(if i % 2 == 0 {
                AppState::Running
            } else {
                AppState::Stopped
            });

            app_clone.increment_restart_count();

            app_clone.set_pid(Some(1000 + i));

            *app_clone.last_exit_code.write() = Some(i as i32);

            // Read operations
            let _ = app_clone.get_state();
            let _ = app_clone.get_pid();
            let _ = app_clone.uptime();
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final state is consistent
    assert_eq!(*app.restart_count.read(), 10);
    assert!(app.get_pid().is_some());
    assert!(app.last_exit_code.read().is_some());
}

#[test]
fn test_app_config_update() {
    let id = AppId::new("test-app").unwrap();
    let initial_config = AppConfig {
        name: "test-app".to_string(),
        command: "bun".to_string(),
        args: vec!["app.ts".to_string()],
        ..Default::default()
    };

    let app = App::new(id, initial_config);

    // Verify initial config
    {
        let config = app.config.read();
        assert_eq!(config.name, "test-app");
        assert_eq!(config.command, "bun");
        assert_eq!(config.args, vec!["app.ts"]);
    }

    // Update config
    {
        let mut config = app.config.write();
        config.command = "node".to_string();
        config.args = vec!["app.js".to_string()];
        config.auto_start = true;
    }

    // Verify updated config
    {
        let config = app.config.read();
        assert_eq!(config.command, "node");
        assert_eq!(config.args, vec!["app.js"]);
        assert!(config.auto_start);
    }
}

#[test]
fn test_app_state_with_backoff() {
    let next_retry = Instant::now() + Duration::from_secs(30);
    let state = AppState::Backoff {
        attempt: 5,
        next_retry,
    };

    match state {
        AppState::Backoff {
            attempt,
            next_retry: retry,
        } => {
            assert_eq!(attempt, 5);
            assert!(retry > Instant::now());
            assert!(retry <= Instant::now() + Duration::from_secs(31));
        }
        _ => panic!("Expected Backoff state"),
    }
}

#[test]
fn test_app_uptime_accuracy() {
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = App::new(id, config);

    // No uptime without PID
    assert_eq!(app.uptime(), None);

    // Set PID and measure uptime
    app.set_pid(Some(12345));

    // Sleep for a known duration
    std::thread::sleep(Duration::from_millis(100));

    let uptime1 = app.uptime().unwrap();
    assert!(uptime1 >= Duration::from_millis(100));
    assert!(uptime1 < Duration::from_millis(200));

    // Sleep more and check uptime increases
    std::thread::sleep(Duration::from_millis(100));

    let uptime2 = app.uptime().unwrap();
    assert!(uptime2 >= Duration::from_millis(200));
    assert!(uptime2 > uptime1);

    // Clear PID removes uptime
    app.set_pid(None);
    assert_eq!(app.uptime(), None);

    // Setting PID again resets uptime
    app.set_pid(Some(54321));
    std::thread::sleep(Duration::from_millis(50));

    let uptime3 = app.uptime().unwrap();
    assert!(uptime3 < Duration::from_millis(100));
}

#[test]
fn test_app_clone_behavior() {
    // Note: App doesn't implement Clone, but its internal components use Arc
    // This test verifies that Arc-wrapped fields work correctly

    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();
    let app = App::new(id, config);

    // Create multiple references to internal state
    let state_ref1 = app.state.clone();
    let state_ref2 = app.state.clone();

    // Modifications through one reference should be visible through others
    *state_ref1.write() = AppState::Running;
    assert_eq!(*state_ref2.read(), AppState::Running);
    assert_eq!(app.get_state(), AppState::Running);
}

#[test]
fn test_app_memory_safety() {
    // Test that dropping app components doesn't cause issues
    let id = AppId::new("test-app").unwrap();
    let config = AppConfig::default();

    {
        let app = App::new(id.clone(), config.clone());
        app.set_state(AppState::Running);
        app.set_pid(Some(12345));
        app.increment_restart_count();
        // App gets dropped here
    }

    // Create new app with same ID - should work fine
    let app2 = App::new(id, config);
    assert_eq!(app2.get_state(), AppState::Stopped);
    assert_eq!(app2.get_pid(), None);
    assert_eq!(*app2.restart_count.read(), 0);
}
