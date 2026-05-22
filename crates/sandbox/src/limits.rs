use serde::{Deserialize, Serialize};

pub mod defaults {
    pub const MAX_COMMANDS: usize = 10_000;
    pub const MAX_LOOP_ITERATIONS: usize = 10_000;
    pub const MAX_TOTAL_LOOP_ITERS: usize = 1_000_000;
    pub const MAX_FUNCTION_DEPTH: usize = 100;
    pub const MAX_AST_DEPTH: usize = 100;
    pub const MAX_PARSER_FUEL: usize = 100_000;
    pub const MAX_SUBST_DEPTH: usize = 32;
    pub const MAX_STDOUT_BYTES: usize = 1_024 * 1_024;
    pub const MAX_STDERR_BYTES: usize = 1_024 * 1_024;
    pub const MAX_INPUT_BYTES: usize = 10 * 1_024 * 1_024;
    pub const MAX_VAR_SIZE: usize = 1_024 * 1_024;
    pub const MAX_FS_BYTES: usize = 100 * 1_024 * 1_024;
    pub const TIMEOUT_SECS: u64 = 30;
    pub const PARSER_TIMEOUT_SECS: u64 = 5;
}

pub mod hard_caps {
    pub const MAX_COMMANDS: usize = 1_000_000;
    pub const MAX_LOOP_ITERATIONS: usize = 1_000_000;
    pub const MAX_TOTAL_LOOP_ITERS: usize = 10_000_000;
    pub const MAX_FUNCTION_DEPTH: usize = 100;
    pub const MAX_AST_DEPTH: usize = 100;
    pub const MAX_PARSER_FUEL: usize = 1_000_000;
    pub const MAX_SUBST_DEPTH: usize = 64;
    pub const MAX_STDOUT_BYTES: usize = 100 * 1_024 * 1_024;
    pub const MAX_STDERR_BYTES: usize = 100 * 1_024 * 1_024;
    pub const MAX_INPUT_BYTES: usize = 100 * 1_024 * 1_024;
    pub const MAX_VAR_SIZE: usize = 10 * 1_024 * 1_024;
    pub const MAX_FS_BYTES: usize = 1_024 * 1_024 * 1_024;
    pub const TIMEOUT_SECS: u64 = 3_600;
    pub const PARSER_TIMEOUT_SECS: u64 = 60;
}

fn clamp(val: usize, hard: usize) -> usize {
    val.min(hard)
}

fn clamp_u64(val: u64, hard: u64) -> u64 {
    val.min(hard)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLimits {
    pub max_commands: usize,
    pub max_loop_iterations: usize,
    pub max_total_loop_iters: usize,
    pub max_function_depth: usize,
    pub max_ast_depth: usize,
    pub max_parser_fuel: usize,
    pub max_subst_depth: usize,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_input_bytes: usize,
    pub max_var_size: usize,
    pub max_fs_bytes: usize,
    pub timeout_secs: u64,
    pub parser_timeout_secs: u64,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_commands: defaults::MAX_COMMANDS,
            max_loop_iterations: defaults::MAX_LOOP_ITERATIONS,
            max_total_loop_iters: defaults::MAX_TOTAL_LOOP_ITERS,
            max_function_depth: defaults::MAX_FUNCTION_DEPTH,
            max_ast_depth: defaults::MAX_AST_DEPTH,
            max_parser_fuel: defaults::MAX_PARSER_FUEL,
            max_subst_depth: defaults::MAX_SUBST_DEPTH,
            max_stdout_bytes: defaults::MAX_STDOUT_BYTES,
            max_stderr_bytes: defaults::MAX_STDERR_BYTES,
            max_input_bytes: defaults::MAX_INPUT_BYTES,
            max_var_size: defaults::MAX_VAR_SIZE,
            max_fs_bytes: defaults::MAX_FS_BYTES,
            timeout_secs: defaults::TIMEOUT_SECS,
            parser_timeout_secs: defaults::PARSER_TIMEOUT_SECS,
        }
    }
}

impl ExecutionLimits {
    pub fn clamped(mut self) -> Self {
        self.max_commands = clamp(self.max_commands, hard_caps::MAX_COMMANDS);
        self.max_loop_iterations = clamp(self.max_loop_iterations, hard_caps::MAX_LOOP_ITERATIONS);
        self.max_total_loop_iters =
            clamp(self.max_total_loop_iters, hard_caps::MAX_TOTAL_LOOP_ITERS);
        self.max_function_depth = clamp(self.max_function_depth, hard_caps::MAX_FUNCTION_DEPTH);
        self.max_ast_depth = clamp(self.max_ast_depth, hard_caps::MAX_AST_DEPTH);
        self.max_parser_fuel = clamp(self.max_parser_fuel, hard_caps::MAX_PARSER_FUEL);
        self.max_subst_depth = clamp(self.max_subst_depth, hard_caps::MAX_SUBST_DEPTH);
        self.max_stdout_bytes = clamp(self.max_stdout_bytes, hard_caps::MAX_STDOUT_BYTES);
        self.max_stderr_bytes = clamp(self.max_stderr_bytes, hard_caps::MAX_STDERR_BYTES);
        self.max_input_bytes = clamp(self.max_input_bytes, hard_caps::MAX_INPUT_BYTES);
        self.max_var_size = clamp(self.max_var_size, hard_caps::MAX_VAR_SIZE);
        self.max_fs_bytes = clamp(self.max_fs_bytes, hard_caps::MAX_FS_BYTES);
        self.timeout_secs = clamp_u64(self.timeout_secs, hard_caps::TIMEOUT_SECS);
        self.parser_timeout_secs =
            clamp_u64(self.parser_timeout_secs, hard_caps::PARSER_TIMEOUT_SECS);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionCounters {
    pub commands: usize,
    pub loop_iterations: usize,
    pub total_loop_iterations: usize,
    pub function_depth: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}

impl ExecutionCounters {
    pub fn tick_command(&mut self, limits: &ExecutionLimits) -> crate::error::ShellResult<()> {
        self.commands += 1;
        if self.commands > limits.max_commands {
            return Err(crate::error::ShellError::LimitExceeded(format!(
                "max commands ({}) exceeded",
                limits.max_commands
            )));
        }
        Ok(())
    }

    pub fn tick_loop(&mut self, limits: &ExecutionLimits) -> crate::error::ShellResult<()> {
        self.loop_iterations += 1;
        self.total_loop_iterations += 1;
        if self.loop_iterations > limits.max_loop_iterations {
            return Err(crate::error::ShellError::LimitExceeded(format!(
                "max loop iterations ({}) exceeded",
                limits.max_loop_iterations
            )));
        }
        if self.total_loop_iterations > limits.max_total_loop_iters {
            return Err(crate::error::ShellError::LimitExceeded(format!(
                "max total loop iterations ({}) exceeded",
                limits.max_total_loop_iters
            )));
        }
        Ok(())
    }

    pub fn reset_loop_counter(&mut self) {
        self.loop_iterations = 0;
    }

    pub fn check_stdout(
        &self,
        additional: usize,
        limits: &ExecutionLimits,
    ) -> crate::error::ShellResult<()> {
        let total = self.stdout_bytes + additional;
        if total > limits.max_stdout_bytes {
            return Err(crate::error::ShellError::OutputLimitExceeded {
                stream: "stdout".into(),
                size: total,
                max: limits.max_stdout_bytes,
            });
        }
        Ok(())
    }
}
