import type { ReactElement } from "react";
import type { Activity } from "../context/LiveContext";
import { IconActivity, IconBolt, IconCheckCircle, IconServer } from "../icons";
import { timeAgo } from "../lib/format";

const KIND_ICON: Record<string, ReactElement> = {
  task: <IconCheckCircle size={16} />,
  "test-case": <IconCheckCircle size={16} />,
  "test-plan": <IconServer size={16} />,
  "test-run": <IconActivity size={16} />,
  socket: <IconBolt size={16} />,
};

const KIND_TONE: Record<string, string> = {
  task: "text-ok",
  "test-case": "text-ok",
  "test-plan": "text-info",
  "test-run": "text-accent",
  socket: "text-info",
};

export default function ActivityFeed({ activities }: { activities: Activity[] }) {
  if (activities.length === 0) {
    return (
      <p className="py-2 text-[0.86rem] text-ink-faint">
        Server events will appear here after test cases run.
      </p>
    );
  }
  return (
    <ul className="flex max-h-[380px] flex-col overflow-y-auto">
      {activities.map((activity, index) => (
        <li
          key={`${activity.timestampMs}-${index}`}
          className="flex items-start gap-2.5 border-b border-line px-0.5 py-2 text-[0.85rem] last:border-b-0"
        >
          <span
            className={`mt-0.5 inline-flex ${KIND_TONE[activity.kind] ?? "text-ink-faint"}`}
          >
            {KIND_ICON[activity.kind] ?? <IconActivity size={16} />}
          </span>
          <span className="min-w-0 flex-1 break-words">{activity.message}</span>
          <time className="mt-0.5 shrink-0 font-mono text-[0.68rem] text-ink-faint">
            {timeAgo(activity.timestampMs)}
          </time>
        </li>
      ))}
    </ul>
  );
}
