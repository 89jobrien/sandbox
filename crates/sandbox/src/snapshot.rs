use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Shell;
use crate::fs::SandboxFs;
use crate::parser::ast::Command;

/// A serializable snapshot of all shell state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSnapshot {
    pub env: HashMap<String, String>,
    pub vars: HashMap<String, String>,
    pub cwd: String,
    pub functions: HashMap<String, Command>,
    pub files: Vec<(String, Vec<u8>)>,
}

impl ShellSnapshot {
    /// Capture the current state of a shell into a snapshot.
    pub fn capture(shell: &Shell) -> Self {
        let files = shell.fs().walk_files();
        Self {
            env: shell.env().clone(),
            vars: shell.vars().clone(),
            cwd: shell.cwd().to_string(),
            functions: shell.functions().clone(),
            files,
        }
    }

    /// Restore a shell from a snapshot.
    pub fn restore(self) -> Shell {
        let fs = SandboxFs::new();
        let caps = crate::capabilities::CapabilitySet::default_set();
        for (path, contents) in &self.files {
            let _ = fs.write_file(path, contents, &caps);
        }

        Shell::builder()
            .envs(self.env)
            .cwd(self.cwd)
            .fs(fs)
            .build_with_state(self.vars, self.functions)
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to bincode bytes.
    pub fn to_bincode(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
    }

    /// Deserialize from bincode bytes.
    pub fn from_bincode(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        bincode::serde::decode_from_slice(bytes, bincode::config::standard()).map(|(val, _)| val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shell;

    #[tokio::test]
    async fn snapshot_roundtrip_state() {
        let mut shell = Shell::builder()
            .env("HOME", "/home/user")
            .cwd("/home/user")
            .build();

        shell.exec("FOO=bar").await.unwrap();
        shell.exec("export PATH=/usr/bin").await.unwrap();
        shell.exec("function greet { echo hello; }").await.unwrap();

        let snap = ShellSnapshot::capture(&shell);
        let mut restored = snap.restore();

        assert_eq!(restored.vars().get("FOO").unwrap(), "bar");
        assert_eq!(restored.env().get("PATH").unwrap(), "/usr/bin");
        assert_eq!(restored.cwd(), "/home/user");

        let output = restored.exec("greet").await.unwrap();
        assert_eq!(output.stdout, "hello\n");
    }

    #[tokio::test]
    async fn snapshot_roundtrip_vfs() {
        let mut shell = Shell::builder().cwd("/").build();

        shell.exec("mkdir -p /data/sub").await.unwrap();
        shell.exec("echo hello > /data/file.txt").await.unwrap();
        shell
            .exec("echo nested > /data/sub/inner.txt")
            .await
            .unwrap();

        let snap = ShellSnapshot::capture(&shell);
        let restored = snap.restore();

        let caps = crate::capabilities::CapabilitySet::default_set();
        let content = restored
            .fs()
            .read_to_string("/data/file.txt", &caps)
            .unwrap();
        assert_eq!(content, "hello\n");
        let content = restored
            .fs()
            .read_to_string("/data/sub/inner.txt", &caps)
            .unwrap();
        assert_eq!(content, "nested\n");
    }

    #[tokio::test]
    async fn snapshot_json_serialization() {
        let mut shell = Shell::builder().env("KEY", "value").cwd("/tmp").build();

        shell.exec("X=42").await.unwrap();
        shell.exec("echo data > /tmp/out.txt").await.unwrap();

        let snap = ShellSnapshot::capture(&shell);
        let json = snap.to_json().unwrap();
        let snap2 = ShellSnapshot::from_json(&json).unwrap();
        let mut restored = snap2.restore();

        assert_eq!(restored.vars().get("X").unwrap(), "42");
        assert_eq!(restored.env().get("KEY").unwrap(), "value");

        let output = restored.exec("cat /tmp/out.txt").await.unwrap();
        assert_eq!(output.stdout, "data\n");
    }

    #[tokio::test]
    async fn snapshot_bincode_serialization() {
        let mut shell = Shell::builder().env("A", "1").cwd("/").build();

        shell.exec("echo bin > /file.bin").await.unwrap();

        let snap = ShellSnapshot::capture(&shell);
        let bytes = snap.to_bincode().unwrap();
        let snap2 = ShellSnapshot::from_bincode(&bytes).unwrap();
        let restored = snap2.restore();

        let caps = crate::capabilities::CapabilitySet::default_set();
        let content = restored.fs().read_to_string("/file.bin", &caps).unwrap();
        assert_eq!(content, "bin\n");
        assert_eq!(restored.env().get("A").unwrap(), "1");
    }
}
