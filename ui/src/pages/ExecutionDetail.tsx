import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Badge from "../components/Badge";
import { EmptyState, ReportView } from "../components/HttpPlanPanels";
import { useToast } from "../context/ToastContext";
import { api } from "../lib/api";
import { IconTrash } from "../icons";

export default function ExecutionDetailPage() {
  const { executionId } = useParams({ strict: false }) as { executionId: string };
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
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
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(360px,0.7fr)]">
      <section className="card">
        <div className="card-head">
          <h2>Report Viewer</h2>
          <span className="card-hint">{execution.id}</span>
        </div>
        <div className="mb-4 flex flex-wrap gap-2">
          <Badge tone={execution.failed === 0 ? "ok" : "err"}>
            {execution.passed}/{execution.total} passed
          </Badge>
          <Badge>{execution.durationMs} ms</Badge>
          <Badge>{execution.reportPath}</Badge>
        </div>
        <ReportView report={execution.report} />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>Execution Log</h2>
          <span className="card-hint">{execution.logPath}</span>
        </div>
        <pre className="max-h-[620px] overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-[0.75rem] whitespace-pre-wrap text-ink-muted">
          {execution.log}
        </pre>
        <div className="mt-4 flex flex-wrap gap-2 border-t border-line pt-4">
          <Link
            className="btn btn-secondary"
            to="/http-plans/$planId"
            params={{ planId: execution.planId }}
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
      </section>
    </div>
  );
}
