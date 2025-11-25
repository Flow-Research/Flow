#[cfg(test)]
mod tests {
    use crate::bootstrap::init::remove_envs;
    use node::modules::network::config::{
        ConnectionLimitPolicy, ConnectionLimits, MdnsConfig, NetworkConfig,
    };
    use serial_test::serial;
    use std::env;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert!(config.enable_quic);
        assert_eq!(config.listen_port, 0);
        assert!(config.bootstrap_peers.is_empty());
        assert!(config.mdns.enabled);
        assert_eq!(config.mdns.service_name, "_flow-p2p._udp.local");
    }

    #[test]
    fn test_mdns_config_default() {
        let mdns = MdnsConfig::default();
        assert!(mdns.enabled);
        assert_eq!(mdns.query_interval_secs, 30);
    }

    #[test]
    #[serial]
    fn test_from_env_defaults() {
        // Clear env vars
        remove_envs(&["NETWORK_PORT", "NETWORK_BOOTSTRAP", "NETWORK_MDNS_ENABLED"]);

        let config = NetworkConfig::from_env();
        assert_eq!(config.listen_port, 0);
        assert!(config.bootstrap_peers.is_empty());
        assert!(config.mdns.enabled);
    }

    #[test]
    #[serial]
    fn test_from_env_mdns_disabled() {
        unsafe {
            env::set_var("NETWORK_MDNS_ENABLED", "false");
        }

        let config = NetworkConfig::from_env();
        assert!(!config.mdns.enabled);

        unsafe {
            env::remove_var("NETWORK_MDNS_ENABLED");
        }
    }

    #[test]
    fn test_connection_limits_unlimited() {
        let limits = ConnectionLimits {
            max_connections: 0,
            policy: ConnectionLimitPolicy::Strict,
            reserved_outbound: 0,
        };

        assert!(limits.can_accept_inbound(1000));
        assert!(limits.can_dial_outbound(1000));
    }

    #[test]
    fn test_connection_limits_strict_policy() {
        let limits = ConnectionLimits {
            max_connections: 10,
            policy: ConnectionLimitPolicy::Strict,
            reserved_outbound: 0,
        };

        // Can accept when under limit
        assert!(limits.can_accept_inbound(5));
        assert!(limits.can_accept_inbound(9));

        // Cannot accept at limit
        assert!(!limits.can_accept_inbound(10));
        assert!(!limits.can_accept_inbound(11));

        // Same for outbound
        assert!(limits.can_dial_outbound(9));
        assert!(!limits.can_dial_outbound(10));
    }

    #[test]
    fn test_connection_limits_prefer_outbound_policy() {
        let limits = ConnectionLimits {
            max_connections: 100,
            policy: ConnectionLimitPolicy::PreferOutbound,
            reserved_outbound: 20,
        };

        // Inbound limit is reduced by reserved slots
        assert!(limits.can_accept_inbound(70));
        assert!(limits.can_accept_inbound(79));
        assert!(!limits.can_accept_inbound(80)); // 100 - 20 = 80 inbound limit
        assert!(!limits.can_accept_inbound(90));

        // Outbound can use full limit
        assert!(limits.can_dial_outbound(90));
        assert!(limits.can_dial_outbound(99));
        assert!(!limits.can_dial_outbound(100));
    }

    #[test]
    fn test_connection_limits_effective_inbound_limit() {
        let limits = ConnectionLimits {
            max_connections: 100,
            policy: ConnectionLimitPolicy::PreferOutbound,
            reserved_outbound: 20,
        };

        assert_eq!(limits.effective_inbound_limit(), 80);
    }

    #[test]
    fn test_connection_limits_effective_inbound_limit_strict() {
        let limits = ConnectionLimits {
            max_connections: 100,
            policy: ConnectionLimitPolicy::Strict,
            reserved_outbound: 20, // Ignored in strict mode
        };

        assert_eq!(limits.effective_inbound_limit(), 100);
    }

    #[test]
    fn test_network_config_includes_connection_limits() {
        let config = NetworkConfig::default();
        assert_eq!(config.connection_limits.max_connections, 100);
    }

    #[test]
    fn test_network_config_from_env_connection_limits() {
        unsafe {
            std::env::set_var("NETWORK_MAX_CONNECTIONS", "50");
            std::env::set_var("NETWORK_RESERVED_OUTBOUND", "10");
        }

        let config = NetworkConfig::from_env();

        assert_eq!(config.connection_limits.max_connections, 50);
        assert_eq!(config.connection_limits.reserved_outbound, 10);

        unsafe {
            std::env::remove_var("NETWORK_MAX_CONNECTIONS");
            std::env::remove_var("NETWORK_RESERVED_OUTBOUND");
        }
    }
}
