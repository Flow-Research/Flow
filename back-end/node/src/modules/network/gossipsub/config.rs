use crate::utils::env::{env_bool, env_u64, env_usize};
use libp2p::gossipsub::{self, MessageAuthenticity, ValidationMode};
use libp2p::identity::Keypair;
use std::time::Duration;
use tracing::{error, info};

/// Configuration for GossipSub protocol
#[derive(Debug, Clone)]
pub struct GossipSubConfig {
    /// Enable GossipSub protocol
    pub enabled: bool,

    /// Heartbeat interval for mesh maintenance
    pub heartbeat_interval: Duration,

    /// Target number of peers in mesh for each topic
    pub mesh_n: usize,

    /// Minimum peers in mesh before emitting GRAFT
    pub mesh_n_low: usize,

    /// Maximum peers in mesh before emitting PRUNE
    pub mesh_n_high: usize,

    /// Number of peers to gossip to (outside mesh)
    pub gossip_lazy: usize,

    /// Message cache time-to-live
    pub message_cache_ttl: Duration,

    /// Duplicate message cache time
    pub duplicate_cache_time: Duration,

    /// Maximum message size in bytes
    pub max_transmit_size: usize,

    /// Whether to validate message signatures
    pub validate_signatures: bool,

    /// Message validation mode
    pub validation_mode: ValidationMode,
}

impl Default for GossipSubConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval: Duration::from_secs(1),
            mesh_n: 6,
            mesh_n_low: 4,
            mesh_n_high: 12,
            gossip_lazy: 6,
            message_cache_ttl: Duration::from_secs(120),
            duplicate_cache_time: Duration::from_secs(60),
            max_transmit_size: 65536, // 64KB
            validate_signatures: true,
            validation_mode: ValidationMode::Strict,
        }
    }
}

impl GossipSubConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let enabled = env_bool("GOSSIPSUB_ENABLED", true);
        let heartbeat_interval = Duration::from_millis(env_u64("GOSSIPSUB_HEARTBEAT_INTERVAL_MS", 1000));
        let mesh_n = env_usize("GOSSIPSUB_MESH_N", 6);
        let mesh_n_low = env_usize("GOSSIPSUB_MESH_N_LOW", 4);
        let mesh_n_high = env_usize("GOSSIPSUB_MESH_N_HIGH", 12);
        let max_transmit_size = env_usize("GOSSIPSUB_MAX_MESSAGE_SIZE", 65536);
        let validate_signatures = env_bool("GOSSIPSUB_VALIDATE_SIGNATURES", true);

        info!(
            enabled,
            heartbeat_interval_ms = heartbeat_interval.as_millis(),
            mesh_n,
            max_message_size = max_transmit_size,
            validate_signatures,
            "GossipSub configuration loaded"
        );

        Self {
            enabled,
            heartbeat_interval,
            mesh_n,
            mesh_n_low,
            mesh_n_high,
            max_transmit_size,
            validate_signatures,
            ..Default::default()
        }
    }

    /// Build a libp2p GossipSub behaviour from this config
    pub fn build_behaviour(
        &self,
        keypair: &Keypair,
    ) -> Result<gossipsub::Behaviour, gossipsub::ConfigBuilderError> {
        let message_authenticity = if self.validate_signatures {
            MessageAuthenticity::Signed(keypair.clone())
        } else {
            MessageAuthenticity::Anonymous
        };

        let config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(self.heartbeat_interval)
            .mesh_n(self.mesh_n)
            .mesh_n_low(self.mesh_n_low)
            .mesh_n_high(self.mesh_n_high)
            .gossip_lazy(self.gossip_lazy)
            .history_length(12)
            .history_gossip(3)
            .max_transmit_size(self.max_transmit_size)
            .duplicate_cache_time(self.duplicate_cache_time)
            .validation_mode(self.validation_mode.clone())
            .build()?;

        Ok(
            gossipsub::Behaviour::new(message_authenticity, config).map_err(|e| {
                error!("Error: {}", e);
                gossipsub::ConfigBuilderError::MeshParametersInvalid
            })?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GossipSubConfig::default();
        assert!(config.enabled);
        assert_eq!(config.mesh_n, 6);
        assert_eq!(config.mesh_n_low, 4);
        assert_eq!(config.mesh_n_high, 12);
        assert!(config.validate_signatures);
    }

    #[test]
    fn test_build_behaviour() {
        let config = GossipSubConfig::default();
        let keypair = Keypair::generate_ed25519();
        let result = config.build_behaviour(&keypair);
        assert!(result.is_ok());
    }
}
