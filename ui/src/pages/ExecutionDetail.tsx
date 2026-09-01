import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import Badge from "../components/Badge";
import { EmptyState, ReportView } from "../components/TestPlanPanels";
import { useToast } from "../context/ToastContext";
import { api } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconReport, IconTrash } from "../icons";

type ExecutionTab = "report" | "log" | "script";

const TABS: { id: ExecutionTab; label: string }[] = [
  { id: "report", label: "Report Viewer" },
  { id: "log", label: "Execution Log" },
  { id: "script", label: "Script Snapshot" },
];

export default function ExecutionDetailPage() {
  const { executionId } = useParams({ strict: false }) as { executionId: string };
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [tab, setTab] = useState<ExecutionTab>("report");
  const executionQuery = useQuery({
    queryKey: ["execution", executionId],
    queryFn: () => api.getExecution(executionId),
  });
  const deleteMutation = useMutation({
    mutationFn: () => api.deleteExecution(executionId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["executions"] });
      push("info", "Execution deleted.");
      navigate({ to: "/executions" });
    },
    onError: notifyError,
  });

  if (executionQuery.isError) {
    return (
      <section className="card">
        <EmptyState label="Could not load this execution report." />
      </section>
    );
  }

  if (!executionQuery.data) {
    return (
      <section className="card">
        <div className="skeleton" />
      </section>
    );
  }

  const execution = executionQuery.data;

  return (
    <section className="card">
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="mb-2 flex items-center gap-2 text-ink-muted">
            <IconReport size={18} />
            <span className="font-mono text-[0.7rem]">{execution.id}</span>
          </div>
          <h2 className="truncate text-[1rem] font-semibold text-ink">{execution.planName}</h2>
          <p className="mt-1 break-all font-mono text-[0.72rem] text-ink-faint">
            {execution.planPath}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Link
            className="btn btn-secondary"
            to="/test-plans"
            search={{ dir: parentPath(execution.planPath), plan: execution.planPath }}
          >
            Open plan
          </Link>
          <button
            className="btn btn-danger"
            onClick={() => deleteMutation.mutate()}
            disabled={deleteMutation.isPending}
          >
            <IconTrash size={16} /> Delete
          </button>
        </div>
      </div>

      <div className="mb-4 grid grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-5">
        <SummaryItem label="Started" value={new Date(execution.startedAtMs).toLocaleString()} />
        <SummaryItem label="Finished" value={new Date(execution.finishedAtMs).toLocaleString()} />
        <SummaryItem label="Duration" value={`${execution.durationMs} ms`} />
        <SummaryItem label="Passed" value={`${execution.passed}/${execution.total}`} tone="ok" />
        <SummaryItem label="Failed" value={String(execution.failed)} tone={execution.failed ? "err" : "ok"} />
      </div>

      <div className="mb-4 flex flex-wrap gap-2 border-b border-line pb-3">
        {TABS.map((item) => (
          <button
            key={item.id}
            className={tab === item.id ? "btn btn-primary btn-sm" : "btn btn-secondary btn-sm"}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      {tab === "report" ? <ReportView report={execution.report} /> : null}
      {tab === "log" ? (
        <div>
          <div className="mb-2 flex flex-wrap gap-2">
            <Badge>{execution.logPath}</Badge>
            <Badge>{timeAgo(execution.startedAtMs)}</Badge>
          </div>
          <pre className="max-h-[720px] overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-[0.75rem] whitespace-pre-wrap text-ink-muted">
            {execution.log}
          </pre>
        </div>
      ) : null}
      {tab === "script" ? (
        <div>
          <div className="mb-2 flex flex-wrap gap-2">
            <Badge>{execution.reportPath}</Badge>
            <Badge>captured at execution time</Badge>
          </div>
          <pre className="max-h-[720px] overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-[0.75rem] whitespace-pre-wrap text-ink-muted">
            {execution.report.script}
          </pre>
        </div>
      ) : null}
    </section>
  );
}

function SummaryItem({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string;
  tone?: "neutral" | "ok" | "err";
}) {
  const color = tone === "ok" ? "text-ok" : tone === "err" ? "text-err" : "text-ink";
  return (
    <div className="rounded-lg border border-line bg-surface-2 p-3">
      <div className="font-mono text-[0.68rem] text-ink-faint uppercase">{label}</div>
      <div className={`mt-1 truncate text-[0.9rem] font-semibold ${color}`}>{value}</div>
    </div>
  );
}

function parentPath(path: string): string {
  const parts = path.split("/");
  parts.pop();
  return parts.join("/");
}
