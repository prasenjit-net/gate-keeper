pub mod api;
#[cfg(test)]
mod tests;
pub mod ws;

use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post};
use axum::Router;

use crate::access_log;
use crate::state::SharedState;
use crate::static_assets;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/health", get(api::health))
        .route("/api/config", get(api::config))
        .route("/api/metrics", get(api::metrics))
        .route("/api/test-plans/preview", post(api::preview_test_plan))
        .route("/api/test-plan-browser", get(api::browse_test_plans))
        .route(
            "/api/test-plan-folders",
            post(api::create_test_plan_folder)
                .put(api::rename_test_plan_folder)
                .delete(api::delete_test_plan_folder),
        )
        .route(
            "/api/test-plans/by-path",
            get(api::get_test_plan_by_path)
                .put(api::update_test_plan_by_path)
                .delete(api::delete_test_plan_by_path),
        )
        .route(
            "/api/test-plans/by-path/execute",
            post(api::execute_test_plan_by_path),
        )
        .route(
            "/api/test-plans",
            get(api::list_test_plans).post(api::create_test_plan),
        )
        .route(
            "/api/test-plans/{id}",
            get(api::get_test_plan)
                .put(api::update_test_plan)
                .delete(api::delete_test_plan),
        )
        .route("/api/test-plans/{id}/execute", post(api::execute_test_plan))
        .route("/api/executions", get(api::list_executions))
        .route("/api/execution-queue", get(api::list_execution_queue))
        .route(
            "/api/executions/{id}",
            get(api::get_execution).delete(api::delete_execution),
        )
        .route("/api/tasks", get(api::list_tasks).post(api::create_task))
        .route("/api/tasks/{id}", delete(api::delete_task))
        .route("/api/tasks/{id}/toggle", post(api::toggle_task))
        .route("/api/error-demo", get(api::error_demo))
        .route("/ws", get(ws::handler))
        .fallback(static_assets::handler)
        .layer(from_fn_with_state(state.clone(), access_log::record))
        .with_state(state)
}
