import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type ChangeEvent } from "react";
import Badge from "../components/Badge";
import { EmptyState, RequestList, SAMPLE_PLAN } from "../components/TestPlanPanels";
import ScriptEditor from "../components/ScriptEditor";
import { useToast } from "../context/ToastContext";
import { api, type TestPlan, type SavePlanInput, type StoredPlanSummary } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconPlus, IconServer, IconTrash } from "../icons";

export default function TestPlansPage() {
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const fileInput = useRef<HTMLInputElement | null>(null);
  const [name, setName] = useState("Gate Keeper smoke plan");
  const [content, setContent] = useState(SAMPLE_PLAN);
  const [preview, setPreview] = useState<TestPlan | null>(null);

  const plansQuery = useQuery({ queryKey: ["test-plans"], queryFn: api.listTestPlans });

  const previewMutation = useMutation({
    mutationFn: (input: SavePlanInput) => api.previewTestPlan(input),
    onSuccess: setPreview,
    onError: notifyError,
  });

  const createMutation = useMutation({
    mutationFn: (input: SavePlanInput) => api.createTestPlan(input),
    onSuccess: (plan) => {
      queryClient.invalidateQueries({ queryKey: ["test-plans"] });
      push("success", `Saved ${plan.name}.`);
      navigate({ to: "/test-plans/$planId", params: { planId: plan.id } });
    },
    onError: notifyError,
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteTestPlan,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["test-plans"] });
      push("info", "Test plan deleted.");
    },
    onError: notifyError,
  });

  const input = (): SavePlanInput => ({ name: name.trim() || "Uploaded test plan", content });

  const upload = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    setName(file.name.replace(/\.(http|rest)$/i, ""));
    setContent(await file.text());
    setPreview(null);
    event.target.value = "";
  };

  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(360px,0.8fr)_minmax(0,1.2fr)]">
      <section className="card">
        <div className="card-head">
          <h2>Saved Plans</h2>
          <span className="card-hint">{plansQuery.data?.length ?? 0} total</span>
        </div>
        {plansQuery.data === undefined ? (
          plansQuery.isError ? (
            <p className="py-2 text-[0.86rem] text-err">Could not load plans.</p>
          ) : (
            <div className="flex flex-col gap-3 py-2">
              <div className="skeleton" />
              <div className="skeleton" />
              <div className="skeleton" />
            </div>
          )
        ) : plansQuery.data.length === 0 ? (
          <EmptyState label="No saved plans yet." />
        ) : (
          <PlanList
            plans={plansQuery.data}
            deletingId={deleteMutation.variables}
            onDelete={(id) => deleteMutation.mutate(id)}
          />
        )}
      </section>

      <section className="card flex min-h-[720px] flex-col">
        <div className="card-head">
          <h2>New Plan</h2>
          <span className="card-hint">upload or paste .http content</span>
        </div>
        <div className="mb-3 grid grid-cols-1 gap-3 md:grid-cols-[minmax(0,1fr)_auto_auto]">
          <input
            className="input"
            value={name}
            onChange={(event) => setName(event.target.value)}
            aria-label="Plan name"
          />
          <input
            ref={fileInput}
            className="hidden"
            type="file"
            accept=".http,.rest,text/plain"
            onChange={upload}
          />
          <button className="btn btn-secondary" onClick={() => fileInput.current?.click()}>
            <IconPlus size={16} /> Upload
          </button>
          <button
            className="btn btn-secondary"
            onClick={() => previewMutation.mutate(input())}
            disabled={previewMutation.isPending || createMutation.isPending}
          >
            <IconServer size={16} /> Preview
          </button>
        </div>
        <ScriptEditor
          value={content}
          onChange={(next) => {
            setContent(next);
            setPreview(null);
          }}
          ariaLabel="test plan content"
          height="420px"
        />
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <button
            className="btn btn-primary"
            onClick={() => createMutation.mutate(input())}
            disabled={previewMutation.isPending || createMutation.isPending}
          >
            <IconPlus size={16} /> Save Plan
          </button>
          {preview ? (
            <span className="font-mono text-[0.72rem] text-ink-faint">
              {preview.requests.length} requests parsed
            </span>
          ) : null}
        </div>
        {preview ? (
          <div className="mt-4 border-t border-line pt-4">
            <RequestList plan={preview} />
          </div>
        ) : null}
      </section>
    </div>
  );
}

function PlanList({
  plans,
  deletingId,
  onDelete,
}: {
  plans: StoredPlanSummary[];
  deletingId?: string;
  onDelete: (id: string) => void;
}) {
  return (
    <ul className="flex max-h-[680px] flex-col overflow-y-auto">
      {plans.map((plan) => (
        <li key={plan.id} className="border-b border-line py-3 last:border-b-0">
          <div className="flex items-start gap-2">
            <Link
              className="min-w-0 flex-1 rounded-md text-ink transition-colors hover:text-accent"
              to="/test-plans/$planId"
              params={{ planId: plan.id }}
            >
              <strong className="block truncate text-[0.95rem]">{plan.name}</strong>
              <span className="mt-1 block font-mono text-[0.7rem] text-ink-faint">
                updated {timeAgo(plan.updatedAtMs)}
              </span>
            </Link>
            <button
              className="icon-btn danger"
              onClick={() => onDelete(plan.id)}
              disabled={deletingId === plan.id}
              aria-label={`Delete ${plan.name}`}
            >
              <IconTrash size={16} />
            </button>
          </div>
          <div className="mt-2 flex flex-wrap gap-2">
            <Badge>{plan.requestCount} requests</Badge>
            {plan.warningCount > 0 ? <Badge tone="warn">{plan.warningCount} warnings</Badge> : null}
          </div>
        </li>
      ))}
    </ul>
  );
}
