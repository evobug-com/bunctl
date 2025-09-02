use bunctl_logging::{LogRotation, RotationConfig, RotationStrategy};
use std::fs;
use tempfile::TempDir;
use tokio;

#[tokio::test]
async fn test_rotation_strategy_size() {
    let config = RotationConfig {
        strategy: RotationStrategy::Size(1024),
        max_files: 5,
        compression: false,
    };

    let rotation = LogRotation::new(config);

    assert!(!rotation.should_rotate(512));
    assert!(!rotation.should_rotate(1023));
    assert!(rotation.should_rotate(1024));
    assert!(rotation.should_rotate(2048));
}

#[tokio::test]
async fn test_rotation_strategy_never() {
    let config = RotationConfig {
        strategy: RotationStrategy::Never,
        max_files: 5,
        compression: false,
    };

    let rotation = LogRotation::new(config);

    assert!(!rotation.should_rotate(0));
    assert!(!rotation.should_rotate(1024 * 1024 * 1024)); // Even with 1GB
}

#[tokio::test]
async fn test_rotation_basic() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("test.log");

    // Create a log file
    fs::write(&log_path, "test content").unwrap();

    let config = RotationConfig {
        strategy: RotationStrategy::Size(10),
        max_files: 3,
        compression: false,
    };

    let mut rotation = LogRotation::new(config);

    // Perform rotation
    let result = rotation.rotate(&log_path).await;
    assert!(result.is_ok(), "Rotation failed: {:?}", result);

    // Give filesystem time to complete operation on Windows
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // List all files for debugging
    let all_files: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    // Should have exactly one file after rotation (the rotated file)
    // Original test.log should be gone, replaced by test.TIMESTAMP.log
    assert!(
        !all_files.is_empty(),
        "Should have at least one file after rotation. Files: {:?}",
        all_files
    );

    // Check if original file exists
    let original_exists = all_files.iter().any(|f| f == "test.log");
    let rotated_exists = all_files
        .iter()
        .any(|f| f.starts_with("test.") && f.ends_with(".log") && f != "test.log");

    if !rotated_exists && original_exists {
        // Rotation failed - original file still exists
        panic!(
            "Rotation appears to have failed. Files present: {:?}",
            all_files
        );
    }

    assert!(
        rotated_exists || !original_exists,
        "Either rotated file should exist or original should be gone. Files: {:?}",
        all_files
    );

    // Rotated file should exist
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    assert_eq!(entries.len(), 1);
    let rotated_path = entries[0].path();
    assert!(rotated_path.to_string_lossy().contains("test."));
    assert!(rotated_path.to_string_lossy().ends_with(".log"));
}

#[tokio::test]
async fn test_rotation_max_files_cleanup() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("app.log");

    let config = RotationConfig {
        strategy: RotationStrategy::Size(10),
        max_files: 2,
        compression: false,
    };

    let mut rotation = LogRotation::new(config);

    // Create and rotate multiple times
    for i in 0..5 {
        fs::write(&log_path, format!("content {}", i)).unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        rotation.rotate(&log_path).await.unwrap();
    }

    // Should only have max_files rotated logs
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().to_string_lossy().contains("app."))
        .collect();

    assert!(
        entries.len() <= 2,
        "Found {} files, expected <= 2",
        entries.len()
    );
}

#[tokio::test]
async fn test_rotation_with_compression() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("compress.log");

    // Create a log file with some content
    let content = "This is test content for compression\n".repeat(100);
    fs::write(&log_path, &content).unwrap();

    let config = RotationConfig {
        strategy: RotationStrategy::Size(10),
        max_files: 5,
        compression: true,
    };

    let mut rotation = LogRotation::new(config);
    rotation.rotate(&log_path).await.unwrap();

    // Original file should not exist
    assert!(!log_path.exists());

    // Compressed file should exist
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    assert_eq!(entries.len(), 1);
    let compressed_path = entries[0].path();
    assert!(compressed_path.to_string_lossy().ends_with(".log.gz"));

    // Verify it's actually compressed
    let compressed_size = fs::metadata(&compressed_path).unwrap().len();
    assert!(compressed_size < content.len() as u64);
}

#[tokio::test]
async fn test_rotation_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("nonexistent.log");

    let config = RotationConfig::default();
    let mut rotation = LogRotation::new(config);

    // Should not error on nonexistent file
    let result = rotation.rotate(&log_path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_rotation_update_size() {
    let config = RotationConfig {
        strategy: RotationStrategy::Size(1024),
        max_files: 5,
        compression: false,
    };

    let mut rotation = LogRotation::new(config);

    assert!(!rotation.should_rotate(0));

    rotation.update_size(512);
    assert!(!rotation.should_rotate(512));

    rotation.update_size(512);
    assert!(rotation.should_rotate(1024));
}

#[tokio::test]
async fn test_rotation_reset() {
    let config = RotationConfig {
        strategy: RotationStrategy::Size(1024),
        max_files: 5,
        compression: false,
    };

    let mut rotation = LogRotation::new(config);

    rotation.update_size(1024);
    assert!(rotation.should_rotate(1024));

    rotation.reset();
    assert!(!rotation.should_rotate(0));
}
