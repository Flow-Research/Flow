use libp2p::Multiaddr;
use std::env;

const P2P_DEFAULT_SERVICE_NAME: &str = "_flow-p2p._udp.local";

/// mDNS discovery configuration
#[derive(Debug, Clone)]
pub struct MdnsConfig {
    /// Enable mDNS discovery for local network
    pub enabled: bool,

    /// Service name for mDNS advertisements
    /// Standard format: "_service._protocol.local"
    pub service_name: String,

    /// Query interval for discovering peers (seconds)
    pub query_interval_secs: u64,
}

impl Default for MdnsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_name: P2P_DEFAULT_SERVICE_NAME.to_string(),
            query_interval_secs: 30,
        }
    }
}

/// Configuration for the networking layer
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Enable QUIC transport (recommended for production)
    pub enable_quic: bool,

    /// Port to listen on (0 = OS assigns random port)
    pub listen_port: u16,

    /// Bootstrap peer addresses for initial DHT connection
    /// Format: "/ip4/1.2.3.4/udp/4001/quic-v1/p2p/12D3K..."
    pub bootstrap_peers: Vec<Multiaddr>,

    /// mDNS configuration
    pub mdns: MdnsConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enable_quic: true,
            listen_port: 0, // Random port
            bootstrap_peers: Vec::new(),
            mdns: MdnsConfig::default(),
        }
    }
}

impl NetworkConfig {
    /// Load configuration from environment variables
    ///
    /// Environment variables:
    /// - NETWORK_PORT: Listen port (default: 0)
    /// - NETWORK_BOOTSTRAP: Comma-separated bootstrap peers
    /// - NETWORK_ENABLE_QUIC: "true" or "false" (default: true)
    /// - NETWORK_MDNS_ENABLED: "true" or "false" (default: true)
    /// - NETWORK_MDNS_SERVICE_NAME: mDNS service name (default: "_flow-p2p._udp.local")
    /// - NETWORK_MDNS_QUERY_INTERVAL: Query interval in seconds (default: 30)
    pub fn from_env() -> Self {
        let listen_port = env::var("NETWORK_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let bootstrap_peers = env::var("NETWORK_BOOTSTRAP")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter_map(|addr| addr.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default();

        let enable_quic = env::var("NETWORK_ENABLE_QUIC")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let mdns_enabled = env::var("NETWORK_MDNS_ENABLED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let mdns_service_name = env::var("NETWORK_MDNS_SERVICE_NAME")
            .ok()
            .unwrap_or_else(|| P2P_DEFAULT_SERVICE_NAME.to_string());

        let mdns_query_interval = env::var("NETWORK_MDNS_QUERY_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        Self {
            enable_quic,
            listen_port,
            bootstrap_peers,
            mdns: MdnsConfig {
                enabled: mdns_enabled,
                service_name: mdns_service_name,
                query_interval_secs: mdns_query_interval,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

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
        unsafe {
            env::remove_var("NETWORK_PORT");
            env::remove_var("NETWORK_BOOTSTRAP");
            env::remove_var("NETWORK_MDNS_ENABLED");
        }

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
}
