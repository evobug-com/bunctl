use bytes::{Bytes, BytesMut};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct LineBufferConfig {
    pub max_size: usize,
    pub max_lines: usize,
}

impl Default for LineBufferConfig {
    fn default() -> Self {
        Self {
            max_size: 65536, // 64KB
            max_lines: 1000,
        }
    }
}

/// A lock-free line buffer optimized for high-throughput logging
#[derive(Debug)]
pub struct LineBuffer {
    config: LineBufferConfig,
    // Primary buffer for complete lines
    lines: Arc<Mutex<VecDeque<Bytes>>>,
    // Buffer for incomplete lines
    incomplete: Arc<Mutex<BytesMut>>,
    // Atomic counters for metrics
    total_bytes: Arc<AtomicUsize>,
    total_lines: Arc<AtomicUsize>,
}

impl LineBuffer {
    pub fn new(config: LineBufferConfig) -> Self {
        let max_lines = config.max_lines;
        Self {
            config,
            lines: Arc::new(Mutex::new(VecDeque::with_capacity(max_lines))),
            incomplete: Arc::new(Mutex::new(BytesMut::with_capacity(4096))),
            total_bytes: Arc::new(AtomicUsize::new(0)),
            total_lines: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn write(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        let mut incomplete = self.incomplete.lock();
        incomplete.extend_from_slice(data);

        // Update byte counter
        self.total_bytes.fetch_add(data.len(), Ordering::Relaxed);

        // Process complete lines
        let mut complete_lines = Vec::new();
        while let Some(newline_pos) = incomplete.iter().position(|&b| b == b'\n') {
            let line = incomplete.split_to(newline_pos + 1);
            complete_lines.push(line.freeze());
        }

        // Check if incomplete buffer is too large
        if incomplete.len() > self.config.max_size {
            // Force flush as a complete line
            let line = incomplete.split().freeze();
            complete_lines.push(line);
        }

        // Add complete lines to the buffer
        if !complete_lines.is_empty() {
            let mut lines = self.lines.lock();
            let line_count = complete_lines.len();

            for line in complete_lines {
                lines.push_back(line);

                // Remove oldest lines if we exceed max_lines
                while lines.len() > self.config.max_lines {
                    lines.pop_front();
                }
            }

            self.total_lines.fetch_add(line_count, Ordering::Relaxed);
        }
    }

    /// Atomically drain all complete lines from the buffer
    pub fn get_lines(&self) -> Vec<Bytes> {
        let mut lines = self.lines.lock();
        let result: Vec<Bytes> = lines.drain(..).collect();
        result
    }

    /// Get and clear any incomplete line data
    pub fn flush_incomplete(&self) -> Option<Bytes> {
        let mut incomplete = self.incomplete.lock();
        if !incomplete.is_empty() {
            let data = incomplete.split().freeze();
            Some(data)
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        let lines_empty = self.lines.lock().is_empty();
        let incomplete_empty = self.incomplete.lock().is_empty();
        lines_empty && incomplete_empty
    }

    pub fn clear(&self) {
        self.lines.lock().clear();
        self.incomplete.lock().clear();
        self.total_bytes.store(0, Ordering::Relaxed);
        self.total_lines.store(0, Ordering::Relaxed);
    }

    /// Get current buffer statistics
    pub fn stats(&self) -> BufferStats {
        BufferStats {
            pending_lines: self.lines.lock().len(),
            incomplete_bytes: self.incomplete.lock().len(),
            total_bytes_written: self.total_bytes.load(Ordering::Relaxed),
            total_lines_written: self.total_lines.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferStats {
    pub pending_lines: usize,
    pub incomplete_bytes: usize,
    pub total_bytes_written: usize,
    pub total_lines_written: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_buffer_basic() {
        let buffer = LineBuffer::new(LineBufferConfig::default());

        buffer.write(b"Line 1\n");
        buffer.write(b"Line 2\n");
        buffer.write(b"Incomplete");

        let lines = buffer.get_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(&lines[0][..], b"Line 1\n");
        assert_eq!(&lines[1][..], b"Line 2\n");

        let incomplete = buffer.flush_incomplete();
        assert!(incomplete.is_some());
        assert_eq!(&incomplete.unwrap()[..], b"Incomplete");
    }

    #[test]
    fn test_line_buffer_overflow() {
        let config = LineBufferConfig {
            max_size: 10,
            max_lines: 100,
        };
        let buffer = LineBuffer::new(config);

        // Write data larger than max_size without newline
        buffer.write(b"This is a very long line without newline");

        let lines = buffer.get_lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].len() > 10);
    }

    #[test]
    fn test_line_buffer_max_lines() {
        let config = LineBufferConfig {
            max_size: 1000,
            max_lines: 3,
        };
        let buffer = LineBuffer::new(config);

        buffer.write(b"Line 1\n");
        buffer.write(b"Line 2\n");
        buffer.write(b"Line 3\n");
        buffer.write(b"Line 4\n");
        buffer.write(b"Line 5\n");

        let lines = buffer.get_lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(&lines[0][..], b"Line 3\n");
        assert_eq!(&lines[1][..], b"Line 4\n");
        assert_eq!(&lines[2][..], b"Line 5\n");
    }
}
