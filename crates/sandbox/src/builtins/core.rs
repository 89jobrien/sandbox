use async_trait::async_trait;

use super::{Builtin, Context};
use crate::capabilities::Cap;
use crate::error::ShellResult;
use crate::interpreter::hooks::ExecResult;

pub struct Echo;

#[async_trait]
impl Builtin for Echo {
    fn name(&self) -> &str {
        "echo"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let mut no_newline = false;
        let mut interpret_escapes = false;
        let mut start = 0;

        for (i, arg) in args.iter().enumerate() {
            match arg.as_str() {
                "-n" => {
                    no_newline = true;
                    start = i + 1;
                }
                "-e" => {
                    interpret_escapes = true;
                    start = i + 1;
                }
                "-E" => {
                    interpret_escapes = false;
                    start = i + 1;
                }
                _ => break,
            }
        }

        let mut output = args[start..].join(" ");
        if interpret_escapes {
            output = interpret_escape_sequences(&output);
        }
        if !no_newline {
            output.push('\n');
        }
        Ok(ExecResult::success(output))
    }
}

fn interpret_escape_sequences(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('a') => result.push('\x07'),
                Some('b') => result.push('\x08'),
                Some('f') => result.push('\x0C'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub struct Printf;

#[async_trait]
impl Builtin for Printf {
    fn name(&self) -> &str {
        "printf"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        if args.is_empty() {
            return Ok(ExecResult::failure(
                1,
                "printf: usage: printf format [args]",
            ));
        }
        let format = &args[0];
        let format_args = &args[1..];
        let output = simple_printf(format, format_args);
        Ok(ExecResult::success(output))
    }
}

fn simple_printf(format: &str, args: &[String]) -> String {
    let mut result = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;
    let mut arg_idx = 0;

    while i < chars.len() {
        if chars[i] == '\\' {
            i += 1;
            if i < chars.len() {
                match chars[i] {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'r' => result.push('\r'),
                    '\\' => result.push('\\'),
                    other => {
                        result.push('\\');
                        result.push(other);
                    }
                }
            }
            i += 1;
        } else if chars[i] == '%' {
            i += 1;
            if i < chars.len() {
                match chars[i] {
                    's' => {
                        if arg_idx < args.len() {
                            result.push_str(&args[arg_idx]);
                            arg_idx += 1;
                        }
                    }
                    'd' => {
                        if arg_idx < args.len() {
                            let n: i64 = args[arg_idx].parse().unwrap_or(0);
                            result.push_str(&n.to_string());
                            arg_idx += 1;
                        }
                    }
                    '%' => result.push('%'),
                    _ => {
                        result.push('%');
                        result.push(chars[i]);
                    }
                }
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

pub struct Cat;

#[async_trait]
impl Builtin for Cat {
    fn name(&self) -> &str {
        "cat"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let files = ctx.args_from(1);

        if files.is_empty() {
            // Read from stdin
            if let Some(stdin) = ctx.stdin {
                let s = String::from_utf8_lossy(stdin).to_string();
                return Ok(ExecResult::success(s));
            }
            return Ok(ExecResult::success(""));
        }

        let mut output = String::new();
        for file in files {
            if file == "-" {
                if let Some(stdin) = ctx.stdin {
                    output.push_str(&String::from_utf8_lossy(stdin));
                }
                continue;
            }
            let path = ctx.resolve_path(file);
            match ctx.fs.read_to_string(&path, ctx.capabilities) {
                Ok(content) => output.push_str(&content),
                Err(e) => {
                    return Ok(ExecResult::failure(1, format!("cat: {file}: {e}")));
                }
            }
        }
        Ok(ExecResult::success(output))
    }
}

pub struct Read_;

#[async_trait]
impl Builtin for Read_ {
    fn name(&self) -> &str {
        "read"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let var_names: Vec<String> = ctx.args_from(1).to_vec();
        let input = ctx
            .stdin
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let line = input.lines().next().unwrap_or("");

        if var_names.is_empty() {
            ctx.vars.insert("REPLY".into(), line.to_string());
        } else {
            let parts: Vec<&str> = line.splitn(var_names.len(), char::is_whitespace).collect();
            for (i, name) in var_names.iter().enumerate() {
                let val = parts.get(i).unwrap_or(&"").to_string();
                ctx.vars.insert(name.clone(), val);
            }
        }
        Ok(ExecResult::code(0))
    }
}

pub struct Head;

#[async_trait]
impl Builtin for Head {
    fn name(&self) -> &str {
        "head"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let mut n_lines: usize = 10;
        let mut files = Vec::new();
        let args = ctx.args_from(1);
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "-n" => {
                    i += 1;
                    if i < args.len() {
                        n_lines = args[i].parse().unwrap_or(10);
                    }
                }
                s if s.starts_with('-') && s[1..].parse::<usize>().is_ok() => {
                    n_lines = s[1..].parse().unwrap();
                }
                _ => files.push(args[i].as_str()),
            }
            i += 1;
        }

        let content = if files.is_empty() {
            ctx.stdin
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default()
        } else {
            let path = ctx.resolve_path(files[0]);
            ctx.fs.read_to_string(&path, ctx.capabilities)?
        };

        let output: String = content.lines().take(n_lines).collect::<Vec<_>>().join("\n") + "\n";

        Ok(ExecResult::success(output))
    }
}

pub struct Tail;

#[async_trait]
impl Builtin for Tail {
    fn name(&self) -> &str {
        "tail"
    }

    fn required_capabilities(&self) -> &[Cap] {
        &[Cap::ReadFs]
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let mut n_lines: usize = 10;
        let mut files = Vec::new();
        let args = ctx.args_from(1);
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "-n" => {
                    i += 1;
                    if i < args.len() {
                        n_lines = args[i].parse().unwrap_or(10);
                    }
                }
                s if s.starts_with('-') && s[1..].parse::<usize>().is_ok() => {
                    n_lines = s[1..].parse().unwrap();
                }
                _ => files.push(args[i].as_str()),
            }
            i += 1;
        }

        let content = if files.is_empty() {
            ctx.stdin
                .map(|b| String::from_utf8_lossy(b).to_string())
                .unwrap_or_default()
        } else {
            let path = ctx.resolve_path(files[0]);
            ctx.fs.read_to_string(&path, ctx.capabilities)?
        };

        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(n_lines);
        let output = lines[start..].join("\n") + "\n";

        Ok(ExecResult::success(output))
    }
}

pub struct Test_;

#[async_trait]
impl Builtin for Test_ {
    fn name(&self) -> &str {
        "test"
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        let result = evaluate_test(args, ctx.fs, ctx.cwd);
        Ok(ExecResult::code(if result { 0 } else { 1 }))
    }
}

pub struct BracketTest;

#[async_trait]
impl Builtin for BracketTest {
    fn name(&self) -> &str {
        "["
    }

    async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
        let args = ctx.args_from(1);
        // Strip trailing ]
        let args = if args.last().map(|s| s.as_str()) == Some("]") {
            &args[..args.len() - 1]
        } else {
            args
        };
        let result = evaluate_test(args, ctx.fs, ctx.cwd);
        Ok(ExecResult::code(if result { 0 } else { 1 }))
    }
}

fn evaluate_test(args: &[String], fs: &crate::fs::SandboxFs, cwd: &str) -> bool {
    if args.is_empty() {
        return false;
    }

    fn resolve(path: &str, cwd: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{cwd}/{path}")
        }
    }

    // Unary operators
    if args.len() == 2 {
        let op = args[0].as_str();
        let val = &args[1];
        return match op {
            "-z" => val.is_empty(),
            "-n" => !val.is_empty(),
            "-e" => fs.exists(&resolve(val, cwd)).unwrap_or(false),
            "-f" => fs.is_file(&resolve(val, cwd)).unwrap_or(false),
            "-d" => fs.is_dir(&resolve(val, cwd)).unwrap_or(false),
            "!" => !evaluate_test(&args[1..], fs, cwd),
            _ => !val.is_empty(),
        };
    }

    // Single arg: true if non-empty
    if args.len() == 1 {
        return !args[0].is_empty();
    }

    // Binary operators
    if args.len() == 3 {
        let left = &args[0];
        let op = args[1].as_str();
        let right = &args[2];
        return match op {
            "=" | "==" => left == right,
            "!=" => left != right,
            "-eq" => left.parse::<i64>().unwrap_or(0) == right.parse::<i64>().unwrap_or(0),
            "-ne" => left.parse::<i64>().unwrap_or(0) != right.parse::<i64>().unwrap_or(0),
            "-lt" => left.parse::<i64>().unwrap_or(0) < right.parse::<i64>().unwrap_or(0),
            "-le" => left.parse::<i64>().unwrap_or(0) <= right.parse::<i64>().unwrap_or(0),
            "-gt" => left.parse::<i64>().unwrap_or(0) > right.parse::<i64>().unwrap_or(0),
            "-ge" => left.parse::<i64>().unwrap_or(0) >= right.parse::<i64>().unwrap_or(0),
            _ => false,
        };
    }

    // Negation with complex expression
    if args.len() > 1 && args[0] == "!" {
        return !evaluate_test(&args[1..], fs, cwd);
    }

    false
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
            stdin: None,
            capabilities: &caps,
            last_exit_code: 0,
            shell_opts: &mut shell_opts,
        };
        builtin.execute(ctx).await.unwrap()
    }

    #[tokio::test]
    async fn echo_basic() {
        let r = run_builtin("echo", &["hello", "world"]).await;
        assert_eq!(r.stdout, "hello world\n");
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn echo_no_newline() {
        let r = run_builtin("echo", &["-n", "hello"]).await;
        assert_eq!(r.stdout, "hello");
    }

    #[tokio::test]
    async fn printf_basic() {
        let r = run_builtin("printf", &["%s is %d", "answer", "42"]).await;
        assert_eq!(r.stdout, "answer is 42");
    }

    #[tokio::test]
    async fn test_string_comparison() {
        let r = run_builtin("test", &["hello", "=", "hello"]).await;
        assert_eq!(r.exit_code, 0);

        let r = run_builtin("test", &["hello", "!=", "world"]).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn test_numeric_comparison() {
        let r = run_builtin("test", &["5", "-gt", "3"]).await;
        assert_eq!(r.exit_code, 0);

        let r = run_builtin("test", &["3", "-gt", "5"]).await;
        assert_eq!(r.exit_code, 1);
    }

    #[tokio::test]
    async fn test_empty_string() {
        let r = run_builtin("test", &["-z", ""]).await;
        assert_eq!(r.exit_code, 0);

        let r = run_builtin("test", &["-n", "hello"]).await;
        assert_eq!(r.exit_code, 0);
    }
}
