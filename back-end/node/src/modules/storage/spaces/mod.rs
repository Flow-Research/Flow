//! Space storage and management module.

mod service;

pub use service::{
    Edge, EdgeDirection, Entity, EntityWithEdges, ServiceError, Space, SpaceService, SpaceStatus,
};
