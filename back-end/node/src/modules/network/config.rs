use crate::bootstrap::init::get_flow_data_dir;
use crate::modules::network::gossipsub::GossipSubConfig;
use libp2p::Multiaddr;
use std::env;
use std::str::FromStr;
use std::time::Duration;
use tracing::error;

const DEFAULT_P2P_SERVICE_NAME: &str = "_flow-p2p._udp.local";
const DEFAULT_MAX_CONNECTIONS: usize = 100;
const DEFAULT_CONNECTION_LIMITS_POLICY: ConnectionLimitPolicy =
    ConnectionLimitPolicy::PreferOutbound;
const DEFAULT_RESERVED_OUTBOUND_CONNECTIONS: usize = 20;
const DEFAULT_BOOTSTRAP_STARTUP_DELAY_MS: u64 = 100;
const DEFAULT_BOOTSTRAP_MAX_RETRIES: u32 = 3;
const DEFAULT_BOOTSTRAP_RETRY_DELAY_MS: u64 = 1000;
const DEFAULT_REPROVIDE_INTERVAL_SECS: u64 = 12 * 60 * 60;
const DEFAULT_QUERY_TIMEOUT_SECS: u64 = 60;
const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = 30;

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

/// Configuration for DHT provider behavior
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Path to RocksDB directory for provider registry
    pub db_path: std::path::PathBuf,

    /// Enable automatic re-announcement of provided content
    pub auto_reprovide_enabled: bool,

    /// Interval between re-announcements in seconds
    pub reprovide_interval_secs: u64,

    /// Timeout for pending provider queries in seconds
    pub query_timeout_secs: u64,

    /// Interval for cleanup task in seconds
    pub cleanup_interval_secs: u64,

    /// Reprovide all CIDs immediately on startup
    pub reprovide_on_startup: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            db_path: get_flow_data_dir()
                .join("network")
                .join("provider_registry"),
            auto_reprovide_enabled: true,
            reprovide_interval_secs: DEFAULT_REPROVIDE_INTERVAL_SECS,
            query_timeout_secs: DEFAULT_QUERY_TIMEOUT_SECS,
            cleanup_interval_secs: DEFAULT_CLEANUP_INTERVAL_SECS,
            reprovide_on_startup: true,
        }
    }
}

impl ProviderConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let db_path = env::var("PROVIDER_REGISTRY_DB_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| Self::default().db_path);

        let auto_reprovide_enabled = env::var("PROVIDER_AUTO_REPROVIDE")
            .map(|v| v.to_lowercase() != "false")
            .unwrap_or(true);

        let reprovide_interval_secs = env::var("PROVIDER_REPROVIDE_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_REPROVIDE_INTERVAL_SECS);

        let query_timeout_secs = env::var("PROVIDER_QUERY_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_QUERY_TIMEOUT_SECS);

        let cleanup_interval_secs = env::var("PROVIDER_CLEANUP_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CLEANUP_INTERVAL_SECS);

        let reprovide_on_startup = env::var("PROVIDER_REPROVIDE_ON_STARTUP")
            .map(|v| v.to_lowercase() != "false")
            .unwrap_or(true);

        Self {
            db_path,
            auto_reprovide_enabled,
            reprovide_interval_secs,
            query_timeout_secs,
            cleanup_interval_secs,
            reprovide_on_startup,
        }
    }

    pub fn reprovide_interval(&self) -> Duration {
        Duration::from_secs(self.reprovide_interval_secs)
    }

    pub fn query_timeout(&self) -> Duration {
        Duration::from_secs(self.query_timeout_secs)
    }

    pub fn cleanup_interval(&self) -> Duration {
        Duration::from_secs(self.cleanup_interval_secs)
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

/// Bootstrap configuration for network initialization
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Whether to automatically dial bootstrap peers on startup
    pub auto_dial: bool,

    /// Whether to run Kademlia bootstrap query on startup
    pub auto_bootstrap: bool,

    /// Delay before starting bootstrap (allows listener to be ready)
    pub startup_delay_ms: u64,

    /// Maximum retry attempts for each bootstrap peer
    pub max_retries: u32,

    /// Base retry delay in milliseconds (exponential backoff)
    pub retry_delay_base_ms: u64,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            auto_dial: true,
            auto_bootstrap: true,
            startup_delay_ms: DEFAULT_BOOTSTRAP_STARTUP_DELAY_MS,
            max_retries: DEFAULT_BOOTSTRAP_MAX_RETRIES,
            retry_delay_base_ms: DEFAULT_BOOTSTRAP_RETRY_DELAY_MS,
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

    /// Bootstrap behaviour configuration
    pub bootstrap: BootstrapConfig,

    /// GossipSub configuration
    pub gossipsub: GossipSubConfig,

    pub provider: ProviderConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enable_quic: true,
            listen_port: 0, // Random port
            bootstrap_peers: Vec::new(),
            mdns: MdnsConfig::default(),
            connection_limits: ConnectionLimits::default(),
            bootstrap: BootstrapConfig::default(),
            gossipsub: GossipSubConfig::default(),
            provider: ProviderConfig::default(),
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
    /// - NETWORK_BOOTSTRAP_AUTO_DIAL: Auto-dial bootstrap peers (default: true)
    /// - NETWORK_BOOTSTRAP_AUTO_QUERY: Auto-run bootstrap query (default: true)
    /// - NETWORK_BOOTSTRAP_DELAY_MS: Startup delay before bootstrap (default: 100)
    /// - NETWORK_BOOTSTRAP_MAX_RETRIES: Max retry attempts (default: 3)
    /// - NETWORK_BOOTSTRAP_RETRY_DELAY_MS: Base retry delay (default: 1000)
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

        let bootstrap_auto_dial = env::var("NETWORK_BOOTSTRAP_AUTO_DIAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let bootstrap_auto_query = env::var("NETWORK_BOOTSTRAP_AUTO_QUERY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let bootstrap_delay_ms = env::var("NETWORK_BOOTSTRAP_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_BOOTSTRAP_STARTUP_DELAY_MS);

        let bootstrap_max_retries = env::var("NETWORK_BOOTSTRAP_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_BOOTSTRAP_MAX_RETRIES);

        let bootstrap_retry_delay_ms = env::var("NETWORK_BOOTSTRAP_RETRY_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_BOOTSTRAP_RETRY_DELAY_MS);

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
            bootstrap: BootstrapConfig {
                auto_dial: bootstrap_auto_dial,
                auto_bootstrap: bootstrap_auto_query,
                startup_delay_ms: bootstrap_delay_ms,
                max_retries: bootstrap_max_retries,
                retry_delay_base_ms: bootstrap_retry_delay_ms,
            },
            gossipsub: GossipSubConfig::from_env(),
            provider: ProviderConfig::from_env(),
        }
    }
}
