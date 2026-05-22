use async_trait::async_trait;

use super::{Builtin, Context};
use crate::capabilities::Cap;
use crate::error::ShellResult;
use crate::interpreter::hooks::ExecResult;

pub struct Mkdir;

#[async_trait]
impl Builtin for Mkdir {
    fn name(&self) -> &str {
        "mkdir"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::WriteFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        if args.is_empty() {
            return Ok(ExecResult::failure(1, "mkdir: missing operand"));
        }

        // -p is always implied in virtual FS (create_dir_all)
        for arg in args {
            if arg.starts_with('-') {
                continue;
            }
            let path = ctx.resolve_path(arg);
            ctx.fs.mkdir(&path, ctx.capabilities)?;
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Rm;

#[async_trait]
impl Builtin for Rm {
    fn name(&self) -> &str {
        "rm"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::WriteFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut recursive = false;
        let mut force = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-f" | "--force" => force = true,
                "-rf" | "-fr" => {
                    recursive = true;
                    force = true;
                }
                _ => files.push(arg.as_str()),
            }
        }

        if files.is_empty() {
            return Ok(ExecResult::failure(1, "rm: missing operand"));
        }

        for file in files {
            let path = ctx.resolve_path(file);
            if ctx.fs.is_dir(&path)? {
                if recursive {
                    if let Err(e) = ctx.fs.remove_dir_all(&path, ctx.capabilities)
                        && !force
                    {
                        return Ok(ExecResult::failure(1, format!("rm: {file}: {e}")));
                    }
                } else {
                    return Ok(ExecResult::failure(
                        1,
                        format!("rm: {file}: is a directory"),
                    ));
                }
            } else if let Err(e) = ctx.fs.remove_file(&path, ctx.capabilities)
                && !force
            {
                return Ok(ExecResult::failure(1, format!("rm: {file}: {e}")));
            }
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Cp;

#[async_trait]
impl Builtin for Cp {
    fn name(&self) -> &str {
        "cp"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs, Cap::WriteFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut files: Vec<&str> = Vec::new();

        for arg in args {
            if !arg.starts_with('-') {
                files.push(arg);
            }
        }

        if files.len() < 2 {
            return Ok(ExecResult::failure(1, "cp: missing file operand"));
        }

        let dst = files.pop().unwrap();
        for src in &files {
            let src_path = ctx.resolve_path(src);
            let dst_path = if ctx.fs.is_dir(&ctx.resolve_path(dst))? {
                let name = src.rsplit('/').next().unwrap_or(src);
                ctx.resolve_path(&format!("{dst}/{name}"))
            } else {
                ctx.resolve_path(dst)
            };
            ctx.fs.copy_file(&src_path, &dst_path, ctx.capabilities)?;
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Mv;

#[async_trait]
impl Builtin for Mv {
    fn name(&self) -> &str {
        "mv"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs, Cap::WriteFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut files: Vec<&str> = Vec::new();

        for arg in args {
            if !arg.starts_with('-') {
                files.push(arg);
            }
        }

        if files.len() < 2 {
            return Ok(ExecResult::failure(1, "mv: missing file operand"));
        }

        let dst = files.pop().unwrap();
        for src in &files {
            let src_path = ctx.resolve_path(src);
            let dst_path = if ctx.fs.is_dir(&ctx.resolve_path(dst))? {
                let name = src.rsplit('/').next().unwrap_or(src);
                ctx.resolve_path(&format!("{dst}/{name}"))
            } else {
                ctx.resolve_path(dst)
            };
            ctx.fs.rename(&src_path, &dst_path, ctx.capabilities)?;
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Touch;

#[async_trait]
impl Builtin for Touch {
    fn name(&self) -> &str {
        "touch"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::WriteFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let files = ctx.args_from(1);
        if files.is_empty() {
            return Ok(ExecResult::failure(1, "touch: missing file operand"));
        }

        for file in files {
            if file.starts_with('-') {
                continue;
            }
            let path = ctx.resolve_path(file);
            if !ctx.fs.exists(&path)? {
                ctx.fs.write_file(&path, b"", ctx.capabilities)?;
            }
            // In virtual FS, touch on existing files is a no-op (no timestamps)
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Find;

#[async_trait]
impl Builtin for Find {
    fn name(&self) -> &str {
        "find"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut search_dir = ".";
        let mut name_pattern: Option<&str> = None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "-name" => {
                    i += 1;
                    if i < args.len() {
                        name_pattern = Some(&args[i]);
                    }
                }
                s if !s.starts_with('-') && name_pattern.is_none() => {
                    search_dir = s;
                }
                _ => {}
            }
            i += 1;
        }

        let root = ctx.resolve_path(search_dir);
        let mut results = Vec::new();
        find_walk(ctx.fs, ctx.capabilities, &root, name_pattern, &mut results)?;
        results.sort();

        let output = if results.is_empty() {
            String::new()
        } else {
            results.join("\n") + "\n"
        };
        Ok(ExecResult::success(output))
    }
}

fn find_walk(
    fs: &crate::fs::SandboxFs,
    caps: &crate::capabilities::CapabilitySet,
    dir: &str,
    pattern: Option<&str>,
    results: &mut Vec<String>,
) -> ShellResult<()> {
    let entries = fs.list_dir(dir, caps)?;
    for entry in entries {
        let full = if dir == "/" {
            format!("/{}", entry.name)
        } else {
            format!("{}/{}", dir, entry.name)
        };
        if let Some(pat) = pattern {
            if glob_match(pat, &entry.name) {
                results.push(full.clone());
            }
        } else {
            results.push(full.clone());
        }
        if entry.is_dir {
            find_walk(fs, caps, &full, pattern, results)?;
        }
    }
    Ok(())
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; t.len() + 1]; p.len() + 1];
    dp[0][0] = true;

    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=p.len() {
        for j in 1..=t.len() {
            if p[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[i - 1] == '?' || p[i - 1] == t[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[p.len()][t.len()]
}

#[cfg(test)]
mod tests {
    use crate::builtins::BuiltinRegistry;
    use crate::capabilities::CapabilitySet;
    use crate::fs::SandboxFs;
    use crate::interpreter::ShellOpts;
    use std::collections::HashMap;

    use super::*;

    async fn run_with_fs(name: &str, args: &[&str], fs: &SandboxFs, cwd: &str) -> ExecResult {
        let reg = BuiltinRegistry::with_core_builtins();
        let builtin = reg.get(name).unwrap();
        let mut env = HashMap::new();
        let mut vars = HashMap::new();
        let mut cwd = cwd.to_string();
        let caps = CapabilitySet::default_set();
        let mut shell_opts = ShellOpts::default();
        let ctx = super::super::Context {
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
    async fn mkdir_and_touch() {
        let fs = SandboxFs::new();
        let _caps = CapabilitySet::default_set();
        run_with_fs("mkdir", &["-p", "subdir"], &fs, "/").await;
        run_with_fs("touch", &["subdir/file.txt"], &fs, "/").await;
        assert!(fs.exists("/subdir/file.txt").unwrap());
    }

    #[tokio::test]
    async fn cp_file() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/src.txt", b"hello", &caps).unwrap();
        run_with_fs("cp", &["src.txt", "dst.txt"], &fs, "/").await;
        let content = fs.read_to_string("/dst.txt", &caps).unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn mv_file() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/old.txt", b"data", &caps).unwrap();
        run_with_fs("mv", &["old.txt", "new.txt"], &fs, "/").await;
        assert!(!fs.exists("/old.txt").unwrap());
        assert!(fs.exists("/new.txt").unwrap());
    }

    #[tokio::test]
    async fn rm_file() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/file.txt", b"data", &caps).unwrap();
        run_with_fs("rm", &["file.txt"], &fs, "/").await;
        assert!(!fs.exists("/file.txt").unwrap());
    }

    #[tokio::test]
    async fn rm_dir_recursive() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.mkdir("/dir", &caps).unwrap();
        fs.write_file("/dir/file.txt", b"data", &caps).unwrap();
        run_with_fs("rm", &["-rf", "dir"], &fs, "/").await;
        assert!(!fs.exists("/dir").unwrap());
    }
}
