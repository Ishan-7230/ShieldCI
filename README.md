# ShieldCI

AI-powered automated penetration testing for your CI pipeline. ShieldCI scans your application for security vulnerabilities using offensive security tools orchestrated by a local LLM.

## Why Local-First?

ShieldCI runs **entirely on your machine** — the engine, the LLM, and all security tooling. This is a deliberate design choice:

- **Your code never leaves your network.** No source code is uploaded to any third-party server. The LLM (Ollama) runs locally, scans happen inside a local Docker container, and results stay on disk.
- **Zero data leakage risk.** Unlike cloud-based security scanners, there is no API that receives your codebase. This matters for proprietary, enterprise, and pre-release code.
- **Full control.** You choose the model, the tools, and the scan depth. Nothing phones home.

> **Future: Hosted option for open-source repos**
>
> We plan to offer an optional hosted version of ShieldCI for projects that don't need code confidentiality (e.g. open-source repositories). This will let maintainers run scans without setting up a self-hosted runner. The local-first mode will always remain the default and recommended path for private codebases.

## Architecture

```
┌──────────────────────────────────────────────┐
│         Rust Orchestrator (src/main.rs)       │
│  1. Read shieldci.yml config                  │
│  2. Build & launch the target app             │
│  3. Generate dynamic test plan                │
│  4. Execute tools via MCP over Docker stdio   │
│  5. LLM-guided adaptive strikes               │
│  6. Generate Markdown report with code fixes   │
└──────────────┬───────────────────────────────┘
               │ docker run -i (JSON-RPC stdio)
┌──────────────▼───────────────────────────────┐
│       Kali Linux Docker Container             │
│  kali_mcp.py — MCP server exposing:           │
│    • sqlmap_scan   (SQL injection)            │
│    • nmap_scan     (port scanning)            │
│    • nikto_scan    (web vulnerability scan)   │
│    • gobuster_scan (directory brute-force)    │
│    • check_headers (security header audit)    │
└──────────────────────────────────────────────┘
```

## Prerequisites

- **Rust** (1.70+)
- **Docker Desktop**
- **Ollama** with a model installed (e.g. `ollama pull llama3.1`)

## Quick Start

```bash
# 1. Build the Rust orchestrator
cargo build --release

# 2. Build the Kali MCP Docker image
docker build -t shieldci-kali-image .

# 3. Add a shieldci.yml to your target repo (see below)

# 4. Run from your target repo directory
/path/to/shield-ci
```

## Configuration — `shieldci.yml`

Place a `shieldci.yml` in the root of the repository you want to scan. This tells ShieldCI how to build, run, and attack your app.

### Full Schema

```yaml
# ── Project metadata ──
project:
  name: "my-app"              # Project name
  framework: "Node.js"        # Framework (Node.js, Python, Rust, etc.)
  language: "javascript"       # Primary language

# ── Build & Run ──
build:
  command: "npm install"       # Build/install command
  run: "node app.js"           # Command to start the app
  port: 3000                   # Port the app listens on

# ── API Endpoints ──
# List all endpoints. Endpoints with params are automatically
# targeted for SQL injection and input validation testing.
endpoints:
  - path: "/"
    method: "GET"
    description: "Health check"

  - path: "/login"
    method: "GET"
    params:
      - name: "username"
        type: "string"
        description: "User login name"
    description: "User login endpoint - queries database"

  - path: "/api/search"
    method: "GET"
    params:
      - name: "query"
        type: "string"
        description: "Search term"
    description: "Search endpoint"

  - path: "/api/users"
    method: "POST"
    params:
      - name: "name"
        type: "string"
      - name: "email"
        type: "string"
      - name: "password"
        type: "string"
    description: "User registration"

# ── Database ──
database:
  type: "sqlite"               # sqlite, postgresql, mongodb, mysql, etc.
  orm: false                   # false = raw SQL queries (HIGH RISK flag)

# ── Authentication ──
auth:
  enabled: false               # Whether the app has auth

# ── Key source files ──
files:
  - "app.js"
  - "routes/auth.js"
```

### Schema Reference

| Section | Field | Type | Required | Description |
|---------|-------|------|----------|-------------|
| `project` | `name` | string | no | Project name |
| `project` | `framework` | string | no | Framework (Node.js, Python, Rust, etc.) |
| `project` | `language` | string | no | Primary language |
| `build` | `command` | string | no | Build/install command |
| `build` | `run` | string | no | Command to start the app |
| `build` | `port` | integer | no | Port the app listens on (default: 3000) |
| `endpoints[]` | `path` | string | **yes** | URL path (e.g. `/login`) |
| `endpoints[]` | `method` | string | no | HTTP method (default: GET) |
| `endpoints[]` | `description` | string | no | What this endpoint does |
| `endpoints[].params[]` | `name` | string | **yes** | Parameter name |
| `endpoints[].params[]` | `type` | string | no | Parameter type (string, integer, etc.) |
| `endpoints[].params[]` | `description` | string | no | What this parameter is for |
| `database` | `type` | string | no | Database engine |
| `database` | `orm` | boolean | no | `true` = ORM, `false` = raw SQL (triggers extra SQLi tests) |
| `auth` | `enabled` | boolean | no | Whether the app uses authentication |
| `files` | | string[] | no | Key source files to focus analysis on |

### How It Drives Testing

- **Endpoints with params** → automatically generate `sqlmap_scan` attack URLs
- **`database.orm: false`** → flags raw SQL usage, prioritizes SQLi testing on all param endpoints
- **Param names** like `username`, `password`, `search`, `query`, `id`, `email` → auto-targeted for injection
- **`build.port`** → used to construct the target URL
- If no `shieldci.yml` exists, ShieldCI falls back to auto-detection via `run.sh`

## Test Phases

ShieldCI runs a dynamic multi-phase test plan:

| Phase | Tool | What It Does |
|-------|------|--------------|
| RECON | `nmap_scan` | Port scan to discover services |
| RECON | `check_headers` | Check for missing security headers (CSP, X-Frame-Options, etc.) |
| VULN SCAN | `nikto_scan` | Scan for known web server vulnerabilities |
| DISCOVERY | `gobuster_scan` | Brute-force hidden directories and files |
| SQLi | `sqlmap_scan` | SQL injection testing on each endpoint with params |
| ADAPTIVE | LLM-guided | LLM analyzes all results and picks additional targeted attacks |

## Output

ShieldCI generates `SHIELD_REPORT.md` (human-readable) and `shield_results.json` (structured, for dashboard ingestion) in the target repo.

`SHIELD_REPORT.md` contains:

- **Executive summary** of findings
- **Scan results** per tool with severity ratings
- **Vulnerable code snippets** — exact lines from your source
- **Recommended fixes** — corrected code with explanations
- **Security header** and configuration findings
- **Actionable recommendations**

## Example

```bash
# Run against the included test app (intentionally vulnerable Express.js app)
cd tests
../target/release/shield-ci
cat SHIELD_REPORT.md
```

## Dashboard Integration

ShieldCI can push scan results to the [Shield-CI dashboard](https://github.com/Zenith1415/Shield-CI) (Next.js frontend):

```bash
# After a scan, push results to your local dashboard
SHIELDCI_API_URL=http://localhost:3000 \
SHIELDCI_API_KEY=your-secret-key \
SHIELDCI_REPO=owner/repo \
python3 push_results.py
```

For CI, this is handled automatically by the GitHub Actions workflow using a **self-hosted runner** — keeping everything local.

## Project Structure

```
├── src/main.rs          # Rust orchestrator
├── kali_mcp.py          # Python MCP tool server (runs inside Docker)
├── Dockerfile           # Kali Linux container with security tools
├── Cargo.toml           # Rust dependencies
├── push_results.py      # Push structured results to dashboard API
├── .github/workflows/
│   └── shieldci.yml     # GitHub Actions workflow (self-hosted runner)
├── run.sh               # Auto-detection fallback script
├── detector.sh          # Full repo profiler
└── tests/
    ├── app.js           # Intentionally vulnerable Express.js app
    ├── shieldci.yml     # Example configuration
    └── package.json     # Test app dependencies
```

## License

Apache 2.0
