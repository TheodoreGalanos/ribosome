use crate::{
    contracts::*,
    error::{Error, Result},
    validation::{counter, now_ms},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub const MAX_ARTIFACT: u64 = 16 * 1024 * 1024;

/// A registered command is trusted host configuration. No model-supplied shell,
/// program, environment, or argument is accepted by the local adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredTool {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub reads: Vec<String>,
    #[serde(default)]
    pub validates: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
}

pub struct CheckOutput {
    pub success: bool,
    pub output: String,
    pub before: Vec<ArtifactRef>,
    pub after: Vec<ArtifactRef>,
    pub validates: Vec<String>,
    pub writes: Vec<String>,
}

pub trait HostAdapter: Send {
    fn read(
        &self,
        grant: &Grant,
        request: &ArtifactRead,
        workspace: Option<&Path>,
    ) -> Result<ArtifactChunk>;
    fn version(&self, grant: &Grant, path: &str, workspace: Option<&Path>) -> Result<ArtifactRef>;
    fn edit(
        &self,
        grant: &Grant,
        path: &str,
        expected: &str,
        content: &str,
        workspace: Option<&Path>,
    ) -> Result<ArtifactRef>;
    fn run_tool(
        &self,
        grant: &Grant,
        tool: &str,
        workspace: Option<&Path>,
        read_only: bool,
        cancellation: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<CheckOutput>;
    fn branch(&self, grant: &Grant, destination: &Path) -> Result<Vec<ArtifactRef>>;
}

pub struct LocalHost {
    root: PathBuf,
    tools: BTreeMap<String, RegisteredTool>,
    _lock: File,
}

impl LocalHost {
    pub fn new(root: impl AsRef<Path>, tools: BTreeMap<String, RegisteredTool>) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join(".ribosome-host.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::conflict("workspace already owned by a local Ribosome host"))?;
        for tool in tools.values() {
            if !tool.program.is_absolute() || tool.timeout_ms == 0 || tool.timeout_ms > 300_000 {
                return Err(Error::invalid(
                    "registered tools need absolute executables and a bounded timeout",
                ));
            }
            if tool.validates.iter().any(|p| !tool.reads.contains(p)) {
                return Err(Error::invalid(
                    "validated artifacts must be included in check reads",
                ));
            }
        }
        Ok(Self {
            root,
            tools,
            _lock: lock,
        })
    }

    fn path(&self, grant: &Grant, path: &str, workspace: Option<&Path>) -> Result<PathBuf> {
        let relative = Path::new(path);
        if !grant.paths.iter().any(|p| p == path)
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || path.starts_with(".ribosome")
        {
            return Err(Error::denied("artifact path not granted"));
        }
        let root = workspace.unwrap_or(&self.root).canonicalize()?;
        let mut resolved = root.clone();
        for part in relative.components() {
            resolved.push(part);
            if let Ok(metadata) = fs::symlink_metadata(&resolved)
                && metadata.file_type().is_symlink()
            {
                return Err(Error::denied("symlinks are not supported artifact paths"));
            }
        }
        let parent = resolved
            .parent()
            .ok_or_else(|| Error::denied("invalid artifact path"))?
            .canonicalize()?;
        if !parent.starts_with(&root) {
            return Err(Error::denied("artifact escapes workspace"));
        }
        Ok(resolved)
    }
}

impl HostAdapter for LocalHost {
    fn read(
        &self,
        grant: &Grant,
        request: &ArtifactRead,
        workspace: Option<&Path>,
    ) -> Result<ArtifactChunk> {
        let path = self.path(grant, &request.path, workspace)?;
        let data = read_bounded(&path)?;
        let start = (request.offset as usize).min(data.len());
        let end = (start + request.length as usize).min(data.len());
        // Text chunks use byte offsets. Trim incomplete UTF-8 at the end and
        // reject offsets inside a character so callers can advance exactly.
        let mut slice = &data[start..end];
        let text = loop {
            match std::str::from_utf8(slice) {
                Ok(text) => break text,
                Err(error) if error.error_len().is_none() => slice = &slice[..error.valid_up_to()],
                Err(_) => {
                    return Err(Error::invalid(
                        "artifact is not UTF-8 or offset splits a character",
                    ));
                }
            }
        };
        if text.is_empty() && start < data.len() {
            return Err(Error::invalid(
                "chunk length is smaller than next UTF-8 character",
            ));
        }
        Ok(ArtifactChunk {
            artifact: ArtifactRef {
                path: request.path.clone(),
                version: hash(&data),
            },
            content: text.into(),
            offset: start as u32,
            total_bytes: data.len().to_string(),
            eof: start + slice.len() == data.len(),
        })
    }

    fn version(&self, grant: &Grant, path: &str, workspace: Option<&Path>) -> Result<ArtifactRef> {
        let resolved = self.path(grant, path, workspace)?;
        let version = if resolved.exists() {
            hash(&read_bounded(&resolved)?)
        } else {
            "absent".into()
        };
        Ok(ArtifactRef {
            path: path.into(),
            version,
        })
    }

    fn edit(
        &self,
        grant: &Grant,
        path: &str,
        expected: &str,
        content: &str,
        workspace: Option<&Path>,
    ) -> Result<ArtifactRef> {
        if grant
            .writable_paths
            .as_ref()
            .is_some_and(|paths| !paths.iter().any(|p| p == path))
        {
            return Err(Error::denied("artifact is read-only in this grant"));
        }
        if grant.mode == Mode::Observe || (grant.mode == Mode::Sandbox && workspace.is_none()) {
            return Err(Error::denied("grant requires an isolated branch for edits"));
        }
        let resolved = self.path(grant, path, workspace)?;
        let before = self.version(grant, path, workspace)?;
        if before.version != expected {
            return Err(Error::conflict(
                "artifact changed; inspect current version before editing",
            ));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(resolved.parent().unwrap())?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        if resolved.exists() {
            temporary
                .as_file()
                .set_permissions(fs::metadata(&resolved)?.permissions())?;
        }
        if self.version(grant, path, workspace)?.version != expected {
            return Err(Error::conflict("artifact changed during edit preparation"));
        }
        temporary
            .persist(&resolved)
            .map_err(|e| Error::internal(e.to_string()))?;
        File::open(resolved.parent().unwrap())?.sync_all()?;
        Ok(ArtifactRef {
            path: path.into(),
            version: hash(content.as_bytes()),
        })
    }

    fn run_tool(
        &self,
        grant: &Grant,
        name: &str,
        workspace: Option<&Path>,
        read_only: bool,
        cancellation: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<CheckOutput> {
        if !grant.tools.contains(&name.to_owned()) || grant.mode == Mode::Observe {
            return Err(Error::denied("check tool not granted"));
        }
        if grant.mode == Mode::Sandbox && workspace.is_none() {
            return Err(Error::denied("check requires a branch in sandbox mode"));
        }
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::denied("host does not support this tool"))?;
        if read_only && !tool.writes.is_empty() {
            return Err(Error::denied(
                "effectful registered tool cannot be used as a check",
            ));
        }
        if !tool.writes.is_empty()
            && workspace.is_none()
            && grant
                .required_checks
                .as_ref()
                .is_some_and(|checks| !checks.is_empty())
        {
            return Err(Error::denied(
                "host-required checks require effectful procedures to run in a branch",
            ));
        }
        for path in &tool.writes {
            if grant
                .writable_paths
                .as_ref()
                .is_some_and(|paths| !paths.contains(path))
            {
                return Err(Error::denied("registered tool writes a read-only artifact"));
            }
            self.path(grant, path, workspace)?;
        }
        let paths = tool
            .reads
            .iter()
            .chain(tool.writes.iter())
            .collect::<std::collections::BTreeSet<_>>();
        let versions = paths
            .iter()
            .map(|p| self.version(grant, p, workspace))
            .collect::<Result<Vec<_>>>()?;
        let remaining = counter(&grant.budget.deadline_ms)?.saturating_sub(now_ms());
        if remaining == 0 {
            return Err(Error::exhausted("deadline reached"));
        }
        let result = crate::process::execute(
            &tool.program,
            &tool.args,
            workspace.unwrap_or(&self.root),
            None,
            &BTreeMap::new(),
            Duration::from_millis(tool.timeout_ms.min(remaining)),
            cancellation,
        )?;
        let mut output = result.stdout;
        output.push_str(&result.stderr);
        if result.cancelled {
            output.push_str("\nHost command cancelled.");
        }
        if result.timed_out {
            output.push_str("\nHost command deadline exceeded.");
        }
        let current = paths
            .iter()
            .map(|p| self.version(grant, p, workspace))
            .collect::<Result<Vec<_>>>()?;
        let unchanged = versions
            .iter()
            .zip(&current)
            .all(|(before, after)| tool.writes.contains(&before.path) || before == after);
        if !unchanged {
            output.push_str("\nInputs changed while the check ran; validation is stale.");
        }
        Ok(CheckOutput {
            success: result.success && unchanged,
            output,
            before: versions,
            after: current,
            validates: tool.validates.clone(),
            writes: tool.writes.clone(),
        })
    }

    fn branch(&self, grant: &Grant, destination: &Path) -> Result<Vec<ArtifactRef>> {
        if grant.mode == Mode::Observe {
            return Err(Error::denied("branch not granted"));
        }
        fs::create_dir(destination)?;
        let mut versions = Vec::new();
        for path in &grant.paths {
            let source = self.path(grant, path, None)?;
            let version = self.version(grant, path, None)?;
            let target = destination.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if source.exists() {
                fs::write(target, read_bounded(&source)?)?;
            }
            versions.push(version);
        }
        Ok(versions)
    }
}

pub fn hash(data: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(data))
}
fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_ARTIFACT {
        return Err(Error::invalid(
            "artifact must be a regular file of at most 16 MiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_ARTIFACT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ARTIFACT {
        return Err(Error::invalid("artifact grew past size limit"));
    }
    Ok(bytes)
}
