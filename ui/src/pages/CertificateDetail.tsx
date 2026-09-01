import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useEffect, useState } from "react";
import Badge from "../components/Badge";
import { EmptyState } from "../components/TestPlanPanels";
import { useToast } from "../context/ToastContext";
import { api } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconCheck, IconChevronLeft, IconPencil, IconShield, IconTrash, IconX } from "../icons";

export default function CertificateDetailPage() {
  const { certificateId } = useParams({ strict: false }) as { certificateId: string };
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [hosts, setHosts] = useState("");
  const [enabled, setEnabled] = useState(true);
  const certificateQuery = useQuery({
    queryKey: ["certificate", certificateId],
    queryFn: () => api.getCertificate(certificateId),
  });
  const deleteMutation = useMutation({
    mutationFn: api.deleteCertificate,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      push("info", "Certificate deleted.");
      navigate({ to: "/certificates" });
    },
    onError: notifyError,
  });
  const updateMutation = useMutation({
    mutationFn: () =>
      api.updateCertificate(certificateId, {
        name,
        hosts: splitHosts(hosts),
        enabled,
      }),
    onSuccess: (certificate) => {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      queryClient.setQueryData(["certificate", certificate.id], certificate);
      push("success", "Certificate updated.");
      setEditing(false);
    },
    onError: notifyError,
  });
  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.setCertificateEnabled(id, enabled),
    onSuccess: (certificate) => {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      queryClient.setQueryData(["certificate", certificate.id], certificate);
      push("info", `${certificate.enabled ? "Enabled" : "Disabled"} ${certificate.name}.`);
    },
    onError: notifyError,
  });

  const loadedCertificate = certificateQuery.data;

  useEffect(() => {
    if (loadedCertificate && !editing) {
      setName(loadedCertificate.name);
      setHosts(loadedCertificate.hosts.join("\n"));
      setEnabled(loadedCertificate.enabled);
    }
  }, [loadedCertificate, editing]);

  if (certificateQuery.data === undefined) {
    return certificateQuery.isError ? (
      <section className="card">
        <EmptyState label="Certificate could not be loaded." />
      </section>
    ) : (
      <section className="card">
        <div className="skeleton" />
      </section>
    );
  }

  const certificate = certificateQuery.data;

  function submitEdit(event: FormEvent) {
    event.preventDefault();
    updateMutation.mutate();
  }

  return (
    <section className="card">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <Link className="mb-3 inline-flex items-center gap-1 text-sm text-ink-muted" to="/certificates">
            <IconChevronLeft size={16} /> Certificates
          </Link>
          <div className="flex min-w-0 items-center gap-2 text-ink">
            <IconShield size={20} />
            <h2 className="truncate text-[1rem] font-semibold">{certificate.name}</h2>
          </div>
          <p className="mt-1 break-all font-mono text-[0.72rem] text-ink-faint">
            {certificate.id}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button className="btn btn-secondary btn-sm" onClick={() => setEditing(true)}>
            <IconPencil size={15} /> Edit
          </button>
          <button
            className={certificate.enabled ? "btn btn-secondary btn-sm" : "btn btn-primary btn-sm"}
            onClick={() =>
              toggleMutation.mutate({
                id: certificate.id,
                enabled: !certificate.enabled,
              })
            }
            disabled={toggleMutation.isPending}
          >
            {certificate.enabled ? <IconX size={15} /> : <IconCheck size={15} />}
            {certificate.enabled ? "Disable" : "Enable"}
          </button>
          <button className="btn btn-danger btn-sm" onClick={() => setConfirmDelete(true)}>
            <IconTrash size={15} /> Delete
          </button>
        </div>
      </div>

      <div className="mb-5 flex flex-wrap gap-2">
        <Badge tone={certificate.enabled ? "ok" : "neutral"}>
          {certificate.enabled ? "enabled" : "disabled"}
        </Badge>
        {certificate.hosts.map((host) => (
          <Badge key={host}>{host}</Badge>
        ))}
      </div>

      {editing ? (
        <form
          className="mb-5 flex flex-col gap-4 rounded-lg border border-line bg-surface-2 p-4"
          onSubmit={submitEdit}
        >
          <label className="flex flex-col gap-1.5">
            <span className="text-[0.78rem] font-medium text-ink-muted">Name</span>
            <input
              className="input"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-[0.78rem] font-medium text-ink-muted">Hosts</span>
            <textarea
              className="input min-h-[112px]"
              value={hosts}
              onChange={(event) => setHosts(event.target.value)}
            />
          </label>
          <label className="flex items-center gap-2 text-[0.84rem] text-ink-muted">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(event) => setEnabled(event.target.checked)}
            />
            Enabled
          </label>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => setEditing(false)}
              disabled={updateMutation.isPending}
            >
              Cancel
            </button>
            <button className="btn btn-primary" disabled={updateMutation.isPending}>
              <IconCheck size={16} /> Save
            </button>
          </div>
        </form>
      ) : null}

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <DetailItem
          label="Subject Distinguished Name"
          value={valueOrEmpty(certificate.subjectDistinguishedName)}
        />
        <DetailItem
          label="Issuer Distinguished Name"
          value={valueOrEmpty(certificate.issuerDistinguishedName)}
        />
        <DetailItem label="Serial Number" value={valueOrEmpty(certificate.serialNumber)} />
        <DetailItem label="Valid From" value={valueOrEmpty(certificate.validFrom)} />
        <DetailItem label="Valid Until" value={valueOrEmpty(certificate.validUntil)} />
        <DetailItem label="Created" value={new Date(certificate.createdAtMs).toLocaleString()} />
        <DetailItem
          label="Updated"
          value={`${new Date(certificate.updatedAtMs).toLocaleString()} (${timeAgo(
            certificate.updatedAtMs,
          )})`}
        />
        <DetailItem label="Certificate File" value={certificate.certFileName} />
        <DetailItem label="Private Key File" value={certificate.keyFileName} />
        <DetailItem label="Stored Certificate Path" value={certificate.certPath} />
        <DetailItem label="Stored Key Path" value={certificate.keyPath} />
      </div>

      <div className="mt-4 rounded-lg border border-line bg-surface-2 p-3">
        <div className="font-mono text-[0.68rem] text-ink-faint uppercase">
          SHA-256 Fingerprint
        </div>
        <div className="mt-1 break-all font-mono text-[0.78rem] text-ink-muted">
          {certificate.fingerprintSha256}
        </div>
      </div>

      {confirmDelete ? (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4">
          <div className="w-full max-w-md rounded-lg border border-line bg-surface p-4 shadow-xl">
            <h2 className="text-[1rem] font-semibold text-ink">Delete Certificate</h2>
            <p className="mt-1 text-[0.84rem] text-ink-muted">
              Delete {certificate.name} and remove its certificate/key files?
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button className="btn btn-secondary" onClick={() => setConfirmDelete(false)}>
                Cancel
              </button>
              <button
                className="btn btn-danger"
                onClick={() => deleteMutation.mutate(certificate.id)}
                disabled={deleteMutation.isPending}
              >
                <IconTrash size={16} /> Delete
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function splitHosts(value: string): string[] {
  return value
    .split(/[\n,]+/)
    .map((host) => host.trim())
    .filter(Boolean);
}

function valueOrEmpty(value: string): string {
  return value.trim() || "Not available";
}

function DetailItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-surface-2 p-3">
      <div className="font-mono text-[0.68rem] text-ink-faint uppercase">{label}</div>
      <div className="mt-1 break-all text-[0.86rem] text-ink">{value}</div>
    </div>
  );
}
