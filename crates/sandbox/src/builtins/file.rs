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

pub struct Wc;

#[async_trait]
impl Builtin for Wc {
    fn name(&self) -> &str {
        "wc"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut flag_l = false;
        let mut flag_w = false;
        let mut flag_c = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-l" => flag_l = true,
                "-w" => flag_w = true,
                "-c" => flag_c = true,
                _ if !arg.starts_with('-') => files.push(arg.as_str()),
                _ => {}
            }
        }

        if !flag_l && !flag_w && !flag_c {
            flag_l = true;
            flag_w = true;
            flag_c = true;
        }

        let content = if files.is_empty() {
            ctx.stdin
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default()
        } else {
            let path = ctx.resolve_path(files[0]);
            ctx.fs.read_to_string(&path, ctx.capabilities)?
        };

        let lines = content.lines().count();
        let words = content.split_whitespace().count();
        let chars = content.len();

        let mut parts = Vec::new();
        if flag_l {
            parts.push(format!("{lines}"));
        }
        if flag_w {
            parts.push(format!("{words}"));
        }
        if flag_c {
            parts.push(format!("{chars}"));
        }

        let output = format!("{}\n", parts.join(" "));
        Ok(ExecResult::success(output))
    }
}

pub struct Basename;

#[async_trait]
impl Builtin for Basename {
    fn name(&self) -> &str {
        "basename"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        if args.is_empty() {
            return Ok(ExecResult::failure(1, "basename: missing operand"));
        }
        let path = &args[0];
        let name = path.rsplit('/').next().unwrap_or(path);
        let name = if name.is_empty() { "/" } else { name };

        let result = if args.len() > 1 {
            let suffix = &args[1];
            name.strip_suffix(suffix.as_str()).unwrap_or(name)
        } else {
            name
        };

        Ok(ExecResult::success(format!("{result}\n")))
    }
}

pub struct Dirname;

#[async_trait]
impl Builtin for Dirname {
    fn name(&self) -> &str {
        "dirname"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        if args.is_empty() {
            return Ok(ExecResult::failure(1, "dirname: missing operand"));
        }
        let path = &args[0];
        let dir = if let Some(pos) = path.rfind('/') {
            if pos == 0 { "/" } else { &path[..pos] }
        } else {
            "."
        };
        Ok(ExecResult::success(format!("{dir}\n")))
    }
}

pub struct Sort;

#[async_trait]
impl Builtin for Sort {
    fn name(&self) -> &str {
        "sort"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut reverse = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-r" => reverse = true,
                _ if !arg.starts_with('-') => files.push(arg.as_str()),
                _ => {}
            }
        }

        let content = if files.is_empty() {
            ctx.stdin
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default()
        } else {
            let path = ctx.resolve_path(files[0]);
            ctx.fs.read_to_string(&path, ctx.capabilities)?
        };

        let mut lines: Vec<&str> = content.lines().collect();
        lines.sort();
        if reverse {
            lines.reverse();
        }

        let output = lines.join("\n") + "\n";
        Ok(ExecResult::success(output))
    }
}

pub struct Uniq;

#[async_trait]
impl Builtin for Uniq {
    fn name(&self) -> &str {
        "uniq"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut files = Vec::new();

        for arg in args {
            if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        let content = if files.is_empty() {
            ctx.stdin
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default()
        } else {
            let path = ctx.resolve_path(files[0]);
            ctx.fs.read_to_string(&path, ctx.capabilities)?
        };

        let mut result = Vec::new();
        let mut prev: Option<&str> = None;
        for line in content.lines() {
            if prev != Some(line) {
                result.push(line);
                prev = Some(line);
            }
        }

        let output = result.join("\n") + "\n";
        Ok(ExecResult::success(output))
    }
}

pub struct Tee;

#[async_trait]
impl Builtin for Tee {
    fn name(&self) -> &str {
        "tee"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs, Cap::WriteFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut files = Vec::new();

        for arg in args {
            if !arg.starts_with('-') {
                files.push(arg.as_str());
            }
        }

        let content = ctx
            .stdin
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();

        for file in &files {
            let path = ctx.resolve_path(file);
            ctx.fs
                .write_file(&path, content.as_bytes(), ctx.capabilities)?;
        }

        Ok(ExecResult::success(content))
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

pub struct GrepBuiltin;

#[async_trait]
impl Builtin for GrepBuiltin {
    fn name(&self) -> &str {
        "grep"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut ignore_case = false;
        let mut count_only = false;
        let mut files_only = false;
        let mut positional = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-i" => ignore_case = true,
                "-c" => count_only = true,
                "-l" => files_only = true,
                _ if !arg.starts_with('-') => positional.push(arg.as_str()),
                _ => {}
            }
        }

        if positional.is_empty() {
            return Ok(ExecResult::failure(1, "grep: missing pattern"));
        }

        let pattern = positional[0];
        let file_args = &positional[1..];

        let search = |content: &str, filename: Option<&str>| -> (Vec<String>, usize) {
            let mut matches = Vec::new();
            let mut count = 0;
            let needle = if ignore_case {
                pattern.to_lowercase()
            } else {
                pattern.to_string()
            };
            for line in content.lines() {
                let haystack = if ignore_case {
                    line.to_lowercase()
                } else {
                    line.to_string()
                };
                if haystack.contains(&needle) {
                    count += 1;
                    if !count_only && !files_only {
                        matches.push(line.to_string());
                    }
                }
            }
            if files_only
                && count > 0
                && let Some(f) = filename
            {
                matches.push(f.to_string());
            }
            (matches, count)
        };

        if file_args.is_empty() {
            let content = ctx
                .stdin
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default();
            let (matches, count) = search(&content, None);
            if count_only {
                return Ok(ExecResult::success(format!("{count}\n")));
            }
            if matches.is_empty() {
                return Ok(ExecResult::code(1));
            }
            return Ok(ExecResult::success(matches.join("\n") + "\n"));
        }

        let mut all_matches = Vec::new();
        let mut total_count = 0;
        for file in file_args {
            let path = ctx.resolve_path(file);
            let content = ctx.fs.read_to_string(&path, ctx.capabilities)?;
            let (matches, count) = search(&content, Some(file));
            total_count += count;
            if count_only {
                all_matches.push(format!("{count}"));
            } else {
                all_matches.extend(matches);
            }
        }

        if all_matches.is_empty() && total_count == 0 {
            return Ok(ExecResult::code(1));
        }

        Ok(ExecResult::success(all_matches.join("\n") + "\n"))
    }
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

    async fn run_with_fs_stdin(
        name: &str,
        args: &[&str],
        fs: &SandboxFs,
        cwd: &str,
        stdin: &[u8],
    ) -> ExecResult {
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
            stdin: Some(stdin),
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap()
    }

    #[tokio::test]
    async fn wc_counts_all() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/f.txt", b"hello world\nfoo\n", &caps)
            .unwrap();
        let r = run_with_fs("wc", &["/f.txt"], &fs, "/").await;
        assert_eq!(r.stdout, "2 3 16\n");
    }

    #[tokio::test]
    async fn wc_lines_only() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/f.txt", b"a\nb\nc\n", &caps).unwrap();
        let r = run_with_fs("wc", &["-l", "/f.txt"], &fs, "/").await;
        assert_eq!(r.stdout, "3\n");
    }

    #[tokio::test]
    async fn basename_basic() {
        let fs = SandboxFs::new();
        let r = run_with_fs("basename", &["/usr/local/bin/foo"], &fs, "/").await;
        assert_eq!(r.stdout, "foo\n");
    }

    #[tokio::test]
    async fn basename_with_suffix() {
        let fs = SandboxFs::new();
        let r = run_with_fs("basename", &["file.txt", ".txt"], &fs, "/").await;
        assert_eq!(r.stdout, "file\n");
    }

    #[tokio::test]
    async fn dirname_basic() {
        let fs = SandboxFs::new();
        let r = run_with_fs("dirname", &["/usr/local/bin"], &fs, "/").await;
        assert_eq!(r.stdout, "/usr/local\n");
    }

    #[tokio::test]
    async fn dirname_no_slash() {
        let fs = SandboxFs::new();
        let r = run_with_fs("dirname", &["file.txt"], &fs, "/").await;
        assert_eq!(r.stdout, ".\n");
    }

    #[tokio::test]
    async fn sort_lines() {
        let fs = SandboxFs::new();
        let r = run_with_fs_stdin("sort", &[], &fs, "/", b"banana\napple\ncherry\n").await;
        assert_eq!(r.stdout, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn sort_reverse() {
        let fs = SandboxFs::new();
        let r = run_with_fs_stdin("sort", &["-r"], &fs, "/", b"a\nb\nc\n").await;
        assert_eq!(r.stdout, "c\nb\na\n");
    }

    #[tokio::test]
    async fn uniq_adjacent_dupes() {
        let fs = SandboxFs::new();
        let r = run_with_fs_stdin("uniq", &[], &fs, "/", b"a\na\nb\nb\na\n").await;
        assert_eq!(r.stdout, "a\nb\na\n");
    }

    #[tokio::test]
    async fn tee_writes_file_and_stdout() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        let r = run_with_fs_stdin("tee", &["/out.txt"], &fs, "/", b"hello").await;
        assert_eq!(r.stdout, "hello");
        let content = fs.read_to_string("/out.txt", &caps).unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn find_by_name() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.mkdir("/d", &caps).unwrap();
        fs.write_file("/d/foo.txt", b"", &caps).unwrap();
        fs.write_file("/d/bar.rs", b"", &caps).unwrap();
        let r = run_with_fs("find", &["/d", "-name", "*.txt"], &fs, "/").await;
        assert_eq!(r.stdout, "/d/foo.txt\n");
    }

    #[tokio::test]
    async fn grep_basic_match() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/f.txt", b"hello world\nfoo bar\nhello again\n", &caps)
            .unwrap();
        let r = run_with_fs("grep", &["hello", "/f.txt"], &fs, "/").await;
        assert_eq!(r.stdout, "hello world\nhello again\n");
    }

    #[tokio::test]
    async fn grep_case_insensitive() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/f.txt", b"Hello\nhello\nHELLO\n", &caps)
            .unwrap();
        let r = run_with_fs("grep", &["-i", "hello", "/f.txt"], &fs, "/").await;
        assert_eq!(r.stdout, "Hello\nhello\nHELLO\n");
    }

    #[tokio::test]
    async fn grep_count() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/f.txt", b"a\nb\na\n", &caps).unwrap();
        let r = run_with_fs("grep", &["-c", "a", "/f.txt"], &fs, "/").await;
        assert_eq!(r.stdout, "2\n");
    }

    #[tokio::test]
    async fn grep_no_match_returns_1() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        fs.write_file("/f.txt", b"hello\n", &caps).unwrap();
        let r = run_with_fs("grep", &["xyz", "/f.txt"], &fs, "/").await;
        assert_eq!(r.exit_code, 1);
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
