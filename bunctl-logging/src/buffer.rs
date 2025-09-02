use bytes::{Bytes, BytesMut};
use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Debug, Clone)]
pub struct LineBufferConfig {
    pub max_size: usize,
    pub max_lines: usize,
}

impl Default for LineBufferConfig {
    fn default() -> Self {
        Self {
            max_size: 8192,
            max_lines: 100,
        }
    }
}

#[derive(Debug)]
pub struct LineBuffer {
    config: LineBufferConfig,
    current: Arc<Mutex<BytesMut>>,
    lines: Arc<Mutex<VecDeque<Bytes>>>,
    incomplete: Arc<Mutex<BytesMut>>,
}

impl LineBuffer {
    pub fn new(config: LineBufferConfig) -> Self {
        Self {
            config,
            current: Arc::new(Mutex::new(BytesMut::with_capacity(4096))),
            lines: Arc::new(Mutex::new(VecDeque::new())),
            incomplete: Arc::new(Mutex::new(BytesMut::new())),
        }
    }
    
    pub fn write(&self, data: &[u8]) {
        let mut incomplete = self.incomplete.lock();
        incomplete.extend_from_slice(data);
        
        while let Some(newline_pos) = incomplete.iter().position(|&b| b == b'\n') {
            let line = incomplete.split_to(newline_pos + 1);
            self.add_line(line.freeze());
        }
        
        if incomplete.len() > self.config.max_size {
            let line = incomplete.split().freeze();
            self.add_line(line);
        }
    }
    
    fn add_line(&self, line: Bytes) {
        let mut lines = self.lines.lock();
        lines.push_back(line);
        
        while lines.len() > self.config.max_lines {
            lines.pop_front();
        }
    }
    
    pub fn get_lines(&self) -> Vec<Bytes> {
        let mut lines = self.lines.lock();
        let result: Vec<Bytes> = lines.drain(..).collect();
        result
    }
    
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
        self.lines.lock().is_empty() && self.incomplete.lock().is_empty()
    }
    
    pub fn clear(&self) {
        self.lines.lock().clear();
        self.incomplete.lock().clear();
    }
}