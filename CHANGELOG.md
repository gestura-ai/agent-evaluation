# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## UnReleased

### Changed

- Clean up env vars that cause agent misbehavior
- Create an empty working directory for each subprocess
- Remove the tree kill from the Ok path (main process exited normally)
- Move the tree kill to the timeout path where it is needed

## [0.2.10] - 2026-04-18

### Changed

- thread-safe pgrep-recursive tree kill implementation

## [0.2.9] - 2026-04-18

fix: replace pgrep-recursive tree kill with single-pass /proc scan

The previous kill_process_tree spawned one pgrep subprocess per process

level (n pgrep calls + n kill calls for a depth-n tree). On busy CI

runners this was slow and had PID-reuse races between pgrep and kill,

causing new hangs at s2_multi_turn that weren't present before.

New approach:

Read /proc once to build a PPID→children map (no subprocess spawning)

BFS from root_pid to collect all descendants in one pass

Send a single kill -9 <pid1> <pid2> ... for all collected PIDs

Also remove the tree kill from the Ok path (main process exited normally):

when the process exits its children are already reparented to init so the

/proc scan finds nothing useful under child_pid. recv_timeout(10 s) is

the correct and sufficient guard for the orphaned-pipe case.

Tree kill is now called only in the timeout path where it is needed:

kill -9 -{pgid} — fast sweep of the original process group

## [0.2.8] - 2026-04-18

### Added

- `kill_process_tree` added to `ChildProcess` trait and `handle_timeout`

## [0.2.6] - 2026-04-18

### Changed

- Fixed an infinite loop in the evaluation pipeline.

## [0.2.6] - 2026-04-18

### Added

- display pipeline errors in evaluation logs and improve child process cleanup on timeout

## [0.2.4] - 2026-04-18

### Added

- Fix agent elapse time metric recording
- resolve agents from crashing in pipeline for automated eval

## [0.2.3] - 2026-04-18

### Added

- Fix agent elapse time metric recording

## [0.2.2] - 2026-04-17

### Added

- Status output for each scenario evaluation

### Changed

- `load_from_path` clippy warning fixed

## [0.2.0] - 2026-04-17

### Added

- `--scenarios` flag to `agent-eval` and `suite` subcommand for external scenario files (JSON only)
- `EvalScenarioSuite::load_from_path` convenience API

## [0.1.0] - 2026-04-16

### Added

- `agent-eval` CLI binary for benchmarking agentic coding assistants
- Multi-agent profile system with baseline inheritance; built-in profiles for gestura, claude-code, augment, codex, and opencode in full, sandboxed, and iterative modes
- Deterministic rule-based evaluator with 17 named checks — no LLM required
- Optional LLM-as-judge semantic scoring via `src/judge.rs`
- Evaluation scenarios embedded at compile time from `testdata/scenarios.json`
- `suite` subcommand for multi-profile coordination across agent families
- Cross-agent comparison and leaderboard generation
- Text, JSON, and standalone HTML dashboard report output
- `--dry-run` flag to validate rubric logic without spawning agent subprocesses
- `--list` and `--list-agents` flags to enumerate scenario and profile IDs
- CI workflow (GitHub Actions) with fmt, clippy, test, and dry-run checks across macOS, Linux, and Windows
- Release workflow producing signed binaries: macOS universal (codesign + notarytool), Linux x86_64 tarball, Windows x86_64 zip (eSigner)
- Eval Pages workflow that runs the full suite and publishes HTML dashboard and JSON reports to GitHub Pages

[Unreleased]: https://github.com/gestura-ai/agent-evaluation/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gestura-ai/agent-evaluation/releases/tag/v0.1.0
