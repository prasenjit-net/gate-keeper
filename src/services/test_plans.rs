use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::time::SystemTime;

use axum::http::{HeaderMap, HeaderName, HeaderValue, Method};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::Url;
use rquickjs::{Context, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};
use crate::services::certificates::{CertificateMatch, CertificateStore};

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
    pub pre_request_scripts: Vec<TestPlanScript>,
    pub response_handler_scripts: Vec<TestPlanScript>,
    pub assertions: Vec<HttpAssertion>,
    #[serde(skip)]
    raw_url: String,
    #[serde(skip)]
    raw_headers: Vec<HttpHeader>,
    #[serde(skip)]
    raw_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TestPlanScript {
    Inline { source: String },
    File { path: String },
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
    pub plan_path: String,
    pub plan_name: String,
    pub script: String,
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
    pub logs: Vec<String>,
    pub assertions: Vec<AssertionResult>,
    #[serde(default)]
    pub diagnostics: Vec<ExecutionDiagnostic>,
    #[serde(default)]
    pub mtls: MtlsResult,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtlsResult {
    pub certificate_selected: bool,
    pub certificate_id: Option<String>,
    pub certificate_name: Option<String>,
    pub matched_host_pattern: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDiagnostic {
    pub kind: String,
    pub phase: String,
    pub message: String,
    pub details: Option<String>,
    pub source_preview: Option<String>,
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
    pub path: String,
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
    pub path: String,
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
    pub directory: Option<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInput {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFolderInput {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestPlanBrowser {
    pub path: String,
    pub name: String,
    pub parent: Option<String>,
    pub entries: Vec<TestPlanBrowserEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestPlanBrowserEntry {
    pub name: String,
    pub path: String,
    pub kind: TestPlanBrowserEntryKind,
    pub updated_at_ms: Option<i64>,
    pub request_count: Option<usize>,
    pub warning_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum TestPlanBrowserEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    pub id: String,
    pub plan_id: String,
    pub plan_path: String,
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
    pub plan_path: String,
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
    plan_cache_dirty: Arc<AtomicBool>,
    plan_watcher: Option<RecommendedWatcher>,
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
        let plan_cache_dirty = Arc::new(AtomicBool::new(false));
        let plan_watcher = watch_plan_directory(&data_dir, Arc::clone(&plan_cache_dirty));
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
            plan_cache_dirty,
            plan_watcher,
            counter: AtomicU64::new(max_id + 1),
        }
    }

    pub async fn list_plans(&self) -> Vec<StoredPlanSummary> {
        self.refresh_plans_if_dirty().await;
        let mut plans: Vec<_> = self.plans.read().await.iter().map(plan_summary).collect();
        plans.sort_by_key(|a| std::cmp::Reverse(a.updated_at_ms));
        plans
    }

    pub async fn browse(&self, path: Option<&str>) -> AppResult<TestPlanBrowser> {
        self.refresh_plans_if_dirty().await;
        let path = clean_directory_path(path.unwrap_or_default())?;
        let directory = plan_directory_path(&self.data_dir, &path)?;
        let mut entries = Vec::new();

        let mut read_dir = tokio::fs::read_dir(&directory).await.map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound(format!("test plan directory {path} does not exist"))
            } else {
                AppError::from(err)
            }
        })?;
        while let Some(entry) = read_dir.next_entry().await.map_err(AppError::from)? {
            let file_type = entry.file_type().await.map_err(AppError::from)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let entry_path = join_relative_path(&path, &name);
            if file_type.is_dir() {
                entries.push(TestPlanBrowserEntry {
                    name,
                    path: entry_path,
                    kind: TestPlanBrowserEntryKind::Directory,
                    updated_at_ms: entry_timestamp_ms(&entry).await,
                    request_count: None,
                    warning_count: None,
                });
            } else if file_type.is_file() && name.ends_with(".http") {
                let plan = self
                    .plans
                    .read()
                    .await
                    .iter()
                    .find(|plan| plan.path == entry_path)
                    .cloned();
                entries.push(TestPlanBrowserEntry {
                    name: plan
                        .as_ref()
                        .map(|plan| plan.name.clone())
                        .unwrap_or_else(|| name.clone()),
                    path: entry_path,
                    kind: TestPlanBrowserEntryKind::File,
                    updated_at_ms: plan.as_ref().map(|plan| plan.updated_at_ms),
                    request_count: plan.as_ref().map(|plan| plan.parsed.requests.len()),
                    warning_count: plan.as_ref().map(|plan| plan.parsed.warnings.len()),
                });
            }
        }

        entries.sort_by(|a, b| {
            a.kind.cmp(&b.kind).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        });

        Ok(TestPlanBrowser {
            name: if path.is_empty() {
                "plans".into()
            } else {
                path.rsplit('/').next().unwrap_or("plans").into()
            },
            parent: parent_directory(&path),
            path,
            entries,
        })
    }

    pub async fn get_plan(&self, id: &str) -> AppResult<StoredPlan> {
        self.refresh_plans_if_dirty().await;
        self.plans
            .read()
            .await
            .iter()
            .find(|plan| plan.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("test plan {id} does not exist")))
    }

    pub async fn get_plan_by_path(&self, path: &str) -> AppResult<StoredPlan> {
        let id = clean_plan_path(path)?;
        self.get_plan(&id).await
    }

    pub async fn create_plan(&self, input: SavePlanInput) -> AppResult<StoredPlan> {
        self.refresh_plans_if_dirty().await;
        let now = chrono::Utc::now().timestamp_millis();
        let directory = clean_directory_path(input.directory.as_deref().unwrap_or_default())?;
        let id = self.unique_plan_id(&directory, &input.name).await;
        persist_plan_content(&self.data_dir, &id, &input.content).await?;
        let name = plan_display_name(&id);
        let parsed = parse_for_display(&name, &input.content, &input.variables);
        let plan = StoredPlan {
            id: id.clone(),
            name,
            path: id,
            content: input.content,
            parsed,
            created_at_ms: now,
            updated_at_ms: now,
        };

        let mut plans = self.plans.write().await;
        plans.push(plan.clone());
        Ok(plan)
    }

    pub async fn update_plan(&self, id: &str, input: SavePlanInput) -> AppResult<StoredPlan> {
        self.refresh_plans_if_dirty().await;
        let id = clean_plan_path(id)?;
        let mut plans = self.plans.write().await;
        let index = plans
            .iter_mut()
            .position(|plan| plan.id == id)
            .ok_or_else(|| AppError::NotFound(format!("test plan {id} does not exist")))?;
        let current_id = plans[index].id.clone();
        persist_plan_content(&self.data_dir, &current_id, &input.content).await?;
        let name = plan_display_name(&current_id);
        let parsed = parse_for_display(&name, &input.content, &input.variables);
        plans[index] = StoredPlan {
            id: current_id.clone(),
            name,
            path: current_id,
            content: input.content,
            parsed,
            created_at_ms: plans[index].created_at_ms,
            updated_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        let saved = plans[index].clone();
        Ok(saved)
    }

    pub async fn delete_plan(&self, id: &str) -> AppResult<()> {
        self.refresh_plans_if_dirty().await;
        let id = clean_plan_path(id)?;
        let mut plans = self.plans.write().await;
        let index = plans
            .iter()
            .position(|plan| plan.id == id)
            .ok_or_else(|| AppError::NotFound(format!("test plan {id} does not exist")))?;
        let plan = plans.remove(index);
        remove_plan_file(&self.data_dir, &plan.id).await
    }

    pub async fn create_folder(&self, path: &str) -> AppResult<()> {
        let path = clean_directory_path(path)?;
        if path.is_empty() {
            return Err(AppError::BadRequest("folder path cannot be empty".into()));
        }
        tokio::fs::create_dir(plan_directory_path(&self.data_dir, &path)?)
            .await
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    AppError::BadRequest(format!("folder {path} already exists"))
                } else {
                    AppError::from(err)
                }
            })?;
        self.plan_cache_dirty.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn rename_folder(&self, input: RenameFolderInput) -> AppResult<()> {
        let path = clean_directory_path(&input.path)?;
        if path.is_empty() {
            return Err(AppError::BadRequest("root folder cannot be renamed".into()));
        }
        let new_name = clean_path_segment(&input.name)?;
        let parent = parent_directory(&path).unwrap_or_default();
        let next_path = join_relative_path(&parent, &new_name);
        tokio::fs::rename(
            plan_directory_path(&self.data_dir, &path)?,
            plan_directory_path(&self.data_dir, &next_path)?,
        )
        .await
        .map_err(AppError::from)?;
        self.plan_cache_dirty.store(true, Ordering::Release);
        self.refresh_plans_if_dirty().await;
        Ok(())
    }

    pub async fn delete_folder(&self, path: &str) -> AppResult<()> {
        let path = clean_directory_path(path)?;
        if path.is_empty() {
            return Err(AppError::BadRequest("root folder cannot be deleted".into()));
        }
        tokio::fs::remove_dir(plan_directory_path(&self.data_dir, &path)?)
            .await
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::DirectoryNotEmpty {
                    AppError::BadRequest(format!("folder {path} is not empty"))
                } else {
                    AppError::from(err)
                }
            })?;
        self.plan_cache_dirty.store(true, Ordering::Release);
        self.refresh_plans_if_dirty().await;
        Ok(())
    }

    pub async fn enqueue_execution(&self, plan_id: &str) -> AppResult<ExecutionQueueItem> {
        let plan = self.get_plan(plan_id).await?;
        let item = ExecutionQueueItem {
            id: self.next_id("exec"),
            plan_id: plan.id.clone(),
            plan_path: plan.path,
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
        queue.sort_by_key(|a| std::cmp::Reverse(a.queued_at_ms));
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

    pub async fn run_queued_execution(
        &self,
        id: &str,
        certificates: &CertificateStore,
    ) -> AppResult<ExecutionQueueItem> {
        let item = self
            .queue
            .read()
            .await
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("queued execution {id} does not exist")))?;
        let plan = self.get_plan(&item.plan_id).await?;
        match self
            .execute_plan_with_id(&plan, item.id.clone(), certificates)
            .await
        {
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
        certificates: &CertificateStore,
    ) -> AppResult<StoredExecution> {
        let script = plan.content.clone();
        let parsed = parse(TestPlanInput {
            name: Some(plan.name.clone()),
            content: script.clone(),
            variables: BTreeMap::new(),
        })?;
        let script_base_dir = self.data_dir.join("plans");
        let report = run_plan(
            &plan.id,
            &parsed,
            execution_id,
            script,
            Some(&script_base_dir),
            Some(certificates),
        )
        .await?;
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
            plan_path: report.plan_path.clone(),
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
        executions.sort_by_key(|a| std::cmp::Reverse(a.started_at_ms));
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

    pub async fn delete_all_executions(&self) -> AppResult<usize> {
        let deleted = {
            let mut executions = self.executions.write().await;
            let deleted = executions.len();
            executions.clear();
            persist_executions(&self.data_dir, &executions).await?;
            deleted
        };

        self.queue.write().await.clear();

        let reports_dir = self.data_dir.join("reports");
        match tokio::fs::read_dir(&reports_dir).await {
            Ok(mut entries) => {
                while let Some(entry) = entries.next_entry().await.map_err(AppError::from)? {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let extension = path.extension().and_then(|extension| extension.to_str());
                    if matches!(extension, Some("json") | Some("log")) {
                        match tokio::fs::remove_file(&path).await {
                            Ok(()) => {}
                            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                            Err(err) => return Err(AppError::from(err)),
                        }
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(AppError::from(err)),
        }

        Ok(deleted)
    }

    fn next_id(&self, prefix: &str) -> String {
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{}-{next}", chrono::Utc::now().timestamp_millis())
    }

    async fn refresh_plans_if_dirty(&self) {
        if self.plan_cache_dirty.swap(false, Ordering::AcqRel) || self.plan_watcher.is_none() {
            let plans = load_plans(&self.data_dir).await;
            *self.plans.write().await = plans;
        }
    }

    async fn unique_plan_id(&self, directory: &str, name: &str) -> String {
        let plans = self.plans.read().await;
        unique_plan_id_for(&self.data_dir, &plans, directory, name, &self.counter)
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

fn plan_summary(plan: &StoredPlan) -> StoredPlanSummary {
    StoredPlanSummary {
        id: plan.id.clone(),
        name: plan.name.clone(),
        path: plan.path.clone(),
        request_count: plan.parsed.requests.len(),
        warning_count: plan.parsed.warnings.len(),
        created_at_ms: plan.created_at_ms,
        updated_at_ms: plan.updated_at_ms,
    }
}

fn parse_for_display(name: &str, content: &str, variables: &BTreeMap<String, String>) -> TestPlan {
    match parse(TestPlanInput {
        name: Some(name.to_string()),
        content: content.to_string(),
        variables: variables.clone(),
    }) {
        Ok(plan) => plan,
        Err(err) => TestPlan {
            name: name.to_string(),
            variables: variables.clone(),
            requests: Vec::new(),
            warnings: vec![err.to_string()],
        },
    }
}

fn clean_plan_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Untitled test plan".into()
    } else {
        trimmed.into()
    }
}

fn plan_display_name(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path);
    clean_plan_name(&stem.replace(['-', '_'], " "))
}

fn slugify_plan_name(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in clean_plan_name(name).chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('-');
            last_was_separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "untitled-test-plan".into()
    } else {
        slug
    }
}

fn clean_path_segment(segment: &str) -> AppResult<String> {
    let segment = segment.trim();
    if segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment.contains('/')
        || segment.contains('\\')
    {
        return Err(AppError::BadRequest("invalid path segment".into()));
    }
    Ok(segment.to_string())
}

fn clean_directory_path(path: &str) -> AppResult<String> {
    clean_relative_path(path, false)
}

fn clean_plan_path(path: &str) -> AppResult<String> {
    let cleaned = clean_relative_path(path, true)?;
    if !cleaned.ends_with(".http") {
        return Err(AppError::BadRequest(
            "test plan path must end with .http".into(),
        ));
    }
    Ok(cleaned)
}

fn clean_relative_path(path: &str, require_file: bool) -> AppResult<String> {
    let path = path.trim().trim_matches('/');
    if path.is_empty() {
        if require_file {
            return Err(AppError::BadRequest(
                "test plan path cannot be empty".into(),
            ));
        }
        return Ok(String::new());
    }

    let raw = Path::new(path);
    if raw.is_absolute() {
        return Err(AppError::BadRequest(
            "absolute paths are not allowed".into(),
        ));
    }

    let mut parts = Vec::new();
    for component in raw.components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| AppError::BadRequest("path must be valid UTF-8".into()))?;
                parts.push(clean_path_segment(part)?);
            }
            _ => return Err(AppError::BadRequest("path traversal is not allowed".into())),
        }
    }

    if require_file && parts.is_empty() {
        return Err(AppError::BadRequest(
            "test plan path cannot be empty".into(),
        ));
    }

    Ok(parts.join("/"))
}

fn join_relative_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}

fn parent_directory(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .or_else(|| Some(String::new()))
}

fn plan_directory_path(data_dir: &Path, path: &str) -> AppResult<PathBuf> {
    Ok(data_dir.join("plans").join(clean_directory_path(path)?))
}

async fn entry_timestamp_ms(entry: &tokio::fs::DirEntry) -> Option<i64> {
    entry
        .metadata()
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_timestamp_ms)
}

fn plan_id_exists(plans: &[StoredPlan], id: &str, excluding: Option<&str>) -> bool {
    plans
        .iter()
        .any(|plan| Some(plan.id.as_str()) != excluding && plan.id == id)
}

fn unique_plan_id_for(
    data_dir: &Path,
    plans: &[StoredPlan],
    directory: &str,
    name: &str,
    counter: &AtomicU64,
) -> String {
    let base_name = format!("{}.http", slugify_plan_name(name));
    let base = join_relative_path(directory, &base_name);
    if !plan_id_exists(plans, &base, None)
        && plan_file_path(data_dir, &base)
            .map(|path| !path.exists())
            .unwrap_or(false)
    {
        return base;
    }

    loop {
        let candidate_name = format!(
            "{}-{}.http",
            slugify_plan_name(name),
            counter.fetch_add(1, Ordering::Relaxed)
        );
        let candidate = join_relative_path(directory, &candidate_name);
        if !plan_id_exists(plans, &candidate, None)
            && plan_file_path(data_dir, &candidate)
                .map(|path| !path.exists())
                .unwrap_or(false)
        {
            return candidate;
        }
    }
}

fn system_time_to_timestamp_ms(time: SystemTime) -> Option<i64> {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

async fn load_plans(data_dir: &Path) -> Vec<StoredPlan> {
    let mut plans = Vec::new();
    let plans_dir = data_dir.join("plans");
    let mut directories = vec![PathBuf::new()];

    while let Some(relative_dir) = directories.pop() {
        let absolute_dir = plans_dir.join(&relative_dir);
        match tokio::fs::read_dir(&absolute_dir).await {
            Ok(mut entries) => loop {
                match entries.next_entry().await {
                    Ok(Some(entry)) => {
                        let path = entry.path();
                        let file_name = entry.file_name();
                        if file_name.to_string_lossy().starts_with('.') {
                            continue;
                        }
                        match entry.file_type().await {
                            Ok(file_type) if file_type.is_dir() => {
                                directories.push(relative_dir.join(file_name));
                            }
                            Ok(file_type) if file_type.is_file() && is_plan_file(&path) => {
                                let relative_path = relative_dir.join(file_name);
                                match load_plan_file(&plans_dir, &relative_path).await {
                                    Ok(plan) => plans.push(plan),
                                    Err(err) => {
                                        tracing::warn!(
                                            "failed to load test plan {}: {err}",
                                            path.display()
                                        );
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(err) => tracing::warn!("failed to inspect test plan file: {err}"),
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
    }

    plans
}

fn watch_plan_directory(data_dir: &Path, dirty: Arc<AtomicBool>) -> Option<RecommendedWatcher> {
    let plans_dir = data_dir.join("plans");
    let watcher =
        notify::recommended_watcher(move |result: notify::Result<notify::Event>| match result {
            Ok(event) => {
                if is_plan_change_event(&event.kind)
                    && event.paths.iter().any(|path| {
                        path.extension().and_then(|extension| extension.to_str()) == Some("http")
                            || path.file_name().and_then(|name| name.to_str()) == Some("plans")
                    })
                {
                    dirty.store(true, Ordering::Release);
                }
            }
            Err(err) => tracing::warn!("failed to watch test plan directory: {err}"),
        });
    match watcher {
        Ok(mut watcher) => match watcher.watch(&plans_dir, RecursiveMode::Recursive) {
            Ok(()) => Some(watcher),
            Err(err) => {
                tracing::warn!(
                    "failed to watch test plan directory {}: {err}",
                    plans_dir.display()
                );
                None
            }
        },
        Err(err) => {
            tracing::warn!("failed to initialize test plan directory watcher: {err}");
            None
        }
    }
}

fn is_plan_change_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Other
    )
}

fn is_plan_file(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("http")
}

fn plan_file_path(data_dir: &Path, path: &str) -> AppResult<PathBuf> {
    Ok(data_dir.join("plans").join(clean_plan_path(path)?))
}

fn executions_index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("executions").join("index.json")
}

async fn load_plan_file(root: &Path, relative_path: &Path) -> AppResult<StoredPlan> {
    let path = root.join(relative_path);
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(AppError::from)?;
    let metadata = tokio::fs::metadata(&path).await.map_err(AppError::from)?;
    let updated_at_ms = metadata
        .modified()
        .ok()
        .and_then(system_time_to_timestamp_ms)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let created_at_ms = metadata
        .created()
        .ok()
        .and_then(system_time_to_timestamp_ms)
        .unwrap_or(updated_at_ms);
    let id = relative_path
        .to_str()
        .ok_or_else(|| AppError::Internal(format!("invalid test plan path {}", path.display())))?
        .replace('\\', "/")
        .to_string();
    let name = plan_display_name(&id);
    let parsed = parse_for_display(&name, &content, &BTreeMap::new());

    Ok(StoredPlan {
        id: id.clone(),
        name,
        path: id,
        content,
        parsed,
        created_at_ms,
        updated_at_ms,
    })
}

async fn persist_plan_content(data_dir: &Path, id: &str, content: &str) -> AppResult<()> {
    let path = plan_file_path(data_dir, id)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppError::from)?;
    }
    tokio::fs::write(path, content)
        .await
        .map_err(AppError::from)
}

async fn remove_plan_file(data_dir: &Path, id: &str) -> AppResult<()> {
    match tokio::fs::remove_file(plan_file_path(data_dir, id)?).await {
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

fn one_line(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
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
        if let Some(message) = &result.mtls.message {
            lines.push(format!("  {message}"));
        }
        if result.mtls.certificate_selected {
            lines.push(format!(
                "  mtls certificate: {} ({})",
                result.mtls.certificate_name.as_deref().unwrap_or("unknown"),
                result
                    .mtls
                    .matched_host_pattern
                    .as_deref()
                    .unwrap_or("unknown host pattern")
            ));
        }
        for diagnostic in &result.diagnostics {
            lines.push(format!(
                "  diagnostic [{}:{}]: {}",
                diagnostic.kind, diagnostic.phase, diagnostic.message
            ));
            if let Some(details) = &diagnostic.details {
                lines.push(format!("    details: {}", one_line(details)));
            }
            if let Some(source_preview) = &diagnostic.source_preview {
                lines.push("    script:".into());
                for line in source_preview.lines() {
                    lines.push(format!("      {line}"));
                }
            }
        }
        for log in &result.logs {
            lines.push(format!("  log: {log}"));
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
    let script = input.content.clone();
    let plan = parse(input)?;
    run_plan("ad-hoc", &plan, new_id("exec"), script, None, None).await
}

async fn run_plan(
    plan_id: &str,
    plan: &TestPlan,
    execution_id: String,
    script: String,
    script_base_dir: Option<&Path>,
    certificates: Option<&CertificateStore>,
) -> AppResult<ExecutionReport> {
    let started = Instant::now();
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let mut clients = HttpClientPool::new(certificates)?;

    let mut globals = BTreeMap::new();
    let mut results = Vec::with_capacity(plan.requests.len());
    for request in &plan.requests {
        results.push(
            execute_one(
                request,
                &plan.variables,
                &mut globals,
                script_base_dir,
                &mut clients,
            )
            .await,
        );
    }

    let finished_at_ms = chrono::Utc::now().timestamp_millis();
    let passed = results.iter().filter(|result| result.ok).count();
    let total = results.len();
    Ok(ExecutionReport {
        id: execution_id,
        plan_id: plan_id.to_string(),
        plan_path: plan_id.to_string(),
        plan_name: plan.name.clone(),
        script,
        started_at_ms,
        finished_at_ms,
        duration_ms: started.elapsed().as_millis(),
        total,
        passed,
        failed: total - passed,
        results,
    })
}

struct HttpClientPool<'a> {
    certificates: Option<&'a CertificateStore>,
    default: reqwest::Client,
    mtls: BTreeMap<String, reqwest::Client>,
}

impl<'a> HttpClientPool<'a> {
    fn new(certificates: Option<&'a CertificateStore>) -> AppResult<Self> {
        Ok(Self {
            certificates,
            default: build_http_client(None).map_err(AppError::Internal)?,
            mtls: BTreeMap::new(),
        })
    }

    async fn client_for(&mut self, url: &str) -> Result<(reqwest::Client, MtlsResult), String> {
        let Ok(parsed) = Url::parse(url) else {
            return Ok((
                self.default.clone(),
                MtlsResult {
                    certificate_selected: false,
                    message: Some("mTLS skipped because request URL could not be parsed".into()),
                    ..MtlsResult::default()
                },
            ));
        };

        if parsed.scheme() != "https" {
            return Ok((
                self.default.clone(),
                MtlsResult {
                    certificate_selected: false,
                    message: Some("mTLS skipped for non-HTTPS URL".into()),
                    ..MtlsResult::default()
                },
            ));
        }

        let Some(host) = parsed.host_str() else {
            return Ok((
                self.default.clone(),
                MtlsResult {
                    certificate_selected: false,
                    message: Some("mTLS skipped because HTTPS URL has no hostname".into()),
                    ..MtlsResult::default()
                },
            ));
        };

        let Some(certificates) = self.certificates else {
            return Ok((
                self.default.clone(),
                MtlsResult {
                    certificate_selected: false,
                    message: Some("mTLS skipped because no certificate store is configured".into()),
                    ..MtlsResult::default()
                },
            ));
        };

        let Some(matched) = certificates.match_host(host).await else {
            return Ok((
                self.default.clone(),
                MtlsResult {
                    certificate_selected: false,
                    message: Some(format!(
                        "mTLS: no configured client certificate matched {host}"
                    )),
                    ..MtlsResult::default()
                },
            ));
        };

        if !self.mtls.contains_key(&matched.id) {
            let identity = certificates.identity(&matched.id).await.map_err(|err| {
                format!("mTLS: failed to load certificate {}: {err}", matched.name)
            })?;
            let client = build_http_client(Some(identity)).map_err(|err| {
                format!("mTLS: failed to create client for {}: {err}", matched.name)
            })?;
            self.mtls.insert(matched.id.clone(), client);
        }

        let message = format!(
            "mTLS: https host {host} matched {} using {}",
            matched.matched_host_pattern, matched.name
        );
        Ok((
            self.mtls
                .get(&matched.id)
                .expect("mTLS client must be inserted before use")
                .clone(),
            mtls_result(matched, message),
        ))
    }
}

fn build_http_client(identity: Option<reqwest::Identity>) -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10));
    let builder = if let Some(identity) = identity {
        builder.identity(identity)
    } else {
        builder
    };
    builder.build().map_err(|err| err.to_string())
}

fn mtls_result(matched: CertificateMatch, message: String) -> MtlsResult {
    MtlsResult {
        certificate_selected: true,
        certificate_id: Some(matched.id),
        certificate_name: Some(matched.name),
        matched_host_pattern: Some(matched.matched_host_pattern),
        message: Some(message),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutableRequest {
    method: String,
    url: String,
    headers: Vec<HttpHeader>,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScriptResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    content_type: String,
    body: Value,
    body_text: String,
    duration_ms: u128,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScriptOutcome {
    #[serde(default)]
    tests: Vec<AssertionResult>,
    #[serde(default)]
    logs: Vec<String>,
    #[serde(default)]
    errors: Vec<ScriptRuntimeError>,
    #[serde(default)]
    globals: BTreeMap<String, Value>,
    #[serde(default)]
    request_variables: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScriptRuntimeError {
    name: Option<String>,
    message: String,
    stack: Option<String>,
}

impl ExecutionDiagnostic {
    fn runtime(phase: &str, message: String, source: Option<&str>) -> Self {
        Self {
            kind: "script".into(),
            phase: phase.into(),
            message,
            details: None,
            source_preview: source.map(script_preview),
        }
    }
}

fn script_diagnostic(phase: &str, source: &str, message: &str) -> ExecutionDiagnostic {
    ExecutionDiagnostic::runtime(phase, message.to_string(), Some(source))
}

fn script_diagnostics(
    phase: &str,
    source: &str,
    errors: &[ScriptRuntimeError],
) -> Vec<ExecutionDiagnostic> {
    errors
        .iter()
        .map(|error| ExecutionDiagnostic {
            kind: error.name.clone().unwrap_or_else(|| "script".into()),
            phase: phase.into(),
            message: error.message.clone(),
            details: error.stack.clone(),
            source_preview: Some(script_preview(source)),
        })
        .collect()
}

fn format_script_error(phase: &str, errors: &[ScriptRuntimeError]) -> String {
    errors
        .first()
        .map(|error| format!("{phase} script failed: {}", error.message))
        .unwrap_or_else(|| format!("{phase} script failed"))
}

fn script_preview(source: &str) -> String {
    const MAX_LINES: usize = 12;
    let lines = source.lines().collect::<Vec<_>>();
    let mut preview = lines
        .iter()
        .take(MAX_LINES)
        .enumerate()
        .map(|(index, line)| format!("{:>3}: {}", index + 1, line))
        .collect::<Vec<_>>();
    if lines.len() > MAX_LINES {
        preview.push("  ...".into());
    }
    preview.join("\n")
}

async fn execute_one(
    request: &TestPlanRequest,
    file_variables: &BTreeMap<String, String>,
    globals: &mut BTreeMap<String, Value>,
    script_base_dir: Option<&Path>,
    clients: &mut HttpClientPool<'_>,
) -> ExecutionResult {
    let started = Instant::now();
    let mut assertion_results = Vec::new();
    let mut logs = Vec::new();
    let mut status = None;
    let mut response_bytes = 0;
    let mut response_preview = String::new();
    let mut error = None;
    let mut request_variables = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut mtls = MtlsResult::default();

    let mut executable_request = ExecutableRequest {
        method: request.method.clone(),
        url: request.raw_url.clone(),
        headers: request.raw_headers.clone(),
        body: request.raw_body.clone(),
    };

    for script in &request.pre_request_scripts {
        match resolve_script(script, script_base_dir).await {
            Ok(source) => {
                match run_js_script(
                    "pre-request",
                    &source,
                    ScriptExecutionContext {
                        plan_request: request,
                        executable_request: &executable_request,
                        response: None,
                        file_variables,
                    },
                    globals,
                    &mut request_variables,
                ) {
                    Ok(outcome) => {
                        logs.extend(outcome.logs);
                        assertion_results.extend(outcome.tests);
                        if !outcome.errors.is_empty() {
                            diagnostics.extend(script_diagnostics(
                                "pre-request",
                                &source,
                                &outcome.errors,
                            ));
                            error = Some(format_script_error("pre-request", &outcome.errors));
                            break;
                        }
                    }
                    Err(err) => {
                        diagnostics.push(script_diagnostic("pre-request", &source, &err));
                        error = Some(err);
                        break;
                    }
                }
            }
            Err(err) => {
                diagnostics.push(ExecutionDiagnostic::runtime(
                    "pre-request",
                    err.clone(),
                    None,
                ));
                error = Some(err);
                break;
            }
        }
    }

    if error.is_none() {
        match resolve_request(
            &executable_request,
            file_variables,
            globals,
            &request_variables,
        ) {
            Ok(resolved) => executable_request = resolved,
            Err(err) => error = Some(err.to_string()),
        }
    }

    if let Some(error) = error {
        return ExecutionResult {
            id: request.id,
            name: request.name.clone(),
            method: executable_request.method,
            url: executable_request.url,
            status: None,
            ok: false,
            duration_ms: started.elapsed().as_millis(),
            response_bytes,
            response_preview,
            error: Some(error),
            logs,
            assertions: assertion_results,
            diagnostics,
            mtls,
        };
    }

    let method = match Method::from_bytes(executable_request.method.as_bytes()) {
        Ok(method) => method,
        Err(err) => {
            return ExecutionResult::failed_before_send(
                request,
                started,
                executable_request,
                ExecutionArtifacts {
                    logs,
                    assertions: assertion_results,
                    diagnostics,
                    mtls,
                },
                format!("invalid method: {err}"),
            );
        }
    };

    let mut headers = HeaderMap::new();
    for header in &executable_request.headers {
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
                    executable_request.clone(),
                    ExecutionArtifacts {
                        logs,
                        assertions: assertion_results,
                        diagnostics,
                        mtls,
                    },
                    format!("invalid header name {}: {err}", header.name),
                );
            }
            (_, Err(err)) => {
                return ExecutionResult::failed_before_send(
                    request,
                    started,
                    executable_request.clone(),
                    ExecutionArtifacts {
                        logs,
                        assertions: assertion_results,
                        diagnostics,
                        mtls,
                    },
                    format!("invalid value for header {}: {err}", header.name),
                );
            }
        }
    }

    let client = match clients.client_for(&executable_request.url).await {
        Ok((client, selection)) => {
            mtls = selection;
            if let Some(message) = &mtls.message {
                tracing::debug!("{message}");
            }
            client
        }
        Err(err) => {
            return ExecutionResult::failed_before_send(
                request,
                started,
                executable_request,
                ExecutionArtifacts {
                    logs,
                    assertions: assertion_results,
                    diagnostics,
                    mtls,
                },
                err,
            );
        }
    };

    let send_result = client
        .request(method, &executable_request.url)
        .headers(headers)
        .body(executable_request.body.clone().unwrap_or_default())
        .send()
        .await;

    match send_result {
        Ok(response) => {
            let response_status = response.status().as_u16();
            let response_headers = response_headers_to_map(response.headers());
            let response_content_type = response_headers
                .get("content-type")
                .cloned()
                .unwrap_or_default();
            status = Some(response_status);
            match response.bytes().await {
                Ok(bytes) => {
                    response_bytes = bytes.len();
                    response_preview = preview_bytes(&bytes);
                    let response_body = response_body_value(&bytes);
                    let script_response = ScriptResponse {
                        status: response_status,
                        headers: response_headers,
                        content_type: response_content_type,
                        body: response_body,
                        body_text: String::from_utf8_lossy(&bytes).to_string(),
                        duration_ms: started.elapsed().as_millis(),
                    };
                    for script in &request.response_handler_scripts {
                        match resolve_script(script, script_base_dir).await {
                            Ok(source) => {
                                match run_js_script(
                                    "response handler",
                                    &source,
                                    ScriptExecutionContext {
                                        plan_request: request,
                                        executable_request: &executable_request,
                                        response: Some(&script_response),
                                        file_variables,
                                    },
                                    globals,
                                    &mut request_variables,
                                ) {
                                    Ok(outcome) => {
                                        logs.extend(outcome.logs);
                                        assertion_results.extend(outcome.tests);
                                        if !outcome.errors.is_empty() {
                                            diagnostics.extend(script_diagnostics(
                                                "response handler",
                                                &source,
                                                &outcome.errors,
                                            ));
                                            error = Some(format_script_error(
                                                "response handler",
                                                &outcome.errors,
                                            ));
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        diagnostics.push(script_diagnostic(
                                            "response handler",
                                            &source,
                                            &err,
                                        ));
                                        error = Some(err);
                                        break;
                                    }
                                }
                            }
                            Err(err) => {
                                diagnostics.push(ExecutionDiagnostic::runtime(
                                    "response handler",
                                    err.clone(),
                                    None,
                                ));
                                error = Some(err);
                                break;
                            }
                        }
                    }
                }
                Err(err) => error = Some(format!("failed to read response body: {err}")),
            }

            if request.response_handler_scripts.is_empty() {
                for assertion in &request.assertions {
                    assertion_results.push(evaluate_assertion(assertion, response_status));
                }
            }
        }
        Err(err) => error = Some(err.to_string()),
    }

    let assertions_pass = assertion_results.iter().all(|assertion| assertion.passed);
    let ok = error.is_none() && status.is_some() && assertions_pass;
    ExecutionResult {
        id: request.id,
        name: request.name.clone(),
        method: executable_request.method,
        url: executable_request.url,
        status,
        ok,
        duration_ms: started.elapsed().as_millis(),
        response_bytes,
        response_preview,
        error,
        logs,
        assertions: assertion_results,
        diagnostics,
        mtls,
    }
}

impl ExecutionResult {
    fn failed_before_send(
        request: &TestPlanRequest,
        started: Instant,
        executable_request: ExecutableRequest,
        artifacts: ExecutionArtifacts,
        error: String,
    ) -> Self {
        Self {
            id: request.id,
            name: request.name.clone(),
            method: executable_request.method,
            url: executable_request.url,
            status: None,
            ok: false,
            duration_ms: started.elapsed().as_millis(),
            response_bytes: 0,
            response_preview: String::new(),
            error: Some(error),
            logs: artifacts.logs,
            assertions: artifacts.assertions,
            diagnostics: artifacts.diagnostics,
            mtls: artifacts.mtls,
        }
    }
}

struct ExecutionArtifacts {
    logs: Vec<String>,
    assertions: Vec<AssertionResult>,
    diagnostics: Vec<ExecutionDiagnostic>,
    mtls: MtlsResult,
}

fn run_js_script(
    phase: &str,
    source: &str,
    context: ScriptExecutionContext<'_>,
    globals: &mut BTreeMap<String, Value>,
    request_variables: &mut BTreeMap<String, Value>,
) -> Result<ScriptOutcome, String> {
    reject_unsupported_script_features(source)?;
    let runtime = Runtime::new().map_err(|err| format!("failed to create JS runtime: {err}"))?;
    runtime.set_memory_limit(8 * 1024 * 1024);
    runtime.set_max_stack_size(256 * 1024);
    let js_context =
        Context::full(&runtime).map_err(|err| format!("failed to create JS context: {err}"))?;

    js_context.with(|ctx| {
        let source = build_script_source(phase, source, context, globals, request_variables)?;
        let raw = ctx
            .eval::<String, _>(source)
            .map_err(|err| format!("{phase} script failed: {err}"))?;
        let outcome = serde_json::from_str::<ScriptOutcome>(&raw)
            .map_err(|err| format!("{phase} script returned invalid state: {err}"))?;
        *globals = outcome.globals.clone();
        *request_variables = outcome.request_variables.clone();
        Ok(outcome)
    })
}

struct ScriptExecutionContext<'a> {
    plan_request: &'a TestPlanRequest,
    executable_request: &'a ExecutableRequest,
    response: Option<&'a ScriptResponse>,
    file_variables: &'a BTreeMap<String, String>,
}

fn reject_unsupported_script_features(source: &str) -> Result<(), String> {
    let trimmed = source.trim_start();
    if trimmed.starts_with("import ") || trimmed.contains("\nimport ") {
        return Err("ES module imports are not supported yet".into());
    }
    if source.contains("crypto.") {
        return Err("JetBrains Crypto API is not supported yet".into());
    }
    Ok(())
}

fn build_script_source(
    phase: &str,
    user_source: &str,
    context: ScriptExecutionContext<'_>,
    globals: &BTreeMap<String, Value>,
    request_variables: &BTreeMap<String, Value>,
) -> Result<String, String> {
    let request_json = serde_json::to_string(&json!({
        "id": context.plan_request.id,
        "name": context.plan_request.name,
        "method": context.executable_request.method,
        "url": context.executable_request.url,
        "headers": headers_to_map(&context.executable_request.headers),
        "body": context.executable_request.body,
    }))
    .map_err(|err| err.to_string())?;
    let response_json = serde_json::to_string(&context.response).map_err(|err| err.to_string())?;
    let globals_json = serde_json::to_string(globals).map_err(|err| err.to_string())?;
    let file_variables_json =
        serde_json::to_string(context.file_variables).map_err(|err| err.to_string())?;
    let request_variables_json =
        serde_json::to_string(request_variables).map_err(|err| err.to_string())?;
    let source = substitute_runtime(
        user_source,
        context.file_variables,
        globals,
        request_variables,
    )
    .map_err(|err| err.to_string())?;
    let source_json = serde_json::to_string(&source).map_err(|err| err.to_string())?;
    let phase_json = serde_json::to_string(phase).map_err(|err| err.to_string())?;

    Ok(format!(
        r#"
(() => {{
  const __gk = {{
    tests: [],
    logs: [],
    errors: [],
    fileVariables: {file_variables_json},
    globals: {globals_json},
    requestVariables: {request_variables_json}
  }};
  const __requestBase = {request_json};
  const __responseBase = {response_json};
  const __phase = {phase_json};
  const __source = {source_json};
  const __stringify = (value) => {{
    if (value === undefined) return "undefined";
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.stack || value.message || String(value);
    try {{ return JSON.stringify(value); }} catch (_) {{ return String(value); }}
  }};
  const __message = (error) => error && error.message ? String(error.message) : String(error);
  const __scriptError = (error) => ({{
    name: error && error.name ? String(error.name) : "Error",
    message: __message(error),
    stack: error && error.stack ? String(error.stack) : null
  }});
  const __captureLog = (level, values) => {{
    const message = values.map(__stringify).join(" ");
    __gk.logs.push(level === "log" ? message : `[${{level}}] ${{message}}`);
  }};
  const __scope = (bag) => ({{
    get(name) {{
      const value = bag[String(name)];
      return value === undefined ? null : value;
    }},
    set(name, value) {{
      bag[String(name)] = value;
    }},
    clear(name) {{
      delete bag[String(name)];
    }}
  }});
  const __globalScope = __scope(__gk.globals);
  const __requestScope = __scope(__gk.requestVariables);
  const __fileScope = __scope(__gk.fileVariables);
  const __environmentScope = __scope({{}});
  let __testDepth = 0;
  const client = {{
    test(name, fn) {{
      const started = Date.now();
      try {{
        __testDepth++;
        fn();
        __gk.tests.push({{
          name: String(name),
          passed: true,
          message: "passed",
          durationMs: Date.now() - started
        }});
      }} catch (error) {{
        __gk.tests.push({{
          name: String(name),
          passed: false,
          message: __message(error),
          durationMs: Date.now() - started
        }});
      }} finally {{
        __testDepth = Math.max(0, __testDepth - 1);
      }}
    }},
    assert(condition, message) {{
      if (!condition) throw new Error(message || "Assertion failed");
      if (__testDepth === 0) {{
        __gk.tests.push({{
          name: message ? String(message) : "Assertion",
          passed: true,
          message: message ? String(message) : "passed"
        }});
      }}
    }},
    log(...values) {{
      __captureLog("log", values);
    }},
    global: __globalScope,
    variables: {{
      get(name) {{
        if (Object.prototype.hasOwnProperty.call(__gk.requestVariables, String(name))) return __gk.requestVariables[String(name)];
        if (Object.prototype.hasOwnProperty.call(__gk.globals, String(name))) return __gk.globals[String(name)];
        if (Object.prototype.hasOwnProperty.call(__gk.fileVariables, String(name))) return __gk.fileVariables[String(name)];
        return null;
      }},
      set(name, value) {{
        __gk.requestVariables[String(name)] = value;
      }},
      global: __globalScope,
      request: __requestScope,
      file: __fileScope,
      environment: __environmentScope
    }}
  }};
  const request = Object.assign({{}}, __requestBase, {{
    variables: __requestScope,
    environment: __environmentScope,
    iteration() {{ return 0; }},
    templateValue() {{ return null; }}
  }});
  const response = __responseBase || null;
  const console = {{
    log(...values) {{ __captureLog("log", values); }},
    info(...values) {{ __captureLog("info", values); }},
    warn(...values) {{ __captureLog("warn", values); }},
    error(...values) {{ __captureLog("error", values); }},
    debug(...values) {{ __captureLog("debug", values); }}
  }};
  try {{
    const __runner = new Function("client", "request", "response", "console", __source);
    __runner(client, request, response, console);
  }} catch (error) {{
    __gk.errors.push(__scriptError(error));
    __gk.tests.push({{
      name: `${{__phase}} script`,
      passed: false,
      message: __message(error),
      durationMs: 0
    }});
  }}
  return JSON.stringify(__gk);
}})()
"#
    ))
}

async fn resolve_script(
    script: &TestPlanScript,
    base_dir: Option<&Path>,
) -> Result<String, String> {
    match script {
        TestPlanScript::Inline { source } => Ok(source.clone()),
        TestPlanScript::File { path } => {
            let Some(base_dir) = base_dir else {
                return Err(format!(
                    "external script {path} cannot be resolved for ad-hoc execution"
                ));
            };
            let full_path = safe_join(base_dir, path)?;
            tokio::fs::read_to_string(&full_path).await.map_err(|err| {
                format!(
                    "failed to read external script {}: {err}",
                    full_path.display()
                )
            })
        }
    }
}

fn safe_join(base_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        return Err(format!("external script path {relative} must be relative"));
    }
    if relative_path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!(
            "external script path {relative} cannot leave data/plans"
        ));
    }
    Ok(base_dir.join(relative_path))
}

fn headers_to_map(headers: &[HttpHeader]) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|header| (header.name.clone(), header.value.clone()))
        .collect()
}

fn response_headers_to_map(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn response_body_value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).to_string()))
}

fn resolve_request(
    request: &ExecutableRequest,
    file_variables: &BTreeMap<String, String>,
    globals: &BTreeMap<String, Value>,
    request_variables: &BTreeMap<String, Value>,
) -> AppResult<ExecutableRequest> {
    let url = substitute_runtime(&request.url, file_variables, globals, request_variables)?;
    validate_url(&url)?;
    let headers = request
        .headers
        .iter()
        .map(|header| {
            Ok(HttpHeader {
                name: header.name.clone(),
                value: substitute_runtime(
                    &header.value,
                    file_variables,
                    globals,
                    request_variables,
                )?,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let body = request
        .body
        .as_ref()
        .map(|body| substitute_runtime(body, file_variables, globals, request_variables))
        .transpose()?;
    Ok(ExecutableRequest {
        method: request.method.clone(),
        url,
        headers,
        body,
    })
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
    let (pre_request_scripts, pre_request_mask) = collect_scripts(&block.lines, '<', id, warnings);
    let (response_handler_scripts, response_handler_mask) =
        collect_scripts(&block.lines, '>', id, warnings);
    let script_mask: Vec<_> = pre_request_mask
        .iter()
        .zip(response_handler_mask.iter())
        .map(|(pre, response)| *pre || *response)
        .collect();

    let mut request_line_index = None;
    for (index, line) in block.lines.iter().enumerate() {
        if script_mask[index] {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
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

    let mut raw_parts = block.lines[index].split_whitespace();
    raw_parts.next();
    let raw_url = raw_parts
        .next()
        .ok_or_else(|| AppError::BadRequest(format!("request {id} is missing a URL")))?
        .to_string();
    let request_line = substitute_allow_unresolved(&block.lines[index], variables);
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_uppercase();
    let url = parts
        .next()
        .ok_or_else(|| AppError::BadRequest(format!("request {id} is missing a URL")))?;
    let mut headers = Vec::new();
    let mut raw_headers = Vec::new();
    let mut body_lines = Vec::new();
    let mut assertions = Vec::new();
    let mut in_body = false;

    for (line_index, raw_line) in block.lines.iter().enumerate().skip(index + 1) {
        if script_mask[line_index] {
            continue;
        }
        let trimmed = raw_line.trim();
        if trimmed.starts_with('>') || trimmed.starts_with('<') {
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
            raw_headers.push(HttpHeader {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            });
            headers.push(HttpHeader {
                name: name.trim().to_string(),
                value: substitute_allow_unresolved(value.trim(), variables),
            });
        } else {
            body_lines.push(raw_line.as_str());
        }
    }

    while body_lines.last().is_some_and(|line| line.trim().is_empty()) {
        body_lines.pop();
    }

    let raw_body = if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n"))
    };
    let body = raw_body
        .as_ref()
        .map(|body| substitute_allow_unresolved(body, variables));
    for script in &response_handler_scripts {
        if let TestPlanScript::Inline { source } = script {
            for line in source.lines() {
                collect_assertion_line(line.trim(), &mut assertions);
            }
            collect_assertion_line(source.trim(), &mut assertions);
        }
    }
    if assertions.is_empty() && response_handler_scripts.is_empty() {
        assertions.push(HttpAssertion {
            name: "HTTP status is successful".into(),
            kind: AssertionKind::StatusEquals { expected: 200 },
        });
    }
    let url = substitute_allow_unresolved(url, variables);
    if !url.contains("{{") {
        validate_url(&url)?;
    }

    Ok(Some(TestPlanRequest {
        id,
        name: block.name.unwrap_or_else(|| format!("{method} {url}")),
        method,
        url: url.to_string(),
        headers,
        body,
        pre_request_scripts,
        response_handler_scripts,
        assertions,
        raw_url,
        raw_headers,
        raw_body,
    }))
}

fn collect_scripts(
    lines: &[String],
    marker: char,
    request_id: usize,
    warnings: &mut Vec<String>,
) -> (Vec<TestPlanScript>, Vec<bool>) {
    let mut scripts = Vec::new();
    let mut mask = vec![false; lines.len()];
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        let Some(rest) = trimmed.strip_prefix(marker) else {
            index += 1;
            continue;
        };
        mask[index] = true;
        let rest = rest.trim();
        if let Some(first_source) = rest.strip_prefix("{%") {
            let mut source = String::new();
            let mut current = first_source;
            loop {
                if let Some(end) = current.find("%}") {
                    source.push_str(&current[..end]);
                    break;
                }
                source.push_str(current);
                index += 1;
                if index >= lines.len() {
                    warnings.push(format!(
                        "Request {request_id} has an unterminated script block"
                    ));
                    break;
                }
                mask[index] = true;
                source.push('\n');
                current = lines[index].as_str();
            }
            scripts.push(TestPlanScript::Inline {
                source: source.trim().to_string(),
            });
        } else if !rest.is_empty() {
            scripts.push(TestPlanScript::File {
                path: rest.to_string(),
            });
        }
        index += 1;
    }
    (scripts, mask)
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

fn substitute_allow_unresolved(value: &str, variables: &BTreeMap<String, String>) -> String {
    substitute_with(value, |key| {
        variables
            .get(key)
            .cloned()
            .or_else(|| dynamic_variable(key))
    })
}

fn substitute_runtime(
    value: &str,
    file_variables: &BTreeMap<String, String>,
    globals: &BTreeMap<String, Value>,
    request_variables: &BTreeMap<String, Value>,
) -> AppResult<String> {
    let resolved = substitute_with(value, |key| {
        request_variables
            .get(key)
            .or_else(|| globals.get(key))
            .map(value_to_variable)
            .or_else(|| file_variables.get(key).cloned())
            .or_else(|| dynamic_variable(key))
            .or_else(|| env_variable(key))
    });
    if let Some(unresolved) = first_unresolved_variable(&resolved) {
        return Err(AppError::BadRequest(format!(
            "unresolved variable {{{{{unresolved}}}}}"
        )));
    }
    Ok(resolved)
}

fn substitute_with(value: &str, mut resolve: impl FnMut(&str) -> Option<String>) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);
        let after_start = &after_start[2..];
        let Some(end) = after_start.find("}}") else {
            output.push_str("{{");
            output.push_str(after_start);
            return output;
        };
        let key = after_start[..end].trim();
        if let Some(replacement) = resolve(key) {
            output.push_str(&replacement);
        } else {
            output.push_str("{{");
            output.push_str(key);
            output.push_str("}}");
        }
        rest = &after_start[end + 2..];
    }
    output.push_str(rest);
    output
}

fn first_unresolved_variable(value: &str) -> Option<String> {
    let start = value.find("{{")?;
    let tail = &value[start + 2..];
    let end = tail.find("}}")?;
    Some(tail[..end].trim().to_string())
}

fn value_to_variable(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn dynamic_variable(key: &str) -> Option<String> {
    match key {
        "$uuid" | "$random.uuid" => Some(random_uuid()),
        "$timestamp" => Some(chrono::Utc::now().timestamp().to_string()),
        "$isoTimestamp" => Some(chrono::Utc::now().to_rfc3339()),
        _ => None,
    }
}

fn env_variable(key: &str) -> Option<String> {
    let name = key.strip_prefix("$env.")?;
    std::env::var(name).ok()
}

fn random_uuid() -> String {
    let bytes: [u8; 16] = rand::random();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-4{:01x}{:02x}-{:01x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6] & 0x0f,
        bytes[7],
        (bytes[8] & 0x3f) | 0x80,
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
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
    fn preserves_unresolved_variables_for_runtime_resolution() {
        let plan = parse(input("GET https://example.com/{{missing}}")).unwrap();
        assert_eq!(plan.requests[0].url, "https://example.com/{{missing}}");
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
        assert_eq!(report.results[0].assertions[0].name, "ok status");
        assert!(report.results[0].assertions[0].passed);
        assert_eq!(report.results[1].status, Some(201));
        assert_eq!(report.results[2].status, Some(404));
        assert!(report.results[2].ok);
    }

    #[tokio::test]
    async fn pre_request_scripts_can_set_request_variables() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/users/42", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let report = execute(input(&format!(
            r#"
### Fetch user
< {{%
  request.variables.set("userId", "42");
%}}

GET http://{addr}/users/{{{{userId}}}}

> {{%
  client.log("fetched", request.url);
  client.test("status", function () {{
    client.assert(response.status === 200, "expected OK");
  }});
%}}
"#
        )))
        .await
        .unwrap();

        server.abort();

        assert_eq!(report.passed, 1);
        assert_eq!(report.results[0].url, format!("http://{addr}/users/42"));
        assert_eq!(
            report.results[0].logs[0],
            format!("fetched http://{addr}/users/42")
        );
    }

    #[tokio::test]
    async fn script_syntax_errors_are_reported_with_diagnostics() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/ok", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let report = execute(input(&format!(
            r#"
### Broken
GET http://{addr}/ok

> {{%
  client.test("broken syntax", () => {{
    client.assert(response.status === 200);
  // missing closing braces
%}}
"#
        )))
        .await
        .unwrap();

        server.abort();

        let result = &report.results[0];
        assert!(!result.ok);
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .contains("response handler script failed"));
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].phase, "response handler");
        assert!(result.diagnostics[0]
            .source_preview
            .as_deref()
            .unwrap()
            .contains("client.test"));
        assert!(execution_log(&report).contains("diagnostic"));
    }

    #[tokio::test]
    async fn console_logs_are_captured_in_execution_logs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/ok", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let report = execute(input(&format!(
            r#"
### Console logs
GET http://{addr}/ok

> {{%
  console.log("plain", request.url);
  console.error("failed-ish", {{ status: response.status }});
  client.test("status", () => {{
    client.assert(response.status === 200);
  }});
%}}
"#
        )))
        .await
        .unwrap();

        server.abort();

        let logs = &report.results[0].logs;
        assert_eq!(logs[0], format!("plain http://{addr}/ok"));
        assert_eq!(logs[1], r#"[error] failed-ish {"status":200}"#);
        let execution_log = execution_log(&report);
        assert!(execution_log.contains(&format!("log: plain http://{addr}/ok")));
        assert!(execution_log.contains(r#"log: [error] failed-ish {"status":200}"#));
    }

    #[tokio::test]
    async fn file_variables_are_available_in_inline_scripts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/users/7", get(|| async { Json(json!({ "id": 7 })) }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let report = execute(input(&format!(
            r#"
@host = http://{addr}
@userId = 7

### Get user
GET {{{{host}}}}/users/{{{{userId}}}}

> {{%
  client.test("interpolated variable", () => {{
    const expected = Number("{{{{userId}}}}");
    client.assert(response.body.id === expected, `expected ${{expected}}`);
  }});
  client.test("file variable scope", () => {{
    const expected = Number(client.variables.get("userId"));
    client.assert(response.body.id === expected, `expected ${{expected}}`);
  }});
%}}
"#
        )))
        .await
        .unwrap();

        server.abort();

        assert_eq!(report.failed, 0);
        assert_eq!(report.results[0].assertions.len(), 2);
        assert!(report.results[0]
            .assertions
            .iter()
            .all(|assertion| assertion.passed));
    }

    #[tokio::test]
    async fn external_response_handler_scripts_are_resolved_from_plans_directory() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/ok", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-external-script-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        tokio::fs::create_dir_all(data_dir.join("plans").join("scripts"))
            .await
            .unwrap();
        tokio::fs::write(
            data_dir.join("plans").join("scripts").join("assert-ok.js"),
            r#"client.test("external status", function () {
  client.assert(response.status === 200, "expected OK");
});
client.log("external script ran");"#,
        )
        .await
        .unwrap();
        let store = TestPlanStore::open(&data_dir).await;
        let certificates = CertificateStore::open(&data_dir).await;
        let plan = store
            .create_plan(SavePlanInput {
                name: "External script".into(),
                content: format!("### OK\nGET http://{addr}/ok\n> scripts/assert-ok.js\n"),
                directory: None,
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();

        let queued = store.enqueue_execution(&plan.id).await.unwrap();
        store.mark_queue_running(&queued.id).await.unwrap();
        let execution = store
            .run_queued_execution(&queued.id, &certificates)
            .await
            .unwrap();

        server.abort();

        let report = store.get_execution(&execution.id).await.unwrap().report;
        assert_eq!(report.passed, 1);
        assert_eq!(report.results[0].assertions[0].name, "external status");
        assert_eq!(report.results[0].logs[0], "external script ran");
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
        let certificates = CertificateStore::open(&data_dir).await;
        let script = format!(
            "### OK\nGET http://{addr}/ok\n> {{% client.assert(response.status === 200); %}}\n"
        );
        let plan = store
            .create_plan(SavePlanInput {
                name: "Persisted".into(),
                content: script.clone(),
                directory: None,
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();
        let queued = store.enqueue_execution(&plan.id).await.unwrap();
        store.mark_queue_running(&queued.id).await.unwrap();
        let execution = store
            .run_queued_execution(&queued.id, &certificates)
            .await
            .unwrap();

        server.abort();

        assert_eq!(execution.status, QueueStatus::Passed);
        assert_eq!(execution.passed, Some(1));
        let stored_execution = store.get_execution(&execution.id).await.unwrap();
        assert_eq!(stored_execution.report.script, script);
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
    async fn store_deletes_all_execution_reports_and_logs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/ok", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-delete-all-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = TestPlanStore::open(&data_dir).await;
        let certificates = CertificateStore::open(&data_dir).await;
        let plan = store
            .create_plan(SavePlanInput {
                name: "Delete all".into(),
                content: format!("### OK\nGET http://{addr}/ok\n"),
                directory: None,
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();
        let queued = store.enqueue_execution(&plan.id).await.unwrap();
        store.mark_queue_running(&queued.id).await.unwrap();
        let execution = store
            .run_queued_execution(&queued.id, &certificates)
            .await
            .unwrap();

        server.abort();

        let report_path = data_dir
            .join("reports")
            .join(format!("{}.json", execution.id));
        let log_path = data_dir
            .join("reports")
            .join(format!("{}.log", execution.id));
        assert!(report_path.exists());
        assert!(log_path.exists());

        let deleted = store.delete_all_executions().await.unwrap();

        assert_eq!(deleted, 1);
        assert!(store.list_executions().await.is_empty());
        assert!(!report_path.exists());
        assert!(!log_path.exists());
    }

    #[tokio::test]
    async fn store_saves_each_test_plan_as_its_own_http_file() {
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-plan-file-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = TestPlanStore::open(&data_dir).await;
        let plan = store
            .create_plan(SavePlanInput {
                name: "File backed".into(),
                content: "### OK\nGET http://127.0.0.1:8080/ok\n".into(),
                directory: None,
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();
        let plan_path = data_dir.join("plans").join(&plan.id);

        assert!(plan_path.exists());
        assert_eq!(
            tokio::fs::read_to_string(&plan_path).await.unwrap(),
            "### OK\nGET http://127.0.0.1:8080/ok\n"
        );

        let updated = store
            .update_plan(
                &plan.id,
                SavePlanInput {
                    name: "Updated file backed".into(),
                    content: "### Missing\nGET http://127.0.0.1:8080/missing\n".into(),
                    directory: None,
                    variables: BTreeMap::new(),
                },
            )
            .await
            .unwrap();
        let updated_path = data_dir.join("plans").join(&updated.id);
        assert!(plan_path.exists());
        assert_eq!(
            tokio::fs::read_to_string(&updated_path).await.unwrap(),
            "### Missing\nGET http://127.0.0.1:8080/missing\n"
        );

        store.delete_plan(&updated.id).await.unwrap();
        assert!(!updated_path.exists());
    }

    #[tokio::test]
    async fn store_refreshes_manual_file_changes_before_execution() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .route("/old", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/ok", get(|| async { StatusCode::OK }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let data_dir = std::env::temp_dir().join(format!(
            "gate-keeper-refresh-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = TestPlanStore::open(&data_dir).await;
        let certificates = CertificateStore::open(&data_dir).await;
        let plan = store
            .create_plan(SavePlanInput {
                name: "Manual edit".into(),
                content: format!(
                    "### Old\nGET http://{addr}/old\n> {{% client.assert(response.status === 200); %}}\n"
                ),
                directory: None,
                variables: BTreeMap::new(),
            })
            .await
            .unwrap();
        let plan_path = data_dir.join("plans").join(&plan.id);
        let updated_script = format!(
            "### OK\nGET http://{addr}/ok\n> {{% client.assert(response.status === 200); %}}\n"
        );
        tokio::fs::write(&plan_path, &updated_script).await.unwrap();
        store.plan_cache_dirty.store(true, Ordering::Release);

        let refreshed = store.get_plan(&plan.id).await.unwrap();
        assert_eq!(refreshed.content, updated_script);
        assert_eq!(
            refreshed.parsed.requests[0].url,
            format!("http://{addr}/ok")
        );

        let queued = store.enqueue_execution(&plan.id).await.unwrap();
        store.mark_queue_running(&queued.id).await.unwrap();
        let execution = store
            .run_queued_execution(&queued.id, &certificates)
            .await
            .unwrap();

        server.abort();

        assert_eq!(execution.status, QueueStatus::Passed);
        let stored_execution = store.get_execution(&execution.id).await.unwrap();
        assert_eq!(stored_execution.report.script, updated_script);
    }
}
