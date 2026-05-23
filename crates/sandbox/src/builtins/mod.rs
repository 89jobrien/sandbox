pub mod core;
pub mod file;
pub mod flow;
pub mod nav;
pub mod text;
pub mod vars;

use std::collections::HashMap;

use async_trait::async_trait;

use crate::capabilities::Cap;
use crate::fs::SandboxFs;
use crate::interpreter::ShellOpts;
use crate::interpreter::hooks::ExecResult;

pub struct Context<'a> {
    pub args: Vec<String>,
    pub env: &'a mut HashMap<String, String>,
    pub vars: &'a mut HashMap<String, String>,
    pub cwd: &'a mut String,
    pub fs: &'a SandboxFs,
    pub stdin: Option<&'a [u8]>,
    pub capabilities: &'a crate::capabilities::CapabilitySet,
    pub last_exit_code: i32,
    pub shell_opts: &'a mut ShellOpts,
}

impl Context<'_> {
    pub fn arg(&self, n: usize) -> Option<&str> {
        self.args.get(n).map(|s| s.as_str())
    }

    pub fn args_from(&self, n: usize) -> &[String] {
        if n < self.args.len() {
            &self.args[n..]
        } else {
            &[]
        }
    }

    pub fn resolve_path(&self, path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{}", self.cwd, path)
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuiltinMeta {
    pub name: &'static str,
    pub summary: &'static str,
    pub usage: &'static str,
}

#[async_trait]
pub trait Builtin: Send + Sync {
    fn name(&self) -> &str;

    fn required_capabilities(&self) -> &[Cap] {
        &[]
    }

    fn metadata(&self) -> Option<BuiltinMeta> {
        None
    }

    async fn execute(&self, ctx: Context<'_>) -> crate::error::ShellResult<ExecResult>;
}

pub struct BuiltinRegistry {
    builtins: HashMap<String, Box<dyn Builtin>>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        Self {
            builtins: HashMap::new(),
        }
    }

    pub fn register(&mut self, builtin: impl Builtin + 'static) {
        let name = builtin.name().to_string();
        self.builtins.insert(name, Box::new(builtin));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Builtin> {
        self.builtins.get(name).map(|b| b.as_ref())
    }

    pub fn with_core_builtins() -> Self {
        let mut reg = Self::new();
        // Core
        reg.register(core::Echo);
        reg.register(core::Printf);
        reg.register(core::Cat);
        reg.register(core::Read_);
        reg.register(core::Head);
        reg.register(core::Tail);
        reg.register(core::Test_);
        reg.register(core::BracketTest);
        // Nav
        reg.register(nav::Cd);
        reg.register(nav::Pwd);
        reg.register(nav::Ls);
        // File
        reg.register(file::Mkdir);
        reg.register(file::Rm);
        reg.register(file::Cp);
        reg.register(file::Mv);
        reg.register(file::Touch);
        reg.register(file::Find);
        // Flow
        reg.register(flow::True_);
        reg.register(flow::False_);
        reg.register(flow::Exit);
        // Vars
        reg.register(vars::Export);
        reg.register(vars::Set_);
        reg.register(vars::Unset);
        // Text
        reg.register(text::Wc);
        reg.register(text::Basename);
        reg.register(text::Dirname);
        reg.register(text::Sort);
        reg.register(text::Uniq);
        reg.register(text::Tee);
        reg.register(text::GrepBuiltin);
        reg
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::with_core_builtins()
    }
}
