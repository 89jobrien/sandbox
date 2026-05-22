use async_trait::async_trait;

use super::{Builtin, Context};
use crate::error::ShellResult;
use crate::interpreter::hooks::ExecResult;

pub struct True_;

#[async_trait]
impl Builtin for True_ {
    fn name(&self) -> &str {
        "true"
    }

    async fn execute(&self, _ctx: Context<'_>) -> ShellResult<ExecResult> {
        Ok(ExecResult::code(0))
    }
}

pub struct False_;

#[async_trait]
impl Builtin for False_ {
    fn name(&self) -> &str {
        "false"
    }

    async fn execute(&self, _ctx: Context<'_>) -> ShellResult<ExecResult> {
        Ok(ExecResult::code(1))
    }
}

pub struct Exit;

#[async_trait]
impl Builtin for Exit {
    fn name(&self) -> &str {
        "exit"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let code = ctx
            .arg(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(ctx.last_exit_code);
        Ok(ExecResult::code(code))
    }
}
