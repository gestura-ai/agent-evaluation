# Agent Evaluation

Evaluation harness for the agentic response quality. Drives any supported
agent CLI as a **black-box subprocess** across standardised test scenarios and produces
structured pass/fail reports with a per-variation score.

---

## Contents

- [How it works](#how-it-works)
- [Quick start](#quick-start)
- [Container](#container)
- [CLI reference](#cli-reference)
- [Test scenarios](#test-scenarios)
- [Agent profiles](#agent-profiles)
- [API keys and credentials](#api-keys-and-credentials)

---

## How it works

```
agent-eval
    │
    ├── loads agent profile (TOML) ──► binary + args_prefix + env + thresholds
    │
    ├── for each scenario variation:
    │       └── spawn subprocess: <bin> [args_prefix...] "<prompt>"
    │               └── capture stdout
    │
    ├── RuleEvaluator ──► deterministic keyword / length / pattern checks
    │
    └── EvalReport ──► text table or JSON
```

---

## Container

The evaluation harness ships a multi-stage `Containerfile` that produces a self-contained
image with all agent CLIs (`claude`, `codex`, `opencode`, `gestura`) and
`agent-eval` binaries. Credentials are never baked into the image — they are passed at
runtime via environment variables.

**Files:**

```

├── Containerfile       — three-stage build (cargo-chef → builder → runtime)
└── .containerignore    — excludes target/, .env, .git, node_modules, etc.
```

**Build** (run from repo root):

```bash
# Podman
podman build \
  -f Containerfile \
  --ignorefile .containerignore \
  -t agent-eval .

# Docker (BuildKit required for --ignorefile equivalent)
docker build \
  -f Containerfile \
  -t agent-eval .
```

**Run with inline credentials:**

> **Gestura uses a `GESTURA_` prefix** for all its env vars. Pass `GESTURA_ANTHROPIC_API_KEY`
> for `gestura-*` profiles **and** bare `ANTHROPIC_API_KEY` for `claude-code-*` / `opencode-*`.

```bash
# gestura-* profiles — needs the GESTURA_ prefixed var
docker run --rm \
  -e GESTURA_ANTHROPIC_API_KEY=sk-ant-... \
  agent-eval --agent gestura-full

# claude-code-* / opencode-* — needs bare ANTHROPIC_API_KEY
docker run --rm \
  -e ANTHROPIC_API_KEY=sk-ant-... \
  agent-eval --agent claude-code-full

# codex-* — needs bare OPENAI_API_KEY
docker run --rm \
  -e OPENAI_API_KEY=sk-... \
  agent-eval --agent codex-full
```

**Run with an `.env` file** (keep credentials out of shell history):

```bash
# .env  — never commit this file
# GESTURA_ANTHROPIC_API_KEY=sk-ant-...   # for gestura-* profiles
# ANTHROPIC_API_KEY=sk-ant-...           # for claude-code-*, opencode-*
# OPENAI_API_KEY=sk-...                  # for codex-*

docker run --rm --env-file .env agent-eval --agent gestura-full
docker run --rm --env-file .env agent-eval --agent claude-code-full
docker run --rm --env-file .env agent-eval --agent codex-full --dry-run --json
```

**All supported runtime env vars:**

| Variable | Required for | Description |
|---|---|---|
| `GESTURA_ANTHROPIC_API_KEY` | gestura-\* | **Required for Gestura.** Gestura reads all config via `GESTURA_` prefix |
| `ANTHROPIC_API_KEY` | claude-code-\*, opencode-\* | **Required.** Bare key read directly by those CLIs (no prefix) |
| `OPENAI_API_KEY` | codex-\* | **Required for Codex.** Bare key read directly by Codex CLI |
| `AUGMENT_SESSION_AUTH` | augment-\* | OAuth session JSON from `auggie token print`; excluded from automated runs |
| `AUGMENT_DISABLE_AUTO_UPDATE` | augment-\* | Pre-set to `1`; disables self-update checks in CI |
| `GESTURA_GROK_API_KEY` | custom gestura profile with `llm.primary=grok` | Grok / xAI key (`xai-...`) |
| `GESTURA_GEMINI_API_KEY` | custom gestura profile with `llm.primary=gemini` | Google Gemini API key |
| `GESTURA_DISABLE_KEYCHAIN` | gestura-\* | Pre-set to `1` in the image; prevents keychain hangs |
| `RUST_LOG` | agent-eval binary | Tracing verbosity; defaults to `warn` (stderr only) |

> **`augment-*` profiles — Auggie auth:** Auggie uses an account session token, not a raw API key.
> 1. Install: `npm install -g @augmentcode/auggie` (Node.js 22+ required)
> 2. Log in once on a real machine: `auggie login`
> 3. Export the session: `export AUGMENT_SESSION_AUTH=$(auggie token print)`
> 4. Pass it to the container: `podman run --rm -e AUGMENT_SESSION_AUTH="$AUGMENT_SESSION_AUTH" agent-eval --agent augment-full`

---

## Quick start

```bash
# Build the eval binary and the gestura CLI
cargo build -p gestura-cli -p agent-evaluation

# List scenario IDs
./target/debug/agent-eval --list

# List all built-in agent profiles
./target/debug/agent-eval --list-agents

# Dry-run — validates check logic, no subprocess calls, no credentials needed
./target/debug/agent-eval --dry-run

# Full run with the default gestura-full profile
export ANTHROPIC_API_KEY=sk-ant-...
./target/debug/agent-eval

# Run against a specific agent profile
./target/debug/agent-eval --agent claude-code-full

# Single scenario, JSON output
./target/debug/agent-eval --agent codex-sandboxed --scenario s3_planning --json

# Load a custom profile from disk
./target/debug/agent-eval --config /path/to/my-agent.toml

# Save a machine-readable report
./target/debug/agent-eval --agent gestura-full --json > reports/gestura-full.json
```

---

## CLI reference

| Flag | Description |
|---|---|
| `--agent <ID>` | Built-in agent profile to use (default: `gestura-full`) |
| `--config <PATH>` | Load a custom TOML profile from disk instead of a built-in |
| `--bin <PATH>` | Override the agent binary path regardless of what the profile specifies |
| `--scenario <ID>` | Run a single scenario only (use `--list` to see IDs) |
| `--dry-run` | Skip subprocess calls; run check logic on stub responses |
| `--list` | Print scenario IDs and exit |
| `--list-agents` | Print built-in agent profile IDs, modes, and descriptions, then exit |
| `--json` | Emit JSON output instead of the human-readable text report |
| `--quiet` / `-q` | Suppress progress output (implies JSON) |

Environment variables:

| Variable | Description |
|---|---|
| `GESTURA_EVAL_AGENT` | Default agent ID when `--agent` is not supplied |
| `GESTURA_BIN` | Default binary path when `--bin` is not supplied |
| `RUST_LOG` | Tracing filter; defaults to `warn` (stderr only) |

---

## Test scenarios

Eight scenarios × three prompt variations each = 24 total invocations per run.

| ID | Name | What is tested |
|---|---|---|
| `s1_simple_query` | Simple Single-Turn Query | Direct factual answer, no padding, uncertainty acknowledged where appropriate |
| `s2_multi_turn` | Multi-Turn Conversation | Context retention and coherence across a four-turn history |
| `s3_planning` | Complex Multi-Step Planning | Structured output, explicit assumptions, no fabricated specifics |
| `s4_error_handling` | Error Handling and Verification | Root cause explained, fix provided, test suggested |
| `s5_tool_extensibility` | Tool Calling and Extensibility | Full definition → registration → invocation chain, no fabricated live output |
| `s6_privacy` | Privacy-Sensitive Local Task | No external API suggestions; honest about data access model |
| `s7_context_retention` | Context Retention | Recalls only facts from the provided set; never invents unlisted details |
| `s8_long_context` | Long-Context Coherence | Grounds answer in document text; distinguishes explicit from inferred |

Scenario definitions live in `testdata/scenarios.json`. Each variation declares the prompt,
expected keywords, word-count bounds, and the named checks that must pass.

---

## Agent profiles

Built-in profiles are TOML files embedded in the binary at compile time via `include_str!`.
They are stored in `agents/` and define the full execution contract for one agent.

```
agents/
├── baseline.toml              # Universal defaults — every profile inherits from this
├── gestura-full.toml          # Gestura CLI · autonomous · all tools
├── gestura-sandboxed.toml     # Gestura CLI · sandboxed · no tools / no network
├── gestura-iterative.toml     # Gestura CLI · iterative · confirmation gates
├── claude-code-full.toml      # Claude Code · --dangerously-skip-permissions
├── claude-code-sandboxed.toml # Claude Code · --allowedTools "" (no tools)
├── claude-code-iterative.toml # Claude Code · default confirmation gates
├── augment-full.toml          # Augment Code · autonomous
├── augment-sandboxed.toml     # Augment Code · sandboxed
├── augment-iterative.toml     # Augment Code · iterative
├── codex-full.toml            # OpenAI Codex · --approval-mode full-auto
├── codex-sandboxed.toml       # OpenAI Codex · --sandbox
├── codex-iterative.toml       # OpenAI Codex · --approval-mode suggest
├── opencode-full.toml         # OpenCode · --yes
├── opencode-sandboxed.toml    # OpenCode · --no-tools
└── opencode-iterative.toml    # OpenCode · --interactive
```

Each profile merges on top of `baseline.toml` — only the fields that differ need to be
declared. The merge is deep: table fields are merged key-by-key; scalar and array fields
are replaced wholesale by the overlay value.

Profile anatomy:

```toml
[agent]
id   = "gestura-full"
name = "Gestura — Full Permission (Autonomous)"
mode = "autonomous"   # autonomous | sandboxed | iterative

[model]
provider    = "anthropic"
name        = "claude-sonnet-4-6"
temperature = 0.7
max_tokens  = 8192

[subprocess]
bin         = "gestura"               # binary to invoke; omit for auto-detect
args_prefix = ["exec"]               # prepended before the prompt argument
response_strip_prefix = "Assistant:" # optional: strip this prefix from stdout

[subprocess.env]
ANTHROPIC_API_KEY = ""               # forwarded to every subprocess invocation

[thresholds]
min_variation_score    = 0.85   # fraction of checks that must pass per variation
min_scenario_pass_rate = 1.0    # fraction of variations that must pass per scenario
min_overall_score      = 0.85   # mean score across all variations

[scenarios.s3_planning.variation_overrides.v1]
min_words         = 120
additional_checks = ["has_structured_sections"]
```

---

## API keys and credentials

### Quick reference

| Profile group | Binary | Required credential | Source |
|---|---|---|---|
| `gestura-*` | `gestura` | `ANTHROPIC_API_KEY` or `~/.gestura/config.yaml` | Anthropic Console |
| `claude-code-*` | `claude` | `ANTHROPIC_API_KEY` | Anthropic Console |
| `augment-*` | `augment` | `ANTHROPIC_API_KEY` (Claude-backed) | Augment workspace login |
| `codex-*` | `codex` | `OPENAI_API_KEY` | OpenAI Platform |
| `opencode-*` | `opencode` | `ANTHROPIC_API_KEY` (profiles use Claude) | Anthropic Console |

