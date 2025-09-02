use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BackoffStrategy {
    base_delay: Duration,
    max_delay: Duration,
    jitter_factor: f64,
    multiplier: f64,
    attempt: u32,
    max_attempts: Option<u32>,
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            jitter_factor: 0.3,
            multiplier: 2.0,
            attempt: 0,
            max_attempts: None,
        }
    }
}

impl BackoffStrategy {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_base_delay(mut self, delay: Duration) -> Self {
        self.base_delay = delay;
        self
    }
    
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }
    
    pub fn with_jitter(mut self, factor: f64) -> Self {
        self.jitter_factor = factor.clamp(0.0, 1.0);
        self
    }
    
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier.max(1.0);
        self
    }
    
    pub fn with_max_attempts(mut self, max: u32) -> Self {
        self.max_attempts = Some(max);
        self
    }
    
    pub fn next_delay(&mut self) -> Option<Duration> {
        if let Some(max) = self.max_attempts {
            if self.attempt >= max {
                return None;
            }
        }
        
        let base_ms = self.base_delay.as_millis() as f64;
        let delay_ms = base_ms * self.multiplier.powi(self.attempt as i32);
        let delay_ms = delay_ms.min(self.max_delay.as_millis() as f64);
        
        let jitter_range = delay_ms * self.jitter_factor;
        use rand::Rng;
        let mut rng = rand::rng();
        let jitter = rng.gen_range(-jitter_range..=jitter_range);
        let final_delay_ms = (delay_ms + jitter).max(0.0) as u64;
        
        self.attempt += 1;
        Some(Duration::from_millis(final_delay_ms))
    }
    
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
    
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
    
    pub fn is_exhausted(&self) -> bool {
        if let Some(max) = self.max_attempts {
            self.attempt >= max
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_exponential_backoff() {
        let mut backoff = BackoffStrategy::new()
            .with_base_delay(Duration::from_millis(100))
            .with_max_delay(Duration::from_secs(10))
            .with_jitter(0.0)
            .with_multiplier(2.0);
        
        assert_eq!(backoff.next_delay(), Some(Duration::from_millis(100)));
        assert_eq!(backoff.next_delay(), Some(Duration::from_millis(200)));
        assert_eq!(backoff.next_delay(), Some(Duration::from_millis(400)));
        assert_eq!(backoff.next_delay(), Some(Duration::from_millis(800)));
    }
    
    #[test]
    fn test_max_attempts() {
        let mut backoff = BackoffStrategy::new()
            .with_max_attempts(3);
        
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_none());
        assert!(backoff.is_exhausted());
    }
    
    #[test]
    fn test_reset() {
        let mut backoff = BackoffStrategy::new()
            .with_max_attempts(2);
        
        backoff.next_delay();
        backoff.next_delay();
        assert!(backoff.is_exhausted());
        
        backoff.reset();
        assert!(!backoff.is_exhausted());
        assert!(backoff.next_delay().is_some());
    }
}