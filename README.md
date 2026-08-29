# Gate Keeper

[![CI](https://github.com/prasenjit-net/gate-keeper/actions/workflows/ci.yml/badge.svg)](https://github.com/prasenjit-net/gate-keeper/actions/workflows/ci.yml)

Gate Keeper is an **HTTP REST API test case designer and executor**. It pairs an
[Axum](https://github.com/tokio-rs/axum) backend with a React SPA so teams can
model request scenarios, run suites, and track execution telemetry from one
single-binary application.

## Features

- **REST API test case workflow** - in-memory test case CRUD demonstrates the
  shape for designing, queuing, executing, reopening, and deleting API checks
- **Runner telemetry** - live metrics and activity events stream over WebSocket
  for dashboards, suite progress, and connected client status
- **Consistent API errors** - every backend error uses the shared `AppError`
  JSON envelope and the frontend renders failures as typed notifications
- **Embedded SPA** - `ui/dist` is compiled into the Rust binary with
  `rust-embed`; deep links work via the SPA fallback and unknown `/api/*` paths
  return JSON 404s
- **Configurable UI boot** - the SPA loads `GET /api/config`, sourced from the
  `[ui]` section of `config.toml`
- **Light / dark / auto theme** - applied before first paint and persisted in
  `localStorage`
- **Operational shell** - dashboard, test lab, settings, responsive sidebar,
  access logging, and render-error boundaries are already wired
- **TOML configuration + CLI overrides** - server, logging, and UI values can be
  overridden with flags

## Quick Start

Prerequisites: Rust stable and Node 18+.

```sh
# 1. Build the frontend (embedded by the Rust build)
cd ui && npm install && npm run build && cd ..

# 2. Build + run the single binary
cargo run --release
# -> http://127.0.0.1:8080
```

Or use `make build` / `make run`.

### Development

Run the two dev servers side by side:

```sh
cargo run                # terminal 1: backend on :8080
cd ui && npm run dev     # terminal 2: Vite on :5173, proxies /api and /ws
```

Open http://localhost:5173.

## CLI

```text
gate-keeper [OPTIONS]

  -c, --config <CONFIG>        Path to the TOML configuration file [default: config.toml]
      --host <HOST>            Override [server].host
  -p, --port <PORT>            Override [server].port
      --log-level <LOG_LEVEL>  Override [logging].level (trace, debug, info, warn, error)
      --access-log <PATH>      Override [logging].access_log file path
      --no-access-log          Disable the access log file entirely
```

## Configuration

Runtime config defaults to `config.toml`.

| Key | Default | Meaning |
|---|---|---|
| `server.host` | `127.0.0.1` | Bind address |
| `server.port` | `8080` | Bind port |
| `logging.level` | `info` | Tracing filter |
| `logging.access_log` | `access.log` | Access-log file path; omit to disable the file |
| `ui.app_name` | `Gate Keeper` | Shown in the sidebar and browser title |
| `ui.tagline` | `Design, run, and track REST API test cases` | Shown under the app name |
| `ui.default_theme` | `auto` | `light` \| `dark` \| `auto` |
| `ui.repo_url` | `https://github.com/prasenjit-net/gate-keeper` | Sidebar repository link |

If the config file is missing, built-in defaults are used with a warning.

## Data Storage

Gate Keeper stores runtime data in the local `data/` folder:

- `data/plans/index.json` contains saved HTTP plans and their parsed previews.
- `data/executions/index.json` contains saved execution summaries.
- `data/reports/<execution-id>.json` contains each full execution report.
- `data/reports/<execution-id>.log` contains the plain-text execution log.

Execution queue status is intentionally in-memory. Completed execution summaries,
reports, and logs are saved to disk.

## API

The current API keeps the original task route names while the UI presents them
as REST test cases. Those routes are the seed workflow for future persisted test
case storage and execution history.

| Method | Path | Description |
|---|---|---|
| GET | `/api/health` | Liveness + version |
| GET | `/api/config` | UI bootstrap config |
| GET | `/api/metrics` | Latest runner metrics snapshot |
| POST | `/api/http-plans/preview` | Parse a JetBrains-style `.http` plan without executing it |
| GET | `/api/http-plans` | List saved HTTP plans |
| POST | `/api/http-plans` | Save a new HTTP plan |
| GET | `/api/http-plans/{id}` | Get a saved HTTP plan detail |
| PUT | `/api/http-plans/{id}` | Update a saved HTTP plan |
| DELETE | `/api/http-plans/{id}` | Delete a saved HTTP plan |
| POST | `/api/http-plans/{id}/execute` | Queue a saved HTTP plan for asynchronous execution |
| GET | `/api/executions` | List saved execution reports |
| GET | `/api/execution-queue` | List queued and running execution status |
| GET | `/api/executions/{id}` | View a saved execution report and log |
| DELETE | `/api/executions/{id}` | Delete a saved execution report and log |
| GET | `/api/tasks` | List test cases |
| POST | `/api/tasks` | Create (`{"title": "GET /users/{id} returns 200"}`; empty title -> 400) |
| POST | `/api/tasks/{id}/toggle` | Mark executed or reopen |
| DELETE | `/api/tasks/{id}` | Delete (204) |
| GET | `/api/error-demo?kind=...` | Demo failures: `internal`, `bad-request`, `not-found` |
| GET | `/ws` | WebSocket server-push events |

Errors always look like:

```json
{ "error": { "code": "NOT_FOUND", "message": "task 42 does not exist", "status": 404 } }
```

WebSocket events are JSON discriminated by `type`:

```json
{ "type": "metrics", "data": { "cpu": 41.3, "memory": 58.0, "requestsPerMin": 12 } }
{ "type": "activity", "kind": "test-case", "message": "Test case \"GET /health\" created", "timestampMs": 0 }
{ "type": "hello", "message": "Connected to Gate Keeper v0.1.0", "timestampMs": 0 }
```

## Project Structure

```text
├── config.toml               server + UI configuration
├── src/
│   ├── main.rs               CLI + startup
│   ├── config.rs             TOML config model
│   ├── error.rs              AppError -> JSON error envelope
│   ├── access_log.rs         access-log middleware
│   ├── static_assets.rs      embedded SPA + fallback routing
│   ├── routes/               REST and WebSocket handlers
│   └── services/             metrics, test case store, events
└── ui/
    └── src/
        ├── lib/api.ts        typed client + ApiError
        ├── context/          Theme, Toast, Config, Live
        ├── components/       layout, cards, chart, feed, controls
        ├── pages/            Dashboard, Test Lab, Settings, 404
        └── styles/           Tailwind v4 entry + theme tokens
```

## Testing

```sh
cargo test
cd ui && npm test
```

For broader verification, also run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cd ui && npm run build
```

## License

[MIT](LICENSE)
