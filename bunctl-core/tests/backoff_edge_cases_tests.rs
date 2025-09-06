use bunctl_core::BackoffStrategy;
use std::time::Duration;

// ============================================================================
// Backoff Strategy Edge Case Tests
// ============================================================================

#[test]
fn test_backoff_zero_base_delay() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(0))
        .with_jitter(0.0);

    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(0)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(0)));
}

#[test]
fn test_backoff_large_multiplier() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1))
        .with_max_delay(Duration::from_secs(60))
        .with_multiplier(10.0)
        .with_jitter(0.0);

    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(1)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(10)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(1000)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(10000)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_secs(60))); // Capped at max
}

#[test]
fn test_backoff_fractional_multiplier() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1000))
        .with_multiplier(1.5)
        .with_jitter(0.0);

    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(1000)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(1500)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(2250)));
}

#[test]
fn test_backoff_multiplier_exactly_one() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(500))
        .with_multiplier(1.0)
        .with_jitter(0.0);

    // With multiplier = 1.0, delay should remain constant
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(500)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(500)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(500)));
}

#[test]
fn test_backoff_negative_multiplier_clamped() {
    // Negative multiplier should be clamped to 1.0
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(100))
        .with_multiplier(-2.0)
        .with_jitter(0.0);

    // Should be clamped to 1.0, so constant delay
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
}

#[test]
fn test_backoff_max_jitter() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1000))
        .with_jitter(1.0) // 100% jitter
        .with_multiplier(1.0);

    // With 100% jitter, delay should be between 0 and 2000ms
    for _ in 0..20 {
        let delay = backoff.next_delay().unwrap();
        assert!(delay <= Duration::from_millis(2000));
        assert!(delay >= Duration::from_millis(0));
        backoff.reset();
    }
}

#[test]
fn test_backoff_negative_jitter_clamped() {
    // Negative jitter should be clamped to 0.0
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(100))
        .with_jitter(-0.5)
        .with_multiplier(1.0);

    // Should be clamped to 0.0, so no jitter
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
}

#[test]
fn test_backoff_excessive_jitter_clamped() {
    // Jitter > 1.0 should be clamped to 1.0
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1000))
        .with_jitter(2.0)
        .with_multiplier(1.0);

    // Should be clamped to 1.0 (100% jitter)
    for _ in 0..10 {
        let delay = backoff.next_delay().unwrap();
        assert!(delay <= Duration::from_millis(2000));
        assert!(delay >= Duration::from_millis(0));
        backoff.reset();
    }
}

#[test]
fn test_backoff_single_attempt() {
    let mut backoff = BackoffStrategy::new().with_max_attempts(1).with_jitter(0.0);

    assert!(!backoff.is_exhausted());
    assert_eq!(backoff.attempt(), 0);

    assert!(backoff.next_delay().is_some());
    assert!(backoff.is_exhausted());
    assert_eq!(backoff.attempt(), 1);

    assert!(backoff.next_delay().is_none());
}

#[test]
fn test_backoff_zero_attempts() {
    let mut backoff = BackoffStrategy::new().with_max_attempts(0).with_jitter(0.0);

    assert!(backoff.is_exhausted());
    assert!(backoff.next_delay().is_none());
}

#[test]
fn test_backoff_max_delay_smaller_than_base() {
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_secs(10))
        .with_max_delay(Duration::from_secs(5))
        .with_jitter(0.0);

    // Even the first delay should be capped at max_delay
    assert_eq!(backoff.next_delay(), Some(Duration::from_secs(5)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_secs(5)));
}

#[test]
fn test_backoff_overflow_protection() {
    // Test with values that could cause overflow
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(u64::MAX / 2))
        .with_max_delay(Duration::from_millis(u64::MAX))
        .with_multiplier(1000.0)
        .with_jitter(0.0);

    // Should not panic, and should be capped at max_delay
    let delay = backoff.next_delay().unwrap();
    assert!(delay <= Duration::from_millis(u64::MAX));
}

#[test]
fn test_backoff_reset_after_exhaustion() {
    let mut backoff = BackoffStrategy::new()
        .with_max_attempts(2)
        .with_base_delay(Duration::from_millis(100))
        .with_multiplier(2.0)
        .with_jitter(0.0);

    // Exhaust attempts
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(200)));
    assert!(backoff.is_exhausted());
    assert!(backoff.next_delay().is_none());

    // Reset and verify it works again
    backoff.reset();
    assert!(!backoff.is_exhausted());
    assert_eq!(backoff.attempt(), 0);
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
}

#[test]
fn test_backoff_partial_reset() {
    let mut backoff = BackoffStrategy::new()
        .with_max_attempts(5)
        .with_base_delay(Duration::from_millis(50))
        .with_multiplier(2.0)
        .with_jitter(0.0);

    // Use some attempts
    backoff.next_delay();
    backoff.next_delay();
    backoff.next_delay();
    assert_eq!(backoff.attempt(), 3);
    assert!(!backoff.is_exhausted());

    // Reset and verify
    backoff.reset();
    assert_eq!(backoff.attempt(), 0);
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(50)));
}

#[test]
fn test_backoff_state_consistency() {
    let mut backoff = BackoffStrategy::new()
        .with_max_attempts(3)
        .with_base_delay(Duration::from_millis(100))
        .with_jitter(0.0);

    // Verify state is consistent throughout usage
    assert_eq!(backoff.attempt(), 0);
    assert!(!backoff.is_exhausted());

    backoff.next_delay();
    assert_eq!(backoff.attempt(), 1);
    assert!(!backoff.is_exhausted());

    backoff.next_delay();
    assert_eq!(backoff.attempt(), 2);
    assert!(!backoff.is_exhausted());

    backoff.next_delay();
    assert_eq!(backoff.attempt(), 3);
    assert!(backoff.is_exhausted());

    // Multiple calls after exhaustion shouldn't change state
    backoff.next_delay();
    assert_eq!(backoff.attempt(), 3);
    assert!(backoff.is_exhausted());
}

#[test]
fn test_backoff_jitter_distribution() {
    // Test that jitter produces reasonable distribution
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1000))
        .with_jitter(0.5)
        .with_multiplier(1.0);

    let mut delays = Vec::new();
    for _ in 0..100 {
        let delay = backoff.next_delay().unwrap();
        delays.push(delay.as_millis());
        backoff.reset();
    }

    // With 50% jitter on 1000ms, delays should be between 500ms and 1500ms
    let min = *delays.iter().min().unwrap();
    let max = *delays.iter().max().unwrap();

    assert!(min >= 500, "Min delay {} should be >= 500", min);
    assert!(max <= 1500, "Max delay {} should be <= 1500", max);

    // Check that we have some variation (not all the same)
    let unique_delays: std::collections::HashSet<_> = delays.iter().collect();
    assert!(
        unique_delays.len() > 10,
        "Should have variety in delays with jitter"
    );
}

#[test]
fn test_backoff_clone_independence() {
    let mut backoff1 = BackoffStrategy::new()
        .with_max_attempts(3)
        .with_base_delay(Duration::from_millis(100))
        .with_jitter(0.0);

    // Use one attempt
    backoff1.next_delay();
    assert_eq!(backoff1.attempt(), 1);

    // Clone should have independent state
    let mut backoff2 = backoff1.clone();
    assert_eq!(backoff2.attempt(), 1);

    // Advancing one shouldn't affect the other
    backoff1.next_delay();
    assert_eq!(backoff1.attempt(), 2);
    assert_eq!(backoff2.attempt(), 1);

    backoff2.next_delay();
    backoff2.next_delay();
    assert_eq!(backoff1.attempt(), 2);
    assert_eq!(backoff2.attempt(), 3);
    assert!(backoff2.is_exhausted());
    assert!(!backoff1.is_exhausted());
}

#[test]
fn test_backoff_precision_limits() {
    // Test with very small delays (millisecond precision is the limit)
    // The backoff strategy internally works with milliseconds
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1))
        .with_max_delay(Duration::from_millis(100))
        .with_multiplier(10.0)
        .with_jitter(0.0);

    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(1)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(10)));
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100))); // Hits max
    assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100))); // Still at max
}

#[test]
fn test_backoff_deterministic_with_zero_jitter() {
    // With zero jitter, backoff should be completely deterministic
    let mut backoff1 = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(100))
        .with_multiplier(2.0)
        .with_jitter(0.0)
        .with_max_attempts(5);

    let mut backoff2 = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(100))
        .with_multiplier(2.0)
        .with_jitter(0.0)
        .with_max_attempts(5);

    for _ in 0..5 {
        assert_eq!(backoff1.next_delay(), backoff2.next_delay());
    }
}

#[test]
fn test_backoff_high_attempt_count() {
    // Test with many attempts (but not infinite)
    let mut backoff = BackoffStrategy::new()
        .with_base_delay(Duration::from_millis(1))
        .with_max_delay(Duration::from_secs(1))
        .with_multiplier(1.1)
        .with_jitter(0.0)
        .with_max_attempts(1000);

    // Run through many attempts
    for i in 0..1000 {
        assert!(
            !backoff.is_exhausted(),
            "Should not be exhausted at attempt {}",
            i
        );
        let delay = backoff.next_delay();
        assert!(delay.is_some(), "Should have delay at attempt {}", i);
        assert!(
            delay.unwrap() <= Duration::from_secs(1),
            "Delay should be capped at max"
        );
    }

    assert!(backoff.is_exhausted());
    assert!(backoff.next_delay().is_none());
}
