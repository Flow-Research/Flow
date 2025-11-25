#[cfg(test)]
mod tests {
    use crate::bootstrap::init::{remove_envs, set_envs};
    use errors::AppError;
    use node::modules::storage::{KvConfig, RocksDbKvStore};
    use node::modules::storage::{KvError, KvStore};
    use serial_test::serial;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_store() -> (RocksDbKvStore, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = KvConfig {
            path: temp_dir.path().to_path_buf(),
            enable_compression: false, // Disable for faster tests
            max_open_files: 100,
            write_buffer_size: 1024 * 1024, // 1MB
        };

        let store = RocksDbKvStore::new(temp_dir.path(), &config).expect("Failed to create store");

        (store, temp_dir)
    }

    #[test]
    fn test_create_store() {
        let (store, _temp) = create_test_store();
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn test_put_and_get() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        let value = b"test_value";

        store.put(key, value).unwrap();
        let retrieved = store.get(key).unwrap();

        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn test_get_nonexistent() {
        let (store, _temp) = create_test_store();

        let result = store.get(b"nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        let value = b"test_value";

        store.put(key, value).unwrap();
        assert!(store.exists(key).unwrap());

        store.delete(key).unwrap();
        assert!(!store.exists(key).unwrap());
    }

    #[test]
    fn test_exists() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        assert!(!store.exists(key).unwrap());

        store.put(key, b"value").unwrap();
        assert!(store.exists(key).unwrap());
    }

    #[test]
    fn test_keys() {
        let (store, _temp) = create_test_store();

        store.put(b"key1", b"value1").unwrap();
        store.put(b"key2", b"value2").unwrap();
        store.put(b"key3", b"value3").unwrap();

        let keys = store.keys().unwrap();
        assert_eq!(keys.len(), 3);

        let key_set: std::collections::HashSet<_> = keys.into_iter().collect();
        assert!(key_set.contains(&b"key1".to_vec()));
        assert!(key_set.contains(&b"key2".to_vec()));
        assert!(key_set.contains(&b"key3".to_vec()));
    }

    #[test]
    fn test_len() {
        let (store, _temp) = create_test_store();

        assert_eq!(store.len().unwrap(), 0);

        store.put(b"key1", b"value1").unwrap();
        store.put(b"key2", b"value2").unwrap();

        let len = store.len().unwrap();
        assert!(len >= 2); // Approximate count
    }

    #[test]
    fn test_flush() {
        let (store, _temp) = create_test_store();

        store.put(b"key", b"value").unwrap();
        store.flush().unwrap();

        // Verify data survives flush
        assert_eq!(store.get(b"key").unwrap(), Some(b"value".to_vec()));
    }

    #[test]
    fn test_overwrite() {
        let (store, _temp) = create_test_store();

        let key = b"test_key";
        store.put(key, b"value1").unwrap();
        store.put(key, b"value2").unwrap();

        assert_eq!(store.get(key).unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();
        let config = KvConfig {
            path: path.clone(),
            enable_compression: false,
            max_open_files: 100,
            write_buffer_size: 1024 * 1024,
        };

        // Create store, write data, and drop it
        {
            let store = RocksDbKvStore::new(&path, &config).unwrap();
            store.put(b"persistent_key", b"persistent_value").unwrap();
        }

        // Reopen and verify data persists
        {
            let store = RocksDbKvStore::new(&path, &config).unwrap();
            assert_eq!(
                store.get(b"persistent_key").unwrap(),
                Some(b"persistent_value".to_vec())
            );
        }
    }

    #[test]
    fn test_large_values() {
        let (store, _temp) = create_test_store();

        let key = b"large_key";
        let large_value = vec![42u8; 1024 * 1024]; // 1MB

        store.put(key, &large_value).unwrap();
        let retrieved = store.get(key).unwrap().unwrap();

        assert_eq!(retrieved.len(), large_value.len());
        assert_eq!(retrieved, large_value);
    }

    #[test]
    fn test_binary_keys_and_values() {
        let (store, _temp) = create_test_store();

        let key = &[0u8, 1, 2, 255, 254, 253];
        let value = &[100u8, 200, 50, 150];

        store.put(key, value).unwrap();
        assert_eq!(store.get(key).unwrap(), Some(value.to_vec()));
    }

    #[test]
    fn test_concurrent_reads() {
        use std::thread;

        let (store, _temp) = create_test_store();
        let store = Arc::new(store);

        // Write some data
        for i in 0u32..100 {
            store
                .put(&i.to_be_bytes(), &format!("value_{}", i).into_bytes())
                .unwrap();
        }

        // Spawn multiple reader threads
        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            let handle = thread::spawn(move || {
                for j in 0u32..100 {
                    let value = store_clone.get(&j.to_be_bytes()).unwrap();
                    assert!(value.is_some());
                }
                i
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    #[serial]
    fn test_config_from_env() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().join("test_kv");

        set_envs(&vec![
            ("KV_STORE_PATH", test_path.to_str().unwrap()),
            ("KV_ENABLE_COMPRESSION", "false"),
            ("KV_MAX_OPEN_FILES", "500"),
        ]);

        let config = KvConfig::from_env();

        assert_eq!(config.path, test_path);
        assert_eq!(config.enable_compression, false);
        assert_eq!(config.max_open_files, 500);

        remove_envs(&[
            "KV_STORE_PATH",
            "KV_ENABLE_COMPRESSION",
            "KV_MAX_OPEN_FILES",
        ]);
    }

    #[test]
    fn test_trait_object_usage() {
        let (store, _temp) = create_test_store();
        let store_arc: Arc<dyn KvStore> = Arc::new(store);

        store_arc.put(b"key", b"value").unwrap();
        assert_eq!(store_arc.get(b"key").unwrap(), Some(b"value".to_vec()));
    }

    #[test]
    fn test_error_conversion() {
        let err = KvError::NotFound;
        let app_err: AppError = err.into();

        match app_err {
            AppError::Storage(msg) => assert!(msg.to_string().contains("not found")),
            _ => panic!("Wrong error variant"),
        }
    }
}
