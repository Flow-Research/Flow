#[cfg(test)]
mod tests {
    use node::modules::storage::content::ChunkingConfig;
    use serial_test::serial;

    use crate::bootstrap::init::{remove_env, remove_envs, set_env, set_envs};

    // ============================================
    // Functional / Happy Path Tests
    // ============================================

    #[test]
    fn test_config_new_valid() {
        let config = ChunkingConfig::new(1024, 4096, 16384);
        assert_eq!(config.min_size, 1024);
        assert_eq!(config.target_size, 4096);
        assert_eq!(config.max_size, 16384);
    }

    #[test]
    fn test_config_default() {
        let config = ChunkingConfig::default();
        assert_eq!(config.min_size, 64 * 1024);
        assert_eq!(config.target_size, 256 * 1024);
        assert_eq!(config.max_size, 1024 * 1024);
    }

    #[test]
    fn test_config_small() {
        let config = ChunkingConfig::small();
        assert_eq!(config.min_size, 1024);
        assert_eq!(config.target_size, 4 * 1024);
        assert_eq!(config.max_size, 16 * 1024);
    }

    #[test]
    fn test_config_large() {
        let config = ChunkingConfig::large();
        assert_eq!(config.min_size, 256 * 1024);
        assert_eq!(config.target_size, 1024 * 1024);
        assert_eq!(config.max_size, 4 * 1024 * 1024);
    }

    #[test]
    fn test_config_validate_valid() {
        let config = ChunkingConfig::new(100, 500, 1000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_target_size_accessor() {
        let config = ChunkingConfig::new(100, 500, 1000);
        assert_eq!(config.target_size, 500);
    }

    // ============================================
    // Edge Cases & Boundary Conditions
    // ============================================

    #[test]
    fn test_config_all_equal() {
        let config = ChunkingConfig::new(1000, 1000, 1000);
        assert!(config.validate().is_ok());
        assert_eq!(config.min_size, config.target_size);
        assert_eq!(config.target_size, config.max_size);
    }

    #[test]
    fn test_config_min_equals_target() {
        let config = ChunkingConfig::new(500, 500, 1000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_target_equals_max() {
        let config = ChunkingConfig::new(100, 1000, 1000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_copy_trait() {
        let config1 = ChunkingConfig::default();
        let config2 = config1; // Copy
        assert_eq!(config1.min_size, config2.min_size);
        assert_eq!(config1.target_size, config2.target_size);
        assert_eq!(config1.max_size, config2.max_size);
    }

    #[test]
    fn test_config_clone_trait() {
        let config1 = ChunkingConfig::default();
        let config2 = config1.clone();
        assert_eq!(config1, config2);
    }

    #[test]
    fn test_config_debug_trait() {
        let config = ChunkingConfig::new(100, 200, 300);
        let debug = format!("{:?}", config);
        assert!(debug.contains("100"));
        assert!(debug.contains("200"));
        assert!(debug.contains("300"));
    }

    // ============================================
    // Error Handling & Negative Tests
    // ============================================

    #[test]
    #[should_panic(expected = "min_size")]
    fn test_config_panics_min_gt_target() {
        ChunkingConfig::new(5000, 1000, 10000);
    }

    #[test]
    #[should_panic(expected = "target_size")]
    fn test_config_panics_target_gt_max() {
        ChunkingConfig::new(1000, 10000, 5000);
    }

    #[test]
    fn test_config_validate_rejects_zero_min() {
        let config = ChunkingConfig {
            min_size: 0,
            target_size: 100,
            max_size: 200,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("greater than 0"));
    }

    #[test]
    fn test_config_validate_rejects_min_gt_target() {
        let config = ChunkingConfig {
            min_size: 1000,
            target_size: 500,
            max_size: 2000,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min_size"));
    }

    #[test]
    fn test_config_validate_rejects_target_gt_max() {
        let config = ChunkingConfig {
            min_size: 100,
            target_size: 2000,
            max_size: 1000,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("target_size"));
    }

    // ============================================
    // Environment Variable Tests (Serial)
    // ============================================

    #[test]
    #[serial]
    fn test_config_from_env_defaults() {
        remove_envs(&[
            "CHUNKING_MIN_SIZE",
            "CHUNKING_TARGET_SIZE",
            "CHUNKING_MAX_SIZE",
        ]);

        let config = ChunkingConfig::from_env();
        assert_eq!(config.min_size, 64 * 1024);
        assert_eq!(config.target_size, 256 * 1024);
        assert_eq!(config.max_size, 1024 * 1024);
    }

    #[test]
    #[serial]
    fn test_config_from_env_reads_all_vars() {
        set_envs(&vec![
            ("CHUNKING_MIN_SIZE", "2048"),
            ("CHUNKING_TARGET_SIZE", "8192"),
            ("CHUNKING_MAX_SIZE", "32768"),
        ]);

        let config = ChunkingConfig::from_env();

        remove_envs(&[
            "CHUNKING_MIN_SIZE",
            "CHUNKING_TARGET_SIZE",
            "CHUNKING_MAX_SIZE",
        ]);

        assert_eq!(config.min_size, 2048);
        assert_eq!(config.target_size, 8192);
        assert_eq!(config.max_size, 32768);
    }

    #[test]
    #[serial]
    fn test_config_from_env_clamps_invalid_hierarchy() {
        set_envs(&vec![
            ("CHUNKING_MIN_SIZE", "10000"),
            ("CHUNKING_TARGET_SIZE", "5000"),
            ("CHUNKING_MAX_SIZE", "20000"),
        ]);

        let config = ChunkingConfig::from_env();

        remove_envs(&[
            "CHUNKING_MIN_SIZE",
            "CHUNKING_TARGET_SIZE",
            "CHUNKING_MAX_SIZE",
        ]);

        // Should clamp: target >= min, max >= target
        assert!(config.target_size >= config.min_size);
        assert!(config.max_size >= config.target_size);
    }

    #[test]
    #[serial]
    fn test_config_from_env_enforces_min_1kb() {
        set_env("CHUNKING_MIN_SIZE", "100");
        remove_env("CHUNKING_TARGET_SIZE");
        remove_env("CHUNKING_MAX_SIZE");

        let config = ChunkingConfig::from_env();

        remove_env("CHUNKING_MIN_SIZE");

        assert_eq!(config.min_size, 1024); // Clamped to 1KB minimum
    }

    #[test]
    #[serial]
    fn test_config_from_env_ignores_non_numeric() {
        set_env("CHUNKING_MIN_SIZE", "abc");
        remove_env("CHUNKING_TARGET_SIZE");
        remove_env("CHUNKING_MAX_SIZE");

        let config = ChunkingConfig::from_env();

        remove_env("CHUNKING_MIN_SIZE");

        // Should use default for invalid value
        assert_eq!(config.min_size, 64 * 1024);
    }

    #[test]
    #[serial]
    fn test_config_from_env_partial_override() {
        set_env("CHUNKING_MIN_SIZE", "2048");
        remove_env("CHUNKING_TARGET_SIZE");
        remove_env("CHUNKING_MAX_SIZE");

        let config = ChunkingConfig::from_env();

        remove_env("CHUNKING_MIN_SIZE");

        assert_eq!(config.min_size, 2048);
        assert_eq!(config.target_size, 256 * 1024); // Default
        assert_eq!(config.max_size, 1024 * 1024); // Default
    }
}
