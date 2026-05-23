pub mod expansion;
pub mod hooks;

use std::collections::HashMap;

use crate::builtins::BuiltinRegistry;
use crate::capabilities::{Cap, CapabilitySet};
use crate::error::ShellResult;
use crate::fs::SandboxFs;
use crate::limits::{ExecutionCounters, ExecutionLimits};
use crate::parser::ast::{Command, RedirectKind, Redirection, SimpleCommand, Word};
use crate::trace::TraceEvent;
use expansion::ExpansionContext;
use hooks::{ExecHandler, ExecResult};

/// Exit code for unknown commands.
const EXIT_COMMAND_NOT_FOUND: i32 = 127;

pub struct Interpreter {
    pub env: HashMap<String, String>,
    pub vars: HashMap<String, String>,
    pub cwd: String,
    pub fs: SandboxFs,
    pub capabilities: CapabilitySet,
    pub limits: ExecutionLimits,
    pub counters: ExecutionCounters,
    pub functions: HashMap<String, Command>,
    pub last_exit_code: i32,
    pub positional_params: Vec<String>,
    pub builtins: BuiltinRegistry,
    pub exec_handler: Box<dyn ExecHandler>,
    pub stdout_buf: String,
    pub stderr_buf: String,
    pub pipeline_stdin: Option<Vec<u8>>,
    pub shell_opts: ShellOpts,
    pub tracing_enabled: bool,
    pub trace_events: Vec<TraceEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct ShellOpts {
    pub errexit: bool,  // set -e
    pub nounset: bool,  // set -u
    pub pipefail: bool, // set -o pipefail
    pub xtrace: bool,   // set -x
}

impl Interpreter {
    pub fn execute<'a>(
        &'a mut self,
        cmd: &'a Command,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ShellResult<ExecResult>> + 'a>> {
        Box::pin(self.execute_inner(cmd))
    }

    async fn execute_inner(&mut self, cmd: &Command) -> ShellResult<ExecResult> {
        match cmd {
            Command::Empty => Ok(ExecResult::code(0)),
            Command::Simple(sc) => self.execute_simple(sc).await,
            Command::Pipeline(cmds) => self.execute_pipeline(cmds).await,
            Command::And(left, right) => {
                let r = self.execute(left).await?;
                if r.exit_code == 0 {
                    self.execute(right).await
                } else {
                    Ok(r)
                }
            }
            Command::Or(left, right) => {
                let r = self.execute(left).await?;
                if r.exit_code != 0 {
                    self.execute(right).await
                } else {
                    Ok(r)
                }
            }
            Command::Not(inner) => {
                let r = self.execute(inner).await?;
                Ok(ExecResult::code(if r.exit_code == 0 { 1 } else { 0 }))
            }
            Command::Sequence(cmds) => self.execute_sequence(cmds).await,
            Command::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.execute_if(condition, then_branch, elif_branches, else_branch)
                    .await
            }
            Command::For { var, words, body } => self.execute_for(var, words, body).await,
            Command::While { condition, body } => self.execute_while(condition, body).await,
            Command::Until { condition, body } => self.execute_until(condition, body).await,
            Command::Case { word, arms } => self.execute_case(word, arms).await,
            Command::FunctionDef { name, body } => {
                self.functions.insert(name.clone(), *body.clone());
                Ok(ExecResult::code(0))
            }
            Command::Subshell(inner) => self.execute_subshell(inner).await,
            Command::Group(inner) => self.execute(inner).await,
            Command::Assignment(assigns) => {
                for a in assigns {
                    let val = self.expand_word(&a.value);
                    self.vars.insert(a.name.clone(), val);
                }
                Ok(ExecResult::code(0))
            }
            Command::Background(_) => {
                Ok(ExecResult::failure(1, "background execution not supported"))
            }
        }
    }

    async fn execute_sequence(&mut self, cmds: &[Command]) -> ShellResult<ExecResult> {
        let mut last = ExecResult::code(0);
        for c in cmds {
            last = self.execute(c).await?;
            self.last_exit_code = last.exit_code;
            if self.shell_opts.errexit && last.exit_code != 0 {
                return Ok(last);
            }
        }
        Ok(last)
    }

    async fn execute_if(
        &mut self,
        condition: &Command,
        then_branch: &Command,
        elif_branches: &[(Command, Command)],
        else_branch: &Option<Box<Command>>,
    ) -> ShellResult<ExecResult> {
        let cond = self.execute(condition).await?;
        if cond.exit_code == 0 {
            return self.execute(then_branch).await;
        }
        for (elif_cond, elif_body) in elif_branches {
            let r = self.execute(elif_cond).await?;
            if r.exit_code == 0 {
                return self.execute(elif_body).await;
            }
        }
        if let Some(else_body) = else_branch {
            self.execute(else_body).await
        } else {
            Ok(ExecResult::code(0))
        }
    }

    async fn execute_for(
        &mut self,
        var: &str,
        words: &[Word],
        body: &Command,
    ) -> ShellResult<ExecResult> {
        let expanded = self.expand_words(words);
        self.counters.reset_loop_counter();
        let mut last = ExecResult::code(0);
        for word in expanded {
            self.counters.tick_loop(&self.limits)?;
            self.vars.insert(var.to_string(), word);
            last = self.execute(body).await?;
            self.last_exit_code = last.exit_code;
        }
        Ok(last)
    }

    async fn execute_while(
        &mut self,
        condition: &Command,
        body: &Command,
    ) -> ShellResult<ExecResult> {
        self.counters.reset_loop_counter();
        let mut last = ExecResult::code(0);
        loop {
            self.counters.tick_loop(&self.limits)?;
            let cond = self.execute(condition).await?;
            if cond.exit_code != 0 {
                break;
            }
            last = self.execute(body).await?;
            self.last_exit_code = last.exit_code;
        }
        Ok(last)
    }

    async fn execute_until(
        &mut self,
        condition: &Command,
        body: &Command,
    ) -> ShellResult<ExecResult> {
        self.counters.reset_loop_counter();
        let mut last = ExecResult::code(0);
        loop {
            self.counters.tick_loop(&self.limits)?;
            let cond = self.execute(condition).await?;
            if cond.exit_code == 0 {
                break;
            }
            last = self.execute(body).await?;
            self.last_exit_code = last.exit_code;
        }
        Ok(last)
    }

    async fn execute_case(
        &mut self,
        word: &Word,
        arms: &[crate::parser::ast::CaseArm],
    ) -> ShellResult<ExecResult> {
        let expanded = self.expand_word(word);
        for arm in arms {
            for pattern in &arm.patterns {
                let pat = self.expand_word(pattern);
                if pattern_matches(&expanded, &pat) {
                    return self.execute(&arm.body).await;
                }
            }
        }
        Ok(ExecResult::code(0))
    }

    async fn execute_subshell(&mut self, inner: &Command) -> ShellResult<ExecResult> {
        let saved_env = self.env.clone();
        let saved_vars = self.vars.clone();
        let saved_cwd = self.cwd.clone();
        let saved_functions = self.functions.clone();
        let saved_params = self.positional_params.clone();
        let saved_opts = self.shell_opts.clone();

        let result = self.execute(inner).await;

        self.env = saved_env;
        self.vars = saved_vars;
        self.cwd = saved_cwd;
        self.functions = saved_functions;
        self.positional_params = saved_params;
        self.shell_opts = saved_opts;

        match result {
            Ok(r) => {
                self.last_exit_code = r.exit_code;
                Ok(r)
            }
            Err(e) => Err(e),
        }
    }

    async fn execute_simple(&mut self, sc: &SimpleCommand) -> ShellResult<ExecResult> {
        let trace_start = std::time::Instant::now();
        self.counters.tick_command(&self.limits)?;

        // Handle prefix assignments
        for a in &sc.assignments {
            let val = self.expand_word(&a.value);
            self.vars.insert(a.name.clone(), val);
        }

        if sc.words.is_empty() {
            return Ok(ExecResult::code(0));
        }

        let mut expanded_words = Vec::new();
        for w in &sc.words {
            let s = self.expand_word(w);
            expanded_words.push(s);
        }

        let cmd_name = &expanded_words[0];
        let args: Vec<String> = expanded_words[1..].to_vec();

        // Handle stdin from redirections, falling back to pipeline stdin
        let stdin_data = self
            .collect_stdin_redirect(&sc.redirections)?
            .or_else(|| self.pipeline_stdin.take());

        // Check exec handler first
        if let Some(result) = self.exec_handler.handle(cmd_name, &args).await {
            let r = result?;
            self.last_exit_code = r.exit_code;
            self.apply_redirections(&sc.redirections, &r.stdout)?;
            if !r.stdout.is_empty() {
                self.write_stdout(&r.stdout)?;
            }
            self.record_trace(&expanded_words, r.exit_code, trace_start, &[]);
            return Ok(r);
        }

        // Check user-defined functions
        if let Some(func_body) = self.functions.get(cmd_name).cloned() {
            let old_params = std::mem::replace(&mut self.positional_params, args);
            let result = self.execute(&func_body).await;
            self.positional_params = old_params;
            if let Ok(ref r) = result {
                self.record_trace(&expanded_words, r.exit_code, trace_start, &[]);
            }
            return result;
        }

        // Check builtins
        if let Some(builtin) = self.builtins.get(cmd_name) {
            // Check capabilities
            let caps_used: Vec<Cap> = builtin.required_capabilities().to_vec();
            for cap in &caps_used {
                self.capabilities.check(*cap)?;
            }

            let ctx = crate::builtins::Context {
                args: expanded_words.clone(),
                env: &mut self.env,
                vars: &mut self.vars,
                cwd: &mut self.cwd,
                fs: &self.fs,
                stdin: stdin_data.as_deref(),
                capabilities: &self.capabilities,
                last_exit_code: self.last_exit_code,
                shell_opts: &mut self.shell_opts,
            };

            let result = builtin.execute(ctx).await?;
            self.last_exit_code = result.exit_code;
            self.apply_redirections(&sc.redirections, &result.stdout)?;
            if !result.stdout.is_empty() {
                self.write_stdout(&result.stdout)?;
            }
            if !result.stderr.is_empty() {
                self.stderr_buf.push_str(&result.stderr);
            }
            self.record_trace(&expanded_words, result.exit_code, trace_start, &caps_used);
            return Ok(result);
        }

        self.last_exit_code = EXIT_COMMAND_NOT_FOUND;
        self.record_trace(&expanded_words, EXIT_COMMAND_NOT_FOUND, trace_start, &[]);
        Ok(ExecResult::failure(
            EXIT_COMMAND_NOT_FOUND,
            format!("{cmd_name}: command not found"),
        ))
    }

    async fn execute_pipeline(&mut self, cmds: &[Command]) -> ShellResult<PipelineResult> {
        if cmds.len() == 1 {
            return self.execute(&cmds[0]).await;
        }

        let mut last_stdout = String::new();
        let mut last_result = ExecResult::code(0);

        for (i, cmd) in cmds.iter().enumerate() {
            // Feed previous stdout as stdin for next command
            if i > 0 && !last_stdout.is_empty() {
                self.pipeline_stdin = Some(last_stdout.clone().into_bytes());
            }

            // For pipeline, we need to capture stdout of each stage
            let old_stdout = std::mem::take(&mut self.stdout_buf);
            last_result = self.execute(cmd).await?;

            // Capture what this stage produced
            let stage_output = if !last_result.stdout.is_empty() {
                last_result.stdout.clone()
            } else {
                std::mem::take(&mut self.stdout_buf)
            };

            self.stdout_buf = old_stdout;
            last_stdout = stage_output;
            self.last_exit_code = last_result.exit_code;
        }

        // Write final pipeline output
        if !last_stdout.is_empty() {
            self.write_stdout(&last_stdout)?;
        }

        Ok(last_result)
    }

    fn record_trace(
        &mut self,
        expanded_words: &[String],
        exit_code: i32,
        start: std::time::Instant,
        caps: &[Cap],
    ) {
        if self.tracing_enabled {
            self.trace_events.push(TraceEvent {
                timestamp: std::time::SystemTime::now(),
                command: expanded_words[0].clone(),
                args: expanded_words[1..].to_vec(),
                exit_code,
                duration_us: start.elapsed().as_micros() as u64,
                capabilities_used: caps.to_vec(),
            });
        }
    }

    fn expand_word(&self, word: &Word) -> String {
        let ctx = ExpansionContext {
            env: &self.env,
            vars: &self.vars,
            last_exit_code: self.last_exit_code,
            positional_params: &self.positional_params,
        };
        ctx.expand_word(word)
    }

    fn expand_words(&self, words: &[Word]) -> Vec<String> {
        let ctx = ExpansionContext {
            env: &self.env,
            vars: &self.vars,
            last_exit_code: self.last_exit_code,
            positional_params: &self.positional_params,
        };
        ctx.expand_words(words)
    }

    fn write_stdout(&mut self, s: &str) -> ShellResult<()> {
        self.counters.check_stdout(s.len(), &self.limits)?;
        self.counters.stdout_bytes += s.len();
        self.stdout_buf.push_str(s);
        Ok(())
    }

    fn collect_stdin_redirect(&self, redirections: &[Redirection]) -> ShellResult<Option<Vec<u8>>> {
        for r in redirections {
            match &r.kind {
                RedirectKind::Input => {
                    let path = self.expand_word(&r.target);
                    let data = self.fs.read_file(&path, &self.capabilities)?;
                    return Ok(Some(data));
                }
                RedirectKind::HereDoc | RedirectKind::HereString => {
                    let content = self.expand_word(&r.target);
                    return Ok(Some(content.into_bytes()));
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn apply_redirections(&self, redirections: &[Redirection], stdout: &str) -> ShellResult<()> {
        for r in redirections {
            match &r.kind {
                RedirectKind::Output => {
                    let path = self.expand_word(&r.target);
                    self.fs
                        .write_file(&path, stdout.as_bytes(), &self.capabilities)?;
                }
                RedirectKind::Append => {
                    let path = self.expand_word(&r.target);
                    self.fs
                        .append_file(&path, stdout.as_bytes(), &self.capabilities)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

type PipelineResult = ExecResult;

fn pattern_matches(value: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    // Simple glob matching
    let re_pattern = pattern
        .replace('.', "\\.")
        .replace('*', ".*")
        .replace('?', ".");
    regex::Regex::new(&format!("^{re_pattern}$"))
        .map(|re| re.is_match(value))
        .unwrap_or(value == pattern)
}
