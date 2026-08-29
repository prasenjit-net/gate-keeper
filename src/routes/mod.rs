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
        .route("/api/http-plans/preview", post(api::preview_http_plan))
        .route(
            "/api/http-plans",
            get(api::list_http_plans).post(api::create_http_plan),
        )
        .route(
            "/api/http-plans/{id}",
            get(api::get_http_plan)
                .put(api::update_http_plan)
                .delete(api::delete_http_plan),
        )
        .route("/api/http-plans/{id}/execute", post(api::execute_http_plan))
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
