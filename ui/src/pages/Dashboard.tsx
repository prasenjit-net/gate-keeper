import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import ActivityFeed from "../components/ActivityFeed";
import Badge from "../components/Badge";
import { EmptyState } from "../components/TestPlanPanels";
import StatCard from "../components/StatCard";
import { useLive } from "../context/LiveContext";
import { IconCheckCircle, IconQueue, IconReport, IconServer } from "../icons";
import { api, type ExecutionQueueItem } from "../lib/api";
import { formatNumber, timeAgo } from "../lib/format";

function activeQueue(items: ExecutionQueueItem[]) {
  return items.filter((item) => item.status === "queued" || item.status === "running");
}

export default function DashboardPage() {
  const { metrics, activities, queue: liveQueue } = useLive();
  const plansQuery = useQuery({ queryKey: ["test-plans"], queryFn: api.listTestPlans });
  const executionsQuery = useQuery({ queryKey: ["executions"], queryFn: api.listExecutions });
  const queueQuery = useQuery({
    queryKey: ["execution-queue"],
    queryFn: api.listExecutionQueue,
    refetchInterval: 30_000,
  });

  const queueMap = new Map<string, ExecutionQueueItem>();
  for (const item of queueQuery.data ?? []) queueMap.set(item.id, item);
  for (const item of liveQueue) queueMap.set(item.id, item);
  const runningQueue = activeQueue([...queueMap.values()]);
  const executions = executionsQuery.data ?? [];
  const totalRequests = (plansQuery.data ?? []).reduce((sum, plan) => sum + plan.requestCount, 0);
  const totalRuns = executions.length;
  const totalPassed = executions.reduce((sum, execution) => sum + execution.passed, 0);
  const totalChecks = executions.reduce((sum, execution) => sum + execution.total, 0);
  const passRate = totalChecks > 0 ? Math.round((totalPassed / totalChecks) * 100) : null;
  const latest = executions[0];

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 min-[560px]:grid-cols-2 xl:grid-cols-4">
        <StatCard
          icon={<IconServer size={18} />}
          label="Saved Plans"
          value={plansQuery.data ? formatNumber(plansQuery.data.length) : "—"}
          sub={`${formatNumber(totalRequests)} requests designed`}
        />
        <StatCard
          icon={<IconReport size={18} />}
          label="Executions"
          value={executionsQuery.data ? formatNumber(totalRuns) : "—"}
          sub={latest ? `latest ${timeAgo(latest.startedAtMs)}` : "no runs yet"}
          color="var(--chart-2)"
        />
        <StatCard
          icon={<IconQueue size={18} />}
          label="Queue"
          value={queueQuery.data ? formatNumber(runningQueue.length) : "—"}
          sub="queued or running now"
        />
        <StatCard
          icon={<IconCheckCircle size={18} />}
          label="Pass Rate"
          value={passRate == null ? "—" : `${passRate}%`}
          sub={totalChecks > 0 ? `${totalPassed}/${totalChecks} checks passed` : "waiting for reports"}
        />
      </div>

      <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-[1fr_1fr]">
        <section className="card">
          <div className="card-head">
            <h2>Recent Executions</h2>
            <span className="card-hint">saved reports</span>
          </div>
          {executions.length === 0 ? (
            <EmptyState label="No executions saved yet." />
          ) : (
            <ul className="flex max-h-[360px] flex-col overflow-y-auto">
              {executions.slice(0, 6).map((execution) => (
                <li key={execution.id} className="border-b border-line py-2.5 last:border-b-0">
                  <Link
                    className="block rounded-md text-ink transition-colors hover:text-accent"
                    to="/executions/$executionId"
                    params={{ executionId: execution.id }}
                  >
                    <div className="flex items-center gap-2">
                      <strong className="min-w-0 flex-1 truncate text-[0.9rem]">
                        {execution.planName}
                      </strong>
                      <Badge tone={execution.failed === 0 ? "ok" : "err"}>
                        {execution.passed}/{execution.total}
                      </Badge>
                    </div>
                    <span className="mt-1 block font-mono text-[0.68rem] text-ink-faint">
                      {timeAgo(execution.startedAtMs)} · {execution.durationMs} ms
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="card">
          <div className="card-head">
            <h2>Running Queue</h2>
            <span className="card-hint">{metrics ? `${metrics.wsClients} live clients` : "live status"}</span>
          </div>
          {runningQueue.length === 0 ? (
            <EmptyState label="No executions are queued or running." />
          ) : (
            <ul className="flex max-h-[360px] flex-col overflow-y-auto">
              {runningQueue.map((item) => (
                <li key={item.id} className="border-b border-line py-2.5 last:border-b-0">
                  <div className="flex items-center gap-2">
                    <strong className="min-w-0 flex-1 truncate text-[0.9rem]">{item.planName}</strong>
                    <Badge tone={item.status === "running" ? "accent" : "neutral"}>{item.status}</Badge>
                  </div>
                  <span className="mt-1 block font-mono text-[0.68rem] text-ink-faint">
                    queued {timeAgo(item.queuedAtMs)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>

      <section className="card">
        <div className="card-head">
          <h2>Activity</h2>
          <span className="card-hint">test plan and runner events</span>
        </div>
        <ActivityFeed activities={activities} />
      </section>
    </div>
  );
}
