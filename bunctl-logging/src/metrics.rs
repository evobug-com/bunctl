use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Debug)]
pub struct LogMetrics {
    pub bytes_written: AtomicU64,
    pub lines_written: AtomicU64,
    pub write_errors: AtomicU64,
    pub flush_count: AtomicU64,
    pub rotation_count: AtomicU64,
    pub buffer_overflows: AtomicU64,
    pub current_buffer_size: AtomicUsize,
    pub total_write_duration_us: AtomicU64,
    pub total_flush_duration_us: AtomicU64,
    pub dropped_messages: AtomicU64,
    start_time: Instant,
}

impl Default for LogMetrics {
    fn default() -> Self {
        Self {
            bytes_written: AtomicU64::new(0),
            lines_written: AtomicU64::new(0),
            write_errors: AtomicU64::new(0),
            flush_count: AtomicU64::new(0),
            rotation_count: AtomicU64::new(0),
            buffer_overflows: AtomicU64::new(0),
            current_buffer_size: AtomicUsize::new(0),
            total_write_duration_us: AtomicU64::new(0),
            total_flush_duration_us: AtomicU64::new(0),
            dropped_messages: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }
}

impl LogMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_write(&self, bytes: u64) {
        self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
        self.lines_written.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_write_error(&self) {
        self.write_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_flush(&self, duration_us: u64) {
        self.flush_count.fetch_add(1, Ordering::Relaxed);
        self.total_flush_duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
    }

    pub fn record_rotation(&self) {
        self.rotation_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_buffer_overflow(&self) {
        self.buffer_overflows.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_dropped_message(&self) {
        self.dropped_messages.fetch_add(1, Ordering::Relaxed);
    }

    pub fn update_buffer_size(&self, size: usize) {
        self.current_buffer_size.store(size, Ordering::Relaxed);
    }

    pub fn record_write_duration(&self, duration_us: u64) {
        self.total_write_duration_us
            .fetch_add(duration_us, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            lines_written: self.lines_written.load(Ordering::Relaxed),
            write_errors: self.write_errors.load(Ordering::Relaxed),
            flush_count: self.flush_count.load(Ordering::Relaxed),
            rotation_count: self.rotation_count.load(Ordering::Relaxed),
            buffer_overflows: self.buffer_overflows.load(Ordering::Relaxed),
            current_buffer_size: self.current_buffer_size.load(Ordering::Relaxed),
            avg_write_latency_us: {
                let total = self.total_write_duration_us.load(Ordering::Relaxed);
                let count = self.lines_written.load(Ordering::Relaxed);
                if count > 0 { total / count } else { 0 }
            },
            avg_flush_latency_us: {
                let total = self.total_flush_duration_us.load(Ordering::Relaxed);
                let count = self.flush_count.load(Ordering::Relaxed);
                if count > 0 { total / count } else { 0 }
            },
            dropped_messages: self.dropped_messages.load(Ordering::Relaxed),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub bytes_written: u64,
    pub lines_written: u64,
    pub write_errors: u64,
    pub flush_count: u64,
    pub rotation_count: u64,
    pub buffer_overflows: u64,
    pub current_buffer_size: usize,
    pub avg_write_latency_us: u64,
    pub avg_flush_latency_us: u64,
    pub dropped_messages: u64,
    pub uptime_seconds: u64,
}
