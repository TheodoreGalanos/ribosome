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
    /// Additional checker code or configuration dependencies. The executable
    /// and absolute file arguments are fingerprinted automatically.
    #[serde(default)]
    pub code_files: Vec<PathBuf>,
    /// Host-authorized coverage of exact obligation versions on task artifacts.
    #[serde(default)]
    pub validated_properties: Vec<PropertyBinding>,
}

pub struct PreparedTool {
    pub writes: Vec<String>,
    pub checker_version: String,
    pub properties: Vec<PropertyBinding>,
}

pub struct CheckOutput {
    pub success: bool,
    pub output: String,
    pub before: Vec<ArtifactRef>,
    pub after: Vec<ArtifactRef>,
    pub validates: Vec<String>,
    pub writes: Vec<String>,
    pub checker_version: String,
    pub validation_outcome: ValidationEvidenceOutcome,
    pub properties: Vec<PropertyBinding>,
}

pub trait HostAdapter: Send + Sync {
    fn read(
        &self,
        grant: &Grant,
        request: &ArtifactRead,
        workspace: Option<&Path>,
    ) -> Result<ArtifactChunk>;
    fn version(&self, grant: &Grant, path: &str, workspace: Option<&Path>) -> Result<ArtifactRef>;
    /// Preflight performs no external effect. Execution repeats these checks.
    fn prepare_edit(
        &self,
        grant: &Grant,
        path: &str,
        expected: &str,
        workspace: Option<&Path>,
    ) -> Result<ArtifactRef>;
    /// Returns the authorized potential writes before dispatch can begin.
    fn prepare_tool(
        &self,
        grant: &Grant,
        tool: &str,
        workspace: Option<&Path>,
        read_only: bool,
    ) -> Result<PreparedTool>;
    /// Identity of the configured checker and its declared code dependencies.
    /// This does not execute it and remains available for recovery inspection.
    fn checker_version(&self, tool: &str) -> Result<String>;
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
    _lock: WorkspaceLock,
}

struct WorkspaceLock(File);

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        // Release this owner's lock explicitly. Closing one descriptor may
        // leave a duplicate inherited during another process launch open.
        let _ = self.0.unlock();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn dropping_the_host_releases_its_lock_even_with_a_duplicated_descriptor() {
        let directory = tempfile::tempdir().unwrap();
        let host = LocalHost::new(directory.path(), BTreeMap::new()).unwrap();
        // A concurrent process launch can temporarily inherit an open file
        // description before exec closes its descriptor. Model that duplicate.
        let duplicate = host._lock.0.try_clone().unwrap();
        assert!(LocalHost::new(directory.path(), BTreeMap::new()).is_err());
        drop(host);
        let replacement = LocalHost::new(directory.path(), BTreeMap::new())
            .expect("the released host must not leave ownership in another descriptor");
        drop(duplicate);
        assert!(LocalHost::new(directory.path(), BTreeMap::new()).is_err());
        drop(replacement);
        assert!(LocalHost::new(directory.path(), BTreeMap::new()).is_ok());
    }
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
        let lock = WorkspaceLock(lock);
        for tool in tools.values() {
            let mut properties = std::collections::BTreeSet::new();
            for binding in &tool.validated_properties {
                crate::validation::validate("PropertyBinding", &serde_json::to_value(binding)?)?;
                if !tool.validates.contains(&binding.path)
                    || !properties.insert((&binding.path, &binding.obligation.id))
                {
                    return Err(Error::invalid(
                        "property bindings require distinct obligation/target pairs included in validates",
                    ));
                }
            }
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
            if tool.code_files.len() > 32 || tool.code_files.iter().any(|path| !path.is_absolute())
            {
                return Err(Error::invalid(
                    "checker code_files must contain at most 32 absolute paths",
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
    fn authorized_tool(
        &self,
        grant: &Grant,
        name: &str,
        workspace: Option<&Path>,
        read_only: bool,
    ) -> Result<&RegisteredTool> {
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
        Ok(tool)
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
            snapshot_id: None,
            required_freshness: None,
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

    fn prepare_edit(
        &self,
        grant: &Grant,
        path: &str,
        expected: &str,
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
        self.path(grant, path, workspace)?;
        let before = self.version(grant, path, workspace)?;
        if before.version != expected {
            return Err(Error::conflict(
                "artifact changed; inspect current version before editing",
            ));
        }
        Ok(before)
    }

    fn edit(
        &self,
        grant: &Grant,
        path: &str,
        expected: &str,
        content: &str,
        workspace: Option<&Path>,
    ) -> Result<ArtifactRef> {
        self.prepare_edit(grant, path, expected, workspace)?;
        let resolved = self.path(grant, path, workspace)?;
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

    fn prepare_tool(
        &self,
        grant: &Grant,
        name: &str,
        workspace: Option<&Path>,
        read_only: bool,
    ) -> Result<PreparedTool> {
        let tool = self.authorized_tool(grant, name, workspace, read_only)?;
        for path in tool.reads.iter().chain(&tool.writes) {
            self.version(grant, path, workspace)?;
        }
        if counter(&grant.budget.deadline_ms)? <= now_ms() {
            return Err(Error::exhausted("deadline reached"));
        }
        Ok(PreparedTool {
            writes: tool.writes.clone(),
            checker_version: self.checker_version(name)?,
            properties: tool.validated_properties.clone(),
        })
    }

    fn checker_version(&self, name: &str) -> Result<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::missing("checker is no longer registered"))?;
        let mut digest = Sha256::new();
        digest.update(serde_json::to_vec(tool)?);
        let paths = std::iter::once(tool.program.clone())
            .chain(
                tool.args
                    .iter()
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute() && path.is_file()),
            )
            .chain(tool.code_files.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        for path in paths {
            let mut file = File::open(&path)?;
            const MAX_CODE: u64 = 256 * 1024 * 1024;
            if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_CODE {
                return Err(Error::invalid(
                    "checker code must be a regular file of at most 256 MiB",
                ));
            }
            digest.update(serde_json::to_vec(&path)?);
            let mut contents = Sha256::new();
            let mut buffer = [0u8; 65536];
            let mut bytes = 0u64;
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                if bytes > MAX_CODE {
                    return Err(Error::invalid("checker code grew beyond its size limit"));
                }
                contents.update(&buffer[..count]);
            }
            digest.update(contents.finalize());
        }
        Ok(format!("sha256:{:x}", digest.finalize()))
    }

    fn run_tool(
        &self,
        grant: &Grant,
        name: &str,
        workspace: Option<&Path>,
        read_only: bool,
        cancellation: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<CheckOutput> {
        let tool = self.authorized_tool(grant, name, workspace, read_only)?;
        let checker_version = self.checker_version(name)?;
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
        let same_checker = self
            .checker_version(name)
            .is_ok_and(|version| version == checker_version);
        if !same_checker {
            output.push_str("\nChecker code changed during execution; validation is stale.");
        }
        Ok(CheckOutput {
            success: result.success && unchanged && same_checker,
            output,
            before: versions,
            after: current,
            validates: tool.validates.clone(),
            writes: tool.writes.clone(),
            checker_version,
            properties: tool.validated_properties.clone(),
            validation_outcome: if !unchanged || !same_checker {
                ValidationEvidenceOutcome::Stale
            } else if result.success {
                ValidationEvidenceOutcome::Passed
            } else {
                ValidationEvidenceOutcome::Failed
            },
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
