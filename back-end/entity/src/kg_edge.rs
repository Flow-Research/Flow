use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "kg_edge")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub cid: String,
    pub space_id: i32,
    pub edge_type: String,
    pub source_id: i32,
    pub target_id: i32,
    #[sea_orm(column_type = "Json", nullable)]
    pub properties: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
    pub created_by: String,
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
    #[sea_orm(
        belongs_to = "super::kg_entity::Entity",
        from = "Column::SourceId",
        to = "super::kg_entity::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Source,
    #[sea_orm(
        belongs_to = "super::kg_entity::Entity",
        from = "Column::TargetId",
        to = "super::kg_entity::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Target,
}

impl Related<super::space::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Space.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
