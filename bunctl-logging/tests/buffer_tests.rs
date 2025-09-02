use bunctl_logging::{LineBuffer, LineBufferConfig};
use bytes::Bytes;

#[test]
fn test_line_buffer_basic() {
    let config = LineBufferConfig {
        max_size: 1024,
        max_lines: 10,
    };

    let buffer = LineBuffer::new(config);

    // Write a complete line
    buffer.write(b"Hello, World!\n");

    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], Bytes::from("Hello, World!\n"));
}

#[test]
fn test_line_buffer_multiple_lines() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    buffer.write(b"Line 1\nLine 2\nLine 3\n");

    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], Bytes::from("Line 1\n"));
    assert_eq!(lines[1], Bytes::from("Line 2\n"));
    assert_eq!(lines[2], Bytes::from("Line 3\n"));
}

#[test]
fn test_line_buffer_incomplete_line() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    buffer.write(b"Incomplete line");

    // No complete lines yet
    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 0);

    // Complete the line
    buffer.write(b" is now complete\n");

    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], Bytes::from("Incomplete line is now complete\n"));
}

#[test]
fn test_line_buffer_max_lines() {
    let config = LineBufferConfig {
        max_size: 8192,
        max_lines: 3,
    };

    let buffer = LineBuffer::new(config);

    buffer.write(b"Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n");

    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 3);
    // Should keep only the last 3 lines
    assert_eq!(lines[0], Bytes::from("Line 3\n"));
    assert_eq!(lines[1], Bytes::from("Line 4\n"));
    assert_eq!(lines[2], Bytes::from("Line 5\n"));
}

#[test]
fn test_line_buffer_max_size() {
    let config = LineBufferConfig {
        max_size: 10, // Very small buffer
        max_lines: 100,
    };

    let buffer = LineBuffer::new(config);

    // Write more than max_size without newline
    buffer.write(b"This is a very long line that exceeds the buffer size");

    // Should force flush incomplete line
    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].len() > 0);
}

#[test]
fn test_line_buffer_flush_incomplete() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    buffer.write(b"No newline");

    assert!(buffer.get_lines().is_empty());

    let incomplete = buffer.flush_incomplete();
    assert!(incomplete.is_some());
    assert_eq!(incomplete.unwrap(), Bytes::from("No newline"));

    // After flush, should be empty
    let incomplete = buffer.flush_incomplete();
    assert!(incomplete.is_none());
}

#[test]
fn test_line_buffer_is_empty() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    assert!(buffer.is_empty());

    buffer.write(b"Some data");
    assert!(!buffer.is_empty());

    buffer.flush_incomplete();
    assert!(buffer.is_empty());
}

#[test]
fn test_line_buffer_clear() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    buffer.write(b"Line 1\nLine 2\nIncomplete");

    assert!(!buffer.is_empty());

    buffer.clear();

    assert!(buffer.is_empty());
    assert!(buffer.get_lines().is_empty());
    assert!(buffer.flush_incomplete().is_none());
}

#[test]
fn test_line_buffer_mixed_data() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    buffer.write(b"Start");
    buffer.write(b" of line\n");
    buffer.write(b"Complete line\n");
    buffer.write(b"Partial");

    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], Bytes::from("Start of line\n"));
    assert_eq!(lines[1], Bytes::from("Complete line\n"));

    let incomplete = buffer.flush_incomplete();
    assert_eq!(incomplete, Some(Bytes::from("Partial")));
}

#[test]
fn test_line_buffer_empty_lines() {
    let config = LineBufferConfig::default();
    let buffer = LineBuffer::new(config);

    buffer.write(b"\n\n\n");

    let lines = buffer.get_lines();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], Bytes::from("\n"));
    assert_eq!(lines[1], Bytes::from("\n"));
    assert_eq!(lines[2], Bytes::from("\n"));
}
