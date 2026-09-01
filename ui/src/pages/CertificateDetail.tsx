import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import Badge from "../components/Badge";
import { EmptyState } from "../components/TestPlanPanels";
import { useToast } from "../context/ToastContext";
import { api } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconChevronLeft, IconShield, IconTrash } from "../icons";

export default function CertificateDetailPage() {
  const { certificateId } = useParams({ strict: false }) as { certificateId: string };
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [confirmDelete, setConfirmDelete] = useState(false);
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
        <button className="btn btn-danger btn-sm" onClick={() => setConfirmDelete(true)}>
          <IconTrash size={15} /> Delete
        </button>
      </div>

      <div className="mb-5 flex flex-wrap gap-2">
        <Badge tone={certificate.enabled ? "ok" : "neutral"}>
          {certificate.enabled ? "enabled" : "disabled"}
        </Badge>
        {certificate.hosts.map((host) => (
          <Badge key={host}>{host}</Badge>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
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

function DetailItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-surface-2 p-3">
      <div className="font-mono text-[0.68rem] text-ink-faint uppercase">{label}</div>
      <div className="mt-1 break-all text-[0.86rem] text-ink">{value}</div>
    </div>
  );
}
