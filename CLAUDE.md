# Agent Evaluation Harness

Standalone CLI tool for benchmarking agentic coding assistants. Extracted from `gestura-core-eval` in `gestura-app` and lives in its own repo.

## Quick Start

```bash
cargo build --release
# Dry run — validates eval logic without spawning any agent CLIs
./target/release/agent-eval --dry-run
```

## Binary

The binary is `agent-eval` (see `[[bin]]` in `Cargo.toml`). On Windows it is `agent-eval.exe`.

## Project Layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point and subcommand dispatch |
| `src/cli_runner.rs` | Spawns agent binary as subprocess, captures stdout |
| `src/evaluator.rs` | Deterministic rule-based response checker (no LLM required) |
| `src/judge.rs` | Optional LLM-as-judge semantic scoring |
| `src/orchestrator.rs` | Multi-profile suite coordination |
| `src/comparison.rs` | Cross-agent comparison and leaderboard generation |
| `src/report.rs` | JSON/text report serialization |
| `src/html_report.rs` | Standalone HTML dashboard generation |
| `src/config/mod.rs` | Config loading and profile merging |
| `src/config/types.rs` | Type definitions for all config fields |
| `agents/baseline.toml` | Universal defaults — all profiles inherit from this |
| `agents/*.toml` | Per-agent, per-mode profile overrides |
| `testdata/scenarios.json` | Evaluation scenarios embedded at compile time via `include_str!` |
| `scripts/` | Build and packaging helpers for CI/release |

## Agent Profiles

Profiles live in `agents/`. Each is a TOML file that overrides `agents/baseline.toml`. Naming convention is `{family}-{mode}.toml` (e.g., `claude-code-sandboxed.toml`).

Key sections:

- `[agent]` — `id`, `name`, `mode` (`autonomous` | `sandboxed` | `iterative`)
- `[model]` — provider, model name, temperature
- `[execution]` — `max_iterations`, `timeout_secs`, `retries`, `delay_between_variations_ms`
- `[subprocess]` — `bin` path, `args_prefix`, per-profile `env`
- `[thresholds]` — `min_variation_score`, `min_scenario_pass_rate`, `min_overall_score`

To add a profile: copy the nearest existing one, set `[agent].id` to a unique value, override only what differs from baseline.

## Evaluation Scenarios

`testdata/scenarios.json` is embedded at compile time. Edit it and recompile. Each scenario has an `id`, `name`, `description`, and a `variations` array where each variation has a `rubric` with the checks applied to the agent's response.

```bash
agent-eval --list          # print all scenario IDs
agent-eval --list-agents   # print all built-in profile IDs
```

## Required Environment Variables

| Variable | Used by |
|----------|---------|
| `ANTHROPIC_API_KEY` | `claude-code-*`, `opencode-*` |
| `GESTURA_ANTHROPIC_API_KEY` | `gestura-*` |
| `OPENAI_API_KEY` | `codex-*` |
| `AUGMENT_SESSION_AUTH` | `augment-*` (OAuth session; requires manual auth) |
| `RUST_LOG` | Tracing verbosity (default: `warn`) |

## Common Commands

```bash
# Single agent
agent-eval --agent gestura-full

# Single agent, specific scenario
agent-eval --agent claude-code-full --scenario s1_simple_query

# Verbose output (show full responses + all check results)
agent-eval --agent gestura-full --verbose

# Multi-agent suite with all output formats
agent-eval suite \
  --families gestura,claude-code,opencode \
  --format all \
  --output-dir ./reports

# Post-process saved JSON reports without re-running
agent-eval report \
  --from reports/gestura-full.json \
  --from reports/claude-code-full.json \
  --format html \
  --output-dir ./html
```

## CI / Release Pipeline

| Workflow | File | Trigger |
|----------|------|---------|
| CI | `.github/workflows/ci.yml` | Push / PR |
| Release | `.github/workflows/release.yml` | `v*` tag or manual dispatch |
| Eval Pages | `.github/workflows/eval-pages.yml` | Release published or manual dispatch |

**Release** produces signed CLI binaries:

- macOS: universal binary (aarch64 + x86_64), signed and notarized → `agent-eval-{tag}-macos-universal.tar.gz`
- Linux: x86_64 → `agent-eval-{tag}-linux-x86_64.tar.gz`
- Windows: x86_64, signed via eSigner → `agent-eval-{tag}-windows-x86_64.zip`

**Eval Pages** runs the suite against gestura, claude-code, and opencode, then publishes the HTML dashboard and JSON reports to GitHub Pages.

### Required Secrets & Variables for Release

macOS signing (all required for `publish`):

| Name | Kind |
|------|------|
| `APPLE_CERTIFICATE` | Secret (base64-encoded p12) |
| `APPLE_CERTIFICATE_PASSWORD` | Secret |
| `APPLE_PASSWORD` | Secret (app-specific password for notarytool) |
| `KEYCHAIN_PASSWORD` | Secret |
| `APPLE_SIGNING_IDENTITY` | Repo variable |
| `APPLE_TEAM_ID` | Repo variable |
| `APPLE_ID` | Repo variable |

Windows signing (optional; release proceeds unsigned if absent):

| Name | Kind |
|------|------|
| `ESIGNER_USERNAME` | Secret |
| `ESIGNER_PASSWORD` | Secret |
| `ESIGNER_CREDENTIAL_ID` | Secret |
| `ESIGNER_TOTP_SECRET` | Secret |

Eval Pages:

| Name | Kind |
|------|------|
| `ANTHROPIC_API_KEY` | Secret |
| `GESTURA_ANTHROPIC_API_KEY` | Secret (falls back to `ANTHROPIC_API_KEY`) |
| `GESTURA_APP_REPO` | Repo variable (default: `gestura-ai/gestura-app`) |

Optional runner overrides:

| Variable | Default |
|----------|---------|
| `RELEASE_MACOS_RUNNER` | `["macos-14"]` |
| `RELEASE_LINUX_RUNNER` | `["ubuntu-22.04"]` |
| `RELEASE_WINDOWS_RUNNER` | `["windows-2022"]` |
