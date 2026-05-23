use async_trait::async_trait;

use super::{Builtin, Context};
use crate::capabilities::Cap;
use crate::error::ShellResult;
use crate::interpreter::hooks::ExecResult;

pub struct Cd;

#[async_trait]
impl Builtin for Cd {
    fn name(&self) -> &str {
        "cd"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let target = ctx
            .arg(1)
            .map(|s| s.to_string())
            .or_else(|| ctx.env.get("HOME").cloned())
            .unwrap_or_else(|| "/".to_string());

        let resolved = ctx.resolve_path(&target);

        if !ctx.fs.is_dir(&resolved)? {
            return Ok(ExecResult::failure(
                1,
                format!("cd: {target}: No such file or directory"),
            ));
        }

        *ctx.cwd = resolved;
        Ok(ExecResult::code(0))
    }
}

pub struct Pwd;

#[async_trait]
impl Builtin for Pwd {
    fn name(&self) -> &str {
        "pwd"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        Ok(ExecResult::success(format!("{}\n", ctx.cwd)))
    }
}

pub struct Ls;

#[async_trait]
impl Builtin for Ls {
    fn name(&self) -> &str {
        "ls"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let mut long_format = false;
        let mut show_all = false;
        let mut paths = Vec::new();

        for arg in ctx.args_from(1) {
            match arg.as_str() {
                "-l" => long_format = true,
                "-a" => show_all = true,
                "-la" | "-al" => {
                    long_format = true;
                    show_all = true;
                }
                _ => paths.push(arg.as_str()),
            }
        }

        if paths.is_empty() {
            paths.push(ctx.cwd.as_str());
        }

        let mut output = String::new();
        for path in &paths {
            let resolved = ctx.resolve_path(path);
            match ctx.fs.list_dir(&resolved, ctx.capabilities) {
                Ok(mut entries) => {
                    if !show_all {
                        entries.retain(|e| !e.name.starts_with('.'));
                    }
                    entries.sort_by(|a, b| a.name.cmp(&b.name));

                    if paths.len() > 1 {
                        output.push_str(&format!("{path}:\n"));
                    }

                    for entry in &entries {
                        format_entry(&mut output, entry, long_format);
                    }
                }
                Err(e) => {
                    return Ok(ExecResult::failure(1, format!("ls: {path}: {e}")));
                }
            }
        }

        Ok(ExecResult::success(output))
    }
}

fn format_entry(output: &mut String, entry: &crate::fs::DirEntry, long_format: bool) {
    if long_format {
        let kind = if entry.is_dir { "d" } else { "-" };
        output.push_str(&format!(
            "{}rwxr-xr-x  1 user  group  {:>8}  {}\n",
            kind, entry.size, entry.name
        ));
    } else {
        output.push_str(&entry.name);
        output.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::BuiltinRegistry;
    use crate::capabilities::CapabilitySet;
    use crate::fs::SandboxFs;
    use crate::interpreter::ShellOpts;
    use std::collections::HashMap;

    async fn run_with_fs(name: &str, args: &[&str], fs: &SandboxFs, cwd: &str) -> ExecResult {
        let reg = BuiltinRegistry::with_core_builtins();
        let builtin = reg.get(name).unwrap();
        let mut env = HashMap::new();
        let mut vars = HashMap::new();
        let mut cwd = cwd.to_string();
        let caps = CapabilitySet::default_set();
        let mut shell_opts = ShellOpts::default();
        let ctx = Context {
            args: std::iter::once(name.to_string())
                .chain(args.iter().map(|s| s.to_string()))
                .collect(),
            env: &mut env,
            vars: &mut vars,
            cwd: &mut cwd,
            fs,
            stdin: None,
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap()
    }

    #[tokio::test]
    async fn pwd_returns_cwd() {
        let fs = SandboxFs::new();
        let r = run_with_fs("pwd", &[], &fs, "/home/user").await;
        assert_eq!(r.stdout, "/home/user\n");
    }

    #[tokio::test]
    async fn ls_empty_dir() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.mkdir("/test", &caps).unwrap();
        let r = run_with_fs("ls", &[], &fs, "/test").await;
        assert_eq!(r.stdout, "");
    }

    #[tokio::test]
    async fn ls_with_files() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.mkdir("/test", &caps).unwrap();
        fs.write_file("/test/a.txt", b"a", &caps).unwrap();
        fs.write_file("/test/b.txt", b"b", &caps).unwrap();
        let r = run_with_fs("ls", &[], &fs, "/test").await;
        assert!(r.stdout.contains("a.txt"));
        assert!(r.stdout.contains("b.txt"));
    }
}
