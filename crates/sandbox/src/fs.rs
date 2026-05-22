use std::path::{Component, Path};

use vfs::{MemoryFS, VfsError, VfsPath};

use crate::capabilities::{Cap, CapabilitySet};
use crate::error::{ShellError, ShellResult};

pub struct SandboxFs {
    root: VfsPath,
}

impl SandboxFs {
    pub fn new() -> Self {
        let root: VfsPath = MemoryFS::new().into();
        Self { root }
    }

    pub fn from_vfs(root: VfsPath) -> Self {
        Self { root }
    }

    fn resolve(&self, path: &str) -> ShellResult<VfsPath> {
        let normalized = normalize_path(path);
        self.root
            .join(&normalized)
            .map_err(|e| ShellError::Io(e.to_string()))
    }

    pub fn read_file(&self, path: &str, caps: &CapabilitySet) -> ShellResult<Vec<u8>> {
        caps.check(Cap::ReadFs)?;
        let vpath = self.resolve(path)?;
        let mut content = Vec::new();
        use std::io::Read;
        vpath
            .open_file()
            .map_err(|e| ShellError::Io(format!("{path}: {e}")))?
            .read_to_end(&mut content)
            .map_err(|e| ShellError::Io(e.to_string()))?;
        Ok(content)
    }

    pub fn read_to_string(&self, path: &str, caps: &CapabilitySet) -> ShellResult<String> {
        let bytes = self.read_file(path, caps)?;
        String::from_utf8(bytes).map_err(|e| ShellError::Io(e.to_string()))
    }

    pub fn write_file(&self, path: &str, content: &[u8], caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        let vpath = self.resolve(path)?;
        // Ensure parent directories exist
        let parent = vpath.parent();
        parent
            .create_dir_all()
            .map_err(|e| ShellError::Io(e.to_string()))?;
        use std::io::Write;
        vpath
            .create_file()
            .map_err(|e| ShellError::Io(format!("{path}: {e}")))?
            .write_all(content)
            .map_err(|e| ShellError::Io(e.to_string()))?;
        Ok(())
    }

    pub fn append_file(&self, path: &str, content: &[u8], caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        let existing = self.read_file(path, caps).unwrap_or_default();
        let mut combined = existing;
        combined.extend_from_slice(content);
        self.write_file(path, &combined, caps)
    }

    pub fn exists(&self, path: &str) -> ShellResult<bool> {
        let vpath = self.resolve(path)?;
        Ok(vpath.exists().unwrap_or(false))
    }

    pub fn is_dir(&self, path: &str) -> ShellResult<bool> {
        let vpath = self.resolve(path)?;
        Ok(vpath
            .metadata()
            .map(|m| m.file_type == vfs::VfsFileType::Directory)
            .unwrap_or(false))
    }

    pub fn is_file(&self, path: &str) -> ShellResult<bool> {
        let vpath = self.resolve(path)?;
        Ok(vpath
            .metadata()
            .map(|m| m.file_type == vfs::VfsFileType::File)
            .unwrap_or(false))
    }

    pub fn mkdir(&self, path: &str, caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        let vpath = self.resolve(path)?;
        vpath
            .create_dir_all()
            .map_err(|e| ShellError::Io(format!("{path}: {e}")))?;
        Ok(())
    }

    pub fn remove_file(&self, path: &str, caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        let vpath = self.resolve(path)?;
        vpath
            .remove_file()
            .map_err(|e| ShellError::Io(format!("{path}: {e}")))?;
        Ok(())
    }

    pub fn remove_dir(&self, path: &str, caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        let vpath = self.resolve(path)?;
        vpath
            .remove_dir()
            .map_err(|e| ShellError::Io(format!("{path}: {e}")))?;
        Ok(())
    }

    pub fn remove_dir_all(&self, path: &str, caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        // VFS doesn't have recursive remove, so we do it manually
        let vpath = self.resolve(path)?;
        self.remove_recursive(&vpath, caps)?;
        Ok(())
    }

    #[allow(clippy::only_used_in_recursion)]
    fn remove_recursive(&self, path: &VfsPath, caps: &CapabilitySet) -> ShellResult<()> {
        let meta = path.metadata().map_err(|e| ShellError::Io(e.to_string()))?;
        if meta.file_type == vfs::VfsFileType::Directory {
            let children: Vec<_> = path
                .read_dir()
                .map_err(|e| ShellError::Io(e.to_string()))?
                .collect();
            for child in children {
                self.remove_recursive(&child, caps)?;
            }
            path.remove_dir()
                .map_err(|e| ShellError::Io(e.to_string()))?;
        } else {
            path.remove_file()
                .map_err(|e| ShellError::Io(e.to_string()))?;
        }
        Ok(())
    }

    pub fn list_dir(&self, path: &str, caps: &CapabilitySet) -> ShellResult<Vec<DirEntry>> {
        caps.check(Cap::ReadFs)?;
        let vpath = self.resolve(path)?;
        let entries: Result<Vec<_>, VfsError> = vpath.read_dir().map(|iter| {
            iter.map(|p| {
                let name = p.filename();
                let is_dir = p
                    .metadata()
                    .map(|m| m.file_type == vfs::VfsFileType::Directory)
                    .unwrap_or(false);
                let size = p.metadata().map(|m| m.len).unwrap_or(0);
                DirEntry { name, is_dir, size }
            })
            .collect()
        });
        entries.map_err(|e| ShellError::Io(format!("{path}: {e}")))
    }

    pub fn copy_file(&self, src: &str, dst: &str, caps: &CapabilitySet) -> ShellResult<()> {
        let content = self.read_file(src, caps)?;
        self.write_file(dst, &content, caps)
    }

    pub fn rename(&self, src: &str, dst: &str, caps: &CapabilitySet) -> ShellResult<()> {
        caps.check(Cap::WriteFs)?;
        let content = self.read_file(src, caps)?;
        self.write_file(dst, &content, caps)?;
        self.remove_file(src, caps)?;
        Ok(())
    }

    pub fn file_size(&self, path: &str) -> ShellResult<u64> {
        let vpath = self.resolve(path)?;
        let meta = vpath
            .metadata()
            .map_err(|e| ShellError::Io(format!("{path}: {e}")))?;
        Ok(meta.len)
    }
}

impl Default for SandboxFs {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

fn normalize_path(path: &str) -> String {
    let p = Path::new(path);
    let mut components = Vec::new();
    for comp in p.components() {
        match comp {
            Component::RootDir => {
                components.clear();
            }
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            Component::Normal(s) => {
                components.push(s.to_string_lossy().to_string());
            }
            Component::Prefix(_) => {}
        }
    }
    components.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> CapabilitySet {
        CapabilitySet::default_set()
    }

    #[test]
    fn write_and_read() {
        let fs = SandboxFs::new();
        fs.write_file("/hello.txt", b"hello", &caps()).unwrap();
        let content = fs.read_to_string("/hello.txt", &caps()).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn mkdir_and_list() {
        let fs = SandboxFs::new();
        fs.mkdir("/dir", &caps()).unwrap();
        fs.write_file("/dir/file.txt", b"data", &caps()).unwrap();
        let entries = fs.list_dir("/dir", &caps()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn path_traversal_protection() {
        let fs = SandboxFs::new();
        fs.write_file("/secret.txt", b"secret", &caps()).unwrap();
        // ../secret.txt should resolve to /secret.txt (within sandbox)
        let content = fs.read_to_string("/dir/../secret.txt", &caps()).unwrap();
        assert_eq!(content, "secret");
    }

    #[test]
    fn capability_denied() {
        let fs = SandboxFs::new();
        let no_write = CapabilitySet::new([Cap::ReadFs]);
        let result = fs.write_file("/test.txt", b"data", &no_write);
        assert!(result.is_err());
    }

    #[test]
    fn remove_file() {
        let fs = SandboxFs::new();
        fs.write_file("/test.txt", b"data", &caps()).unwrap();
        fs.remove_file("/test.txt", &caps()).unwrap();
        assert!(!fs.exists("/test.txt").unwrap());
    }

    #[test]
    fn copy_file() {
        let fs = SandboxFs::new();
        fs.write_file("/src.txt", b"data", &caps()).unwrap();
        fs.copy_file("/src.txt", "/dst.txt", &caps()).unwrap();
        let content = fs.read_to_string("/dst.txt", &caps()).unwrap();
        assert_eq!(content, "data");
    }
}
