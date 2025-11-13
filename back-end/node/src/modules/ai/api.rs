use errors::AppError;

use crate::modules::ai::pipeline_manager::PipelineManager;

pub async fn query_space(
    pipeline_manager: &PipelineManager,
    space_key: &str,
    query: &str,
) -> Result<String, AppError> {
    pipeline_manager.query_space(space_key, query).await
}
