// Typed API client. Every backend failure — network, HTTP status, or the
// server's JSON error envelope — is normalized into ApiError, which the
// toast layer renders as an error notification bubble.

export interface UiConfig {
  appName: string;
  tagline: string;
  defaultTheme: string;
  repoUrl?: string | null;
}

export interface ServerConfig {
  ui: UiConfig;
  version: string;
  startedAtMs: number;
}

export interface Metrics {
  cpu: number;
  memory: number;
  requestsTotal: number;
  requestsPerMin: number;
  wsClients: number;
  uptimeSecs: number;
  timestampMs: number;
}

export interface Task {
  id: number;
  title: string;
  done: boolean;
  createdAt: string;
}

export interface HttpPlanInput {
  name?: string;
  content: string;
  variables?: Record<string, string>;
}

export interface SavePlanInput {
  name: string;
  content: string;
  variables?: Record<string, string>;
}

export interface HttpPlan {
  name: string;
  variables: Record<string, string>;
  requests: HttpPlanRequest[];
  warnings: string[];
}

export interface HttpPlanRequest {
  id: number;
  name: string;
  method: string;
  url: string;
  headers: HttpHeader[];
  body?: string | null;
  assertions: HttpAssertion[];
}

export interface StoredPlanSummary {
  id: string;
  name: string;
  requestCount: number;
  warningCount: number;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface StoredPlan {
  id: string;
  name: string;
  content: string;
  parsed: HttpPlan;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface HttpHeader {
  name: string;
  value: string;
}

export interface HttpAssertion {
  name: string;
  kind: { type: "statusEquals"; expected: number };
}

export interface ExecutionReport {
  id: string;
  planId: string;
  planName: string;
  startedAtMs: number;
  finishedAtMs: number;
  durationMs: number;
  total: number;
  passed: number;
  failed: number;
  results: ExecutionResult[];
}

export interface ExecutionSummary {
  id: string;
  planId: string;
  planName: string;
  startedAtMs: number;
  finishedAtMs: number;
  durationMs: number;
  total: number;
  passed: number;
  failed: number;
  reportPath: string;
  logPath: string;
}

export type QueueStatus = "queued" | "running" | "passed" | "failed" | "error";

export interface ExecutionQueueItem {
  id: string;
  planId: string;
  planName: string;
  status: QueueStatus;
  queuedAtMs: number;
  startedAtMs?: number | null;
  finishedAtMs?: number | null;
  total?: number | null;
  passed?: number | null;
  failed?: number | null;
  error?: string | null;
  reportPath?: string | null;
  logPath?: string | null;
}

export interface StoredExecution extends ExecutionSummary {
  report: ExecutionReport;
  log: string;
}

export interface ExecutionResult {
  id: number;
  name: string;
  method: string;
  url: string;
  status?: number | null;
  ok: boolean;
  durationMs: number;
  responseBytes: number;
  responsePreview: string;
  error?: string | null;
  assertions: AssertionResult[];
}

export interface AssertionResult {
  name: string;
  passed: boolean;
  message: string;
}

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(init?.headers as Record<string, string> | undefined),
      },
    });
  } catch {
    throw new ApiError("NETWORK", 0, "Cannot reach the server");
  }

  if (!res.ok) {
    // Prefer the backend's { error: { code, message } } envelope.
    let code = `HTTP_${res.status}`;
    let message = res.statusText || "Request failed";
    try {
      const body = (await res.json()) as {
        error?: { code?: string; message?: string };
      };
      if (body.error) {
        code = body.error.code ?? code;
        message = body.error.message ?? message;
      }
    } catch {
      /* body was not JSON — keep the status text */
    }
    throw new ApiError(code, res.status, message);
  }

  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

export const api = {
  config: () => request<ServerConfig>("/api/config"),
  health: () => request<{ status: string; version: string }>("/api/health"),
  metrics: () => request<Metrics>("/api/metrics"),
  previewHttpPlan: (input: HttpPlanInput) =>
    request<HttpPlan>("/api/http-plans/preview", {
      method: "POST",
      body: JSON.stringify(input),
    }),
  listHttpPlans: () => request<StoredPlanSummary[]>("/api/http-plans"),
  createHttpPlan: (input: SavePlanInput) =>
    request<StoredPlan>("/api/http-plans", {
      method: "POST",
      body: JSON.stringify(input),
    }),
  getHttpPlan: (id: string) => request<StoredPlan>(`/api/http-plans/${id}`),
  updateHttpPlan: (id: string, input: SavePlanInput) =>
    request<StoredPlan>(`/api/http-plans/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  deleteHttpPlan: (id: string) =>
    request<void>(`/api/http-plans/${id}`, { method: "DELETE" }),
  executeHttpPlan: (id: string) =>
    request<ExecutionQueueItem>(`/api/http-plans/${id}/execute`, {
      method: "POST",
    }),
  listExecutions: () => request<ExecutionSummary[]>("/api/executions"),
  listExecutionQueue: () => request<ExecutionQueueItem[]>("/api/execution-queue"),
  getExecution: (id: string) => request<StoredExecution>(`/api/executions/${id}`),
  deleteExecution: (id: string) =>
    request<void>(`/api/executions/${id}`, { method: "DELETE" }),
  listTasks: () => request<Task[]>("/api/tasks"),
  createTask: (title: string) =>
    request<Task>("/api/tasks", { method: "POST", body: JSON.stringify({ title }) }),
  toggleTask: (id: number) => request<Task>(`/api/tasks/${id}/toggle`, { method: "POST" }),
  deleteTask: (id: number) => request<void>(`/api/tasks/${id}`, { method: "DELETE" }),
  /** Always fails server-side — demonstrates the error pipeline. */
  errorDemo: (kind: string) => request<never>(`/api/error-demo?kind=${kind}`),
  /** Hits an endpoint that does not exist — demonstrates the JSON 404. */
  missing: () => request<never>("/api/this-endpoint-does-not-exist"),
};
