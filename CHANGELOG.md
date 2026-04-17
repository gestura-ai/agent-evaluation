# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
