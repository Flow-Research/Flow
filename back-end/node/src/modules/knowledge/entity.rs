use entity::kg_entity;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use sha2::{Digest, Sha256};

use super::types::{CreateEntity, EntityFilter, KgError};

pub struct EntityStore {
    db: DatabaseConnection,
}

impl EntityStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create(&self, input: CreateEntity) -> Result<kg_entity::Model, KgError> {
        let cid = generate_entity_cid(&input)?;

        let model = kg_entity::ActiveModel {
            cid: Set(cid),
            space_id: Set(input.space_id),
            entity_type: Set(input.entity_type.to_string()),
            name: Set(input.name),
            properties: Set(input.properties.map(sea_orm::JsonValue::from)),
            embedding: Set(None),
            created_at: Set(chrono::Utc::now().into()),
            created_by: Set(input.created_by),
            source_chunk_id: Set(input.source_chunk_id),
            ..Default::default()
        };

        let result = model.insert(&self.db).await?;
        Ok(result)
    }

    pub async fn get(&self, id: i32) -> Result<Option<kg_entity::Model>, KgError> {
        let entity = kg_entity::Entity::find_by_id(id).one(&self.db).await?;
        Ok(entity)
    }

    pub async fn get_by_cid(&self, cid: &str) -> Result<Option<kg_entity::Model>, KgError> {
        let entity = kg_entity::Entity::find()
            .filter(kg_entity::Column::Cid.eq(cid))
            .one(&self.db)
            .await?;
        Ok(entity)
    }

    pub async fn list(
        &self,
        space_id: i32,
        filter: EntityFilter,
    ) -> Result<Vec<kg_entity::Model>, KgError> {
        let mut query = kg_entity::Entity::find()
            .filter(kg_entity::Column::SpaceId.eq(space_id))
            .order_by_desc(kg_entity::Column::CreatedAt);

        if let Some(entity_type) = filter.entity_type {
            query = query.filter(kg_entity::Column::EntityType.eq(entity_type.to_string()));
        }

        if let Some(name_contains) = filter.name_contains {
            query = query.filter(kg_entity::Column::Name.contains(&name_contains));
        }

        if let Some(offset) = filter.offset {
            query = query.offset(offset);
        }

        if let Some(limit) = filter.limit {
            query = query.limit(limit);
        }

        let entities = query.all(&self.db).await?;
        Ok(entities)
    }

    pub async fn count(&self, space_id: i32) -> Result<u64, KgError> {
        let count = kg_entity::Entity::find()
            .filter(kg_entity::Column::SpaceId.eq(space_id))
            .count(&self.db)
            .await?;
        Ok(count)
    }

    pub async fn find_by_name(
        &self,
        space_id: i32,
        name: &str,
        entity_type: Option<&str>,
    ) -> Result<Option<kg_entity::Model>, KgError> {
        let mut query = kg_entity::Entity::find()
            .filter(kg_entity::Column::SpaceId.eq(space_id))
            .filter(kg_entity::Column::Name.eq(name));

        if let Some(et) = entity_type {
            query = query.filter(kg_entity::Column::EntityType.eq(et));
        }

        let entity = query.one(&self.db).await?;
        Ok(entity)
    }

    pub async fn update_embedding(
        &self,
        id: i32,
        embedding: Vec<u8>,
    ) -> Result<kg_entity::Model, KgError> {
        let entity = kg_entity::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(KgError::EntityNotFound(id))?;

        let mut active_model: kg_entity::ActiveModel = entity.into();
        active_model.embedding = Set(Some(embedding));

        let result = active_model.update(&self.db).await?;
        Ok(result)
    }

    pub async fn delete(&self, id: i32) -> Result<(), KgError> {
        kg_entity::Entity::delete_by_id(id)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn delete_by_space(&self, space_id: i32) -> Result<u64, KgError> {
        let result = kg_entity::Entity::delete_many()
            .filter(kg_entity::Column::SpaceId.eq(space_id))
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }
}

fn generate_entity_cid(input: &CreateEntity) -> Result<String, KgError> {
    let content = serde_json::to_string(&serde_json::json!({
        "space_id": input.space_id,
        "entity_type": input.entity_type.to_string(),
        "name": input.name,
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
    use crate::modules::knowledge::types::EntityType;
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

        create_test_space(&db, 1).await;

        db
    }

    async fn create_test_space(db: &DatabaseConnection, id: i32) {
        let model = space::ActiveModel {
            id: Set(id),
            key: Set(format!("test-space-{}", id)),
            name: Set(Some(format!("Test Space {}", id))),
            location: Set("/tmp/test".to_string()),
            time_created: Set(chrono::Utc::now().into()),
        };
        model.insert(db).await.unwrap();
    }

    #[tokio::test]
    async fn test_create_and_get_entity() {
        let db = setup_test_db().await;
        let store = EntityStore::new(db);

        let input = CreateEntity {
            space_id: 1,
            entity_type: EntityType::Person,
            name: "John Doe".to_string(),
            properties: Some(serde_json::json!({"occupation": "Engineer"})),
            created_by: "did:key:test".to_string(),
            source_chunk_id: None,
        };

        let entity = store.create(input).await.unwrap();
        assert_eq!(entity.name, "John Doe");
        assert_eq!(entity.entity_type, "person");

        let fetched = store.get(entity.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, entity.id);
        assert_eq!(fetched.name, "John Doe");
    }

    #[tokio::test]
    async fn test_list_entities_with_filter() {
        let db = setup_test_db().await;
        let store = EntityStore::new(db);

        for i in 0..5 {
            let input = CreateEntity {
                space_id: 1,
                entity_type: if i % 2 == 0 {
                    EntityType::Person
                } else {
                    EntityType::Concept
                },
                name: format!("Entity {}", i),
                properties: None,
                created_by: "did:key:test".to_string(),
                source_chunk_id: None,
            };
            store.create(input).await.unwrap();
        }

        let all = store.list(1, EntityFilter::default()).await.unwrap();
        assert_eq!(all.len(), 5);

        let persons = store
            .list(
                1,
                EntityFilter {
                    entity_type: Some(EntityType::Person),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(persons.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_entity() {
        let db = setup_test_db().await;
        let store = EntityStore::new(db);

        let input = CreateEntity {
            space_id: 1,
            entity_type: EntityType::Concept,
            name: "Test Concept".to_string(),
            properties: None,
            created_by: "did:key:test".to_string(),
            source_chunk_id: None,
        };

        let entity = store.create(input).await.unwrap();
        store.delete(entity.id).await.unwrap();

        let fetched = store.get(entity.id).await.unwrap();
        assert!(fetched.is_none());
    }
}
