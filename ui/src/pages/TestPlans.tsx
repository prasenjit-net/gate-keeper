import { useNavigate, useSearch } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import Badge from "../components/Badge";
import { EmptyState, RequestList, SAMPLE_PLAN } from "../components/TestPlanPanels";
import { useToast } from "../context/ToastContext";
import {
  api,
  type StoredPlan,
  type TestPlanBrowser,
  type TestPlanBrowserEntry,
} from "../lib/api";
import { timeAgo } from "../lib/format";
import {
  IconActivity,
  IconChevronLeft,
  IconFile,
  IconFolder,
  IconFolderPlus,
  IconPencil,
  IconPlus,
  IconTrash,
} from "../icons";

type DialogState =
  | { type: "new-folder" }
  | { type: "rename-folder" }
  | { type: "new-plan" }
  | { type: "delete-folder" }
  | { type: "delete-plan"; plan: StoredPlan }
  | null;

export default function TestPlansPage() {
  const search = useSearch({ from: "/test-plans" });
  const currentDir = search.dir ?? "";
  const selectedPath = search.plan;
  const navigate = useNavigate();
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const [dialog, setDialog] = useState<DialogState>(null);

  const browserQuery = useQuery({
    queryKey: ["test-plan-browser", currentDir],
    queryFn: () => api.browseTestPlans(currentDir),
  });

  const planQuery = useQuery({
    queryKey: ["test-plan", selectedPath],
    queryFn: () => api.getTestPlanByPath(selectedPath ?? ""),
    enabled: Boolean(selectedPath),
  });

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ["test-plan-browser"] });
    queryClient.invalidateQueries({ queryKey: ["test-plans"] });
    queryClient.invalidateQueries({ queryKey: ["test-plan"] });
  };

  const createFolderMutation = useMutation({
    mutationFn: (name: string) => api.createTestPlanFolder(joinPath(currentDir, name)),
    onSuccess: () => {
      refresh();
      push("success", "Folder created.");
      setDialog(null);
    },
    onError: notifyError,
  });

  const renameFolderMutation = useMutation({
    mutationFn: (name: string) => api.renameTestPlanFolder(currentDir, name),
    onSuccess: () => {
      const parent = browserQuery.data?.parent ?? "";
      refresh();
      push("success", "Folder renamed.");
      setDialog(null);
      navigate({ to: "/test-plans", search: { dir: parent, plan: undefined } });
    },
    onError: notifyError,
  });

  const deleteFolderMutation = useMutation({
    mutationFn: () => api.deleteTestPlanFolder(currentDir),
    onSuccess: () => {
      const parent = browserQuery.data?.parent ?? "";
      refresh();
      push("info", "Folder deleted.");
      setDialog(null);
      navigate({ to: "/test-plans", search: { dir: parent, plan: undefined } });
    },
    onError: notifyError,
  });

  const createPlanMutation = useMutation({
    mutationFn: (name: string) =>
      api.createTestPlan({ name, directory: currentDir, content: SAMPLE_PLAN }),
    onSuccess: (plan) => {
      refresh();
      push("success", `Created ${plan.name}.`);
      setDialog(null);
      navigate({ to: "/test-plans/edit", search: { path: plan.path } });
    },
    onError: notifyError,
  });

  const deletePlanMutation = useMutation({
    mutationFn: (path: string) => api.deleteTestPlanByPath(path),
    onSuccess: () => {
      refresh();
      push("info", "Test plan deleted.");
      setDialog(null);
      navigate({ to: "/test-plans", search: { dir: currentDir, plan: undefined } });
    },
    onError: notifyError,
  });

  const executeMutation = useMutation({
    mutationFn: (path: string) => api.executeTestPlanByPath(path),
    onSuccess: (item) => {
      queryClient.invalidateQueries({ queryKey: ["execution-queue"] });
      push("info", `${item.planName} queued for execution.`);
    },
    onError: notifyError,
  });

  const openDirectory = (path: string) => {
    navigate({ to: "/test-plans", search: { dir: path, plan: undefined } });
  };

  const openPlan = (path: string) => {
    navigate({ to: "/test-plans", search: { dir: currentDir, plan: path } });
  };

  const plan = planQuery.data;
  const isBusy =
    createFolderMutation.isPending ||
    renameFolderMutation.isPending ||
    deleteFolderMutation.isPending ||
    createPlanMutation.isPending ||
    deletePlanMutation.isPending ||
    executeMutation.isPending;

  return (
    <div className="grid min-h-[calc(100vh-116px)] grid-cols-1 gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
      <section className="card flex min-h-[520px] flex-col">
        <div className="card-head">
          <h2>Explorer</h2>
          <span className="card-hint">{currentDir || "data/plans"}</span>
        </div>

        <div className="mb-3 flex flex-wrap items-center gap-2">
          <button className="btn btn-secondary btn-sm" onClick={() => setDialog({ type: "new-folder" })}>
            <IconFolderPlus size={15} /> Folder
          </button>
          <button className="btn btn-primary btn-sm" onClick={() => setDialog({ type: "new-plan" })}>
            <IconPlus size={15} /> Plan
          </button>
          {currentDir ? (
            <>
              <button
                className="icon-btn"
                onClick={() => setDialog({ type: "rename-folder" })}
                aria-label="Rename current folder"
              >
                <IconPencil size={15} />
              </button>
              <button
                className="icon-btn danger"
                onClick={() => setDialog({ type: "delete-folder" })}
                aria-label="Delete current folder"
              >
                <IconTrash size={15} />
              </button>
            </>
          ) : null}
        </div>

        <BrowserPanel
          browser={browserQuery.data}
          isError={browserQuery.isError}
          selectedPath={selectedPath}
          onOpenDirectory={openDirectory}
          onOpenPlan={openPlan}
        />
      </section>

      <section className="card min-h-[520px] overflow-hidden">
        {selectedPath && planQuery.isError ? (
          <EmptyState label="Could not load this test plan." />
        ) : selectedPath && !plan ? (
          <div className="skeleton" />
        ) : plan ? (
          <PlanPreview
            plan={plan}
            isBusy={isBusy}
            onEdit={() => navigate({ to: "/test-plans/edit", search: { path: plan.path } })}
            onExecute={() => executeMutation.mutate(plan.path)}
            onDelete={() => setDialog({ type: "delete-plan", plan })}
          />
        ) : (
          <EmptyState label="Select a test plan." />
        )}
      </section>

      {dialog ? (
        <PlanDialog
          dialog={dialog}
          currentDir={currentDir}
          isBusy={isBusy}
          onCancel={() => setDialog(null)}
          onCreateFolder={(name) => createFolderMutation.mutate(name)}
          onRenameFolder={(name) => renameFolderMutation.mutate(name)}
          onCreatePlan={(name) => createPlanMutation.mutate(name)}
          onDeleteFolder={() => deleteFolderMutation.mutate()}
          onDeletePlan={(path) => deletePlanMutation.mutate(path)}
        />
      ) : null}
    </div>
  );
}

function BrowserPanel({
  browser,
  isError,
  selectedPath,
  onOpenDirectory,
  onOpenPlan,
}: {
  browser?: TestPlanBrowser;
  isError: boolean;
  selectedPath?: string;
  onOpenDirectory: (path: string) => void;
  onOpenPlan: (path: string) => void;
}) {
  if (isError) return <EmptyState label="Could not load this folder." />;
  if (!browser) {
    return (
      <div className="flex flex-col gap-3">
        <div className="skeleton" />
        <div className="skeleton" />
        <div className="skeleton" />
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      {browser.parent !== null && browser.parent !== undefined ? (
        <button
          className="mb-2 flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-[0.86rem] text-ink-muted hover:bg-surface-2"
          onClick={() => onOpenDirectory(browser.parent ?? "")}
        >
          <IconChevronLeft size={16} />
          <span className="truncate">Parent folder</span>
        </button>
      ) : null}
      {browser.entries.length === 0 ? (
        <EmptyState label="This folder is empty." />
      ) : (
        <ul className="flex flex-col gap-1">
          {browser.entries.map((entry) => (
            <BrowserEntryRow
              key={`${entry.kind}:${entry.path}`}
              entry={entry}
              selected={selectedPath === entry.path}
              onOpenDirectory={onOpenDirectory}
              onOpenPlan={onOpenPlan}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function BrowserEntryRow({
  entry,
  selected,
  onOpenDirectory,
  onOpenPlan,
}: {
  entry: TestPlanBrowserEntry;
  selected: boolean;
  onOpenDirectory: (path: string) => void;
  onOpenPlan: (path: string) => void;
}) {
  const isDirectory = entry.kind === "directory";
  return (
    <li>
      <button
        className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition-colors ${
          selected ? "bg-accent-soft text-accent" : "text-ink hover:bg-surface-2"
        }`}
        onClick={() => (isDirectory ? onOpenDirectory(entry.path) : onOpenPlan(entry.path))}
      >
        {isDirectory ? <IconFolder size={17} /> : <IconFile size={17} />}
        <span className="min-w-0 flex-1 truncate text-[0.9rem] font-semibold">{entry.name}</span>
        {!isDirectory && entry.requestCount !== null && entry.requestCount !== undefined ? (
          <span className="font-mono text-[0.68rem] text-ink-faint">{entry.requestCount}</span>
        ) : null}
      </button>
    </li>
  );
}

function PlanPreview({
  plan,
  isBusy,
  onEdit,
  onExecute,
  onDelete,
}: {
  plan: StoredPlan;
  isBusy: boolean;
  onEdit: () => void;
  onExecute: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="card-head">
        <div className="min-w-0">
          <h2 className="truncate">{plan.name}</h2>
          <span className="card-hint block truncate">{plan.path}</span>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <button className="btn btn-secondary btn-sm" onClick={onEdit}>
            <IconPencil size={15} /> Edit
          </button>
          <button className="btn btn-primary btn-sm" onClick={onExecute} disabled={isBusy}>
            <IconActivity size={15} /> Execute
          </button>
          <button className="icon-btn danger" onClick={onDelete} disabled={isBusy} aria-label="Delete test plan">
            <IconTrash size={16} />
          </button>
        </div>
      </div>

      <div className="mb-4 flex flex-wrap gap-2">
        <Badge>{plan.parsed.requests.length} requests</Badge>
        <Badge>updated {timeAgo(plan.updatedAtMs)}</Badge>
        {plan.parsed.warnings.length > 0 ? <Badge tone="warn">{plan.parsed.warnings.length} warnings</Badge> : null}
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 overflow-y-auto 2xl:grid-cols-[minmax(0,0.95fr)_minmax(0,1.05fr)]">
        <div>
          <h3 className="mb-2 text-[0.8rem] font-semibold uppercase tracking-[0.14em] text-ink-faint">
            Parsed
          </h3>
          <RequestList plan={plan.parsed} />
          {plan.parsed.warnings.length > 0 ? (
            <div className="mt-3 rounded-md border border-warn/35 bg-warn/10 p-3 text-[0.82rem] text-warn">
              {plan.parsed.warnings.map((warning) => (
                <p key={warning}>{warning}</p>
              ))}
            </div>
          ) : null}
        </div>
        <div className="min-w-0">
          <h3 className="mb-2 text-[0.8rem] font-semibold uppercase tracking-[0.14em] text-ink-faint">
            File Content
          </h3>
          <pre className="max-h-[620px] overflow-auto rounded-md border border-line bg-surface-2 p-3 font-mono text-[0.78rem] leading-relaxed text-ink">
            {plan.content}
          </pre>
        </div>
      </div>
    </div>
  );
}

function PlanDialog({
  dialog,
  currentDir,
  isBusy,
  onCancel,
  onCreateFolder,
  onRenameFolder,
  onCreatePlan,
  onDeleteFolder,
  onDeletePlan,
}: {
  dialog: Exclude<DialogState, null>;
  currentDir: string;
  isBusy: boolean;
  onCancel: () => void;
  onCreateFolder: (name: string) => void;
  onRenameFolder: (name: string) => void;
  onCreatePlan: (name: string) => void;
  onDeleteFolder: () => void;
  onDeletePlan: (path: string) => void;
}) {
  const [value, setValue] = useState(dialog.type === "rename-folder" ? currentDir.split("/").pop() ?? "" : "");
  const isDelete = dialog.type === "delete-folder" || dialog.type === "delete-plan";
  const title = {
    "new-folder": "New Folder",
    "rename-folder": "Rename Folder",
    "new-plan": "New Plan",
    "delete-folder": "Delete Folder",
    "delete-plan": "Delete Test Plan",
  }[dialog.type];

  const submit = () => {
    const trimmed = value.trim();
    if (dialog.type === "new-folder" && trimmed) onCreateFolder(trimmed);
    if (dialog.type === "rename-folder" && trimmed) onRenameFolder(trimmed);
    if (dialog.type === "new-plan" && trimmed) onCreatePlan(trimmed);
    if (dialog.type === "delete-folder") onDeleteFolder();
    if (dialog.type === "delete-plan") onDeletePlan(dialog.plan.path);
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4">
      <div className="w-full max-w-md rounded-lg border border-line bg-surface p-4 shadow-xl">
        <div className="mb-3">
          <h2 className="text-[1rem] font-semibold text-ink">{title}</h2>
          {isDelete ? (
            <p className="mt-1 text-[0.84rem] text-ink-muted">
              {dialog.type === "delete-folder"
                ? `Delete empty folder ${currentDir}?`
                : `Delete ${dialog.plan.path}?`}
            </p>
          ) : null}
        </div>
        {!isDelete ? (
          <input
            className="input"
            value={value}
            onChange={(event) => setValue(event.target.value)}
            autoFocus
            aria-label={title}
          />
        ) : (
          <p className="rounded-md border border-err/35 bg-err/10 p-3 text-[0.84rem] text-err">
            This action removes the file system entry from data/plans.
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button className="btn btn-secondary" onClick={onCancel} disabled={isBusy}>
            Cancel
          </button>
          <button
            className={isDelete ? "btn btn-danger" : "btn btn-primary"}
            onClick={submit}
            disabled={isBusy || (!isDelete && !value.trim())}
          >
            {isDelete ? "Delete" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}

function joinPath(parent: string, child: string): string {
  return parent ? `${parent}/${child}` : child;
}
