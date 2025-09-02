use bunctl_core::BackoffStrategy;
use std::time::Duration;

#[test]
fn test_exponential_backoff_basic() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_secs(10))
        .with_jitter(0.0) // No jitter for predictable testing
        .with_multiplier(2.0);
    
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(200)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(400)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(800)));
    assert_eq!(backoff.attempt(), 4);
}

#[test]
fn test_backoff_max_delay() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_secs(1))
        .with_max_delay(Duration::from_secs(5))
        .with_jitter(0.0)
        .with_multiplier(10.0);
    
    assert_eq!(backoff.next_delay(), Some(Duration::from_secs(1)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_secs(5))); // Capped at max
    assert_eq!(backoff.next_delay(), Some(Duration::from_secs(5))); // Still capped
}

#[test]
fn test_backoff_max_attempts() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(10))
        .with_max_attempts(3);
    
    assert!(backoff.next_delay().is_some());
    assert!(!backoff.is_exhausted());
    
    assert!(backoff.next_delay().is_some());
    assert!(!backoff.is_exhausted());
    
    assert!(backoff.next_delay().is_some());
    assert!(backoff.is_exhausted());
    
    assert!(backoff.next_delay().is_none());
    assert!(backoff.is_exhausted());
}

#[test]
fn test_backoff_reset() {
    let mut backoff = BackoffStrategy::new()
        .with_max_attempts(2);
    
    backoff.next_delay();
    backoff.next_delay();
    assert!(backoff.is_exhausted());
    
    backoff.reset();
    assert!(!backoff.is_exhausted());
    assert_eq!(backoff.attempt(), 0);
    assert!(backoff.next_delay().is_some());
}

#[test]
fn test_backoff_with_jitter() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1000))
        .with_jitter(0.5)
        .with_multiplier(1.0); // No multiplier to isolate jitter testing
    
    // With 50% jitter on 1000ms, delay should be between 500ms and 1500ms
    for _ in 0..10 {
        let delay = backoff.next_delay().unwrap();
        assert!(delay >= Duration::from_millis(500));
        assert!(delay <= Duration::from_millis(1500));
        backoff.reset();
    }
}

#[test]
fn test_backoff_infinite_attempts() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1))
        .with_max_delay(Duration::from_millis(10))
        .with_jitter(0.0);
    
    // Without max_attempts, should never be exhausted
    for _ in 0..100 {
        assert!(backoff.next_delay().is_some());
        assert!(!backoff.is_exhausted());
    }
}