#[cfg(test)]
mod tests {
    use crate::bootstrap::init::{remove_envs, set_envs};
    use node::modules::network::storage::StorageConfig;
    use serial_test::serial;

    #[test]
    fn test_default_storage_config() {
        let config = StorageConfig::default();
        assert_eq!(config.max_records, 65536);
        assert_eq!(config.max_providers_per_key, 20);
        assert!(config.enable_compression);
    }

    #[test]
    #[serial]
    fn test_storage_config_from_env() {
        set_envs(&vec![
            ("DHT_MAX_RECORDS", "100000"),
            ("DHT_MAX_PROVIDERS", "30"),
        ]);

        let config = StorageConfig::from_env();
        assert_eq!(config.max_records, 100000);
        assert_eq!(config.max_providers_per_key, 30);

        remove_envs(&["DHT_MAX_RECORDS", "DHT_MAX_PROVIDERS"]);
    }
}
