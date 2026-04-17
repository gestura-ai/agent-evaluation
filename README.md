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
- [Custom scenarios](#custom-scenarios)
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

# Load a custom agent profile from disk
./target/debug/agent-eval --config /path/to/my-agent.toml

# Load a custom scenarios file at runtime (no recompile)
./target/debug/agent-eval --scenarios /path/to/my-scenarios.json --dry-run

# Save a machine-readable report
./target/debug/agent-eval --agent gestura-full --json > reports/gestura-full.json
```

---

## CLI reference

### Single-agent mode (default)

```
agent-eval [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--agent <ID>` | `gestura-full` | Built-in profile to run — see `--list-agents` for IDs |
| `--config <PATH>` | _(built-in)_ | Load a custom agent profile TOML instead of a built-in |
| `--bin <PATH>` | _(from profile)_ | Override the agent binary path |
| `--scenario <ID>` | _(all)_ | Run a single scenario only — see `--list` for IDs |
| `--scenarios <PATH>` | _(built-in)_ | Load a custom `scenarios.json` instead of the embedded one |
| `--dry-run` | `false` | Skip subprocess calls; validate check logic on stub responses |
| `--json` | `false` | Emit JSON instead of the human-readable text report |
| `--quiet` / `-q` | `false` | Suppress all non-report output |
| `--verbose` / `-v` | `false` | Show full response and all check results for every variation |
| `--show-breaking` | `false` | Show agent response only for failed variations |
| `--list` | — | Print scenario IDs and exit |
| `--list-agents` | — | Print built-in agent profile IDs, modes, and descriptions, then exit |

### `suite` subcommand

```
agent-eval suite [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--families <FAMILY,...>` | _(all 5)_ | Comma-separated agent families: `gestura`, `claude-code`, `augment`, `codex`, `opencode` |
| `--agents <ID,...>` | _(all)_ | Explicit profile IDs — takes precedence over `--families` |
| `--scenario <ID,...>` | _(all)_ | Restrict to specific scenario IDs |
| `--scenarios <PATH>` | _(built-in)_ | Load a custom `scenarios.json` instead of the embedded one |
| `--output-dir <DIR>` | _(stdout)_ | Write per-agent JSON, comparison JSON, and HTML report here |
| `--format <FORMAT>` | `all` / `text` | Output format: `text`, `json`, `html`, `all`. Defaults to `all` when `--output-dir` is set |
| `--trials <N>` | `1` | Times each variation is run — scores averaged, pass/fail by majority vote |
| `--variation-delay-ms <MS>` | `2000` | Pause between subprocess calls; raise to stay within token-rate limits |
| `--dry-run` | `false` | Skip subprocess calls; validate check logic on stub responses |
| `--bin <PATH>` | _(from profile)_ | Override binary for every profile |
| `--quiet` / `-q` | `false` | Suppress per-variation lines; show only progress bars |

### `report` subcommand

```
agent-eval report --from <PATH> [--from <PATH> ...] [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `--from <PATH>` | _(required)_ | Directory of JSON reports or a single JSON file; repeat to merge runs |
| `--format <FORMAT>` | `text` | Output format: `text`, `json`, `html`, `all` |
| `--output-dir <DIR>` | _(stdout)_ | Write HTML/JSON here instead of stdout |

### Environment variables

| Variable | Description |
|---|---|
| `GESTURA_EVAL_AGENT` | Default agent ID when `--agent` is not supplied |
| `GESTURA_BIN` | Default binary path when `--bin` is not supplied |
| `RUST_LOG` | Tracing filter; defaults to `warn` (stderr only) |

---

## Test scenarios

14 scenarios × three prompt variations each = 42 total invocations per run.

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
| `s9_bug_diagnosis` | Bug Diagnosis | Identifies the specific bug, explains root cause, suggests a fix |
| `s10_security_review` | Security Review | Identifies and explains security vulnerabilities in code |
| `s11_system_design` | System Design | Compares technical approaches with explicit reasoning about trade-offs |
| `s12_instruction_following` | Instruction Following | Follows multi-constraint instructions exactly, including restrictions and required elements |
| `s13_regression_debugging` | Regression Debugging | Diagnoses why previously-working code now fails; pinpoints the regression |
| `s14_technical_communication` | Technical Communication | Communicates concepts clearly at the appropriate level for the stated audience |

> The full scenario definitions — prompts, expected keywords, word-count bounds, named
> checks, and multi-turn history — live in [`testdata/scenarios.json`](testdata/scenarios.json).
> This file is included in the repo so you can use it as a starting point for your own
> baseline test suite. See [Custom scenarios](#custom-scenarios) below for how to load
> your own version at runtime without recompiling.

---

## Custom scenarios

The built-in `testdata/scenarios.json` is compiled into the binary at build time, so
`agent-eval` works as a single self-contained executable with no extra files. When you
want to add, remove, or modify scenarios you have two options.

### Option A — Edit and recompile (permanent change)

1. Edit `testdata/scenarios.json` — add new scenario objects, adjust prompts, or change checks.
2. `cargo build --release`
3. The new scenarios are embedded in the binary and available everywhere, including CI.

This is the right choice when changing the official benchmark scenarios that apply to all
agents and all runs.

### Option B — Supply a file at runtime (no recompile)

Pass `--scenarios <PATH>` to load a JSON file that follows the same schema as
`testdata/scenarios.json`. The file is loaded at startup and replaces the built-in suite
for that run only — no recompile, no changes to the repo.

```bash
# Single-agent run with custom scenarios
agent-eval --scenarios ./my-scenarios.json --agent claude-code-full

# Multi-agent suite with custom scenarios
agent-eval suite --scenarios ./my-scenarios.json --families gestura,claude-code

# Dry-run to validate your scenario file without spawning any agents
agent-eval --scenarios ./my-scenarios.json --dry-run
```

This is the right choice for team-specific or project-specific scenario sets that live
outside this repo, or for rapid iteration on new scenario ideas before committing them.

### Scenario file schema

Start from a copy of `testdata/scenarios.json`. The top-level structure is:

```json
{
  "version": 1,
  "description": "...",
  "scenarios": [ ... ]
}
```

Each scenario:

```json
{
  "id": "my_scenario",
  "name": "Human-readable name",
  "category": "category_label",
  "description": "Short description of what is tested",
  "rubric": {
    "tools_enabled": false,
    "max_iterations": 1
  },
  "variations": [ ... ]
}
```

Each variation:

```json
{
  "id": "v1",
  "prompt": "The prompt sent to the agent",
  "history": [],
  "expected_keywords": ["word1", "word2"],
  "min_words": 20,
  "max_words": 200,
  "forbidden_patterns": ["regex.*pattern"],
  "checks": ["response_not_empty", "contains_expected_keyword"]
}
```

**Available named checks:**

| Check | What it verifies |
|---|---|
| `response_not_empty` | Response contains at least one non-whitespace character |
| `response_is_concise` | Word count ≤ `max_words` (default 100) |
| `response_is_substantive` | Word count ≥ `min_words` (default 20) |
| `contains_expected_keyword` | At least one `expected_keywords` entry appears (case-insensitive) |
| `no_forbidden_pattern` | None of `forbidden_patterns` (regex) match the response |
| `acknowledges_uncertainty` | Response uses hedging language when appropriate |
| `no_price_hallucination` | Response does not fabricate specific prices or costs |
| `has_verification_step` | Response mentions testing or verification |
| `has_structured_sections` | Response contains markdown headers or numbered sections |
| `builds_on_context` | Response references content from the provided conversation history |
| `no_external_api_suggestion` | Response does not suggest sending data to an external service |
| `summarizes_provided_content` | Response draws from explicitly provided content |
| `no_invented_detail` | Response does not introduce facts not present in the prompt |
| `root_cause_explained` | Response identifies and explains an underlying cause |
| `suggests_test` | Response proposes a test, assertion, or verification step |
| `has_recommendation` | Response makes a concrete recommendation |
| `no_fabricated_live_output` | Response does not simulate or fabricate tool/command output |
| `cites_source_material` | Response references the source material provided in the prompt |
| `confidence_declared` | Response explicitly states its confidence level |

The `min_words` / `max_words` fields on each variation act as implicit conciseness checks
in addition to any named checks in the `checks` array.

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

