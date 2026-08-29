import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import Badge from "../components/Badge";
import { EmptyState } from "../components/TestPlanPanels";
import { useLive } from "../context/LiveContext";
import { api, type ExecutionQueueItem, type QueueStatus } from "../lib/api";
import { timeAgo } from "../lib/format";

function statusTone(status: QueueStatus) {
  if (status === "passed") return "ok";
  if (status === "failed" || status === "error") return "err";
  if (status === "running") return "accent";
  return "neutral";
}

function mergeQueue(initial: ExecutionQueueItem[] | undefined, live: ExecutionQueueItem[]) {
  const map = new Map<string, ExecutionQueueItem>();
  for (const item of initial ?? []) map.set(item.id, item);
  for (const item of live) map.set(item.id, item);
  return [...map.values()]
    .filter((item) => item.status === "queued" || item.status === "running")
    .sort((a, b) => b.queuedAtMs - a.queuedAtMs);
}

export default function ExecutionQueuePage() {
  const { queue: liveQueue } = useLive();
  const queueQuery = useQuery({
    queryKey: ["execution-queue"],
    queryFn: api.listExecutionQueue,
    refetchInterval: 30_000,
  });
  const queue = mergeQueue(queueQuery.data, liveQueue);

  return (
    <section className="card">
      <div className="card-head">
        <h2>Execution Queue</h2>
        <span className="card-hint">live over WebSocket</span>
      </div>
      {queueQuery.isError && queue.length === 0 ? (
        <p className="py-2 text-[0.86rem] text-err">Could not load queue status.</p>
      ) : queue.length === 0 ? (
        <EmptyState label="No queued executions yet." />
      ) : (
        <ul className="flex max-h-[720px] flex-col overflow-y-auto">
          {queue.map((item) => (
            <QueueItem key={item.id} item={item} />
          ))}
        </ul>
      )}
    </section>
  );
}

function QueueItem({ item }: { item: ExecutionQueueItem }) {
  const complete = (item.status === "passed" || item.status === "failed") && item.reportPath;
  return (
    <li className="border-b border-line py-3 last:border-b-0">
      <div className="flex flex-wrap items-start gap-2">
        <div className="min-w-0 flex-1">
          <strong className="block truncate text-[0.95rem]">{item.planName}</strong>
          <span className="mt-1 block font-mono text-[0.7rem] text-ink-faint">
            queued {timeAgo(item.queuedAtMs)}
          </span>
        </div>
        <Badge tone={statusTone(item.status)}>{item.status}</Badge>
      </div>
      <div className="mt-2 flex flex-wrap gap-2">
        {item.total != null ? (
          <Badge tone={item.failed === 0 ? "ok" : "err"}>
            {item.passed}/{item.total} passed
          </Badge>
        ) : null}
        {item.startedAtMs ? <Badge>started {timeAgo(item.startedAtMs)}</Badge> : null}
        {item.finishedAtMs ? <Badge>finished {timeAgo(item.finishedAtMs)}</Badge> : null}
      </div>
      {item.error ? <p className="mt-2 text-sm text-err">{item.error}</p> : null}
      <div className="mt-3 flex flex-wrap gap-2">
        <Link className="btn btn-secondary btn-sm" to="/test-plans/$planId" params={{ planId: item.planId }}>
          Open plan
        </Link>
        {complete ? (
          <Link
            className="btn btn-primary btn-sm"
            to="/executions/$executionId"
            params={{ executionId: item.id }}
          >
            View report
          </Link>
        ) : null}
      </div>
    </li>
  );
}
