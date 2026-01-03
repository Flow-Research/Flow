use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "kg_entity")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub cid: String,
    pub space_id: i32,
    pub entity_type: String,
    pub name: String,
    #[sea_orm(column_type = "Json", nullable)]
    pub properties: Option<Json>,
    #[sea_orm(column_type = "Blob", nullable)]
    pub embedding: Option<Vec<u8>>,
    pub created_at: DateTimeWithTimeZone,
    pub created_by: String,
    pub source_chunk_id: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::space::Entity",
        from = "Column::SpaceId",
        to = "super::space::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Space,
}

impl Related<super::space::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Space.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
