import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Badge from "../components/Badge";
import { EmptyState } from "../components/HttpPlanPanels";
import { useToast } from "../context/ToastContext";
import { api, type ExecutionSummary } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconTrash } from "../icons";

export default function ExecutionsPage() {
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const executionsQuery = useQuery({ queryKey: ["executions"], queryFn: api.listExecutions });
  const deleteMutation = useMutation({
    mutationFn: api.deleteExecution,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["executions"] });
      push("info", "Execution deleted.");
    },
    onError: notifyError,
  });

  return (
    <section className="card">
      <div className="card-head">
        <h2>Executions</h2>
        <span className="card-hint">{executionsQuery.data?.length ?? 0} saved reports</span>
      </div>
      {executionsQuery.data === undefined ? (
        executionsQuery.isError ? (
          <p className="py-2 text-[0.86rem] text-err">Could not load executions.</p>
        ) : (
          <div className="flex flex-col gap-3 py-2">
            <div className="skeleton" />
            <div className="skeleton" />
            <div className="skeleton" />
          </div>
        )
      ) : executionsQuery.data.length === 0 ? (
        <EmptyState label="No executions saved yet." />
      ) : (
        <ExecutionList
          executions={executionsQuery.data}
          deletingId={deleteMutation.variables}
          onDelete={(id) => deleteMutation.mutate(id)}
        />
      )}
    </section>
  );
}

function ExecutionList({
  executions,
  deletingId,
  onDelete,
}: {
  executions: ExecutionSummary[];
  deletingId?: string;
  onDelete: (id: string) => void;
}) {
  return (
    <ul className="flex max-h-[720px] flex-col overflow-y-auto">
      {executions.map((execution) => (
        <li key={execution.id} className="border-b border-line py-3 last:border-b-0">
          <div className="flex items-start gap-2">
            <Link
              className="min-w-0 flex-1 rounded-md text-ink transition-colors hover:text-accent"
              to="/executions/$executionId"
              params={{ executionId: execution.id }}
            >
              <strong className="block truncate text-[0.95rem]">{execution.planName}</strong>
              <span className="mt-1 block font-mono text-[0.7rem] text-ink-faint">
                ran {timeAgo(execution.startedAtMs)} in {execution.durationMs} ms
              </span>
            </Link>
            <button
              className="icon-btn danger"
              onClick={() => onDelete(execution.id)}
              disabled={deletingId === execution.id}
              aria-label={`Delete execution ${execution.id}`}
            >
              <IconTrash size={16} />
            </button>
          </div>
          <div className="mt-2 flex flex-wrap gap-2">
            <Badge tone={execution.failed === 0 ? "ok" : "err"}>
              {execution.passed}/{execution.total} passed
            </Badge>
            <Badge>{execution.failed} failed</Badge>
            <Badge>{execution.reportPath}</Badge>
          </div>
        </li>
      ))}
    </ul>
  );
}
