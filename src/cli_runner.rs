//! Subprocess-based CLI runner.
//!
//! Drives an agent binary as a black-box subprocess — one call per variation — then
//! feeds the captured stdout to [`RuleEvaluator`].
//!
//! Everything the runner needs is read from [`EvalConfig`]: which binary to invoke,
//! what `args_prefix` to prepend, what environment variables to forward, what
//! pass/fail thresholds to apply, and any per-scenario rubric overrides.

use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use tracing::{debug, info, warn};

use crate::{
    config::EvalConfig,
    evaluator::{CheckResult, RuleEvaluator},
    judge::LlmJudge,
    progress::{ProgressCallback, ProgressEvent},
    report::{EvalReport, ScenarioResult, VariationResult},
    scenario::{EvalScenario, EvalScenarioSuite, EvalVariation},
};

// ─── Internal trial result ────────────────────────────────────────────────────

/// Raw outcome of a single subprocess invocation (one trial of one variation).
struct TrialOutcome {
    response: String,
    pipeline_error: Option<String>,
    duration_ms: u64,
    checks: Vec<CheckResult>,
    score: f32,
    passed: bool,
    judge_score: Option<crate::judge::JudgeScore>,
}

/// Runtime options for one eval run — agent profile + ephemeral CLI flags.
#[derive(Clone)]
pub struct CliRunnerOptions {
    /// Loaded agent profile (model, permissions, subprocess settings, thresholds).
    pub eval_config: EvalConfig,
    /// IDs of specific scenarios to run (empty = all).
    pub scenario_ids: Vec<String>,
    /// When true, no subprocess is launched — rule checks run on a stub response.
    pub dry_run: bool,
    /// CLI-level binary override (takes precedence over `eval_config.subprocess.bin`).
    pub bin_override: Option<PathBuf>,
    /// Optional progress callback. Fires `ProfileStarted`, `VariationDone`, and
    /// `ProfileFinished` events. When `None` the runner is zero-overhead.
    pub progress: Option<ProgressCallback>,
    /// Optional LLM-as-judge for quality scoring. When `None` judge scores
    /// are skipped entirely and rule-based scoring is the only signal.
    pub judge: Option<LlmJudge>,
}

impl CliRunnerOptions {
    /// Build options from the baseline Gestura profile with auto-detected binary.
    pub fn new() -> Self {
        Self {
            eval_config: EvalConfig::baseline(),
            scenario_ids: Vec::new(),
            dry_run: false,
            bin_override: None,
            progress: None,
            judge: LlmJudge::from_env(),
        }
    }

    /// Build options from a named built-in agent profile.
    pub fn for_agent(agent_id: &str) -> Result<Self, crate::config::ConfigError> {
        Ok(Self {
            eval_config: EvalConfig::load_builtin(agent_id)?,
            ..Self::new()
        })
    }
}

impl Default for CliRunnerOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Runner that drives an agent CLI as a subprocess.
pub struct CliEvalRunner {
    options: CliRunnerOptions,
}

impl CliEvalRunner {
    pub fn new(options: CliRunnerOptions) -> Self {
        Self { options }
    }

    /// The resolved binary path for this run.
    fn bin(&self) -> PathBuf {
        self.options
            .eval_config
            .resolve_bin(self.options.bin_override.as_ref())
    }

    /// Run the full (or filtered) suite and return a finalised [`EvalReport`].
    pub fn run_suite(&self, suite: &EvalScenarioSuite) -> EvalReport {
        let cfg = &self.options.eval_config;
        let bin = self.bin();

        let mut report = EvalReport::new(
            &cfg.agent.id,
            &cfg.agent.name,
            format!("{:?}", cfg.agent.mode).to_lowercase(),
            &cfg.model.provider,
            &cfg.model.name,
            self.options.dry_run,
        );

        let scenarios = suite.filter_by_ids(&self.options.scenario_ids);

        // Count total variations upfront for ProfileStarted.
        let total_variations: usize = scenarios.iter().map(|s| s.variations.len()).sum();

        info!(
            total = scenarios.len(),
            dry_run = self.options.dry_run,
            agent_id = %cfg.agent.id,
            bin = %bin.display(),
            "agent-eval run starting"
        );

        if let Some(ref cb) = self.options.progress {
            cb(ProgressEvent::ProfileStarted {
                agent_id: cfg.agent.id.clone(),
                total_variations,
            });
        }

        for scenario in scenarios {
            report.scenarios.push(self.run_scenario(scenario));
        }

        report.finalize();
        info!(
            score = report.summary.overall_score,
            passed = report.summary.passed_variations,
            total = report.summary.total_variations,
            "agent-eval run complete"
        );

        if let Some(ref cb) = self.options.progress {
            cb(ProgressEvent::ProfileFinished {
                agent_id: cfg.agent.id.clone(),
                report: report.clone(),
            });
        }

        report
    }

    fn run_scenario(&self, scenario: &EvalScenario) -> ScenarioResult {
        info!(id = %scenario.id, "scenario");
        let mut var_results = Vec::with_capacity(scenario.variations.len());
        for variation in &scenario.variations {
            var_results.push(self.run_variation(scenario, variation));
        }
        let total = var_results.len() as f32;
        let passed_count = var_results.iter().filter(|v| v.passed).count() as f32;
        let score = if total > 0.0 {
            passed_count / total
        } else {
            1.0
        };
        ScenarioResult {
            scenario_id: scenario.id.clone(),
            scenario_name: scenario.name.clone(),
            category: scenario.category.clone(),
            passed: var_results.iter().all(|v| v.passed),
            variations: var_results,
            score,
        }
    }

    fn run_variation(&self, scenario: &EvalScenario, variation: &EvalVariation) -> VariationResult {
        debug!(scenario = %scenario.id, variation = %variation.id, "variation");
        let prompt_preview = truncate(&variation.prompt, 80);
        let cfg = &self.options.eval_config;
        let trials = cfg.execution.trials.max(1) as usize;

        // ── Run N trials ──────────────────────────────────────────────────────
        let mut outcomes: Vec<TrialOutcome> = Vec::with_capacity(trials);

        for trial_idx in 0..trials {
            if trials > 1 {
                debug!(
                    scenario = %scenario.id,
                    variation = %variation.id,
                    trial = trial_idx + 1,
                    total = trials,
                    "trial"
                );
            }

            if let Some(ref cb) = self.options.progress {
                cb(ProgressEvent::TrialStarted {
                    agent_id: cfg.agent.id.clone(),
                    scenario_id: scenario.id.clone(),
                    variation_id: variation.id.clone(),
                    trial: (trial_idx + 1) as u32,
                    total_trials: trials as u32,
                });
            }

            outcomes.push(self.run_trial(scenario, variation));

            // Throttle between trials (same budget as inter-variation delay).
            // Skip after the last trial — the inter-variation delay below handles that.
            if trial_idx < trials - 1 && cfg.execution.delay_between_variations_ms > 0 {
                thread::sleep(Duration::from_millis(
                    cfg.execution.delay_between_variations_ms,
                ));
            }
        }

        // ── Aggregate ─────────────────────────────────────────────────────────
        let trial_scores: Vec<f32> = outcomes.iter().map(|o| o.score).collect();
        let trial_responses: Vec<String> = outcomes.iter().map(|o| o.response.clone()).collect();
        let total_duration_ms: u64 = outcomes.iter().map(|o| o.duration_ms).sum();

        let avg_score = trial_scores.iter().sum::<f32>() / trial_scores.len() as f32;
        let passed_trials = outcomes.iter().filter(|o| o.passed).count();
        // Majority vote: more than half of trials must pass.
        let passed = passed_trials * 2 > outcomes.len();

        // Representative trial: the one whose score is closest to the average.
        // Used for `checks` and `pipeline_error` in the aggregate result.
        let rep_idx = trial_scores
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (*a - avg_score)
                    .abs()
                    .partial_cmp(&(*b - avg_score).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        // Remove the representative outcome so we can take ownership of its fields.
        let rep = outcomes.swap_remove(rep_idx);

        let result = VariationResult {
            variation_id: variation.id.clone(),
            prompt_preview,
            response: rep.response,
            duration_ms: total_duration_ms,
            pipeline_error: rep.pipeline_error,
            checks: rep.checks,
            score: avg_score,
            passed,
            trial_scores,
            trial_responses,
            judge_score: rep.judge_score,
        };

        if let Some(ref cb) = self.options.progress {
            cb(ProgressEvent::VariationDone {
                agent_id: cfg.agent.id.clone(),
                scenario_id: scenario.id.clone(),
                variation_id: variation.id.clone(),
                passed: result.passed,
                score: result.score,
                duration_ms: result.duration_ms,
                pipeline_error: result.pipeline_error.clone(),
            });
        }

        // Inter-variation throttle — fires after progress so the terminal shows
        // the result immediately, then pauses before the next variation starts.
        if cfg.execution.delay_between_variations_ms > 0 {
            thread::sleep(Duration::from_millis(
                cfg.execution.delay_between_variations_ms,
            ));
        }

        result
    }

    /// Run one trial of a variation: invoke the agent with the retry loop and
    /// evaluate the response.  Returns a raw [`TrialOutcome`] — no aggregation.
    fn run_trial(&self, scenario: &EvalScenario, variation: &EvalVariation) -> TrialOutcome {
        let cfg = &self.options.eval_config;
        let max_attempts = 1 + cfg.execution.retries as usize;

        // Timer is reset at the start of each attempt so backoff sleeps are
        // excluded — duration_ms reflects only the agent's actual execution time.
        let mut start = Instant::now();

        let (response_text, pipeline_error) = if self.options.dry_run {
            (
                format!(
                    "[DRY-RUN stub — {} / {}] Placeholder response for check-logic validation.",
                    scenario.id, variation.id
                ),
                None,
            )
        } else {
            // Retry loop: re-attempt on rate-limit (429) errors up to `retries` times.
            let mut attempt = 0usize;
            loop {
                attempt += 1;
                start = Instant::now(); // reset: only the successful attempt's time counts
                let (text, err) = self.invoke_agent(scenario, variation);

                // Three rate-limit detection paths:
                //  1. stderr contained 429/rate-limit text (most agents)
                //  2. stdout contained it (opencode exits 0 with error in stdout)
                //  3. Silent failure: exit non-zero, both stdout and stderr empty —
                //     agent crashed before writing anything; treat as transient.
                let is_rl = err.as_deref().map(is_rate_limit_error).unwrap_or(false)
                    || is_rate_limit_error(&text)
                    || is_silent_transient_failure(err.as_deref(), &text);

                if is_rl && attempt < max_attempts {
                    let wait_secs = cfg
                        .execution
                        .rate_limit_backoff_secs
                        .saturating_mul(1u64 << (attempt - 1).min(3));

                    if let Some(ref cb) = self.options.progress {
                        cb(ProgressEvent::RateLimitRetry {
                            agent_id: cfg.agent.id.clone(),
                            scenario_id: scenario.id.clone(),
                            variation_id: variation.id.clone(),
                            attempt: attempt as u32,
                            max_attempts: max_attempts as u32,
                            wait_secs,
                        });
                    }

                    thread::sleep(Duration::from_secs(wait_secs));
                    continue;
                }

                break (text, err);
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let eval = RuleEvaluator::evaluate(variation, &response_text);

        // LLM judge runs after rule evaluation (optional, non-fatal).
        let judge_score = self
            .options
            .judge
            .as_ref()
            .and_then(|j| j.judge(variation, &response_text));

        TrialOutcome {
            response: response_text,
            pipeline_error,
            duration_ms,
            checks: eval.checks,
            score: eval.score,
            passed: eval.passed,
            judge_score,
        }
    }

    /// Build the prompt and invoke the agent binary as a subprocess.
    ///
    /// Command structure: `<bin> [args_prefix...] <prompt>`
    ///
    /// Environment variables from `subprocess.env` are forwarded. If the scenario
    /// rubric disables tools, `GESTURA_TOOLS_ENABLED=false` is also set (Gestura-
    /// specific; other agents ignore it safely).
    fn invoke_agent(
        &self,
        scenario: &EvalScenario,
        variation: &EvalVariation,
    ) -> (String, Option<String>) {
        let cfg = &self.options.eval_config;
        let prompt = build_prompt(variation);
        let bin = self.bin();

        let mut cmd = Command::new(&bin);

        // Null stdin so agents that probe stdin (e.g. codex falling back to
        // interactive mode after an auth failure) get immediate EOF instead of
        // blocking the eval run indefinitely.
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Prepend agent-specific args (e.g. `["--dangerously-skip-permissions", "-p"]`)
        // then append the prompt as the final argument.
        cmd.args(&cfg.subprocess.args_prefix);
        cmd.arg(&prompt);

        // Forward config-defined env vars.
        for (k, v) in &cfg.subprocess.env {
            cmd.env(k, v);
        }

        // Explicitly forward credential env vars from the calling process.
        // Docker passes these the same way: -e KEY=value on every `docker run`.
        // Explicit forwarding ensures they reach the subprocess regardless of
        // how the outer runner (GitHub Actions, etc.) scopes its environment.
        for key in &[
            "ANTHROPIC_API_KEY",
            "GESTURA_ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "AUGMENT_SESSION_AUTH",
        ] {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }

        // Strip CI/GitHub Actions env vars from subprocess invocations.
        // Agents inherit the full parent environment by default, which in GitHub
        // Actions includes CI=true, GITHUB_ACTIONS=true, GITHUB_WORKSPACE
        // (pointing to a checkout containing Cargo.toml), and 20+ GITHUB_*/
        // RUNNER_* vars. OpenCode detects these and alters behaviour — e.g.
        // treating GITHUB_WORKSPACE as a project root and attempting to run
        // tests or install toolchains — causing indefinite hangs on Rust
        // scenarios. Docker has none of these vars; stripping them makes CI
        // behaviour match Docker.
        for (key, _) in std::env::vars() {
            if key == "CI"
                || key.starts_with("GITHUB_")
                || key.starts_with("RUNNER_")
            {
                cmd.env_remove(&key);
            }
        }

        // Gestura-specific: disable tool execution when rubric says no tools.
        if !scenario.rubric.tools_enabled {
            cmd.env("GESTURA_TOOLS_ENABLED", "false");
        }

        // Give each subprocess its own empty working directory so agents that
        // scan CWD for project context find nothing. Without this, agents in
        // full-permission mode write files (Cargo.toml, *.rs, etc.) to a shared
        // directory during earlier variations; subsequent variations and profiles
        // then find that project, interpret it as context, and hang trying to
        // compile or test it.
        let invocation_cwd = std::env::temp_dir()
            .join(format!("agent-eval-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&invocation_cwd);
        cmd.current_dir(&invocation_cwd);

        // Own process group so a timeout kill reaches grandchildren (e.g. opencode
        // spawning a .opencode child that holds the stalled TCP connection).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                warn!(bin = %bin.display(), error = %e, "failed to launch agent subprocess");
                return (String::new(), Some(e.to_string()));
            }
        };

        let child_pid = child.id();
        let timeout = Duration::from_secs(cfg.execution.timeout_secs);

        // Read stdout and stderr on separate threads to prevent pipe-buffer deadlock
        // when the subprocess writes to both simultaneously.
        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>();
        let (stderr_tx, stderr_rx) = mpsc::channel::<Vec<u8>>();
        let mut stdout_pipe = child.stdout.take().expect("stdout piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr piped");
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buf);
            let _ = stdout_tx.send(buf);
        });
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf);
            let _ = stderr_tx.send(buf);
        });

        // Wait for child exit on a background thread; bound the main-thread wait
        // to timeout_secs so a stalled subprocess never blocks the harness.
        let (exit_tx, exit_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = exit_tx.send(child.wait());
        });

        let exit_status = match exit_rx.recv_timeout(timeout) {
            Ok(res) => res,
            Err(_elapsed) => {
                // Timeout: kill the process group immediately (fast, catches all
                // processes that share the original PGID = child_pid).
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-9", &format!("-{child_pid}")])
                        .status();
                    // Walk the full descendant tree in a background thread so
                    // that /proc reads cannot block the main evaluation thread.
                    // On Linux, reading /proc/{pid}/status for a process that is
                    // in D-state (uninterruptible kernel sleep — e.g. cargo
                    // waiting on a slow crates.io download) can block
                    // indefinitely.  Fire-and-forget: the group kill above
                    // already handles the common case; the tree walk mops up
                    // any stragglers that changed their process group.
                    thread::spawn(move || kill_process_tree(child_pid));
                }
                #[cfg(not(unix))]
                {
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &child_pid.to_string()])
                        .status();
                }
                warn!(
                    scenario = %scenario.id,
                    variation = %variation.id,
                    timeout_secs = cfg.execution.timeout_secs,
                    pid = child_pid,
                    "agent subprocess timed out — killed process group"
                );
                return (
                    String::new(),
                    Some(format!("timeout after {}s", cfg.execution.timeout_secs)),
                );
            }
        };

        // Drain stdout/stderr with a hard deadline.  Even after killing descendants
        // above, the reader threads may still be in the middle of reading buffered
        // data.  10 s is ample for any remaining pipe bytes to flush.
        let io_drain = Duration::from_secs(10);

        let raw_stdout_bytes = match stdout_rx.recv_timeout(io_drain) {
            Ok(b) => b,
            Err(_) => {
                warn!(
                    scenario = %scenario.id,
                    variation = %variation.id,
                    pid = child_pid,
                    "stdout pipe still open {}s after process exit — orphaned grandchild likely",
                    io_drain.as_secs(),
                );
                Vec::new()
            }
        };
        let stderr_bytes = stderr_rx.recv_timeout(io_drain).unwrap_or_default();

        let raw_stdout = String::from_utf8_lossy(&raw_stdout_bytes)
            .trim()
            .to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();

        // Strip configured response prefix (e.g. "Assistant: " labels).
        let stdout = if let Some(ref prefix) = cfg.subprocess.response_strip_prefix {
            raw_stdout
                .strip_prefix(prefix.as_str())
                .unwrap_or(&raw_stdout)
                .trim()
                .to_string()
        } else {
            raw_stdout
        };

        match exit_status {
            Ok(status) => {
                if !status.success() {
                    let err = if !stderr.is_empty() {
                        stderr
                    } else {
                        format!("exit {}", status)
                    };
                    // Log stdout preview so silent failures (empty stderr) are
                    // diagnosable without requiring RUST_LOG=debug.
                    let stdout_preview = if stdout.is_empty() {
                        "<empty>".to_string()
                    } else {
                        truncate(&stdout, 200)
                    };
                    warn!(
                        scenario = %scenario.id,
                        variation = %variation.id,
                        error = %err,
                        stdout = %stdout_preview,
                        "agent subprocess failed"
                    );
                    (stdout, Some(err))
                } else {
                    debug!(
                        scenario = %scenario.id,
                        variation = %variation.id,
                        words = stdout.split_whitespace().count(),
                        "response captured"
                    );
                    (stdout, None)
                }
            }
            Err(e) => {
                warn!(bin = %bin.display(), error = %e, "failed to wait for agent subprocess");
                (String::new(), Some(e.to_string()))
            }
        }
    }
}

/// Prepend conversation history (if any) to the final prompt so the LLM has
/// the full conversational context even via a single CLI invocation.
fn build_prompt(variation: &EvalVariation) -> String {
    if variation.history.is_empty() {
        return variation.prompt.clone();
    }

    let mut buf = String::new();
    buf.push_str("Continue this conversation and respond to the final user message.\n\n");
    for msg in &variation.history {
        let role = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            other => other,
        };
        buf.push_str(&format!("{role}: {}\n\n", msg.content));
    }
    buf.push_str(&format!("User: {}", variation.prompt));
    buf
}

/// Kill a process and every one of its descendants.
///
/// Reads `/proc` once to build the full parent→child map, then does a BFS
/// from `root_pid` to collect all descendant PIDs.  All collected PIDs are
/// sent to a single `kill -9` invocation so no subprocess is spawned per
/// process (unlike the previous pgrep-recursive approach, which was slow and
/// prone to PID-reuse races on busy systems).
///
/// This catches processes that called `setsid()`/`setpgid()` because we walk
/// by PPID (which the process cannot change) rather than by PGID.
///
/// Non-fatal: any I/O or kill errors are silently ignored.
#[cfg(unix)]
fn kill_process_tree(root_pid: u32) {
    use std::collections::{HashMap, VecDeque};

    // Build PPID → [child PID] map from /proc in a single pass.
    let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.filter_map(|e| e.ok()) {
            let pid = match entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            {
                Some(p) => p,
                None => continue,
            };
            if let Some(ppid) = proc_ppid(pid) {
                children_of.entry(ppid).or_default().push(pid);
            }
        }
    }

    // BFS from root_pid to collect root + all descendants.
    let mut to_kill: Vec<u32> = Vec::new();
    let mut queue: VecDeque<u32> = VecDeque::new();
    queue.push_back(root_pid);
    while let Some(pid) = queue.pop_front() {
        to_kill.push(pid);
        if let Some(children) = children_of.get(&pid) {
            for &child in children {
                queue.push_back(child);
            }
        }
    }

    if to_kill.is_empty() {
        return;
    }

    // Single kill(1) call for all PIDs — minimises subprocess overhead.
    let mut args = vec!["-9".to_string()];
    args.extend(to_kill.iter().map(|p| p.to_string()));
    let _ = Command::new("kill").args(&args).status();
}

/// Read the PPid field from `/proc/{pid}/status`.  Returns `None` on any
/// error (process already gone, non-numeric entry, permission denied, etc.).
#[cfg(unix)]
fn proc_ppid(pid: u32) -> Option<u32> {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find(|l| l.starts_with("PPid:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Returns `true` when the subprocess exited non-zero but produced no output
/// on either stdout or stderr.  This is almost never a deterministic application
/// error (those always print something); it usually means the process was killed
/// before it could write anything — OOM, SIGTERM during startup, or a connection
/// failure that the agent didn't handle before crashing.  Safe to retry.
fn is_silent_transient_failure(pipeline_error: Option<&str>, stdout: &str) -> bool {
    stdout.trim().is_empty()
        && pipeline_error
            .map(|e| e.starts_with("exit "))
            .unwrap_or(false)
}

/// Returns `true` if the subprocess error string looks like a provider
/// rate-limit rejection (HTTP 429).  Checked case-insensitively against
/// patterns common across Anthropic, OpenAI, and other providers.
fn is_rate_limit_error(error: &str) -> bool {
    let e = error.to_lowercase();
    e.contains("429")
        || e.contains("rate limit")
        || e.contains("rate_limit")
        || e.contains("too many requests")
        || e.contains("tokens per minute")
        || e.contains("requests per minute")
        || e.contains("request rejected")
        || e.contains("overloaded")
}

fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max_chars {
        s
    } else {
        let mut t: String = s.chars().take(max_chars - 1).collect();
        t.push('…');
        t
    }
}
