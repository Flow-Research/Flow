use entity::{kg_edge, kg_entity};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};

use super::types::{CreateEdge, Direction, EdgeFilter, KgError, TraversalResult};

pub struct EdgeStore {
    db: DatabaseConnection,
}

impl EdgeStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create(&self, input: CreateEdge) -> Result<kg_edge::Model, KgError> {
        let cid = generate_edge_cid(&input)?;

        let model = kg_edge::ActiveModel {
            cid: Set(cid),
            space_id: Set(input.space_id),
            edge_type: Set(input.edge_type.to_string()),
            source_id: Set(input.source_id),
            target_id: Set(input.target_id),
            properties: Set(input.properties.map(sea_orm::JsonValue::from)),
            created_at: Set(chrono::Utc::now().into()),
            created_by: Set(input.created_by),
            ..Default::default()
        };

        let result = model.insert(&self.db).await?;
        Ok(result)
    }

    pub async fn get(&self, id: i32) -> Result<Option<kg_edge::Model>, KgError> {
        let edge = kg_edge::Entity::find_by_id(id).one(&self.db).await?;
        Ok(edge)
    }

    pub async fn get_by_cid(&self, cid: &str) -> Result<Option<kg_edge::Model>, KgError> {
        let edge = kg_edge::Entity::find()
            .filter(kg_edge::Column::Cid.eq(cid))
            .one(&self.db)
            .await?;
        Ok(edge)
    }

    pub async fn list(
        &self,
        space_id: i32,
        filter: EdgeFilter,
    ) -> Result<Vec<kg_edge::Model>, KgError> {
        let mut query = kg_edge::Entity::find()
            .filter(kg_edge::Column::SpaceId.eq(space_id))
            .order_by_desc(kg_edge::Column::CreatedAt);

        if let Some(edge_type) = filter.edge_type {
            query = query.filter(kg_edge::Column::EdgeType.eq(edge_type.to_string()));
        }

        if let Some(offset) = filter.offset {
            query = query.offset(offset);
        }

        if let Some(limit) = filter.limit {
            query = query.limit(limit);
        }

        let edges = query.all(&self.db).await?;
        Ok(edges)
    }

    pub async fn get_edges_for_entity(
        &self,
        entity_id: i32,
        direction: Direction,
    ) -> Result<Vec<kg_edge::Model>, KgError> {
        let edges = match direction {
            Direction::Outgoing => {
                kg_edge::Entity::find()
                    .filter(kg_edge::Column::SourceId.eq(entity_id))
                    .all(&self.db)
                    .await?
            }
            Direction::Incoming => {
                kg_edge::Entity::find()
                    .filter(kg_edge::Column::TargetId.eq(entity_id))
                    .all(&self.db)
                    .await?
            }
            Direction::Both => {
                let outgoing = kg_edge::Entity::find()
                    .filter(kg_edge::Column::SourceId.eq(entity_id))
                    .all(&self.db)
                    .await?;

                let incoming = kg_edge::Entity::find()
                    .filter(kg_edge::Column::TargetId.eq(entity_id))
                    .all(&self.db)
                    .await?;

                [outgoing, incoming].concat()
            }
        };

        Ok(edges)
    }

    pub async fn count(&self, space_id: i32) -> Result<u64, KgError> {
        let count = kg_edge::Entity::find()
            .filter(kg_edge::Column::SpaceId.eq(space_id))
            .count(&self.db)
            .await?;
        Ok(count)
    }

    pub async fn find_edge(
        &self,
        source_id: i32,
        target_id: i32,
        edge_type: Option<&str>,
    ) -> Result<Option<kg_edge::Model>, KgError> {
        let mut query = kg_edge::Entity::find()
            .filter(kg_edge::Column::SourceId.eq(source_id))
            .filter(kg_edge::Column::TargetId.eq(target_id));

        if let Some(et) = edge_type {
            query = query.filter(kg_edge::Column::EdgeType.eq(et));
        }

        let edge = query.one(&self.db).await?;
        Ok(edge)
    }

    pub async fn delete(&self, id: i32) -> Result<(), KgError> {
        kg_edge::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    pub async fn delete_by_entity(&self, entity_id: i32) -> Result<u64, KgError> {
        let result = kg_edge::Entity::delete_many()
            .filter(
                kg_edge::Column::SourceId
                    .eq(entity_id)
                    .or(kg_edge::Column::TargetId.eq(entity_id)),
            )
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }

    pub async fn delete_by_space(&self, space_id: i32) -> Result<u64, KgError> {
        let result = kg_edge::Entity::delete_many()
            .filter(kg_edge::Column::SpaceId.eq(space_id))
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }

    pub async fn traverse(
        &self,
        start_id: i32,
        edge_type: Option<&str>,
        max_depth: usize,
    ) -> Result<Vec<TraversalResult>, KgError> {
        if max_depth > 2 {
            return Err(KgError::MaxDepthExceeded);
        }

        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        visited.insert(start_id);
        queue.push_back((start_id, 0, vec![start_id]));

        while let Some((current_id, depth, path)) = queue.pop_front() {
            if depth > 0 {
                if let Some(entity) = kg_entity::Entity::find_by_id(current_id)
                    .one(&self.db)
                    .await?
                {
                    results.push(TraversalResult {
                        entity_id: entity.id,
                        entity_name: entity.name,
                        entity_type: entity.entity_type,
                        depth,
                        path: path.clone(),
                    });
                }
            }

            if depth < max_depth {
                let mut query = kg_edge::Entity::find()
                    .filter(kg_edge::Column::SourceId.eq(current_id));

                if let Some(et) = edge_type {
                    query = query.filter(kg_edge::Column::EdgeType.eq(et));
                }

                let edges = query.all(&self.db).await?;

                for edge in edges {
                    if !visited.contains(&edge.target_id) {
                        visited.insert(edge.target_id);
                        let mut new_path = path.clone();
                        new_path.push(edge.target_id);
                        queue.push_back((edge.target_id, depth + 1, new_path));
                    }
                }
            }
        }

        Ok(results)
    }
}

fn generate_edge_cid(input: &CreateEdge) -> Result<String, KgError> {
    let content = serde_json::to_string(&serde_json::json!({
        "space_id": input.space_id,
        "edge_type": input.edge_type.to_string(),
        "source_id": input.source_id,
        "target_id": input.target_id,
        "properties": input.properties,
        "created_by": input.created_by,
        "timestamp": chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
    }))?;

    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();

    Ok(format!("bafk{}", hex::encode(&hash[..16])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::knowledge::types::EdgeType;
    use entity::space;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Schema};

    async fn setup_test_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();

        let schema = Schema::new(DbBackend::Sqlite);
        let builder = db.get_database_backend();

        let space_stmt = schema.create_table_from_entity(space::Entity);
        db.execute(builder.build(&space_stmt)).await.unwrap();

        let entity_stmt = schema.create_table_from_entity(kg_entity::Entity);
        db.execute(builder.build(&entity_stmt)).await.unwrap();

        let edge_stmt = schema.create_table_from_entity(kg_edge::Entity);
        db.execute(builder.build(&edge_stmt)).await.unwrap();

        create_test_space(&db, 1).await;

        db
    }

    async fn create_test_space(db: &DatabaseConnection, id: i32) {
        let model = space::ActiveModel {
            id: Set(id),
            key: Set(format!("test-space-{}", id)),
            location: Set("/tmp/test".to_string()),
            time_created: Set(chrono::Utc::now().into()),
        };
        model.insert(db).await.unwrap();
    }

    async fn create_test_entity(db: &DatabaseConnection, name: &str) -> kg_entity::Model {
        let model = kg_entity::ActiveModel {
            cid: Set(format!("bafk{}", name)),
            space_id: Set(1),
            entity_type: Set("person".to_string()),
            name: Set(name.to_string()),
            properties: Set(None),
            embedding: Set(None),
            created_at: Set(chrono::Utc::now().into()),
            created_by: Set("did:key:test".to_string()),
            source_chunk_id: Set(None),
            ..Default::default()
        };
        model.insert(db).await.unwrap()
    }

    #[tokio::test]
    async fn test_create_and_get_edge() {
        let db = setup_test_db().await;
        let store = EdgeStore::new(db.clone());

        let source = create_test_entity(&db, "Source").await;
        let target = create_test_entity(&db, "Target").await;

        let input = CreateEdge {
            space_id: 1,
            edge_type: EdgeType::RelatedTo,
            source_id: source.id,
            target_id: target.id,
            properties: None,
            created_by: "did:key:test".to_string(),
        };

        let edge = store.create(input).await.unwrap();
        assert_eq!(edge.source_id, source.id);
        assert_eq!(edge.target_id, target.id);
        assert_eq!(edge.edge_type, "related_to");

        let fetched = store.get(edge.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, edge.id);
    }

    #[tokio::test]
    async fn test_get_edges_for_entity() {
        let db = setup_test_db().await;
        let store = EdgeStore::new(db.clone());

        let entity_a = create_test_entity(&db, "A").await;
        let entity_b = create_test_entity(&db, "B").await;
        let entity_c = create_test_entity(&db, "C").await;

        store
            .create(CreateEdge {
                space_id: 1,
                edge_type: EdgeType::RelatedTo,
                source_id: entity_a.id,
                target_id: entity_b.id,
                properties: None,
                created_by: "did:key:test".to_string(),
            })
            .await
            .unwrap();

        store
            .create(CreateEdge {
                space_id: 1,
                edge_type: EdgeType::Mentions,
                source_id: entity_c.id,
                target_id: entity_a.id,
                properties: None,
                created_by: "did:key:test".to_string(),
            })
            .await
            .unwrap();

        let outgoing = store
            .get_edges_for_entity(entity_a.id, Direction::Outgoing)
            .await
            .unwrap();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].target_id, entity_b.id);

        let incoming = store
            .get_edges_for_entity(entity_a.id, Direction::Incoming)
            .await
            .unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].source_id, entity_c.id);

        let both = store
            .get_edges_for_entity(entity_a.id, Direction::Both)
            .await
            .unwrap();
        assert_eq!(both.len(), 2);
    }

    #[tokio::test]
    async fn test_traverse() {
        let db = setup_test_db().await;
        let store = EdgeStore::new(db.clone());

        let entity_a = create_test_entity(&db, "A").await;
        let entity_b = create_test_entity(&db, "B").await;
        let entity_c = create_test_entity(&db, "C").await;

        store
            .create(CreateEdge {
                space_id: 1,
                edge_type: EdgeType::RelatedTo,
                source_id: entity_a.id,
                target_id: entity_b.id,
                properties: None,
                created_by: "did:key:test".to_string(),
            })
            .await
            .unwrap();

        store
            .create(CreateEdge {
                space_id: 1,
                edge_type: EdgeType::RelatedTo,
                source_id: entity_b.id,
                target_id: entity_c.id,
                properties: None,
                created_by: "did:key:test".to_string(),
            })
            .await
            .unwrap();

        let results = store.traverse(entity_a.id, None, 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.entity_name == "B" && r.depth == 1));
        assert!(results.iter().any(|r| r.entity_name == "C" && r.depth == 2));
    }
}
