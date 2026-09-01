import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useState } from "react";
import Badge from "../components/Badge";
import { EmptyState } from "../components/TestPlanPanels";
import { useToast } from "../context/ToastContext";
import { api, type CertificateConfig } from "../lib/api";
import { timeAgo } from "../lib/format";
import { IconCheck, IconShield, IconTrash, IconUpload, IconX } from "../icons";

export default function CertificatesPage() {
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const [uploadOpen, setUploadOpen] = useState(false);
  const certificatesQuery = useQuery({
    queryKey: ["certificates"],
    queryFn: api.listCertificates,
  });
  const deleteMutation = useMutation({
    mutationFn: api.deleteCertificate,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      push("info", "Certificate deleted.");
    },
    onError: notifyError,
  });
  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.setCertificateEnabled(id, enabled),
    onSuccess: (certificate) => {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      queryClient.invalidateQueries({ queryKey: ["certificate", certificate.id] });
      push("info", `${certificate.enabled ? "Enabled" : "Disabled"} ${certificate.name}.`);
    },
    onError: notifyError,
  });
  const certificates = certificatesQuery.data ?? [];

  return (
    <>
      <section className="card">
        <div className="card-head">
          <h2>Certificates</h2>
          <span className="card-hint mr-auto">{certificates.length} configured</span>
          <button className="btn btn-primary btn-sm" onClick={() => setUploadOpen(true)}>
            <IconUpload size={15} /> Upload
          </button>
        </div>

        {certificatesQuery.data === undefined ? (
          certificatesQuery.isError ? (
            <p className="py-2 text-[0.86rem] text-err">Could not load certificates.</p>
          ) : (
            <div className="flex flex-col gap-3 py-2">
              <div className="skeleton" />
              <div className="skeleton" />
              <div className="skeleton" />
            </div>
          )
        ) : certificates.length === 0 ? (
          <EmptyState label="No client certificates configured yet." />
        ) : (
          <ul className="flex max-h-[720px] flex-col overflow-y-auto">
            {certificates.map((certificate, index) => (
              <CertificateRow
                key={certificate.id}
                certificate={certificate}
                order={index + 1}
                deleting={deleteMutation.variables === certificate.id}
                toggling={toggleMutation.variables?.id === certificate.id}
                onToggle={() =>
                  toggleMutation.mutate({
                    id: certificate.id,
                    enabled: !certificate.enabled,
                  })
                }
                onDelete={() => deleteMutation.mutate(certificate.id)}
              />
            ))}
          </ul>
        )}
      </section>

      {uploadOpen ? <UploadDialog onClose={() => setUploadOpen(false)} /> : null}
    </>
  );
}

function CertificateRow({
  certificate,
  order,
  deleting,
  toggling,
  onToggle,
  onDelete,
}: {
  certificate: CertificateConfig;
  order: number;
  deleting: boolean;
  toggling: boolean;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);

  return (
    <li className="border-b border-line py-3 last:border-b-0">
      <div className="flex items-start gap-2">
        <Link
          className="min-w-0 flex-1 rounded-md text-ink transition-colors hover:text-accent"
          to="/certificates/$certificateId"
          params={{ certificateId: certificate.id }}
        >
          <div className="flex min-w-0 items-center gap-2">
            <IconShield size={17} />
            <strong className="truncate text-[0.95rem]">{certificate.name}</strong>
          </div>
          <span className="mt-1 block font-mono text-[0.7rem] text-ink-faint">
            order {order} · updated {timeAgo(certificate.updatedAtMs)}
          </span>
        </Link>
        <button
          className={certificate.enabled ? "btn btn-secondary btn-sm" : "btn btn-primary btn-sm"}
          onClick={onToggle}
          disabled={toggling || deleting}
        >
          {certificate.enabled ? <IconX size={15} /> : <IconCheck size={15} />}
          {certificate.enabled ? "Disable" : "Enable"}
        </button>
        <button
          className="icon-btn danger"
          onClick={() => setConfirmDelete(true)}
          disabled={deleting}
          aria-label={`Delete certificate ${certificate.name}`}
        >
          <IconTrash size={16} />
        </button>
      </div>
      <div className="mt-2 flex flex-wrap gap-2">
        <Badge tone={certificate.enabled ? "ok" : "neutral"}>
          {certificate.enabled ? "enabled" : "disabled"}
        </Badge>
        {certificate.hosts.map((host) => (
          <Badge key={host}>{host}</Badge>
        ))}
      </div>

      {confirmDelete ? (
        <div className="mt-3 rounded-lg border border-err/35 bg-err/10 p-3">
          <p className="text-[0.84rem] text-err">
            Delete {certificate.name} and remove its certificate/key files?
          </p>
          <div className="mt-3 flex gap-2">
            <button className="btn btn-secondary btn-sm" onClick={() => setConfirmDelete(false)}>
              Cancel
            </button>
            <button className="btn btn-danger btn-sm" onClick={onDelete} disabled={deleting}>
              <IconTrash size={15} /> Delete
            </button>
          </div>
        </div>
      ) : null}
    </li>
  );
}

function UploadDialog({ onClose }: { onClose: () => void }) {
  const { push, notifyError } = useToast();
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [hosts, setHosts] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [cert, setCert] = useState<File | null>(null);
  const [key, setKey] = useState<File | null>(null);
  const uploadMutation = useMutation({
    mutationFn: () => {
      if (!cert || !key) {
        throw new Error("Certificate and private key files are required.");
      }
      return api.uploadCertificate({ name, hosts, enabled, cert, key });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      push("success", "Certificate uploaded.");
      onClose();
    },
    onError: notifyError,
  });

  function submit(event: FormEvent) {
    event.preventDefault();
    uploadMutation.mutate();
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4">
      <form
        className="flex w-full max-w-xl flex-col gap-4 rounded-lg border border-line bg-surface p-4 shadow-xl"
        onSubmit={submit}
      >
        <div>
          <h2 className="text-[1rem] font-semibold text-ink">Upload Client Certificate</h2>
          <p className="mt-1 text-[0.84rem] text-ink-muted">
            Unencrypted private keys only. Host order in this list is preserved.
          </p>
        </div>
        <label className="flex flex-col gap-1.5">
          <span className="text-[0.78rem] font-medium text-ink-muted">Name</span>
          <input className="input" value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="text-[0.78rem] font-medium text-ink-muted">Hosts</span>
          <textarea
            className="input min-h-[96px]"
            value={hosts}
            onChange={(event) => setHosts(event.target.value)}
            placeholder={"api.example.com\n*.internal.example.com"}
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
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <FileInput label="Certificate PEM" onChange={setCert} />
          <FileInput label="Private Key PEM" onChange={setKey} />
        </div>
        <div className="flex justify-end gap-2 border-t border-line pt-4">
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" disabled={uploadMutation.isPending}>
            <IconUpload size={16} /> Upload
          </button>
        </div>
      </form>
    </div>
  );
}

function FileInput({ label, onChange }: { label: string; onChange: (file: File | null) => void }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[0.78rem] font-medium text-ink-muted">{label}</span>
      <input
        className="input file:mr-3 file:rounded-md file:border-0 file:bg-surface-2 file:px-2 file:py-1 file:text-ink"
        type="file"
        accept=".pem,.crt,.cer,.key"
        onChange={(event) => onChange(event.target.files?.[0] ?? null)}
      />
    </label>
  );
}
