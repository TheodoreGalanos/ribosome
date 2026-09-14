use crate::{
    Error, Limits, Result,
    decode::{Decoded, decode},
    profile::{ImportProfile, Table, identifier, project},
    write_json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub size: u64,
    /// Whole local files or concrete downloaded byte ranges of a pinned file.
    pub ranges: Vec<ByteRange>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByteRange {
    pub offset: u64,
    pub length: usize,
    pub cache_file: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLock {
    pub version: String,
    pub profile: ImportProfile,
    pub source: String,
    pub revision: Option<String>,
    pub snapshot_kind: String,
    pub files: Vec<SourceFile>,
    pub selected: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub source_id: String,
    pub task: String,
    pub family: String,
    pub acquired_ms: String,
    pub decoded: Decoded,
    pub metadata: Value,
    pub annotations: Value,
    pub raw: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quarantine {
    pub locator: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    pub profile: String,
    pub episode_ids: Vec<String>,
    pub quarantined: Vec<Quarantine>,
    pub duplicate_rows: usize,
    pub examined: usize,
    pub limits: Limits,
}

pub fn prepare(profile_path: &Path, output: &Path, limits: &Limits) -> Result<ImportReport> {
    if limits.episodes == 0 || limits.episodes > 1000 || limits.table_rows < limits.episodes {
        return Err(Error::invalid(
            "episode limit must be 1..1000 and no greater than table_rows",
        ));
    }
    std::fs::create_dir_all(output)?;
    let profile = ImportProfile::read(profile_path)?;
    let mut source_lock = SourceLock {
        version: "1".into(),
        profile: profile.clone(),
        source: profile
            .input
            .dataset
            .clone()
            .unwrap_or_else(|| profile.id.clone()),
        revision: None,
        snapshot_kind: profile.input.mode.clone(),
        files: vec![],
        selected: vec![],
    };
    let mut tables = BTreeMap::new();
    if profile.input.kind == "local" {
        let mut source_bytes = 0;
        for table in &profile.input.tables {
            let path = profile_path
                .parent()
                .unwrap_or(Path::new("."))
                .join(table.path.as_ref().unwrap());
            source_bytes += std::fs::metadata(&path)?.len();
            if source_bytes > limits.download_bytes {
                return Err(Error::invalid("local sources exceed aggregate byte limit"));
            }
            let bytes = std::fs::read(&path)?;
            let cache_name = format!("cache/table-{}-local", source_lock.files.len());
            std::fs::create_dir_all(output.join("cache"))?;
            std::fs::write(output.join(&cache_name), &bytes)?;
            source_lock.files.push(SourceFile {
                path: path.to_string_lossy().into(),
                size: bytes.len() as u64,
                ranges: vec![ByteRange {
                    offset: 0,
                    length: bytes.len(),
                    cache_file: cache_name,
                }],
            });
            tables.insert(table.name.clone(), read_local(&path, table, limits)?);
        }
    } else {
        #[cfg(feature = "remote")]
        {
            tables = crate::remote::read_tables(&profile, output, limits, &mut source_lock)?;
        }
        #[cfg(not(feature = "remote"))]
        return Err(Error::invalid(
            "HF acquisition requires the ribosome-import remote feature",
        ));
    }
    let base = tables
        .remove(&profile.assemble.base)
        .ok_or_else(|| Error::invalid("missing base table"))?;
    let mut indexes = Vec::new();
    for join in &profile.assemble.joins {
        let mut index = BTreeMap::new();
        for row in tables
            .get(&join.table)
            .ok_or_else(|| Error::invalid("missing join table"))?
        {
            let key = identifier(row, &join.right)?;
            if index.insert(key.clone(), row).is_some() {
                return Err(Error::invalid(format!(
                    "duplicate dimension key {key} in {}",
                    join.table
                )));
            }
        }
        indexes.push(index);
    }
    let mut report = ImportReport {
        profile: profile.id.clone(),
        episode_ids: vec![],
        quarantined: vec![],
        duplicate_rows: 0,
        examined: 0,
        limits: limits.clone(),
    };
    let mut identities = BTreeMap::<String, Value>::new();
    let mut base_identities = BTreeMap::<String, Value>::new();
    let mut one_to_one = vec![BTreeSet::new(); profile.assemble.joins.len()];
    for (position, row) in base.into_iter().take(limits.episodes).enumerate() {
        report.examined += 1;
        let locator = format!("{}:row:{position}", profile.assemble.base);
        let base_view = Value::Object(serde_json::Map::from_iter([(
            profile.assemble.namespace.clone(),
            row.clone(),
        )]));
        if let Ok(identity) = identifier(&base_view, &profile.episode.id)
            && let Some(previous) = base_identities.insert(identity, row.clone())
        {
            if previous != row {
                return Err(Error::conflict(
                    "same base episode identity has different content",
                ));
            }
            report.duplicate_rows += 1;
            continue;
        }

        let assembled = (|| -> Result<Value> {
            let mut assembled =
                serde_json::Map::from_iter([(profile.assemble.namespace.clone(), row.clone())]);
            for (i, join) in profile.assemble.joins.iter().enumerate() {
                let key = identifier(&row, &join.left)?;
                if join.cardinality == "one_to_one" && !one_to_one[i].insert(key.clone()) {
                    return Err(Error::invalid(format!(
                        "duplicate base key {key} for one_to_one join"
                    )));
                }
                let right = indexes[i].get(&key).ok_or_else(|| {
                    Error::invalid(format!("missing {} join key {key}", join.table))
                })?;
                // AEC repeats these identities in its two trial tables. A join
                // must not conceal a disagreeing task, model or attempt.
                for field in ["trial_id", "task_id", "model", "repetition"] {
                    if let (Some(left), Some(right)) = (row.get(field), right.get(field))
                        && !left.is_null()
                        && !right.is_null()
                        && left != right
                    {
                        return Err(Error::invalid(format!("joined rows disagree on {field}")));
                    }
                }
                assembled.insert(join.namespace.clone(), (*right).clone());
            }
            Ok(Value::Object(assembled))
        })();
        let episode = assembled.and_then(|raw| normalize(&profile, raw, limits));
        let mut episode = match episode {
            Ok(episode) => episode,
            Err(error) => {
                report.quarantined.push(Quarantine {
                    locator,
                    reason: error.message,
                });
                continue;
            }
        };
        if let Some(previous) = identities.insert(episode.source_id.clone(), episode.raw.clone()) {
            if previous != episode.raw {
                return Err(Error::conflict(
                    "same episode identity has different content",
                ));
            }
            report.duplicate_rows += 1;
            continue;
        }
        episode.id = format!("episode-{position:04}");
        let path = output.join("episodes").join(format!("{}.json", episode.id));
        if path.exists() {
            let previous: Episode = serde_json::from_slice(&std::fs::read(&path)?)?;
            if previous.raw != episode.raw || previous.decoded != episode.decoded {
                return Err(Error::conflict(
                    "saved episode identity has different content",
                ));
            }
            episode = previous;
        } else {
            write_json(&path, &episode)?;
        }
        source_lock.selected.push(episode.source_id.clone());
        report.episode_ids.push(episode.id);
    }
    write_json(&output.join("source-lock.json"), &source_lock)?;
    write_json(&output.join("report.json"), &report)?;
    Ok(report)
}

pub fn normalize(profile: &ImportProfile, raw: Value, limits: &Limits) -> Result<Episode> {
    let bytes = serde_json::to_vec(&raw)?;
    if bytes.len() > limits.row_bytes {
        return Err(Error::invalid("assembled episode exceeds row byte limit"));
    }
    let source_id = identifier(&raw, &profile.episode.id)?;
    let task = identifier(&raw, &profile.episode.task)?;
    let family = profile
        .episode
        .family
        .as_ref()
        .map(|p| identifier(&raw, p))
        .transpose()?
        .unwrap_or_else(|| task.clone());
    let decoded = decode(&raw, &profile.episode.decoder, limits)?;
    let id = String::new();
    let acquired_ms = ribosome_core::validation::now_ms().to_string();
    Ok(Episode {
        id,
        source_id,
        task,
        family,
        acquired_ms,
        decoded,
        metadata: project(&raw, &profile.metadata),
        annotations: project(&raw, &profile.annotations),
        raw,
    })
}

fn read_local(path: &Path, table: &Table, limits: &Limits) -> Result<Vec<Value>> {
    let file = std::fs::File::open(path)?;
    match table.encoding.as_str() {
        "jsonl" => read_lines(BufReader::new(file), limits),
        "jsonl-gzip" => read_lines(BufReader::new(flate2::read::GzDecoder::new(file)), limits),
        "json" => {
            let value: Value = serde_json::from_reader(file)?;
            let rows = table
                .rows_pointer
                .as_ref()
                .map(|p| {
                    value
                        .pointer(p)
                        .ok_or_else(|| Error::invalid("rows_pointer is missing"))
                })
                .transpose()?
                .unwrap_or(&value);
            let rows = rows
                .as_array()
                .cloned()
                .unwrap_or_else(|| vec![rows.clone()]);
            if rows.len() > limits.table_rows {
                return Err(Error::invalid("JSON table exceeds row limit"));
            }
            Ok(rows)
        }
        "parquet" => {
            #[cfg(feature = "remote")]
            {
                crate::remote::parquet_rows(file, limits.table_rows, true, limits)
            }
            #[cfg(not(feature = "remote"))]
            {
                Err(Error::invalid(
                    "Parquet requires the ribosome-import remote feature",
                ))
            }
        }
        _ => Err(Error::invalid("unsupported table encoding")),
    }
}

fn read_lines(mut reader: impl BufRead, limits: &Limits) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    let mut total = 0;
    loop {
        let mut line = String::new();
        let n = reader
            .by_ref()
            .take(limits.row_bytes as u64 + 1)
            .read_line(&mut line)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if n > limits.row_bytes || total > limits.download_bytes {
            return Err(Error::invalid("JSONL exceeds decoded byte limit"));
        }
        if line.trim().is_empty() {
            continue;
        }
        if rows.len() == limits.table_rows {
            return Err(Error::invalid("JSONL exceeds table row limit"));
        }
        rows.push(serde_json::from_str(&line)?);
    }
    Ok(rows)
}
