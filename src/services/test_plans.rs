use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::http::{HeaderMap, HeaderName, HeaderValue, Method};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};

const MAX_BODY_PREVIEW: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPlanInput {
    #[serde(default)]
    pub name: Option<String>,
    pub content: String,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPlan {
    pub name: String,
    pub variables: BTreeMap<String, String>,
    pub requests: Vec<TestPlanRequest>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPlanRequest {
    pub id: usize,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub body: Option<String>,
    pub assertions: Vec<HttpAssertion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpAssertion {
    pub name: String,
    pub kind: AssertionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AssertionKind {
    StatusEquals { expected: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReport {
    pub id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub duration_ms: u128,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<ExecutionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    pub id: usize,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub ok: bool,
    pub duration_ms: u128,
    pub response_bytes: usize,
    pub response_preview: String,
    pub error: Option<String>,
    pub assertions: Vec<AssertionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPlanSummary {
    pub id: String,
    pub name: String,
    pub request_count: usize,
    pub warning_count: usize,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPlan {
    pub id: String,
    pub name: String,
    pub content: String,
    pub parsed: TestPlan,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePlanInput {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    pub id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub duration_ms: u128,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub report_path: String,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredExecution {
    #[serde(flatten)]
    pub summary: ExecutionSummary,
    pub report: ExecutionReport,
    pub log: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlansIndex {
    plans: Vec<StoredPlan>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionsIndex {
    executions: Vec<ExecutionSummary>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QueueStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionQueueItem {
    pub id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub status: QueueStatus,
    pub queued_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub total: Option<usize>,
    pub passed: Option<usize>,
    pub failed: Option<usize>,
    pub error: Option<String>,
    pub report_path: Option<String>,
    pub log_path: Option<String>,
}

pub struct TestPlanStore {
    data_dir: PathBuf,
    plans: RwLock<Vec<StoredPlan>>,
    executions: RwLock<Vec<ExecutionSummary>>,
    queue: RwLock<Vec<ExecutionQueueItem>>,
    counter: AtomicU64,
}

impl TestPlanStore {
    pub async fn open(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        if let Err(err) = tokio::fs::create_dir_all(data_dir.join("plans")).await {
            tracing::warn!("failed to create plan data directory: {err}");
        }
        if let Err(err) = tokio::fs::create_dir_all(data_dir.join("reports")).await {
            tracing::warn!("failed to create report data directory: {err}");
        }

        let plans = load_plans(&data_dir).await;
        let executions = read_json::<ExecutionsIndex>(&executions_index_path(&data_dir))
            .await
            .map(|index| index.executions)
            .unwrap_or_default();
        let max_id = plans
            .iter()
            .filter_map(|plan| id_suffix(&plan.id))
            .chain(
                executions
                    .iter()
                    .filter_map(|execution| id_suffix(&execution.id)),
            )
            .max()
            .unwrap_or(0);

        Self {
            data_dir,
            plans: RwLock::new(plans),
            executions: RwLock::new(executions),
            queue: RwLock::new(Vec::new()),
            counter: AtomicU64::new(max_id + 1),
        }
    }

    pub async fn list_plans(&self) -> Vec<StoredPlanSummary> {
        let mut plans: Vec<_> = self.plans.read().await.iter().map(plan_summary).collect();
        plans.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        plans
    }

    pub async fn get_plan(&self, id: &str) -> AppResult<StoredPlan> {
        self.plans
            .read()
            .await
            .iter()
            .find(|plan| plan.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("test plan {id} does not exist")))
    }

    pub async fn create_plan(&self, input: SavePlanInput) -> AppResult<StoredPlan> {
        let parsed = parse(save_input_to_plan_input(&input))?;
        let now = chrono::Utc::now().timestamp_millis();
        let plan = StoredPlan {
            id: self.next_id("plan"),
            name: parsed.name.clone(),
            content: input.content,
            parsed,
            created_at_ms: now,
            updated_at_ms: now,
        };

        let mut plans = self.plans.write().await;
        plans.push(plan.clone());
        persist_plan(&self.data_dir, &plan).await?;
        Ok(plan)
    }

    pub async fn update_plan(&self, id: &str, input: SavePlanInput) -> AppResult<StoredPlan> {
        let parsed = parse(save_input_to_plan_input(&input))?;
        let mut plans = self.plans.write().await;
        let plan = plans
            .iter_mut()
            .find(|plan| plan.id == id)
            .ok_or_else(|| AppError::NotFound(format!("test plan {id} does not exist")))?;
        plan.name = parsed.name.clone();
        plan.content = input.content;
        plan.parsed = parsed;
        plan.updated_at_ms = chrono::Utc::now().timestamp_millis();
        let saved = plan.clone();
        persist_plan(&self.data_dir, &saved).await?;
        Ok(saved)
    }

    pub async fn delete_plan(&self, id: &str) -> AppResult<()> {
        let mut plans = self.plans.write().await;
        let index = plans
            .iter()
            .position(|plan| plan.id == id)
            .ok_or_else(|| AppError::NotFound(format!("test plan {id} does not exist")))?;
        let plan = plans.remove(index);
        remove_plan_file(&self.data_dir, &plan.id).await
    }

    pub async fn enqueue_execution(&self, plan_id: &str) -> AppResult<ExecutionQueueItem> {
        let plan = self.get_plan(plan_id).await?;
        let item = ExecutionQueueItem {
            id: self.next_id("exec"),
            plan_id: plan.id,
            plan_name: plan.name,
            status: QueueStatus::Queued,
            queued_at_ms: chrono::Utc::now().timestamp_millis(),
            started_at_ms: None,
            finished_at_ms: None,
            total: None,
            passed: None,
            failed: None,
            error: None,
            report_path: None,
            log_path: None,
        };
        let mut queue = self.queue.write().await;
        queue.push(item.clone());
        Ok(item)
    }

    pub async fn list_queue(&self) -> Vec<ExecutionQueueItem> {
        let mut queue: Vec<_> = self
            .queue
            .read()
            .await
            .iter()
            .filter(|item| matches!(item.status, QueueStatus::Queued | QueueStatus::Running))
            .cloned()
            .collect();
        queue.sort_by(|a, b| b.queued_at_ms.cmp(&a.queued_at_ms));
        queue
    }

    pub async fn mark_queue_running(&self, id: &str) -> AppResult<ExecutionQueueItem> {
        let mut queue = self.queue.write().await;
        let item = queue_item_mut(&mut queue, id)?;
        item.status = QueueStatus::Running;
        item.started_at_ms = Some(chrono::Utc::now().timestamp_millis());
        item.error = None;
        let saved = item.clone();
        Ok(saved)
    }

    pub async fn run_queued_execution(&self, id: &str) -> AppResult<ExecutionQueueItem> {
        let item = self
            .queue
            .read()
            .await
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("queued execution {id} does not exist")))?;
        let plan = self.get_plan(&item.plan_id).await?;
        match self.execute_plan_with_id(&plan, item.id.clone()).await {
            Ok(execution) => {
                let summary = execution.summary;
                self.complete_queue_item(
                    &item.id,
                    QueueStatus::from_failures(summary.failed),
                    |item| {
                        item.total = Some(summary.total);
                        item.passed = Some(summary.passed);
                        item.failed = Some(summary.failed);
                        item.report_path = Some(summary.report_path);
                        item.log_path = Some(summary.log_path);
                    },
                )
                .await
            }
            Err(err) => {
                let message = err.to_string();
                self.complete_queue_item(&item.id, QueueStatus::Error, |item| {
                    item.error = Some(message);
                })
                .await
            }
        }
    }

    async fn execute_plan_with_id(
        &self,
        plan: &StoredPlan,
        execution_id: String,
    ) -> AppResult<StoredExecution> {
        let report = run_plan(&plan.id, &plan.parsed, execution_id).await?;
        let log = execution_log(&report);
        let report_path = self
            .data_dir
            .join("reports")
            .join(format!("{}.json", report.id));
        let log_path = self
            .data_dir
            .join("reports")
            .join(format!("{}.log", report.id));
        write_json(&report_path, &report).await?;
        tokio::fs::write(&log_path, &log)
            .await
            .map_err(AppError::from)?;

        let summary = ExecutionSummary {
            id: report.id.clone(),
            plan_id: report.plan_id.clone(),
            plan_name: report.plan_name.clone(),
            started_at_ms: report.started_at_ms,
            finished_at_ms: report.finished_at_ms,
            duration_ms: report.duration_ms,
            total: report.total,
            passed: report.passed,
            failed: report.failed,
            report_path: display_path(&report_path),
            log_path: display_path(&log_path),
        };

        let mut executions = self.executions.write().await;
        executions.push(summary.clone());
        persist_executions(&self.data_dir, &executions).await?;

        Ok(StoredExecution {
            summary,
            report,
            log,
        })
    }

    async fn complete_queue_item(
        &self,
        id: &str,
        status: QueueStatus,
        update: impl FnOnce(&mut ExecutionQueueItem),
    ) -> AppResult<ExecutionQueueItem> {
        let mut queue = self.queue.write().await;
        let item = queue_item_mut(&mut queue, id)?;
        item.status = status;
        item.finished_at_ms = Some(chrono::Utc::now().timestamp_millis());
        update(item);
        let saved = item.clone();
        Ok(saved)
    }

    pub async fn list_executions(&self) -> Vec<ExecutionSummary> {
        let mut executions = self.executions.read().await.clone();
        executions.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
        executions
    }

    pub async fn get_execution(&self, id: &str) -> AppResult<StoredExecution> {
        let summary = self
            .executions
            .read()
            .await
            .iter()
            .find(|execution| execution.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("execution {id} does not exist")))?;
        let report_path = self.data_dir.join("reports").join(format!("{id}.json"));
        let log_path = self.data_dir.join("reports").join(format!("{id}.log"));
        let report = read_json::<ExecutionReport>(&report_path).await?;
        let log = tokio::fs::read_to_string(&log_path)
            .await
            .map_err(AppError::from)?;
        Ok(StoredExecution {
            summary,
            report,
            log,
        })
    }

    pub async fn delete_execution(&self, id: &str) -> AppResult<()> {
        {
            let mut executions = self.executions.write().await;
            let index = executions
                .iter()
                .position(|execution| execution.id == id)
                .ok_or_else(|| AppError::NotFound(format!("execution {id} does not exist")))?;
            executions.remove(index);
            persist_executions(&self.data_dir, &executions).await?;
        }

        {
            let mut queue = self.queue.write().await;
            if let Some(index) = queue.iter().position(|item| item.id == id) {
                queue.remove(index);
            }
        }

        for path in [
            self.data_dir.join("reports").join(format!("{id}.json")),
            self.data_dir.join("reports").join(format!("{id}.log")),
        ] {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(AppError::from(err)),
            }
        }
        Ok(())
    }

    fn next_id(&self, prefix: &str) -> String {
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{}-{next}", chrono::Utc::now().timestamp_millis())
    }
}

impl QueueStatus {
    fn from_failures(failed: usize) -> Self {
        if failed == 0 {
            QueueStatus::Passed
        } else {
            QueueStatus::Failed
        }
    }
}

fn save_input_to_plan_input(input: &SavePlanInput) -> TestPlanInput {
    TestPlanInput {
        name: Some(input.name.clone()),
        content: input.content.clone(),
        variables: input.variables.clone(),
    }
}

fn plan_summary(plan: &StoredPlan) -> StoredPlanSummary {
    StoredPlanSummary {
        id: plan.id.clone(),
        name: plan.name.clone(),
        request_count: plan.parsed.requests.len(),
        warning_count: plan.parsed.warnings.len(),
        created_at_ms: plan.created_at_ms,
        updated_at_ms: plan.updated_at_ms,
    }
}

async fn load_plans(data_dir: &Path) -> Vec<StoredPlan> {
    let mut plans = Vec::new();
    let plans_dir = data_dir.join("plans");
    match tokio::fs::read_dir(&plans_dir).await {
        Ok(mut entries) => loop {
            match entries.next_entry().await {
                Ok(Some(entry)) => {
                    let path = entry.path();
                    if !is_plan_file(&path) {
                        continue;
                    }
                    match read_json::<StoredPlan>(&path).await {
                        Ok(plan) => plans.push(plan),
                        Err(err) => {
                            tracing::warn!("failed to load test plan {}: {err}", path.display());
                        }
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    tracing::warn!("failed to read test plan data directory: {err}");
                    break;
                }
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => tracing::warn!("failed to read test plan data directory: {err}"),
    }

    if plans.is_empty() {
        match read_json::<PlansIndex>(&plans_index_path(data_dir)).await {
            Ok(index) => {
                for plan in &index.plans {
                    if let Err(err) = persist_plan(data_dir, plan).await {
                        tracing::warn!("failed to migrate test plan {}: {err}", plan.id);
                    }
                }
                plans = index.plans;
            }
            Err(AppError::Internal(message)) if message.starts_with("failed to parse ") => {
                tracing::warn!("{message}");
            }
            Err(_) => {}
        }
    }

    plans
}

fn is_plan_file(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("json")
        && path.file_name().and_then(|name| name.to_str()) != Some("index.json")
}

fn plans_index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("plans").join("index.json")
}

fn plan_file_path(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join("plans").join(format!("{id}.json"))
}

fn executions_index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("executions").join("index.json")
}

async fn persist_plan(data_dir: &Path, plan: &StoredPlan) -> AppResult<()> {
    write_json(&plan_file_path(data_dir, &plan.id), plan).await
}

async fn remove_plan_file(data_dir: &Path, id: &str) -> AppResult<()> {
    match tokio::fs::remove_file(plan_file_path(data_dir, id)).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::from(err)),
    }
}

async fn persist_executions(data_dir: &Path, executions: &[ExecutionSummary]) -> AppResult<()> {
    write_json(
        &executions_index_path(data_dir),
        &ExecutionsIndex {
            executions: executions.into(),
        },
    )
    .await
}

fn queue_item_mut<'a>(
    queue: &'a mut [ExecutionQueueItem],
    id: &str,
) -> AppResult<&'a mut ExecutionQueueItem> {
    queue
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::NotFound(format!("queued execution {id} does not exist")))
}

async fn read_json<T>(path: &Path) -> AppResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(AppError::from)?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::Internal(format!("failed to parse {}: {err}", path.display())))
}

async fn write_json<T>(path: &Path, value: &T) -> AppResult<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppError::from)?;
    }
    let raw = serde_json::to_vec_pretty(value)
        .map_err(|err| AppError::Internal(format!("failed to serialize JSON: {err}")))?;
    tokio::fs::write(path, raw).await.map_err(AppError::from)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn id_suffix(id: &str) -> Option<u64> {
    id.rsplit('-').next()?.parse().ok()
}

#[cfg(test)]
fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", chrono::Utc::now().timestamp_millis())
}

fn execution_log(report: &ExecutionReport) -> String {
    let mut lines = vec![
        format!("Execution {}", report.id),
        format!("Plan {} ({})", report.plan_name, report.plan_id),
        format!(
            "Result: {}/{} passed, {} failed in {} ms",
            report.passed, report.total, report.failed, report.duration_ms
        ),
        String::new(),
    ];

    for result in &report.results {
        lines.push(format!(
            "{} {} {} -> {} in {} ms",
            if result.ok { "PASS" } else { "FAIL" },
            result.method,
            result.url,
            result
                .status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "ERR".into()),
            result.duration_ms
        ));
        if let Some(error) = &result.error {
            lines.push(format!("  error: {error}"));
        }
        for assertion in &result.assertions {
            lines.push(format!(
                "  {} {}: {}",
                if assertion.passed { "PASS" } else { "FAIL" },
                assertion.name,
                assertion.message
            ));
        }
    }

    lines.join("\n")
}

pub fn parse(input: TestPlanInput) -> AppResult<TestPlan> {
    let mut variables = BTreeMap::new();
    let mut request_blocks: Vec<Block> = Vec::new();
    let mut current = Block::default();
    let mut pending_name: Option<String> = None;

    for raw_line in input.content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("###") {
            push_block(&mut request_blocks, &mut current, &mut pending_name);
            let name = rest.trim().trim_start_matches('#').trim();
            pending_name = (!name.is_empty()).then(|| name.to_string());
            continue;
        }

        if current.lines.iter().all(|line| line.trim().is_empty()) {
            if let Some((name, value)) = parse_variable(line) {
                current.lines.clear();
                variables.insert(name, value);
                continue;
            }
            if let Some(name) = parse_name_comment(line) {
                current.lines.clear();
                pending_name = Some(name);
                continue;
            }
        }

        current.lines.push(raw_line.to_string());
    }
    push_block(&mut request_blocks, &mut current, &mut pending_name);

    variables.extend(input.variables);
    let mut warnings = Vec::new();
    let mut requests = Vec::new();
    for block in request_blocks {
        if let Some(request) = parse_block(requests.len() + 1, block, &variables, &mut warnings)? {
            requests.push(request);
        }
    }

    if requests.is_empty() {
        return Err(AppError::BadRequest(
            "test plan does not contain any executable requests".into(),
        ));
    }

    Ok(TestPlan {
        name: input
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Uploaded test plan".into()),
        variables,
        requests,
        warnings,
    })
}

#[cfg(test)]
pub async fn execute(input: TestPlanInput) -> AppResult<ExecutionReport> {
    let plan = parse(input)?;
    run_plan("ad-hoc", &plan, new_id("exec")).await
}

async fn run_plan(
    plan_id: &str,
    plan: &TestPlan,
    execution_id: String,
) -> AppResult<ExecutionReport> {
    let started = Instant::now();
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|err| AppError::Internal(format!("failed to create HTTP client: {err}")))?;

    let mut results = Vec::with_capacity(plan.requests.len());
    for request in &plan.requests {
        results.push(execute_one(&client, request).await);
    }

    let finished_at_ms = chrono::Utc::now().timestamp_millis();
    let passed = results.iter().filter(|result| result.ok).count();
    let total = results.len();
    Ok(ExecutionReport {
        id: execution_id,
        plan_id: plan_id.to_string(),
        plan_name: plan.name.clone(),
        started_at_ms,
        finished_at_ms,
        duration_ms: started.elapsed().as_millis(),
        total,
        passed,
        failed: total - passed,
        results,
    })
}

async fn execute_one(client: &reqwest::Client, request: &TestPlanRequest) -> ExecutionResult {
    let started = Instant::now();
    let mut assertion_results = Vec::new();
    let mut status = None;
    let mut response_bytes = 0;
    let mut response_preview = String::new();
    let mut error = None;

    let method = match Method::from_bytes(request.method.as_bytes()) {
        Ok(method) => method,
        Err(err) => {
            return ExecutionResult::failed_before_send(
                request,
                started,
                format!("invalid method: {err}"),
            );
        }
    };

    let mut headers = HeaderMap::new();
    for header in &request.headers {
        match (
            HeaderName::from_bytes(header.name.as_bytes()),
            HeaderValue::from_str(&header.value),
        ) {
            (Ok(name), Ok(value)) => {
                headers.insert(name, value);
            }
            (Err(err), _) => {
                return ExecutionResult::failed_before_send(
                    request,
                    started,
                    format!("invalid header name {}: {err}", header.name),
                );
            }
            (_, Err(err)) => {
                return ExecutionResult::failed_before_send(
                    request,
                    started,
                    format!("invalid value for header {}: {err}", header.name),
                );
            }
        }
    }

    let send_result = client
        .request(method, &request.url)
        .headers(headers)
        .body(request.body.clone().unwrap_or_default())
        .send()
        .await;

    match send_result {
        Ok(response) => {
            let response_status = response.status().as_u16();
            status = Some(response_status);
            match response.bytes().await {
                Ok(bytes) => {
                    response_bytes = bytes.len();
                    response_preview = preview_bytes(&bytes);
                }
                Err(err) => error = Some(format!("failed to read response body: {err}")),
            }

            for assertion in &request.assertions {
                assertion_results.push(evaluate_assertion(assertion, response_status));
            }
        }
        Err(err) => error = Some(err.to_string()),
    }

    let assertions_pass = assertion_results.iter().all(|assertion| assertion.passed);
    let ok = error.is_none() && status.is_some() && assertions_pass;
    ExecutionResult {
        id: request.id,
        name: request.name.clone(),
        method: request.method.clone(),
        url: request.url.clone(),
        status,
        ok,
        duration_ms: started.elapsed().as_millis(),
        response_bytes,
        response_preview,
        error,
        assertions: assertion_results,
    }
}

impl ExecutionResult {
    fn failed_before_send(request: &TestPlanRequest, started: Instant, error: String) -> Self {
        Self {
            id: request.id,
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            status: None,
            ok: false,
            duration_ms: started.elapsed().as_millis(),
            response_bytes: 0,
            response_preview: String::new(),
            error: Some(error),
            assertions: Vec::new(),
        }
    }
}

fn evaluate_assertion(assertion: &HttpAssertion, status: u16) -> AssertionResult {
    match assertion.kind {
        AssertionKind::StatusEquals { expected } => {
            let passed = status == expected;
            AssertionResult {
                name: assertion.name.clone(),
                passed,
                message: if passed {
                    format!("status was {expected}")
                } else {
                    format!("expected status {expected}, got {status}")
                },
            }
        }
    }
}

#[derive(Default)]
struct Block {
    name: Option<String>,
    lines: Vec<String>,
}

fn push_block(blocks: &mut Vec<Block>, current: &mut Block, pending_name: &mut Option<String>) {
    if current.lines.iter().any(|line| !line.trim().is_empty()) {
        current.name = pending_name.take();
        blocks.push(std::mem::take(current));
    } else {
        current.lines.clear();
    }
}

fn parse_block(
    id: usize,
    block: Block,
    variables: &BTreeMap<String, String>,
    warnings: &mut Vec<String>,
) -> AppResult<Option<TestPlanRequest>> {
    let mut request_line_index = None;
    for (index, line) in block.lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("//")
            || trimmed.starts_with('<')
            || trimmed.starts_with('>')
        {
            continue;
        }
        if is_request_line(trimmed) {
            request_line_index = Some(index);
            break;
        }
    }

    let Some(index) = request_line_index else {
        return Ok(None);
    };

    let request_line = substitute(&block.lines[index], variables)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_uppercase();
    let url = parts
        .next()
        .ok_or_else(|| AppError::BadRequest(format!("request {id} is missing a URL")))?;
    validate_url(url)?;

    let mut headers = Vec::new();
    let mut body_lines = Vec::new();
    let mut assertions = Vec::new();
    let mut in_body = false;
    let mut in_response_handler = false;

    for raw_line in block.lines.iter().skip(index + 1) {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('>') {
            in_response_handler = true;
            collect_assertion_line(trimmed, &mut assertions);
            continue;
        }
        if in_response_handler {
            collect_assertion_line(trimmed, &mut assertions);
            continue;
        }
        if trimmed.starts_with('<') {
            warnings.push(format!(
                "Request {id} includes a pre-request script; scripts are not executed yet"
            ));
            continue;
        }
        if !in_body && trimmed.is_empty() {
            in_body = true;
            continue;
        }
        if !in_body {
            if trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }
            let Some((name, value)) = raw_line.split_once(':') else {
                warnings.push(format!("Request {id} ignored malformed header: {trimmed}"));
                continue;
            };
            headers.push(HttpHeader {
                name: name.trim().to_string(),
                value: substitute(value.trim(), variables)?,
            });
        } else {
            body_lines.push(raw_line.as_str());
        }
    }

    let body = if body_lines.is_empty() {
        None
    } else {
        Some(substitute(&body_lines.join("\n"), variables)?)
    };
    if assertions.is_empty() {
        assertions.push(HttpAssertion {
            name: "HTTP status is successful".into(),
            kind: AssertionKind::StatusEquals { expected: 200 },
        });
    }

    Ok(Some(TestPlanRequest {
        id,
        name: block.name.unwrap_or_else(|| format!("{method} {url}")),
        method,
        url: url.to_string(),
        headers,
        body,
        assertions,
    }))
}

fn is_request_line(line: &str) -> bool {
    let Some(method) = line.split_whitespace().next() else {
        return false;
    };
    matches!(
        method.to_ascii_uppercase().as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
    )
}

fn validate_url(value: &str) -> AppResult<()> {
    let url = Url::parse(value)
        .map_err(|err| AppError::BadRequest(format!("invalid request URL {value}: {err}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(format!(
            "unsupported URL scheme {}; only http and https are supported",
            url.scheme()
        )));
    }
    Ok(())
}

fn parse_variable(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('@')?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    (!name.is_empty()).then(|| (name.to_string(), value.trim().to_string()))
}

fn parse_name_comment(line: &str) -> Option<String> {
    let stripped = line
        .strip_prefix("# @name")
        .or_else(|| line.strip_prefix("// @name"))?;
    let name = stripped.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn substitute(value: &str, variables: &BTreeMap<String, String>) -> AppResult<String> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);
        let after_start = &after_start[2..];
        let Some(end) = after_start.find("}}") else {
            return Err(AppError::BadRequest(
                "unterminated variable expression".into(),
            ));
        };
        let key = after_start[..end].trim();
        let Some(replacement) = variables.get(key) else {
            return Err(AppError::BadRequest(format!(
                "unresolved variable {{{{{key}}}}}"
            )));
        };
        output.push_str(replacement);
        rest = &after_start[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

fn collect_assertion_line(line: &str, assertions: &mut Vec<HttpAssertion>) {
    let Some(expected) = parse_status_assertion(line) else {
        return;
    };
    let name = parse_client_test_name(line).unwrap_or_else(|| format!("Status is {expected}"));
    assertions.push(HttpAssertion {
        name,
        kind: AssertionKind::StatusEquals { expected },
    });
}

fn parse_status_assertion(line: &str) -> Option<u16> {
    for operator in ["===", "=="] {
        let pattern = format!("response.status {operator}");
        if let Some(index) = line.find(&pattern) {
            let tail = line[index + pattern.len()..].trim_start();
            let digits: String = tail.chars().take_while(|ch| ch.is_ascii_digit()).collect();
            if let Ok(status) = digits.parse() {
                return Some(status);
            }
        }
    }
    None
}

fn parse_client_test_name(line: &str) -> Option<String> {
    let start = line.find("client.test(")? + "client.test(".len();
    let tail = line[start..].trim_start();
    let quote = tail.chars().next()?;
    if quote != '"' && quote != '\'' && quote != '`' {
        return None;
    }
    let mut name = String::new();
    let mut escaped = false;
    for ch in tail[quote.len_utf8()..].chars() {
        if escaped {
            name.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(name);
        } else {
            name.push(ch);
        }
    }
    None
}

fn preview_bytes(bytes: &[u8]) -> String {
    let end = bytes.len().min(MAX_BODY_PREVIEW);
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::Json;
    use serde_json::json;

    fn input(content: &str) -> TestPlanInput {
        TestPlanInput {
            name: Some("Smoke".into()),
            content: content.into(),
            variables: BTreeMap::new(),
        }
    }

    #[test]
    fn parses_variables_multiple_requests_body_and_assertions() {
        let plan = parse(input(
            r#"
@host = https://example.com

### Create user
POST {{host}}/users
Content-Type: application/json

{"name":"Ada"}
> {% client.test("created", () => client.assert(response.status === 201)); %}

###
# @name Fetch user
GET {{host}}/users/1
"#,
        ))
        .unwrap();

        assert_eq!(plan.name, "Smoke");
        assert_eq!(plan.requests.len(), 2);
        assert_eq!(plan.requests[0].name, "Create user");
        assert_eq!(plan.requests[0].method, "POST");
        assert_eq!(plan.requests[0].url, "https://example.com/users");
        assert_eq!(plan.requests[0].body.as_deref(), Some("{\"name\":\"Ada\"}"));
        assert_eq!(plan.requests[0].assertions[0].name, "created");
        assert_eq!(plan.requests[1].name, "Fetch user");
    }

    #[test]
    fn rejects_unresolved_variables() {
        let err = parse(input("GET https://example.com/{{missing}}")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rejects_empty_plans() {
        let err = parse(input("@host = https://example.com")).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn executes_requests_and_evaluates_status_assertions() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .route("/ok", get(|| async { Json(json!({ "ok": true })) }))
            .route("/created", post(|| async { StatusCode::CREATED }))
            .route("/missing", get(|| async { StatusCode::NOT_FOUND }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let report = execute(input(&format!(
            r#"
### OK
GET http://{addr}/ok

> {{% client.test("ok status", () => client.assert(response.status === 200)); %}}

### Created
POST http://{addr}/created
Content-Type: application/json

{{"name":"Ada"}}
> {{% client.test("created status", () => client.assert(response.status === 201)); %}}

### Expected miss
GET http://{addr}/missing

> {{% client.test("missing status", () => client.assert(response.status === 404)); %}}
"#
        )))
        .await
        .unwrap();

        server.abort();

        assert_eq!(report.total, 3);
        assert_eq!(report.passed, 3);
        assert_eq!(report.failed, 0);
        assert_eq!(report.results[0].status, Some(200));
        assert!(report.results[0].response_preview.contains("\"ok\":true"));
        assert_eq!(report.results[1].status, Some(201));
        assert_eq!(report.results[2].status, Some(404));
        assert!(report.results[2].ok);
    }

    #[tokio::test]
    async fn store_saves_execution_report_and_log_files() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/ok", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-store-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = TestPlanStore::open(&data_dir).await;
        let plan = store
            .create_plan(SavePlanInput {
                name: "Persisted".into(),
                content: format!(
                    "### OK\nGET http://{addr}/ok\n> {{% client.assert(response.status === 200); %}}\n"
                ),
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();
        let queued = store.enqueue_execution(&plan.id).await.unwrap();
        store.mark_queue_running(&queued.id).await.unwrap();
        let execution = store.run_queued_execution(&queued.id).await.unwrap();

        server.abort();

        assert_eq!(execution.status, QueueStatus::Passed);
        assert_eq!(execution.passed, Some(1));
        assert!(data_dir
            .join("reports")
            .join(format!("{}.json", execution.id))
            .exists());
        assert!(data_dir
            .join("reports")
            .join(format!("{}.log", execution.id))
            .exists());
        assert_eq!(store.list_executions().await.len(), 1);
        assert!(store.list_queue().await.is_empty());
    }

    #[tokio::test]
    async fn store_saves_each_test_plan_as_its_own_file() {
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-plan-file-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = TestPlanStore::open(&data_dir).await;
        let plan = store
            .create_plan(SavePlanInput {
                name: "File backed".into(),
                content: "### OK\nGET http://127.0.0.1:8080/ok\n".into(),
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();
        let plan_path = data_dir.join("plans").join(format!("{}.json", plan.id));

        assert!(plan_path.exists());
        assert!(!data_dir.join("plans").join("index.json").exists());

        store
            .update_plan(
                &plan.id,
                SavePlanInput {
                    name: "Updated file backed".into(),
                    content: "### Missing\nGET http://127.0.0.1:8080/missing\n".into(),
                    variables: BTreeMap::new(),
                },
            )
            .await
            .unwrap();
        let saved = read_json::<StoredPlan>(&plan_path).await.unwrap();
        assert_eq!(saved.name, "Updated file backed");

        store.delete_plan(&plan.id).await.unwrap();
        assert!(!plan_path.exists());
    }
}
