import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import Badge from "../components/Badge";
import { EmptyState } from "../components/TestPlanPanels";
import { useToast } from "../context/ToastContext";
import { api, type ExecutionSummary } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconTrash } from "../icons";

export default function ExecutionsPage() {
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const [confirmDeleteAll, setConfirmDeleteAll] = useState(false);
  const executionsQuery = useQuery({ queryKey: ["executions"], queryFn: api.listExecutions });
  const deleteMutation = useMutation({
    mutationFn: api.deleteExecution,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["executions"] });
      push("info", "Execution deleted.");
    },
    onError: notifyError,
  });
  const deleteAllMutation = useMutation({
    mutationFn: api.deleteAllExecutions,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["executions"] });
      queryClient.invalidateQueries({ queryKey: ["execution"] });
      push("info", "All executions deleted.");
      setConfirmDeleteAll(false);
    },
    onError: notifyError,
  });
  const executions = executionsQuery.data ?? [];

  return (
    <>
      <section className="card">
        <div className="card-head">
          <h2>Executions</h2>
          <span className="card-hint mr-auto">{executionsQuery.data?.length ?? 0} saved reports</span>
          {executions.length > 0 ? (
            <button
              className="btn btn-danger btn-sm"
              onClick={() => setConfirmDeleteAll(true)}
              disabled={deleteMutation.isPending || deleteAllMutation.isPending}
            >
              <IconTrash size={15} /> Delete All
            </button>
          ) : null}
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
        ) : executions.length === 0 ? (
          <EmptyState label="No executions saved yet." />
        ) : (
          <ExecutionList
            executions={executions}
            deletingId={deleteMutation.variables}
            onDelete={(id) => deleteMutation.mutate(id)}
          />
        )}
      </section>

      {confirmDeleteAll ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4">
          <div className="w-full max-w-md rounded-lg border border-line bg-surface p-4 shadow-xl">
            <h2 className="text-[1rem] font-semibold text-ink">Delete All Executions</h2>
            <p className="mt-1 text-[0.84rem] text-ink-muted">
              Delete {executions.length} saved execution reports and logs?
            </p>
            <p className="mt-3 rounded-md border border-err/35 bg-err/10 p-3 text-[0.84rem] text-err">
              This removes execution JSON reports and log files from data/reports.
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button
                className="btn btn-secondary"
                onClick={() => setConfirmDeleteAll(false)}
                disabled={deleteAllMutation.isPending}
              >
                Cancel
              </button>
              <button
                className="btn btn-danger"
                onClick={() => deleteAllMutation.mutate()}
                disabled={deleteAllMutation.isPending}
              >
                <IconTrash size={16} /> Delete All
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
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
