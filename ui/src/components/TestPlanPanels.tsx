import Badge from "./Badge";
import type { ExecutionReport, ExecutionResult, TestPlan, TestPlanRequest } from "../lib/api";
import { IconCheckCircle, IconXCircle } from "../icons";

export const SAMPLE_PLAN = `@host = http://127.0.0.1:8080

### Health check
GET {{host}}/api/health
Accept: application/json

> {% client.test("health endpoint returns 200", () => {
  client.assert(response.status === 200);
}); %}

### Missing endpoint contract
GET {{host}}/api/this-does-not-exist
Accept: application/json

> {% client.test("unknown API route returns JSON 404", () => {
  client.assert(response.status === 404);
}); %}
`;

export function requestTone(method: string) {
  if (method === "GET") return "info";
  if (method === "POST" || method === "PUT" || method === "PATCH") return "accent";
  if (method === "DELETE") return "err";
  return "neutral";
}

export function EmptyState({ label }: { label: string }) {
  return <p className="py-3 text-[0.86rem] text-ink-faint">{label}</p>;
}

export function RequestList({ plan }: { plan: TestPlan }) {
  return (
    <div className="flex flex-col gap-3">
      {plan.warnings.length > 0 ? (
        <div className="rounded-lg border border-warn bg-warn-soft p-3 text-sm text-ink">
          {plan.warnings.map((warning) => (
            <p key={warning}>{warning}</p>
          ))}
        </div>
      ) : null}
      <ul className="flex max-h-[430px] flex-col overflow-y-auto">
        {plan.requests.map((request) => (
          <RequestItem key={request.id} request={request} />
        ))}
      </ul>
    </div>
  );
}

function RequestItem({ request }: { request: TestPlanRequest }) {
  return (
    <li className="border-b border-line py-3 last:border-b-0">
      <div className="mb-1.5 flex items-center gap-2">
        <Badge tone={requestTone(request.method)}>{request.method}</Badge>
        <strong className="min-w-0 flex-1 truncate text-[0.9rem]">{request.name}</strong>
      </div>
      <p className="break-all font-mono text-[0.75rem] text-ink-muted">{request.url}</p>
      <div className="mt-2 flex flex-wrap gap-2">
        <Badge>{request.headers.length} headers</Badge>
        <Badge>{request.body ? "body" : "no body"}</Badge>
        <Badge>{request.assertions.length} assertions</Badge>
        {request.preRequestScripts.length > 0 ? (
          <Badge>{request.preRequestScripts.length} pre scripts</Badge>
        ) : null}
        {request.responseHandlerScripts.length > 0 ? (
          <Badge>{request.responseHandlerScripts.length} response scripts</Badge>
        ) : null}
      </div>
    </li>
  );
}

export function ReportView({ report }: { report: ExecutionReport }) {
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-3 gap-2">
        <ReportStat label="Total" value={report.total} />
        <ReportStat label="Passed" value={report.passed} tone="ok" />
        <ReportStat label="Failed" value={report.failed} tone={report.failed ? "err" : "ok"} />
      </div>
      <ul className="flex max-h-[560px] flex-col overflow-y-auto">
        {report.results.map((result) => (
          <ResultItem key={result.id} result={result} />
        ))}
      </ul>
    </div>
  );
}

function ReportStat({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: number;
  tone?: "neutral" | "ok" | "err";
}) {
  const color = tone === "ok" ? "text-ok" : tone === "err" ? "text-err" : "text-ink";
  return (
    <div className="rounded-lg border border-line bg-surface-2 p-3">
      <div className="font-mono text-[0.68rem] text-ink-faint uppercase">{label}</div>
      <div className={`mt-1 text-xl font-semibold ${color}`}>{value}</div>
    </div>
  );
}

function ResultItem({ result }: { result: ExecutionResult }) {
  return (
    <li className="border-b border-line py-3 last:border-b-0">
      <div className="flex items-start gap-2">
        <span className={result.ok ? "mt-0.5 text-ok" : "mt-0.5 text-err"}>
          {result.ok ? <IconCheckCircle size={17} /> : <IconXCircle size={17} />}
        </span>
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex flex-wrap items-center gap-2">
            <Badge tone={requestTone(result.method)}>{result.method}</Badge>
            <strong className="min-w-0 text-[0.9rem]">{result.name}</strong>
            <span className="font-mono text-[0.7rem] text-ink-faint">{result.durationMs} ms</span>
          </div>
          <p className="break-all font-mono text-[0.73rem] text-ink-muted">{result.url}</p>
          <div className="mt-2 flex flex-wrap gap-2">
            <Badge tone={result.ok ? "ok" : "err"}>{result.status ?? "ERR"}</Badge>
            <Badge>{result.responseBytes} bytes</Badge>
          </div>
          {result.error ? <p className="mt-2 text-sm text-err">{result.error}</p> : null}
          {(result.diagnostics ?? []).length > 0 ? (
            <div className="mt-2 flex flex-col gap-2">
              {(result.diagnostics ?? []).map((diagnostic, index) => (
                <div
                  key={`${diagnostic.phase}-${index}`}
                  className="rounded-lg border border-err/35 bg-err-soft p-3 text-[0.78rem] text-err"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge tone="err">{diagnostic.kind}</Badge>
                    <span className="font-mono text-[0.7rem]">{diagnostic.phase}</span>
                  </div>
                  <p className="mt-2">{diagnostic.message}</p>
                  {diagnostic.details ? (
                    <pre className="mt-2 max-h-[120px] overflow-auto rounded-md bg-surface p-2 font-mono text-[0.7rem] whitespace-pre-wrap text-ink-muted">
                      {diagnostic.details}
                    </pre>
                  ) : null}
                  {diagnostic.sourcePreview ? (
                    <pre className="mt-2 max-h-[160px] overflow-auto rounded-md bg-surface p-2 font-mono text-[0.7rem] whitespace-pre-wrap text-ink-muted">
                      {diagnostic.sourcePreview}
                    </pre>
                  ) : null}
                </div>
              ))}
            </div>
          ) : null}
          {result.logs.length > 0 ? (
            <pre className="mt-2 max-h-[120px] overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-[0.72rem] whitespace-pre-wrap text-ink-muted">
              {result.logs.join("\n")}
            </pre>
          ) : null}
          {result.assertions.length > 0 ? (
            <ul className="mt-2 flex flex-col gap-1">
              {result.assertions.map((assertion) => (
                <li key={assertion.name} className="flex items-center gap-2 text-[0.8rem]">
                  <Badge tone={assertion.passed ? "ok" : "err"}>
                    {assertion.passed ? "PASS" : "FAIL"}
                  </Badge>
                  <span className="text-ink-muted">
                    {assertion.name}: {assertion.message}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
          {result.responsePreview ? (
            <pre className="mt-2 max-h-[160px] overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-[0.72rem] whitespace-pre-wrap text-ink-muted">
              {result.responsePreview}
            </pre>
          ) : null}
        </div>
      </div>
    </li>
  );
}
