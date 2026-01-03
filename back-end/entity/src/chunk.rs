use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "chunk")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub cid: String,
    pub document_id: i32,
    pub space_id: i32,
    pub position_start: Option<i32>,
    pub position_end: Option<i32>,
    pub char_count: Option<i32>,
    #[sea_orm(column_type = "Json", nullable)]
    pub metadata: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::kg_entity::Entity",
        from = "Column::DocumentId",
        to = "super::kg_entity::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Document,
    #[sea_orm(
        belongs_to = "super::space::Entity",
        from = "Column::SpaceId",
        to = "super::space::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Space,
}

impl Related<super::kg_entity::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Document.def()
    }
}

impl Related<super::space::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Space.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
