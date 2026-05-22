use async_trait::async_trait;

use crate::error::ShellResult;

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ExecResult {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub fn failure(exit_code: i32, stderr: impl Into<String>) -> Self {
        Self {
            exit_code,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    pub fn code(exit_code: i32) -> Self {
        Self {
            exit_code,
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

#[async_trait]
pub trait ExecHandler: Send + Sync {
    async fn handle(&self, cmd: &str, args: &[String]) -> Option<ShellResult<ExecResult>>;
}

pub struct DefaultExecHandler;

#[async_trait]
impl ExecHandler for DefaultExecHandler {
    async fn handle(&self, _cmd: &str, _args: &[String]) -> Option<ShellResult<ExecResult>> {
        None
    }
}
