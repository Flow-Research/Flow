use libp2p::Multiaddr;
use std::env;
use std::str::FromStr;
use tracing::error;

const DEFAULT_P2P_SERVICE_NAME: &str = "_flow-p2p._udp.local";
const DEFAULT_MAX_CONNECTIONS: usize = 100;
const DEFAULT_CONNECTION_LIMITS_POLICY: ConnectionLimitPolicy =
    ConnectionLimitPolicy::PreferOutbound;
const DEFAULT_RESERVED_OUTBOUND_CONNECTIONS: usize = 20;

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
            service_name: DEFAULT_P2P_SERVICE_NAME.to_string(),
            query_interval_secs: 30,
        }
    }
}

/// Connection limit policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionLimitPolicy {
    /// Reject all new connections when at limit
    Strict,
    /// Allow outbound connections even when at limit (default)
    PreferOutbound,
}

impl Default for ConnectionLimitPolicy {
    fn default() -> Self {
        Self::PreferOutbound
    }
}

impl FromStr for ConnectionLimitPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "strict" => Ok(Self::Strict),
            "preferoutbound" | "prefer_outbound" | "prefer-outbound" => Ok(Self::PreferOutbound),
            _ => Err(format!(
                "Invalid connection limit policy: '{}'. Valid values: 'strict', 'prefer_outbound'",
                s
            )),
        }
    }
}

/// Connection limit configuration
#[derive(Debug, Clone)]
pub struct ConnectionLimits {
    /// Maximum total connections (0 = unlimited)
    pub max_connections: usize,

    /// Policy for handling limits
    pub policy: ConnectionLimitPolicy,

    /// Reserved slots for outbound connections
    /// When using PreferOutbound policy, this many slots are reserved
    pub reserved_outbound: usize,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_MAX_CONNECTIONS,
            policy: DEFAULT_CONNECTION_LIMITS_POLICY,
            reserved_outbound: DEFAULT_RESERVED_OUTBOUND_CONNECTIONS,
        }
    }
}

impl ConnectionLimits {
    /// Check if we can accept a new inbound connection
    pub fn can_accept_inbound(&self, current_count: usize) -> bool {
        if self.max_connections == 0 {
            return true; // Unlimited
        }

        match self.policy {
            ConnectionLimitPolicy::Strict => current_count < self.max_connections,
            ConnectionLimitPolicy::PreferOutbound => {
                // Leave room for reserved outbound slots
                let inbound_limit = self.max_connections.saturating_sub(self.reserved_outbound);
                current_count < inbound_limit
            }
        }
    }

    /// Check if we can make a new outbound connection
    pub fn can_dial_outbound(&self, current_count: usize) -> bool {
        if self.max_connections == 0 {
            return true; // Unlimited
        }

        current_count < self.max_connections
    }

    /// Get effective inbound limit
    pub fn effective_inbound_limit(&self) -> usize {
        if self.max_connections == 0 {
            return usize::MAX;
        }

        match self.policy {
            ConnectionLimitPolicy::Strict => self.max_connections,
            ConnectionLimitPolicy::PreferOutbound => {
                self.max_connections.saturating_sub(self.reserved_outbound)
            }
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

    /// Connection limits
    pub connection_limits: ConnectionLimits,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enable_quic: true,
            listen_port: 0, // Random port
            bootstrap_peers: Vec::new(),
            mdns: MdnsConfig::default(),
            connection_limits: ConnectionLimits::default(),
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
    /// - NETWORK_MAX_CONNECTIONS: Maximum number of connections
    /// - NETWORK_RESERVED_OUTBOUND: Reserved outbound connections
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
            .unwrap_or_else(|| DEFAULT_P2P_SERVICE_NAME.to_string());

        let mdns_query_interval = env::var("NETWORK_MDNS_QUERY_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let max_connections = env::var("NETWORK_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_CONNECTIONS);

        let reserved_outbound = env::var("NETWORK_RESERVED_OUTBOUND")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_RESERVED_OUTBOUND_CONNECTIONS);

        let connection_limits_policy = env::var("NETWORK_CONNECTION_POLICY")
            .ok()
            .and_then(|s| {
                s.parse::<ConnectionLimitPolicy>()
                    .map_err(|e| {
                        error!("Warning: {}", e);
                        e
                    })
                    .ok()
            })
            .unwrap_or(DEFAULT_CONNECTION_LIMITS_POLICY);

        Self {
            enable_quic,
            listen_port,
            bootstrap_peers,
            mdns: MdnsConfig {
                enabled: mdns_enabled,
                service_name: mdns_service_name,
                query_interval_secs: mdns_query_interval,
            },
            connection_limits: ConnectionLimits {
                max_connections,
                policy: connection_limits_policy,
                reserved_outbound,
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
