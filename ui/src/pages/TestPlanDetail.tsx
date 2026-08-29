import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { EmptyState, RequestList } from "../components/TestPlanPanels";
import ScriptEditor from "../components/ScriptEditor";
import { useToast } from "../context/ToastContext";
import { api, type SavePlanInput } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconActivity, IconCheckCircle, IconTrash } from "../icons";

export default function TestPlanDetailPage() {
  const { planId } = useParams({ strict: false }) as { planId: string };
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [content, setContent] = useState("");

  const planQuery = useQuery({
    queryKey: ["test-plan", planId],
    queryFn: () => api.getTestPlan(planId),
  });

  useEffect(() => {
    if (!planQuery.data) return;
    setName(planQuery.data.name);
    setContent(planQuery.data.content);
  }, [planQuery.data]);

  const updateMutation = useMutation({
    mutationFn: (input: SavePlanInput) => api.updateTestPlan(planId, input),
    onSuccess: (plan) => {
      queryClient.setQueryData(["test-plan", planId], plan);
      queryClient.invalidateQueries({ queryKey: ["test-plans"] });
      push("success", `Updated ${plan.name}.`);
    },
    onError: notifyError,
  });

  const executeMutation = useMutation({
    mutationFn: () => api.executeTestPlan(planId),
    onSuccess: (item) => {
      queryClient.invalidateQueries({ queryKey: ["execution-queue"] });
      queryClient.invalidateQueries({ queryKey: ["executions"] });
      push("info", `${item.planName} queued for execution.`);
    },
    onError: notifyError,
  });

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteTestPlan(planId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["test-plans"] });
      push("info", "Test plan deleted.");
      navigate({ to: "/test-plans" });
    },
    onError: notifyError,
  });

  const save = () => {
    updateMutation.mutate({ name: name.trim() || "Uploaded test plan", content });
  };

  if (planQuery.isError) {
    return (
      <section className="card">
        <EmptyState label="Could not load this test plan." />
      </section>
    );
  }

  if (!planQuery.data) {
    return (
      <section className="card">
        <div className="skeleton" />
      </section>
    );
  }

  const isBusy = updateMutation.isPending || executeMutation.isPending || deleteMutation.isPending;
  const plan = planQuery.data;

  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(380px,0.72fr)]">
      <section className="card flex min-h-[720px] flex-col">
        <div className="card-head">
          <h2>Edit Plan</h2>
          <span className="card-hint">updated {timeAgo(plan.updatedAtMs)}</span>
        </div>
        <div className="mb-3 grid grid-cols-1 gap-3 md:grid-cols-[minmax(0,1fr)_auto_auto_auto]">
          <input
            className="input"
            value={name}
            onChange={(event) => setName(event.target.value)}
            aria-label="Plan name"
          />
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
            onClick={() => deleteMutation.mutate()}
            disabled={isBusy}
          >
            <IconTrash size={16} /> Delete
          </button>
        </div>
        <ScriptEditor
          value={content}
          onChange={setContent}
          ariaLabel="test plan content"
          height="600px"
        />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>Parsed Requests</h2>
          <span className="card-hint">{plan.parsed.requests.length} requests</span>
        </div>
        <RequestList plan={plan.parsed} />
        <div className="mt-4 border-t border-line pt-4">
          <Link className="btn btn-secondary" to="/executions">
            View executions
          </Link>
        </div>
      </section>
    </div>
  );
}
