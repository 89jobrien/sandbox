use async_trait::async_trait;
use regex::Regex;

use super::{Builtin, Context};
use crate::capabilities::Cap;
use crate::error::ShellResult;
use crate::interpreter::hooks::ExecResult;

/// Read content from file args or stdin fallback.
fn read_input(ctx: &Context<'_>, files: &[&str]) -> Result<String, ExecResult> {
    if files.is_empty() {
        Ok(ctx
            .stdin
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default())
    } else {
        let mut content = String::new();
        for file in files {
            if *file == "-" {
                if let Some(stdin) = ctx.stdin {
                    content.push_str(&String::from_utf8_lossy(stdin));
                }
                continue;
            }
            let path = ctx.resolve_path(file);
            match ctx.fs.read_to_string(&path, ctx.capabilities) {
                Ok(c) => content.push_str(&c),
                Err(e) => {
                    return Err(ExecResult::failure(1, format!("{file}: {e}")));
                }
            }
        }
        Ok(content)
    }
}

// ── Wc ──────────────────────────────────────────────────────────────

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
        let mut show_lines = false;
        let mut show_words = false;
        let mut show_bytes = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-l" => show_lines = true,
                "-w" => show_words = true,
                "-c" => show_bytes = true,
                _ if arg.starts_with('-') => {
                    // Handle combined flags like -lw
                    for ch in arg[1..].chars() {
                        match ch {
                            'l' => show_lines = true,
                            'w' => show_words = true,
                            'c' => show_bytes = true,
                            _ => {}
                        }
                    }
                }
                _ => files.push(arg.as_str()),
            }
        }

        // Default: show all three
        if !show_lines && !show_words && !show_bytes {
            show_lines = true;
            show_words = true;
            show_bytes = true;
        }

        let content = match read_input(&ctx, &files) {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        let lines = content.lines().count();
        let words = content.split_whitespace().count();
        let bytes = content.len();

        let mut parts = Vec::new();
        if show_lines {
            parts.push(lines.to_string());
        }
        if show_words {
            parts.push(words.to_string());
        }
        if show_bytes {
            parts.push(bytes.to_string());
        }

        Ok(ExecResult::success(format!("{}\n", parts.join(" "))))
    }
}

// ── Basename ────────────────────────────────────────────────────────

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
        let mut name = path.rsplit('/').next().unwrap_or(path).to_string();
        if name.is_empty() {
            name = "/".to_string();
        }

        // Strip suffix if provided
        if args.len() > 1 {
            let suffix = &args[1];
            if let Some(stripped) = name.strip_suffix(suffix.as_str())
                && !stripped.is_empty()
            {
                name = stripped.to_string();
            }
        }

        Ok(ExecResult::success(format!("{name}\n")))
    }
}

// ── Dirname ─────────────────────────────────────────────────────────

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

// ── Sort ────────────────────────────────────────────────────────────

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
        let mut numeric = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-r" => reverse = true,
                "-n" => numeric = true,
                "-rn" | "-nr" => {
                    reverse = true;
                    numeric = true;
                }
                _ if arg.starts_with('-') => {
                    for ch in arg[1..].chars() {
                        match ch {
                            'r' => reverse = true,
                            'n' => numeric = true,
                            _ => {}
                        }
                    }
                }
                _ => files.push(arg.as_str()),
            }
        }

        let content = match read_input(&ctx, &files) {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        let mut lines: Vec<&str> = content.lines().collect();

        if numeric {
            lines.sort_by(|a, b| {
                let na: f64 = a
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0.0);
                let nb: f64 = b
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0.0);
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            lines.sort();
        }

        if reverse {
            lines.reverse();
        }

        let mut output = lines.join("\n");
        if !output.is_empty() {
            output.push('\n');
        }
        Ok(ExecResult::success(output))
    }
}

// ── Uniq ────────────────────────────────────────────────────────────

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
        let mut count = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-c" => count = true,
                _ if arg.starts_with('-') => {}
                _ => files.push(arg.as_str()),
            }
        }

        let content = match read_input(&ctx, &files) {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        let mut output = String::new();
        let mut prev: Option<&str> = None;
        let mut run_count: usize = 0;

        for line in content.lines() {
            if prev == Some(line) {
                run_count += 1;
            } else {
                if let Some(p) = prev {
                    if count {
                        output.push_str(&format!("{run_count} {p}\n"));
                    } else {
                        output.push_str(p);
                        output.push('\n');
                    }
                }
                prev = Some(line);
                run_count = 1;
            }
        }
        // Flush last group
        if let Some(p) = prev {
            if count {
                output.push_str(&format!("{run_count} {p}\n"));
            } else {
                output.push_str(p);
                output.push('\n');
            }
        }

        Ok(ExecResult::success(output))
    }
}

// ── Tee ─────────────────────────────────────────────────────────────

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

        // Pass through to stdout
        Ok(ExecResult::success(&content))
    }
}

// ── GrepBuiltin ─────────────────────────────────────────────────────

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
        let mut case_insensitive = false;
        let mut invert = false;
        let mut count_only = false;
        let mut pattern_str: Option<&str> = None;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-i" => case_insensitive = true,
                "-v" => invert = true,
                "-c" => count_only = true,
                _ if arg.starts_with('-') => {
                    for ch in arg[1..].chars() {
                        match ch {
                            'i' => case_insensitive = true,
                            'v' => invert = true,
                            'c' => count_only = true,
                            _ => {}
                        }
                    }
                }
                _ => {
                    if pattern_str.is_none() {
                        pattern_str = Some(arg.as_str());
                    } else {
                        files.push(arg.as_str());
                    }
                }
            }
        }

        let Some(pat) = pattern_str else {
            return Ok(ExecResult::failure(2, "grep: missing pattern"));
        };

        let regex_pat = if case_insensitive {
            format!("(?i){pat}")
        } else {
            pat.to_string()
        };

        let re = match Regex::new(&regex_pat) {
            Ok(r) => r,
            Err(e) => {
                return Ok(ExecResult::failure(
                    2,
                    format!("grep: invalid pattern: {e}"),
                ));
            }
        };

        let content = match read_input(&ctx, &files) {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        let mut matched = Vec::new();
        for line in content.lines() {
            let is_match = re.is_match(line);
            if is_match != invert {
                matched.push(line);
            }
        }

        if count_only {
            return Ok(ExecResult {
                exit_code: if matched.is_empty() { 1 } else { 0 },
                stdout: format!("{}\n", matched.len()),
                stderr: String::new(),
            });
        }

        if matched.is_empty() {
            Ok(ExecResult::code(1))
        } else {
            let mut output = matched.join("\n");
            output.push('\n');
            Ok(ExecResult::success(output))
        }
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

    async fn run_builtin(name: &str, args: &[&str]) -> ExecResult {
        run_with_stdin(name, args, None).await
    }

    async fn run_with_stdin(name: &str, args: &[&str], stdin: Option<&[u8]>) -> ExecResult {
        let reg = BuiltinRegistry::with_core_builtins();
        let builtin = reg.get(name).unwrap();
        let fs = SandboxFs::new();
        let mut env = HashMap::new();
        let mut vars = HashMap::new();
        let mut cwd = "/".to_string();
        let caps = CapabilitySet::default_set();
        let mut shell_opts = ShellOpts::default();
        let ctx = Context {
            args: std::iter::once(name.to_string())
                .chain(args.iter().map(|s| s.to_string()))
                .collect(),
            env: &mut env,
            vars: &mut vars,
            cwd: &mut cwd,
            fs: &fs,
            stdin,
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap()
    }

    async fn run_with_fs_stdin(
        name: &str,
        args: &[&str],
        fs: &SandboxFs,
        stdin: Option<&[u8]>,
    ) -> ExecResult {
        let reg = BuiltinRegistry::with_core_builtins();
        let builtin = reg.get(name).unwrap();
        let mut env = HashMap::new();
        let mut vars = HashMap::new();
        let mut cwd = "/".to_string();
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
            stdin,
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap()
    }

    // ── Wc ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn wc_lines_from_stdin() {
        let r = run_with_stdin("wc", &["-l"], Some(b"a\nb\nc\n")).await;
        assert_eq!(r.stdout, "3\n");
    }

    #[tokio::test]
    async fn wc_all_from_stdin() {
        let r = run_with_stdin("wc", &[], Some(b"hello world\n")).await;
        // 1 line, 2 words, 12 bytes
        assert_eq!(r.stdout, "1 2 12\n");
    }

    #[tokio::test]
    async fn wc_words_only() {
        let r = run_with_stdin("wc", &["-w"], Some(b"one two three")).await;
        assert_eq!(r.stdout, "3\n");
    }

    // ── Basename ────────────────────────────────────────────────────

    #[tokio::test]
    async fn basename_simple() {
        let r = run_builtin("basename", &["/usr/local/bin/foo"]).await;
        assert_eq!(r.stdout, "foo\n");
    }

    #[tokio::test]
    async fn basename_with_suffix() {
        let r = run_builtin("basename", &["/path/to/file.txt", ".txt"]).await;
        assert_eq!(r.stdout, "file\n");
    }

    #[tokio::test]
    async fn basename_no_slash() {
        let r = run_builtin("basename", &["file.rs"]).await;
        assert_eq!(r.stdout, "file.rs\n");
    }

    // ── Dirname ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn dirname_simple() {
        let r = run_builtin("dirname", &["/usr/local/bin/foo"]).await;
        assert_eq!(r.stdout, "/usr/local/bin\n");
    }

    #[tokio::test]
    async fn dirname_no_slash() {
        let r = run_builtin("dirname", &["file.rs"]).await;
        assert_eq!(r.stdout, ".\n");
    }

    #[tokio::test]
    async fn dirname_root() {
        let r = run_builtin("dirname", &["/foo"]).await;
        assert_eq!(r.stdout, "/\n");
    }

    // ── Sort ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn sort_alphabetical() {
        let r = run_with_stdin("sort", &[], Some(b"banana\napple\ncherry\n")).await;
        assert_eq!(r.stdout, "apple\nbanana\ncherry\n");
    }

    #[tokio::test]
    async fn sort_reverse() {
        let r = run_with_stdin("sort", &["-r"], Some(b"a\nb\nc\n")).await;
        assert_eq!(r.stdout, "c\nb\na\n");
    }

    #[tokio::test]
    async fn sort_numeric() {
        let r = run_with_stdin("sort", &["-n"], Some(b"10\n2\n1\n")).await;
        assert_eq!(r.stdout, "1\n2\n10\n");
    }

    // ── Uniq ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn uniq_removes_adjacent_dupes() {
        let r = run_with_stdin("uniq", &[], Some(b"a\na\nb\nb\na\n")).await;
        assert_eq!(r.stdout, "a\nb\na\n");
    }

    #[tokio::test]
    async fn uniq_count() {
        let r = run_with_stdin("uniq", &["-c"], Some(b"a\na\na\nb\n")).await;
        assert_eq!(r.stdout, "3 a\n1 b\n");
    }

    // ── Tee ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn tee_writes_file_and_passes_through() {
        let fs = SandboxFs::new();
        let caps = CapabilitySet::default_set();
        let r = run_with_fs_stdin("tee", &["/out.txt"], &fs, Some(b"hello\n")).await;
        assert_eq!(r.stdout, "hello\n");
        let content = fs.read_to_string("/out.txt", &caps).unwrap();
        assert_eq!(content, "hello\n");
    }

    // ── Grep ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn grep_basic_match() {
        let r = run_with_stdin("grep", &["hello"], Some(b"hello world\nfoo bar\n")).await;
        assert_eq!(r.stdout, "hello world\n");
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn grep_no_match() {
        let r = run_with_stdin("grep", &["xyz"], Some(b"hello\nworld\n")).await;
        assert_eq!(r.exit_code, 1);
    }

    #[tokio::test]
    async fn grep_case_insensitive() {
        let r = run_with_stdin("grep", &["-i", "HELLO"], Some(b"Hello World\nfoo\n")).await;
        assert_eq!(r.stdout, "Hello World\n");
    }

    #[tokio::test]
    async fn grep_invert() {
        let r = run_with_stdin("grep", &["-v", "foo"], Some(b"foo\nbar\nbaz\n")).await;
        assert_eq!(r.stdout, "bar\nbaz\n");
    }

    #[tokio::test]
    async fn grep_count() {
        let r = run_with_stdin("grep", &["-c", "a"], Some(b"apple\nbanana\ncherry\n")).await;
        assert_eq!(r.stdout, "2\n");
        assert_eq!(r.exit_code, 0);
    }
}
