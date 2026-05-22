use async_trait::async_trait;

use super::{Builtin, Context};
use crate::error::ShellResult;
use crate::interpreter::hooks::ExecResult;

pub struct Export;

#[async_trait]
impl Builtin for Export {
    fn name(&self) -> &str {
        "export"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args: Vec<String> = ctx.args_from(1).to_vec();
        for arg in &args {
            if let Some(eq) = arg.find('=') {
                let name = &arg[..eq];
                let value = &arg[eq + 1..];
                ctx.env.insert(name.to_string(), value.to_string());
            } else {
                if let Some(val) = ctx.vars.get(arg.as_str()).cloned() {
                    ctx.env.insert(arg.clone(), val);
                }
            }
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Set_;

#[async_trait]
impl Builtin for Set_ {
    fn name(&self) -> &str {
        "set"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args: Vec<String> = ctx.args_from(1).to_vec();
        if args.is_empty() {
            let mut output = String::new();
            let mut vars: Vec<_> = ctx.vars.iter().collect();
            vars.sort_by_key(|(k, _)| (*k).clone());
            for (k, v) in vars {
                output.push_str(&format!("{k}={v}\n"));
            }
            return Ok(ExecResult::success(output));
        }

        for arg in &args {
            match arg.as_str() {
                "-e" => ctx.shell_opts.errexit = true,
                "+e" => ctx.shell_opts.errexit = false,
                "-u" => ctx.shell_opts.nounset = true,
                "+u" => ctx.shell_opts.nounset = false,
                "-x" => ctx.shell_opts.xtrace = true,
                "+x" => ctx.shell_opts.xtrace = false,
                "-o" | "+o" => {}
                "pipefail" => ctx.shell_opts.pipefail = true,
                _ => {}
            }
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Unset;

#[async_trait]
impl Builtin for Unset {
    fn name(&self) -> &str {
        "unset"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args: Vec<String> = ctx.args_from(1).to_vec();
        for arg in &args {
            if arg.starts_with('-') {
                continue;
            }
            ctx.vars.remove(arg.as_str());
            ctx.env.remove(arg.as_str());
        }
        Ok(ExecResult::code(0))
    }
}

#[cfg(test)]
mod tests {
    use crate::builtins::BuiltinRegistry;
    use crate::capabilities::CapabilitySet;
    use crate::fs::SandboxFs;
    use crate::interpreter::ShellOpts;
    use std::collections::HashMap;

    #[tokio::test]
    async fn export_creates_env_var() {
        let reg = BuiltinRegistry::with_core_builtins();
        let builtin = reg.get("export").unwrap();
        let fs = SandboxFs::new();
        let mut env = HashMap::new();
        let mut vars = HashMap::new();
        let mut cwd = "/".to_string();
        let caps = CapabilitySet::default_set();
        let mut shell_opts = ShellOpts::default();
        let ctx = super::super::Context {
            args: vec!["export".into(), "FOO=bar".into()],
            env: &mut env,
            vars: &mut vars,
            cwd: &mut cwd,
            fs: &fs,
            stdin: None,
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap();
        assert_eq!(env.get("FOO").unwrap(), "bar");
    }

    #[tokio::test]
    async fn unset_removes_var() {
        let reg = BuiltinRegistry::with_core_builtins();
        let builtin = reg.get("unset").unwrap();
        let fs = SandboxFs::new();
        let mut env = HashMap::new();
        let mut vars = HashMap::new();
        vars.insert("FOO".into(), "bar".into());
        let mut cwd = "/".to_string();
        let caps = CapabilitySet::default_set();
        let mut shell_opts = ShellOpts::default();
        let ctx = super::super::Context {
            args: vec!["unset".into(), "FOO".into()],
            env: &mut env,
            vars: &mut vars,
            cwd: &mut cwd,
            fs: &fs,
            stdin: None,
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap();
        assert!(!vars.contains_key("FOO"));
    }
}
