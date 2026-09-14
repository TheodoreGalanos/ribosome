use crate::{
    Error, Limits, Result,
    profile::ImportProfile,
    reader::{ByteRange, SourceFile, SourceLock},
};
use bytes::Bytes;
use parquet::{
    errors::ParquetError,
    file::{
        reader::{ChunkReader, FileReader, Length},
        serialized_reader::SerializedFileReader,
    },
};
use reqwest::{
    blocking::Client,
    header::{CONTENT_RANGE, RANGE},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const CHUNK: u64 = 1024 * 1024;

pub fn read_tables(
    profile: &ImportProfile,
    output: &Path,
    limits: &Limits,
    lock: &mut SourceLock,
) -> Result<BTreeMap<String, Vec<Value>>> {
    let dataset = profile
        .input
        .dataset
        .as_ref()
        .ok_or_else(|| Error::invalid("HF dataset is required"))?;
    if dataset.split('/').count() != 2
        || !dataset
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_/ .".contains(c))
        || dataset.contains("..")
        || dataset.contains(' ')
    {
        return Err(Error::invalid("invalid HF dataset name"));
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|_| Error::internal("could not create HTTP client"))?;
    let credential = profile
        .input
        .credential_env
        .as_ref()
        .and_then(|key| std::env::var(key).ok());
    let previous = output.join("source-lock.json");
    let requested_revision = if previous.exists() {
        let old: SourceLock = serde_json::from_slice(&std::fs::read(previous)?)?;
        if serde_json::to_value(&old.profile)? != serde_json::to_value(&lock.profile)? {
            return Err(Error::conflict(
                "profile changed; use a new acquisition directory",
            ));
        }
        old.revision
            .ok_or_else(|| Error::invalid("saved HF lock has no revision"))?
    } else {
        profile
            .input
            .revision
            .clone()
            .unwrap_or_else(|| "main".into())
    };
    if !requested_revision
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err(Error::invalid(
            "revision must be a commit or simple ref name",
        ));
    }
    let mut request = client.get(format!(
        "https://huggingface.co/api/datasets/{dataset}/revision/{requested_revision}?blobs=true"
    ));
    if let Some(token) = &credential {
        request = request.bearer_auth(token);
    }
    let metadata: Value = request
        .send()
        .map_err(|_| Error::internal("HF metadata request failed"))?
        .error_for_status()
        .map_err(|e| {
            Error::invalid(format!(
                "HF metadata returned {}",
                e.status().map(|s| s.as_u16()).unwrap_or(0)
            ))
        })?
        .json()
        .map_err(|_| Error::invalid("invalid HF metadata response"))?;
    let revision = metadata["sha"]
        .as_str()
        .filter(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| Error::invalid("HF did not resolve a concrete revision"))?
        .to_owned();
    lock.revision = Some(revision.clone());
    let budget = Arc::new(Mutex::new(0u64));
    let mut tables = BTreeMap::new();
    for table in &profile.input.tables {
        if table.encoding != "parquet" {
            return Err(Error::invalid(
                "HF pinned_files currently reads Parquet; use local snapshots for JSON/JSONL",
            ));
        }
        let siblings = metadata["siblings"]
            .as_array()
            .ok_or_else(|| Error::invalid("HF metadata has no file list"))?;
        let mut files = siblings
            .iter()
            .filter(|file| {
                let Some(path) = file["rfilename"].as_str() else {
                    return false;
                };
                table
                    .path
                    .as_ref()
                    .map(|expected| path == expected)
                    .unwrap_or_else(|| {
                        let prefix = format!(
                            "data/{}/{}-",
                            table.config.as_deref().unwrap_or("default"),
                            table.split.as_deref().unwrap_or("train")
                        );
                        path.starts_with(&prefix) && path.ends_with(".parquet")
                    })
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|file| file["rfilename"].as_str());
        if files.is_empty() {
            return Err(Error::invalid(format!(
                "no pinned files matched table {}",
                table.name
            )));
        }
        let mut rows = Vec::new();
        let target = if table.name == profile.assemble.base {
            limits.episodes
        } else {
            limits.table_rows
        };
        for file in files {
            let path = file["rfilename"].as_str().unwrap();
            let size = file["size"]
                .as_u64()
                .or_else(|| file["lfs"]["size"].as_u64())
                .ok_or_else(|| Error::invalid("pinned file has no size"))?;
            let reader = RangeReader {
                shared: Arc::new(RemoteFile {
                    client: client.clone(),
                    url: format!(
                        "https://huggingface.co/datasets/{dataset}/resolve/{revision}/{path}"
                    ),
                    credential: credential.clone(),
                    size,
                    output: output.to_owned(),
                    cache_prefix: format!("cache/{revision}/file-{}", lock.files.len()),
                    ranges: Mutex::new(BTreeMap::new()),
                    budget: budget.clone(),
                    max_bytes: limits.download_bytes,
                }),
            };
            rows.extend(parquet_rows(
                reader.clone(),
                target - rows.len(),
                table.name != profile.assemble.base,
                limits,
            )?);
            lock.files.push(SourceFile {
                path: path.into(),
                size,
                ranges: reader
                    .shared
                    .ranges
                    .lock()
                    .unwrap()
                    .values()
                    .cloned()
                    .collect(),
            });
            if rows.len() == target && table.name == profile.assemble.base {
                break;
            }
        }
        tables.insert(table.name.clone(), rows);
    }
    Ok(tables)
}

pub fn parquet_rows<R: ChunkReader + 'static>(
    source: R,
    count: usize,
    require_complete: bool,
    limits: &Limits,
) -> Result<Vec<Value>> {
    let reader = SerializedFileReader::new(source)
        .map_err(|e| Error::invalid(format!("Parquet metadata: {e}")))?;
    let total = reader.metadata().file_metadata().num_rows();
    if require_complete && total > count as i64 {
        return Err(Error::invalid(
            "join table exceeds row limit; increase table_rows explicitly",
        ));
    }
    let mut decoded_bytes = 0;
    reader
        .get_row_iter(None)
        .map_err(|e| Error::invalid(e.to_string()))?
        .take(count)
        .map(|row| {
            let value = row
                .map_err(|e| Error::invalid(format!("Parquet row: {e}")))?
                .to_json_value();
            let bytes = serde_json::to_vec(&value)?.len();
            decoded_bytes += bytes as u64;
            if bytes > limits.row_bytes {
                return Err(Error::invalid("Parquet row exceeds byte limit"));
            }
            if decoded_bytes > limits.download_bytes {
                return Err(Error::invalid("decoded Parquet file exceeds byte limit"));
            }
            Ok(value)
        })
        .collect()
}

struct RemoteFile {
    client: Client,
    url: String,
    credential: Option<String>,
    size: u64,
    output: PathBuf,
    cache_prefix: String,
    ranges: Mutex<BTreeMap<u64, ByteRange>>,
    budget: Arc<Mutex<u64>>,
    max_bytes: u64,
}
#[derive(Clone)]
struct RangeReader {
    shared: Arc<RemoteFile>,
}
struct RangeStream {
    reader: RangeReader,
    position: u64,
}

impl RangeReader {
    fn chunk(&self, offset: u64) -> std::io::Result<Vec<u8>> {
        let length = CHUNK.min(self.shared.size - offset) as usize;
        let name = format!("{}-{offset}.bin", self.shared.cache_prefix);
        let path = self.shared.output.join(&name);
        let mut ranges = self.shared.ranges.lock().unwrap();
        if let std::collections::btree_map::Entry::Vacant(entry) = ranges.entry(offset) {
            let mut budget = self.shared.budget.lock().unwrap();
            if *budget + length as u64 > self.shared.max_bytes {
                return Err(std::io::Error::other(
                    "acquisition exceeds download byte limit",
                ));
            }
            // Count cached reads too: the limit describes the source material
            // required to reproduce the sample, independent of a warm cache.
            *budget += length as u64;
            if !path.exists() {
                let end = offset + length as u64 - 1;
                let mut request = self
                    .shared
                    .client
                    .get(&self.shared.url)
                    .header(RANGE, format!("bytes={offset}-{end}"));
                if let Some(token) = &self.shared.credential {
                    request = request.bearer_auth(token);
                }
                let response = request
                    .send()
                    .map_err(|_| std::io::Error::other("pinned range request failed"))?;
                let expected_range = format!("bytes {offset}-{end}/{}", self.shared.size);
                if response.status().as_u16() != 206
                    || response
                        .headers()
                        .get(CONTENT_RANGE)
                        .and_then(|h| h.to_str().ok())
                        != Some(expected_range.as_str())
                {
                    return Err(std::io::Error::other(
                        "source did not honor the exact byte range",
                    ));
                }
                let mut bytes = Vec::with_capacity(length);
                response.take(length as u64 + 1).read_to_end(&mut bytes)?;
                if bytes.len() != length {
                    return Err(std::io::Error::other("source range has wrong length"));
                }
                std::fs::create_dir_all(path.parent().unwrap())?;
                let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
                temporary.write_all(&bytes)?;
                temporary.persist(&path).map_err(|e| e.error)?;
            }
            let bytes = std::fs::read(&path)?;
            if bytes.len() != length {
                return Err(std::io::Error::other(
                    "cached range has wrong length; reacquire it",
                ));
            }
            entry.insert(ByteRange {
                offset,
                length,
                cache_file: name,
            });
        }
        std::fs::read(path)
    }
}
impl Length for RangeReader {
    fn len(&self) -> u64 {
        self.shared.size
    }
}
impl ChunkReader for RangeReader {
    type T = RangeStream;
    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        if start > self.len() {
            return Err(ParquetError::General("range starts beyond file".into()));
        }
        Ok(RangeStream {
            reader: self.clone(),
            position: start,
        })
    }
    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<Bytes> {
        if start
            .checked_add(length as u64)
            .is_none_or(|end| end > self.len())
        {
            return Err(ParquetError::General("range extends beyond file".into()));
        }
        let mut stream = self.get_read(start)?;
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes)?;
        Ok(bytes.into())
    }
}
impl Read for RangeStream {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.position == self.reader.len() || output.is_empty() {
            return Ok(0);
        }
        let start = self.position / CHUNK * CHUNK;
        let chunk = self.reader.chunk(start)?;
        let offset = (self.position - start) as usize;
        let n = output.len().min(chunk.len() - offset);
        output[..n].copy_from_slice(&chunk[offset..offset + n]);
        self.position += n as u64;
        Ok(n)
    }
}
