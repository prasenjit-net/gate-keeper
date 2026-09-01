use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::services::events::Event;
use crate::services::metrics::MetricsSnapshot;
use crate::services::tasks::{NewTask, Task};
use crate::services::test_plans::{
    self, ExecutionQueueItem, ExecutionSummary, FolderInput, RenameFolderInput, SavePlanInput,
    StoredExecution, StoredPlan, StoredPlanSummary, TestPlan, TestPlanBrowser, TestPlanInput,
};
use crate::state::SharedState;

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Bootstrap configuration for the SPA, sourced from config.toml.
pub async fn config(State(state): State<SharedState>) -> Json<Value> {
    Json(json!({
        "ui": state.config.ui,
        "version": env!("CARGO_PKG_VERSION"),
        "startedAtMs": state.started_at_ms,
    }))
}

pub async fn metrics(State(state): State<SharedState>) -> AppResult<Json<MetricsSnapshot>> {
    state
        .latest_metrics
        .read()
        .await
        .clone()
        .map(Json)
        .ok_or_else(|| AppError::Internal("metrics are not available yet".into()))
}

pub async fn preview_test_plan(Json(body): Json<TestPlanInput>) -> AppResult<Json<TestPlan>> {
    test_plans::parse(body).map(Json)
}

pub async fn list_test_plans(State(state): State<SharedState>) -> Json<Vec<StoredPlanSummary>> {
    Json(state.test_plans.list_plans().await)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserQuery {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPathQuery {
    path: String,
}

pub async fn browse_test_plans(
    State(state): State<SharedState>,
    Query(query): Query<BrowserQuery>,
) -> AppResult<Json<TestPlanBrowser>> {
    state
        .test_plans
        .browse(query.path.as_deref())
        .await
        .map(Json)
}

pub async fn create_test_plan(
    State(state): State<SharedState>,
    Json(body): Json<SavePlanInput>,
) -> AppResult<(StatusCode, Json<StoredPlan>)> {
    let plan = state.test_plans.create_plan(body).await?;
    state.activity("test-plan", format!("Saved test plan \"{}\"", plan.name));
    Ok((StatusCode::CREATED, Json(plan)))
}

pub async fn get_test_plan(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<StoredPlan>> {
    state.test_plans.get_plan(&id).await.map(Json)
}

pub async fn get_test_plan_by_path(
    State(state): State<SharedState>,
    Query(query): Query<PlanPathQuery>,
) -> AppResult<Json<StoredPlan>> {
    state
        .test_plans
        .get_plan_by_path(&query.path)
        .await
        .map(Json)
}

pub async fn update_test_plan(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(body): Json<SavePlanInput>,
) -> AppResult<Json<StoredPlan>> {
    let plan = state.test_plans.update_plan(&id, body).await?;
    state.activity("test-plan", format!("Updated test plan \"{}\"", plan.name));
    Ok(Json(plan))
}

pub async fn update_test_plan_by_path(
    State(state): State<SharedState>,
    Query(query): Query<PlanPathQuery>,
    Json(body): Json<SavePlanInput>,
) -> AppResult<Json<StoredPlan>> {
    let plan = state.test_plans.update_plan(&query.path, body).await?;
    state.activity("test-plan", format!("Updated test plan \"{}\"", plan.name));
    Ok(Json(plan))
}

pub async fn delete_test_plan(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    state.test_plans.delete_plan(&id).await?;
    state.activity("test-plan", format!("Deleted test plan {id}"));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_test_plan_by_path(
    State(state): State<SharedState>,
    Query(query): Query<PlanPathQuery>,
) -> AppResult<StatusCode> {
    state.test_plans.delete_plan(&query.path).await?;
    state.activity("test-plan", format!("Deleted test plan {}", query.path));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_test_plan_folder(
    State(state): State<SharedState>,
    Json(body): Json<FolderInput>,
) -> AppResult<StatusCode> {
    state.test_plans.create_folder(&body.path).await?;
    state.activity("test-plan", format!("Created folder {}", body.path));
    Ok(StatusCode::CREATED)
}

pub async fn rename_test_plan_folder(
    State(state): State<SharedState>,
    Json(body): Json<RenameFolderInput>,
) -> AppResult<StatusCode> {
    let path = body.path.clone();
    let name = body.name.clone();
    state.test_plans.rename_folder(body).await?;
    state.activity("test-plan", format!("Renamed folder {path} to {name}"));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_test_plan_folder(
    State(state): State<SharedState>,
    Query(query): Query<PlanPathQuery>,
) -> AppResult<StatusCode> {
    state.test_plans.delete_folder(&query.path).await?;
    state.activity("test-plan", format!("Deleted folder {}", query.path));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn execute_test_plan(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<(StatusCode, Json<ExecutionQueueItem>)> {
    enqueue_execution(state, &id).await
}

pub async fn execute_test_plan_by_path(
    State(state): State<SharedState>,
    Query(query): Query<PlanPathQuery>,
) -> AppResult<(StatusCode, Json<ExecutionQueueItem>)> {
    enqueue_execution(state, &query.path).await
}

async fn enqueue_execution(
    state: SharedState,
    plan_id: &str,
) -> AppResult<(StatusCode, Json<ExecutionQueueItem>)> {
    let item = state.test_plans.enqueue_execution(plan_id).await?;
    state.broadcast(Event::Queue {
        data: Box::new(item.clone()),
    });
    state.activity(
        "test-run",
        format!("Queued \"{}\" for execution", item.plan_name),
    );

    let worker_state = state.clone();
    let queue_id = item.id.clone();
    tokio::spawn(async move {
        run_queued_execution(worker_state, queue_id).await;
    });

    Ok((StatusCode::ACCEPTED, Json(item)))
}

pub async fn list_executions(State(state): State<SharedState>) -> Json<Vec<ExecutionSummary>> {
    Json(state.test_plans.list_executions().await)
}

pub async fn list_execution_queue(
    State(state): State<SharedState>,
) -> Json<Vec<ExecutionQueueItem>> {
    Json(state.test_plans.list_queue().await)
}

pub async fn get_execution(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<StoredExecution>> {
    state.test_plans.get_execution(&id).await.map(Json)
}

pub async fn delete_execution(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    state.test_plans.delete_execution(&id).await?;
    state.activity("test-run", format!("Deleted execution report {id}"));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_all_executions(State(state): State<SharedState>) -> AppResult<StatusCode> {
    let deleted = state.test_plans.delete_all_executions().await?;
    state.activity("test-run", format!("Deleted {deleted} execution reports"));
    Ok(StatusCode::NO_CONTENT)
}

async fn run_queued_execution(state: SharedState, queue_id: String) {
    match state.test_plans.mark_queue_running(&queue_id).await {
        Ok(item) => {
            state.broadcast(Event::Queue {
                data: Box::new(item.clone()),
            });
            state.activity(
                "test-run",
                format!("Started \"{}\" execution", item.plan_name),
            );
        }
        Err(err) => {
            tracing::error!("failed to start queued execution {queue_id}: {err}");
            return;
        }
    }

    match state.test_plans.run_queued_execution(&queue_id).await {
        Ok(item) => {
            state.broadcast(Event::Queue {
                data: Box::new(item.clone()),
            });
            let outcome = match item.status {
                test_plans::QueueStatus::Passed => "passed",
                test_plans::QueueStatus::Failed => "failed",
                test_plans::QueueStatus::Error => "errored",
                test_plans::QueueStatus::Queued | test_plans::QueueStatus::Running => "updated",
            };
            state.activity(
                "test-run",
                format!("Execution \"{}\" {outcome}", item.plan_name),
            );
        }
        Err(err) => {
            tracing::error!("failed to run queued execution {queue_id}: {err}");
        }
    }
}

pub async fn list_tasks(State(state): State<SharedState>) -> Json<Vec<Task>> {
    Json(state.tasks.list().await)
}

pub async fn create_task(
    State(state): State<SharedState>,
    Json(body): Json<NewTask>,
) -> AppResult<(StatusCode, Json<Task>)> {
    let task = state.tasks.create(&body.title).await?;
    state.activity("test-case", format!("Test case \"{}\" created", task.title));
    Ok((StatusCode::CREATED, Json(task)))
}

pub async fn toggle_task(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> AppResult<Json<Task>> {
    let task = state.tasks.toggle(id).await?;
    let verb = if task.done { "completed" } else { "reopened" };
    state.activity("test-case", format!("Test case \"{}\" {verb}", task.title));
    Ok(Json(task))
}

pub async fn delete_task(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> AppResult<StatusCode> {
    let task = state.tasks.delete(id).await?;
    state.activity("test-case", format!("Test case \"{}\" deleted", task.title));
    Ok(StatusCode::NO_CONTENT)
}

/// Always fails — lets the UI demonstrate the whole error pipeline.
/// `?kind=bad-request|not-found|internal` picks the failure mode.
pub async fn error_demo(Query(params): Query<HashMap<String, String>>) -> AppResult<Json<Value>> {
    let kind = params.get("kind").map(String::as_str).unwrap_or("internal");
    Err(match kind {
        "bad-request" => {
            AppError::BadRequest("the request payload failed validation (demo)".into())
        }
        "not-found" => AppError::NotFound("the demo resource does not exist (demo)".into()),
        _ => AppError::Internal("something exploded deep inside the server (demo)".into()),
    })
}
