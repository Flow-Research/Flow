use libp2p::Multiaddr;
use std::env;

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
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enable_quic: true,
            listen_port: 0, // Random port
            bootstrap_peers: Vec::new(),
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

        Self {
            enable_quic,
            listen_port,
            bootstrap_peers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert!(config.enable_quic);
        assert_eq!(config.listen_port, 0);
        assert!(config.bootstrap_peers.is_empty());
    }

    #[test]
    fn test_from_env_defaults() {
        // Clear env vars
        unsafe { env::remove_var("NETWORK_PORT") };
        unsafe { env::remove_var("NETWORK_BOOTSTRAP") };

        let config = NetworkConfig::from_env();
        assert_eq!(config.listen_port, 0);
        assert!(config.bootstrap_peers.is_empty());
    }
}
