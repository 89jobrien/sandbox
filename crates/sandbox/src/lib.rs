pub mod builtins;
pub mod capabilities;
pub mod error;
pub mod fs;
pub mod interpreter;
pub mod limits;
pub mod parser;
pub mod snapshot;
pub mod trace;

use std::collections::HashMap;

use builtins::BuiltinRegistry;
use capabilities::{Cap, CapabilitySet};
use error::{ShellError, ShellResult};
use fs::SandboxFs;
use interpreter::hooks::{DefaultExecHandler, ExecHandler};
use interpreter::{Interpreter, ShellOpts};
use limits::{ExecutionCounters, ExecutionLimits};
use parser::Parser;

#[derive(Debug, Clone)]
pub struct ShellOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct Shell {
    interpreter: Interpreter,
}

impl Shell {
    pub fn builder() -> ShellBuilder {
        ShellBuilder::default()
    }

    pub async fn exec(&mut self, input: &str) -> ShellResult<ShellOutput> {
        if input.len() > self.interpreter.limits.max_input_bytes {
            return Err(ShellError::LimitExceeded(format!(
                "input too large ({} bytes, max {})",
                input.len(),
                self.interpreter.limits.max_input_bytes
            )));
        }

        let cmd = Parser::parse(
            input,
            self.interpreter.limits.max_parser_fuel,
            self.interpreter.limits.max_ast_depth,
        )?;

        self.interpreter.stdout_buf.clear();
        self.interpreter.stderr_buf.clear();

        let result = self.interpreter.execute(&cmd).await?;

        let stdout = if !result.stdout.is_empty() && self.interpreter.stdout_buf.is_empty() {
            result.stdout
        } else {
            std::mem::take(&mut self.interpreter.stdout_buf)
        };

        let stderr = if !result.stderr.is_empty() && self.interpreter.stderr_buf.is_empty() {
            result.stderr
        } else {
            std::mem::take(&mut self.interpreter.stderr_buf)
        };

        Ok(ShellOutput {
            exit_code: result.exit_code,
            stdout,
            stderr,
        })
    }

    pub fn env(&self) -> &HashMap<String, String> {
        &self.interpreter.env
    }

    pub fn vars(&self) -> &HashMap<String, String> {
        &self.interpreter.vars
    }

    pub fn cwd(&self) -> &str {
        &self.interpreter.cwd
    }

    pub fn fs(&self) -> &SandboxFs {
        &self.interpreter.fs
    }

    pub fn last_exit_code(&self) -> i32 {
        self.interpreter.last_exit_code
    }

    pub fn register_builtin(&mut self, builtin: impl builtins::Builtin + 'static) {
        self.interpreter.builtins.register(builtin);
    }
}

pub struct ShellBuilder {
    env: HashMap<String, String>,
    cwd: String,
    limits: ExecutionLimits,
    capabilities: Option<CapabilitySet>,
    fs: Option<SandboxFs>,
    exec_handler: Option<Box<dyn ExecHandler>>,
    builtins: Option<BuiltinRegistry>,
}

impl Default for ShellBuilder {
    fn default() -> Self {
        Self {
            env: HashMap::new(),
            cwd: "/".to_string(),
            limits: ExecutionLimits::default(),
            capabilities: None,
            fs: None,
            exec_handler: None,
            builtins: None,
        }
    }
}

impl ShellBuilder {
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn envs(mut self, envs: impl IntoIterator<Item = (String, String)>) -> Self {
        self.env.extend(envs);
        self
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = cwd.into();
        self
    }

    pub fn limits(mut self, limits: ExecutionLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = Some(caps);
        self
    }

    pub fn capability(mut self, cap: Cap) -> Self {
        self.capabilities
            .get_or_insert_with(CapabilitySet::default_set)
            .grant(cap);
        self
    }

    pub fn fs(mut self, fs: SandboxFs) -> Self {
        self.fs = Some(fs);
        self
    }

    pub fn exec_handler(mut self, handler: impl ExecHandler + 'static) -> Self {
        self.exec_handler = Some(Box::new(handler));
        self
    }

    pub fn builtins(mut self, registry: BuiltinRegistry) -> Self {
        self.builtins = Some(registry);
        self
    }

    pub fn build(self) -> Shell {
        let fs = self.fs.unwrap_or_default();
        let capabilities = self.capabilities.unwrap_or_else(CapabilitySet::default_set);
        let builtins = self.builtins.unwrap_or_default();

        // Ensure cwd exists in the VFS
        let _ = fs.mkdir(&self.cwd, &capabilities);

        Shell {
            interpreter: Interpreter {
                env: self.env,
                vars: HashMap::new(),
                cwd: self.cwd,
                fs,
                capabilities,
                limits: self.limits.clamped(),
                counters: ExecutionCounters::default(),
                functions: HashMap::new(),
                last_exit_code: 0,
                positional_params: Vec::new(),
                builtins,
                exec_handler: self
                    .exec_handler
                    .unwrap_or_else(|| Box::new(DefaultExecHandler)),
                stdout_buf: String::new(),
                stderr_buf: String::new(),
                shell_opts: ShellOpts::default(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_hello() {
        let mut shell = Shell::builder()
            .env("HOME", "/home/user")
            .cwd("/home/user")
            .build();

        let output = shell.exec("echo hello").await.unwrap();
        assert_eq!(output.stdout, "hello\n");
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn variable_expansion() {
        let mut shell = Shell::builder().env("NAME", "world").cwd("/").build();

        let output = shell.exec("echo hello $NAME").await.unwrap();
        assert_eq!(output.stdout, "hello world\n");
    }

    #[tokio::test]
    async fn assignment_and_use() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("FOO=bar").await.unwrap();
        let output = shell.exec("echo $FOO").await.unwrap();
        assert_eq!(output.stdout, "bar\n");
    }

    #[tokio::test]
    async fn for_loop() {
        let mut shell = Shell::builder().cwd("/").build();

        let output = shell
            .exec("for i in 1 2 3; do echo $i; done")
            .await
            .unwrap();
        assert_eq!(output.stdout, "1\n2\n3\n");
    }

    #[tokio::test]
    async fn if_then_else() {
        let mut shell = Shell::builder().cwd("/").build();

        let output = shell
            .exec("if true; then echo yes; else echo no; fi")
            .await
            .unwrap();
        assert_eq!(output.stdout, "yes\n");

        let output = shell
            .exec("if false; then echo yes; else echo no; fi")
            .await
            .unwrap();
        assert_eq!(output.stdout, "no\n");
    }

    #[tokio::test]
    async fn sequence() {
        let mut shell = Shell::builder().cwd("/").build();

        let output = shell.exec("echo a; echo b; echo c").await.unwrap();
        assert_eq!(output.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn and_or() {
        let mut shell = Shell::builder().cwd("/").build();

        let output = shell.exec("true && echo yes").await.unwrap();
        assert_eq!(output.stdout, "yes\n");

        let output = shell.exec("false && echo yes || echo no").await.unwrap();
        assert_eq!(output.stdout, "no\n");
    }

    #[tokio::test]
    async fn file_operations() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("mkdir -p /tmp/test").await.unwrap();
        shell.exec("touch /tmp/test/file.txt").await.unwrap();

        let output = shell.exec("ls /tmp/test").await.unwrap();
        assert!(output.stdout.contains("file.txt"));
    }

    #[tokio::test]
    async fn redirect_to_file() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("echo hello > /output.txt").await.unwrap();
        let output = shell.exec("cat /output.txt").await.unwrap();
        assert_eq!(output.stdout, "hello\n");
    }

    #[tokio::test]
    async fn command_not_found() {
        let mut shell = Shell::builder().cwd("/").build();
        let output = shell.exec("nonexistent").await.unwrap();
        assert_eq!(output.exit_code, 127);
    }

    #[tokio::test]
    async fn resource_limit_loop() {
        let mut shell = Shell::builder()
            .cwd("/")
            .limits(ExecutionLimits {
                max_loop_iterations: 5,
                ..Default::default()
            })
            .build();

        let result = shell.exec("while true; do echo x; done").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn capability_denied() {
        let mut shell = Shell::builder()
            .cwd("/")
            .capabilities(CapabilitySet::new([Cap::EnvRead, Cap::EnvWrite]))
            .build();

        let result = shell.exec("cat /etc/passwd").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            error::ShellError::CapabilityDenied(Cap::ReadFs)
        ));
    }

    #[tokio::test]
    async fn function_definition() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("function greet { echo hello; }").await.unwrap();
        let output = shell.exec("greet").await.unwrap();
        assert_eq!(output.stdout, "hello\n");
    }

    #[tokio::test]
    async fn while_loop_with_counter() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("count=0").await.unwrap();
        let output = shell
            .exec("for i in a b c; do echo $i; done")
            .await
            .unwrap();
        assert_eq!(output.stdout, "a\nb\nc\n");
    }

    #[tokio::test]
    async fn test_builtin() {
        let mut shell = Shell::builder().cwd("/").build();

        let output = shell
            .exec("if test 5 -gt 3; then echo yes; fi")
            .await
            .unwrap();
        assert_eq!(output.stdout, "yes\n");
    }

    #[tokio::test]
    async fn head_and_tail() {
        let mut shell = Shell::builder().cwd("/").build();

        shell
            .exec("printf '1\\n2\\n3\\n4\\n5\\n' > /nums.txt")
            .await
            .unwrap();
        let output = shell.exec("head -n 2 /nums.txt").await.unwrap();
        assert_eq!(output.stdout, "1\n2\n");

        let output = shell.exec("tail -n 2 /nums.txt").await.unwrap();
        assert_eq!(output.stdout, "4\n5\n");
    }

    #[tokio::test]
    async fn export_and_env() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("export FOO=bar").await.unwrap();
        let output = shell.exec("echo $FOO").await.unwrap();
        assert_eq!(output.stdout, "bar\n");
        assert_eq!(shell.env().get("FOO").unwrap(), "bar");
    }

    #[tokio::test]
    async fn custom_builtin() {
        use crate::builtins::{Builtin, Context};
        use crate::interpreter::hooks::ExecResult;
        use async_trait::async_trait;

        struct Hello;
        #[async_trait]
        impl Builtin for Hello {
            fn name(&self) -> &str {
                "hello"
            }
            async fn execute(&self, _ctx: Context<'_>) -> ShellResult<ExecResult> {
                Ok(ExecResult::success("hello from custom\n"))
            }
        }

        let mut shell = Shell::builder().cwd("/").build();
        shell.register_builtin(Hello);
        let output = shell.exec("hello").await.unwrap();
        assert_eq!(output.stdout, "hello from custom\n");
    }
}
