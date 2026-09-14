use crate::{
    Error, Result,
    reader::{Episode, SourceLock},
    write_json,
};
use ribosome_core::{contracts::*, store::Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Owner-authored choices about experimental use. Import mappings do not grant
/// evidence access. Ranges use message indexes, with an exclusive end.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyManifest {
    pub version: String,
    pub id: String,
    pub cohorts: BTreeMap<String, PathBuf>,
    pub episodes: Vec<StudyEpisode>,
    pub assignments: Vec<Assignment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyEpisode {
    pub cohort: String,
    pub episode: String,
    pub split: Split,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub id: String,
    pub visibility: DiscoveryCorpusVisibility,
    pub windows: Vec<MessageWindow>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageWindow {
    pub cohort: String,
    pub episode: String,
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedAssignment {
    pub corpus: DiscoveryCorpus,
    pub paths: Vec<String>,
    pub source_records: Vec<String>,
}

/// Withdraw an imported source through the existing source graph and remove
/// its generated workspace files. Owner acquisition files remain available.
pub fn withdraw(
    manifest_path: &Path,
    config: &Value,
    cohort: &str,
    episode_id: &str,
) -> Result<String> {
    let manifest: StudyManifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
    let selected = manifest
        .episodes
        .iter()
        .find(|e| e.cohort == cohort && e.episode == episode_id)
        .ok_or_else(|| Error::invalid("episode is outside the study"))?;
    let directory = manifest
        .cohorts
        .get(cohort)
        .ok_or_else(|| Error::invalid("unknown cohort"))?;
    safe_name(episode_id)?;
    let episode: Episode = serde_json::from_slice(&std::fs::read(
        manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(directory)
            .join("episodes")
            .join(format!("{episode_id}.json")),
    )?)?;
    let grant: Grant = serde_json::from_value(config["grant"].clone())?;
    let (source, _, snapshots) = project_episode(
        &manifest.id,
        cohort,
        &episode,
        &grant.scope,
        &selected.split,
    )?;
    let workspace = Path::new(
        config["workspace"]
            .as_str()
            .ok_or_else(|| Error::invalid("config needs workspace"))?,
    );
    let state = Path::new(
        config["state_dir"]
            .as_str()
            .ok_or_else(|| Error::invalid("config needs state_dir"))?,
    );
    let store = Store::open(state.join("ribosome.db"))?;
    store.retire(
        &grant,
        &RetireRequest {
            id: source.id.clone(),
            expected_version: "1".into(),
            delete: true,
        },
    )?;
    for snapshot in snapshots {
        match std::fs::remove_file(workspace.join(&snapshot.artifact.path)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(Error::internal(format!(
                    "source withdrawn, but could not remove projection {}: {error}",
                    snapshot.artifact.path
                )));
            }
        }
    }
    Ok(source.id)
}

pub fn assign(
    manifest_path: &Path,
    config: &Value,
    output: &Path,
) -> Result<Vec<PublishedAssignment>> {
    let manifest: StudyManifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
    if manifest.version != "1"
        || manifest.episodes.len() > 1000
        || manifest.assignments.len() > 1000
    {
        return Err(Error::invalid(
            "study requires version 1 and at most 1000 episodes and assignments",
        ));
    }
    safe_name(&manifest.id)?;
    let grant: Grant = serde_json::from_value(config["grant"].clone())?;
    let workspace = Path::new(
        config["workspace"]
            .as_str()
            .ok_or_else(|| Error::invalid("config needs workspace"))?,
    );
    let state = Path::new(
        config["state_dir"]
            .as_str()
            .ok_or_else(|| Error::invalid("config needs state_dir"))?,
    );
    std::fs::create_dir_all(state)?;
    let store = Store::open(state.join("ribosome.db"))?;
    let mut episodes = BTreeMap::new();
    let mut groups = BTreeMap::new();
    for selection in &manifest.episodes {
        safe_name(&selection.cohort)?;
        safe_name(&selection.episode)?;
        let directory = manifest
            .cohorts
            .get(&selection.cohort)
            .ok_or_else(|| Error::invalid("unknown cohort"))?;
        let directory = manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(directory);
        let lock: SourceLock =
            serde_json::from_slice(&std::fs::read(directory.join("source-lock.json"))?)?;
        let episode: Episode = serde_json::from_slice(&std::fs::read(
            directory
                .join("episodes")
                .join(format!("{}.json", selection.episode)),
        )?)?;
        if episode.id != selection.episode || !lock.selected.contains(&episode.source_id) {
            return Err(Error::invalid("episode is absent from the source lock"));
        }
        for group in [
            format!("task:{}", episode.task),
            format!("family:{}", episode.family),
        ] {
            if let Some(split) =
                groups.insert((lock.source.clone(), group), selection.split.clone())
                && split != selection.split
            {
                return Err(Error::invalid(
                    "repeated tasks and source families must remain in one split",
                ));
            }
        }
        let key = (selection.cohort.clone(), selection.episode.clone());
        if episodes.insert(key, (selection, episode)).is_some() {
            return Err(Error::invalid("study selects an episode more than once"));
        }
    }
    // Validate all windows before publishing any episode or assignment.
    let mut names = BTreeSet::new();
    for assignment in &manifest.assignments {
        safe_name(&assignment.id)?;
        if !names.insert(&assignment.id) || assignment.windows.is_empty() {
            return Err(Error::invalid(
                "assignment names must be unique and windows nonempty",
            ));
        }
        let mut count = 0;
        for window in &assignment.windows {
            let (_, episode) = episodes
                .get(&(window.cohort.clone(), window.episode.clone()))
                .ok_or_else(|| Error::invalid("window selects an episode outside the study"))?;
            if window.start >= window.end
                || window.end > episode.decoded.messages.len()
                || (assignment.visibility == DiscoveryCorpusVisibility::Online && window.start != 0)
            {
                return Err(Error::invalid(
                    "invalid message window; online prefixes must start at zero",
                ));
            }
            count += window.end - window.start;
            require_capabilities(episode, &assignment.required_capabilities)?;
        }
        if count > 1000 {
            return Err(Error::invalid(
                "assignment exceeds 1000 messages; divide it into bounded assignments",
            ));
        }
    }
    let mut published = BTreeMap::new();
    let mut owner = grant.clone();
    for ((cohort, id), (selection, episode)) in &episodes {
        let publication = project_episode(
            &manifest.id,
            cohort,
            episode,
            &grant.scope,
            &selection.split,
        )?;
        owner
            .paths
            .extend(publication.2.iter().map(|s| s.artifact.path.clone()));
        published.insert((cohort.clone(), id.clone()), publication);
    }
    owner.paths.sort();
    owner.paths.dedup();
    let mut assignments = Vec::new();
    for assignment in &manifest.assignments {
        let mut windows = Vec::new();
        let mut artifacts = Vec::new();
        let mut source_records = BTreeSet::new();
        for window in &assignment.windows {
            let (source, events, snapshots) =
                &published[&(window.cohort.clone(), window.episode.clone())];
            let selected = &events[window.start..window.end];
            for segment in selected.chunks(128) {
                let last = segment.last().unwrap();
                windows.push(DiscoveryWindow {
                    execution: last.run_id.clone(),
                    event_refs: segment.iter().map(|e| e.id.clone()).collect(),
                    frontier: serde_json::Map::from_iter([(
                        format!("{}/{}", last.run_id, last.producer),
                        json!(last.sequence),
                    )]),
                });
            }
            for event in selected {
                for reference in &event.artifacts {
                    let snapshot = snapshots.iter().find(|s| s.artifact == *reference).unwrap();
                    artifacts.push(CorpusArtifact {
                        artifact: reference.clone(),
                        snapshot_id: snapshot.snapshot_id.clone().unwrap(),
                    });
                }
            }
            source_records.insert(source.id.clone());
        }
        let corpus = DiscoveryCorpus {id:format!("{}:{}",manifest.id,assignment.id),version:"1".into(),visibility:assignment.visibility.clone(),
            source_windows:windows,definition_refs:vec![],limitations:vec!["External source messages are quoted observations. Tool responses describe the source execution; they are not Ribosome execution receipts.".into(),"Publisher annotations and unclassified source fields remain owner-side.".into()],
            artifacts,dependencies:vec![]};
        ribosome_core::validation::validate("DiscoveryCorpus", &serde_json::to_value(&corpus)?)?;
        if serde_json::to_vec(&corpus)?.len() > 256 * 1024 {
            return Err(Error::invalid(
                "assignment exceeds 256 KiB; divide it into smaller assignments",
            ));
        }
        let assigned = PublishedAssignment {
            paths: corpus
                .artifacts
                .iter()
                .map(|a| a.artifact.path.clone())
                .collect(),
            corpus,
            source_records: source_records.into_iter().collect(),
        };

        assignments.push(assigned);
    }
    store.register_grant(&owner)?;
    let mut owner_config = config.clone();
    owner_config["grant"] = serde_json::to_value(&owner)?;
    write_json(&output.join("owner-config.json"), &owner_config)?;
    for (source, events, snapshots) in published.values() {
        for snapshot in snapshots {
            let path = workspace.join(&snapshot.artifact.path);
            std::fs::create_dir_all(path.parent().unwrap())?;
            if path.exists() {
                if std::fs::read_to_string(&path)? != snapshot.content {
                    return Err(Error::conflict(
                        "projected message path already contains different evidence",
                    ));
                }
            } else {
                std::fs::write(path, &snapshot.content)?;
            }
        }
        store.import_external_episode(&owner, source, events, snapshots)?;
    }
    for (assignment, assigned) in manifest.assignments.iter().zip(&assignments) {
        store.register_discovery_corpus(&owner, &assigned.corpus)?;
        write_json(&output.join(format!("{}.json", assignment.id)), assigned)?;
    }
    Ok(assignments)
}

pub fn project_episode(
    study: &str,
    cohort: &str,
    episode: &Episode,
    scope: &Scope,
    split: &Split,
) -> Result<(RecordEnvelope, Vec<Event>, Vec<ArtifactChunk>)> {
    safe_name(study)?;
    safe_name(cohort)?;
    safe_name(&episode.id)?;
    let execution = format!("{study}:{cohort}:{}", episode.id);
    let source_id = format!("source:{execution}");
    let provenance = Provenance {origin:Origin::Observed,source_refs:vec![],scenario_family:episode.family.clone(),split:split.clone(),
        limitations:vec!["Imported external record; original timestamps and missing fields remain unknown where absent.".into()]};
    let source = RecordEnvelope {schema_version:"1".into(),id:source_id.clone(),scope:scope.clone(),kind:RecordKind::Memory,version:"1".into(),
        created_ms:episode.acquired_ms.clone(),updated_ms:episode.acquired_ms.clone(),retired:false,provenance:provenance.clone(),
        body:json!({"kind":"episodic","content":format!("External source for {execution}."),"applicability":"Source availability for an imported episode.",
            "evidence_refs":[],"counterexamples":[],"responses":[],"regression_cases":[],"conflicts":[],"supersedes":[]}).as_object().unwrap().clone()};
    let mut events: Vec<Event> = Vec::new();
    let mut snapshots = Vec::new();
    let mut pending_calls = BTreeMap::new();
    for (index, message) in episode.decoded.messages.iter().enumerate() {
        let event_id = format!("event:{execution}:{index}");
        // Sequence records chronology. Only an identified call/result pair
        // establishes a source-reported parent; later messages may be independent.
        let parents = if message.role == "tool" {
            message
                .tool_call_id
                .as_ref()
                .and_then(|id| pending_calls.remove(id.as_str()))
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        if message.role == "assistant" {
            for call in &message.tool_calls {
                if let Some(id) = call["id"].as_str() {
                    pending_calls.insert(id, event_id.clone());
                }
            }
        }
        let mut artifacts = Vec::new();
        let material = serde_json::to_string(message)?;
        let payload = if material.len() > 16 * 1024 {
            let mut parts = Vec::new();
            let mut offset = 0;
            while offset < material.len() {
                let mut end = (offset + 64 * 1024).min(material.len());
                while !material.is_char_boundary(end) {
                    end -= 1;
                }
                let part = parts.len();
                let content = material[offset..end].to_owned();
                let artifact = ArtifactRef {
                    path: format!(
                        "evidence/{study}/{cohort}/{}/message-{index}-part-{part}.txt",
                        episode.id
                    ),
                    version: format!("import:{execution}:{index}:{part}"),
                };
                let snapshot_id = format!("snapshot:{execution}:{index}:{part}");
                snapshots.push(ArtifactChunk {
                    artifact: artifact.clone(),
                    total_bytes: content.len().to_string(),
                    content,
                    offset: 0,
                    eof: true,
                    snapshot_id: Some(snapshot_id.clone()),
                    required_freshness: Some(Freshness::Historical),
                });
                artifacts.push(artifact.clone());
                parts.push(json!({"part":part,"message_byte_offset":offset,"artifact":artifact,"snapshot_id":snapshot_id}));
                offset = end;
            }
            json!({"authority":"external_record","role":message.role,"source_index":message.source_index,"time_basis":"import_time",
                "preview":material.chars().take(2048).collect::<String>(),"message_bytes":material.len(),"message_parts":parts})
        } else {
            json!({"authority":"external_record","time_basis":"import_time","message":message})
        };
        let mut event_provenance = provenance.clone();
        event_provenance.source_refs.push(source_id.clone());
        events.push(Event {
            id: event_id,
            scope: scope.clone(),
            run_id: execution.clone(),
            producer: "external-import".into(),
            sequence: index.to_string(),
            kind: "external_message".into(),
            timestamp_ms: episode.acquired_ms.clone(),
            parents,
            correlation: execution.clone(),
            artifacts,
            payload: payload.as_object().unwrap().clone(),
            provenance: event_provenance,
        });
    }
    Ok((source, events, snapshots))
}

fn safe_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 40
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
    {
        return Err(Error::invalid(
            "study, cohort, assignment and episode names require 1..40 letters, digits, hyphens or underscores",
        ));
    }
    Ok(())
}
fn require_capabilities(episode: &Episode, required: &[String]) -> Result<()> {
    let c = &episode.decoded.coverage;
    for name in required {
        let available = match name.as_str() {
            "text" => c.modalities.iter().any(|m| m == "text"),
            "tool_calls" => c.tool_calls > 0,
            "paired_results" => c.paired_results > 0,
            "timestamps" => c.original_timestamps == c.messages,
            "prefix_safe" => c.prefix_safe,
            _ => {
                return Err(Error::invalid(format!(
                    "unknown evidence capability {name}"
                )));
            }
        };
        if !available {
            return Err(Error::invalid(format!(
                "episode {} lacks required capability {name}",
                episode.id
            )));
        }
    }
    Ok(())
}
