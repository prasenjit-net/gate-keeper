import { Link, useNavigate, useSearch } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { EmptyState } from "../components/TestPlanPanels";
import ScriptEditor from "../components/ScriptEditor";
import { useToast } from "../context/ToastContext";
import { api, type SavePlanInput, type StoredPlan } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconActivity, IconCheckCircle, IconChevronLeft, IconTrash } from "../icons";

type DialogState = { type: "delete-plan"; plan: StoredPlan } | null;

export default function TestPlanDetailPage() {
  const { path } = useSearch({ from: "/test-plans/edit" });
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [content, setContent] = useState("");
  const [dialog, setDialog] = useState<DialogState>(null);

  const planQuery = useQuery({
    queryKey: ["test-plan", path],
    queryFn: () => api.getTestPlanByPath(path),
    enabled: Boolean(path),
  });

  useEffect(() => {
    if (planQuery.data) setContent(planQuery.data.content);
  }, [planQuery.data]);

  const updateMutation = useMutation({
    mutationFn: (input: SavePlanInput) => api.updateTestPlanByPath(path, input),
    onSuccess: (plan) => {
      queryClient.setQueryData(["test-plan", path], plan);
      queryClient.invalidateQueries({ queryKey: ["test-plan-browser"] });
      queryClient.invalidateQueries({ queryKey: ["test-plans"] });
      push("success", `Updated ${plan.name}.`);
    },
    onError: notifyError,
  });

  const executeMutation = useMutation({
    mutationFn: () => api.executeTestPlanByPath(path),
    onSuccess: (item) => {
      queryClient.invalidateQueries({ queryKey: ["execution-queue"] });
      push("info", `${item.planName} queued for execution.`);
    },
    onError: notifyError,
  });

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteTestPlanByPath(path),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["test-plan-browser"] });
      queryClient.invalidateQueries({ queryKey: ["test-plans"] });
      push("info", "Test plan deleted.");
      setDialog(null);
      navigate({ to: "/test-plans", search: { dir: parentPath(path), plan: undefined } });
    },
    onError: notifyError,
  });

  const plan = planQuery.data;
  const isBusy = updateMutation.isPending || executeMutation.isPending || deleteMutation.isPending;

  if (!path) {
    return (
      <section className="card">
        <EmptyState label="No test plan selected." />
      </section>
    );
  }

  if (planQuery.isError) {
    return (
      <section className="card">
        <EmptyState label="Could not load this test plan." />
      </section>
    );
  }

  if (!plan) {
    return (
      <section className="card">
        <div className="skeleton" />
      </section>
    );
  }

  const save = () => {
    updateMutation.mutate({ name: plan.name, content });
  };

  return (
    <div className="flex min-h-[calc(100vh-116px)] flex-col gap-3">
      <section className="card flex min-h-0 flex-1 flex-col">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <Link
              className="mb-2 inline-flex items-center gap-1 text-[0.82rem] text-ink-muted hover:text-accent"
              to="/test-plans"
              search={{ dir: parentPath(plan.path), plan: plan.path }}
            >
              <IconChevronLeft size={15} /> Test Plans
            </Link>
            <h2 className="truncate text-[1rem] font-semibold text-ink">{plan.name}</h2>
            <p className="truncate font-mono text-[0.7rem] text-ink-faint">
              {plan.path} · updated {timeAgo(plan.updatedAtMs)}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <button className="btn btn-secondary" onClick={save} disabled={isBusy}>
              <IconCheckCircle size={16} /> Save
            </button>
            <button
              className="btn btn-primary"
              onClick={() => executeMutation.mutate()}
              disabled={isBusy}
            >
              <IconActivity size={16} /> Execute
            </button>
            <button
              className="btn btn-danger"
              onClick={() => setDialog({ type: "delete-plan", plan })}
              disabled={isBusy}
            >
              <IconTrash size={16} /> Delete
            </button>
          </div>
        </div>

        <div className="min-h-[520px] flex-1">
          <ScriptEditor
            value={content}
            onChange={setContent}
            ariaLabel="test plan editor"
            height="100%"
          />
        </div>
      </section>

      {dialog ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4">
          <div className="w-full max-w-md rounded-lg border border-line bg-surface p-4 shadow-xl">
            <h2 className="text-[1rem] font-semibold text-ink">Delete Test Plan</h2>
            <p className="mt-1 text-[0.84rem] text-ink-muted">Delete {dialog.plan.path}?</p>
            <p className="mt-3 rounded-md border border-err/35 bg-err/10 p-3 text-[0.84rem] text-err">
              This action removes the file from data/plans.
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button className="btn btn-secondary" onClick={() => setDialog(null)} disabled={isBusy}>
                Cancel
              </button>
              <button className="btn btn-danger" onClick={() => deleteMutation.mutate()} disabled={isBusy}>
                Delete
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function parentPath(path: string): string {
  const parts = path.split("/");
  parts.pop();
  return parts.join("/");
}
