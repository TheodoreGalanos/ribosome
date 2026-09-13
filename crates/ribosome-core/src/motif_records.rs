use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{counter, decode},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value};
use std::collections::HashSet;

impl Store {
    /// Supply bookkeeping known to the host without choosing the evidence or
    /// interpreting its function. Explicit caller metadata is still validated.
    pub(crate) fn complete_motif_context(
        &self,
        grant: &Grant,
        run: &str,
        submission: &mut RecordSubmission,
    ) -> Result<()> {
        match submission.kind {
            RecordKind::Discovery => {
                submission
                    .body
                    .entry("run_refs")
                    .or_insert_with(|| serde_json::json!([run]));
                if let Some(windows) = submission
                    .body
                    .get_mut("source_windows")
                    .and_then(Value::as_array_mut)
                {
                    for window in windows {
                        if let Some(window) = window.as_object_mut()
                            && !window.contains_key("frontier")
                        {
                            let frontier =
                                self.selected_frontier(grant, window.get("event_refs"))?;
                            window.insert("frontier".into(), Value::Object(frontier));
                        }
                    }
                }
            }
            RecordKind::Occurrence => {
                if !submission.body.contains_key("frontier") {
                    let frontier =
                        self.selected_frontier(grant, submission.body.get("event_refs"))?;
                    submission
                        .body
                        .insert("frontier".into(), Value::Object(frontier));
                }
                let operator: String = self.db.query_row(
                    "SELECT json_extract(request,'$.operator') FROM runs WHERE id=?1",
                    [run],
                    |row| row.get(0),
                )?;
                submission
                    .body
                    .entry("operator")
                    .or_insert_with(|| Value::String(operator.clone()));
                if let Some(grounding) = submission
                    .body
                    .get_mut("grounding")
                    .and_then(Value::as_object_mut)
                {
                    grounding
                        .entry("annotator")
                        .or_insert_with(|| serde_json::json!({"run_id":run,"operator":operator}));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn selected_frontier(
        &self,
        grant: &Grant,
        references: Option<&Value>,
    ) -> Result<Map<String, Value>> {
        let mut frontier = Map::new();
        let references = references
            .and_then(Value::as_array)
            .ok_or_else(|| Error::invalid("event_refs must be an array"))?;
        for reference in references {
            let event = self.motif_event(
                grant,
                reference
                    .as_str()
                    .ok_or_else(|| Error::invalid("event_refs must contain event IDs"))?,
            )?;
            let key = format!("{}/{}", event.run_id, event.producer);
            let previous = frontier
                .get(&key)
                .and_then(Value::as_str)
                .map(counter)
                .transpose()?
                .unwrap_or(0);
            if counter(&event.sequence)? > previous {
                frontier.insert(key, Value::String(event.sequence));
            }
        }
        Ok(frontier)
    }

    pub(crate) fn validate_motif_submission(
        &self,
        grant: &Grant,
        submission: &RecordSubmission,
        run: Option<&str>,
    ) -> Result<()> {
        let body = Value::Object(submission.body.clone());
        // Legacy records remain readable/importable. Newly authored discovery
        // outputs must use the structured contract and its grounding checks.
        if let Some(run) = run {
            let operator: String = self.db.query_row(
                "SELECT json_extract(request,'$.operator') FROM runs WHERE id=?1",
                [run],
                |row| row.get(0),
            )?;
            let required = match (operator.as_str(), &submission.kind) {
                ("discovery@1", RecordKind::Definition) => Some("functional_contract"),
                ("discovery@1", RecordKind::Occurrence) => Some("grounding"),
                _ => None,
            };
            if let Some(field) = required
                && !submission.body.contains_key(field)
            {
                return Err(Error::invalid(format!("discovery requires {field}")));
            }
        }
        match submission.kind {
            RecordKind::Definition => {
                let definition: Definition = decode("Definition", body)?;
                if let Some(contract) = definition.functional_contract {
                    let mut roles = HashSet::new();
                    for role in contract.roles {
                        if !roles.insert(role.name) {
                            return Err(Error::invalid("functional role names must be unique"));
                        }
                    }
                    for relation in contract.relations {
                        self.motif_definition(grant, &relation.definition)?;
                    }
                }
            }
            RecordKind::Occurrence => {
                let occurrence: Occurrence = decode("Occurrence", body.clone())?;
                if let Some(grounding) = &occurrence.grounding {
                    self.validate_occurrence_grounding(grant, &occurrence, grounding, run)?;
                    self.require_motif_observations(run, &body)?;
                }
            }
            RecordKind::Discovery => {
                let discovery: Discovery = decode("Discovery", body.clone())?;
                if grant
                    .discovery_corpus
                    .as_ref()
                    .is_some_and(|assigned| assigned != &discovery.corpus)
                {
                    return Err(Error::denied(
                        "discovery corpus must match the host assignment",
                    ));
                }
                if discovery.decision == DiscoveryDecision::Supported
                    && (discovery.definition_refs.is_empty() || discovery.hypotheses.is_empty())
                {
                    return Err(Error::invalid(
                        "supported discovery requires a hypothesis and pinned definition",
                    ));
                }
                if discovery.decision == DiscoveryDecision::NoMotif
                    && !discovery.definition_refs.is_empty()
                {
                    return Err(Error::invalid("no_motif cannot claim output definitions"));
                }
                for hypothesis in &discovery.hypotheses {
                    if !matches!(
                        hypothesis.decision,
                        DiscoveryHypothesisDecision::Reject
                            | DiscoveryHypothesisDecision::Inconclusive
                    ) && hypothesis.support_refs.is_empty()
                    {
                        return Err(Error::invalid(
                            "a proposed motif requires supporting references",
                        ));
                    }
                }
                let mut selected_events = HashSet::new();
                for window in &discovery.source_windows {
                    validate_frontier(&window.frontier)?;
                    for id in &window.event_refs {
                        selected_events.insert(id.as_str());
                        let event = self.motif_event(grant, id)?;
                        self.require_corpus_event(grant, id)?;
                        if event.run_id != window.execution {
                            return Err(Error::invalid(
                                "discovery window event belongs to a different execution",
                            ));
                        }
                        within_frontier(&event, &window.frontier)?;
                    }
                }
                if selected_events.len() > 1000 {
                    return Err(Error::invalid("discovery selection exceeds 1000 events"));
                }
                let mut references = Vec::new();
                crate::exports::collect_references(&body, &mut references);
                references.sort();
                references.dedup();
                for reference in references {
                    let event: bool = self.db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
                        [&reference],
                        |r| r.get(0),
                    )?;
                    if event && !selected_events.contains(reference.as_str()) {
                        return Err(Error::invalid(
                            "discovery event evidence must appear in a declared source window",
                        ));
                    }
                }
                for reference in &discovery.definition_refs {
                    self.motif_definition(grant, reference)?;
                }
                for reference in &discovery.occurrence_refs {
                    if self.record(grant, reference)?.kind != RecordKind::Occurrence {
                        return Err(Error::invalid(
                            "occurrence_refs must identify occurrence records",
                        ));
                    }
                }
                if run.is_some_and(|run| !discovery.run_refs.iter().any(|id| id == run)) {
                    return Err(Error::invalid(
                        "discovery run_refs must include the submitting run",
                    ));
                }
                for id in &discovery.run_refs {
                    if self.run_grant(id)?.id != grant.id {
                        return Err(Error::denied("discovery run is outside this root grant"));
                    }
                }
                if let Some(work) = &discovery.work_ref {
                    let owned: bool = self.db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM work WHERE id=?1 AND grant_id=?2)",
                        params![work, grant.id],
                        |r| r.get(0),
                    )?;
                    if !owned || run.is_some_and(|run| run != work) {
                        return Err(Error::denied(
                            "discovery work must be the submitting run's owned work item",
                        ));
                    }
                }
                self.require_motif_observations(run, &body)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn motif_definition(
        &self,
        grant: &Grant,
        reference: &VersionRef,
    ) -> Result<Definition> {
        let record = self.record(grant, &reference.id)?;
        if record.kind != RecordKind::Definition {
            return Err(Error::invalid("motif reference must identify a definition"));
        }
        let definition: Definition = decode("Definition", Value::Object(record.body))?;
        if definition.version != reference.version {
            return Err(Error::conflict(
                "motif definition semantic version does not match",
            ));
        }
        Ok(definition)
    }

    pub(crate) fn motif_event(&self, grant: &Grant, id: &str) -> Result<Event> {
        self.require_reference(grant, id)?;
        let body: Option<String> = self
            .db
            .query_row("SELECT body FROM events WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        Ok(serde_json::from_str(&body.ok_or_else(|| {
            Error::invalid("event reference must identify an event")
        })?)?)
    }

    fn validate_occurrence_grounding(
        &self,
        grant: &Grant,
        occurrence: &Occurrence,
        grounding: &OccurrenceGrounding,
        run: Option<&str>,
    ) -> Result<()> {
        let definition = self.motif_definition(grant, &occurrence.definition)?;
        let contract = definition.functional_contract.ok_or_else(|| {
            Error::invalid("grounded occurrence requires a definition with a functional contract")
        })?;
        validate_frontier(&occurrence.frontier)?;
        for id in &occurrence.event_refs {
            self.require_corpus_event(grant, id)?;
            within_frontier(&self.motif_event(grant, id)?, &occurrence.frontier)?;
        }
        let roles: HashSet<_> = contract.roles.iter().map(|r| r.name.as_str()).collect();
        let mut bound = HashSet::new();
        for binding in &grounding.role_bindings {
            if !roles.contains(binding.role.as_str()) || !bound.insert(binding.role.as_str()) {
                return Err(Error::invalid(
                    "role binding must name a unique role in the pinned definition",
                ));
            }
            if binding
                .event_refs
                .iter()
                .any(|id| !occurrence.event_refs.contains(id))
            {
                return Err(Error::invalid(
                    "role binding event lies outside the occurrence",
                ));
            }
        }
        for dependency in &grounding.dependency_evidence {
            if dependency.source_event_ref == dependency.target_event_ref
                || !occurrence.event_refs.contains(&dependency.target_event_ref)
                || !(occurrence.event_refs.contains(&dependency.source_event_ref)
                    || grounding
                        .incoming_context_refs
                        .contains(&dependency.source_event_ref))
            {
                return Err(Error::invalid(
                    "dependency must connect occurrence evidence or an explicit incoming event",
                ));
            }
            self.motif_event(grant, &dependency.source_event_ref)?;
            let target = self.motif_event(grant, &dependency.target_event_ref)?;
            if dependency.basis == MotifDependencyEvidenceBasis::SourceReported
                && !target.parents.contains(&dependency.source_event_ref)
            {
                return Err(Error::invalid(
                    "source_reported dependency is absent from the target event's parents; use inferred for a hypothesis",
                ));
            }
        }
        if grounding.local_outcome.state != MotifLocalOutcomeState::Unresolved
            && grounding.local_outcome.evidence_refs.is_empty()
        {
            return Err(Error::invalid(
                "local outcome requires evidence; otherwise leave it unresolved",
            ));
        }
        let annotator = self.run_grant(&grounding.annotator.run_id)?;
        if annotator.id != grant.id || run.is_some_and(|id| id != grounding.annotator.run_id) {
            return Err(Error::denied(
                "occurrence annotator is outside the submitting run or root grant",
            ));
        }
        let operator: String = self.db.query_row(
            "SELECT json_extract(request,'$.operator') FROM runs WHERE id=?1",
            [&grounding.annotator.run_id],
            |r| r.get(0),
        )?;
        if operator != grounding.annotator.operator || operator != occurrence.operator {
            return Err(Error::invalid(
                "occurrence operator must match the annotator's pinned operator",
            ));
        }
        let reported_online =
            grounding.recognition_visibility == OccurrenceGroundingRecognitionVisibility::Online;
        let assigned_online = annotator
            .discovery_corpus
            .as_ref()
            .map(|reference| self.discovery_corpus(&annotator, reference))
            .transpose()?
            .is_some_and(|corpus| corpus.visibility == DiscoveryCorpusVisibility::Online);
        if reported_online != assigned_online {
            return Err(Error::denied(
                "recognition visibility must match the host's source-prefix assignment",
            ));
        }
        Ok(())
    }

    fn require_motif_observations(&self, run: Option<&str>, body: &Value) -> Result<()> {
        let Some(run) = run else {
            return Ok(());
        };
        let mut references = Vec::new();
        crate::exports::collect_references(body, &mut references);
        references.sort();
        references.dedup();
        for reference in references {
            let observed: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM artifact_snapshots a, json_each(a.result_sources) s WHERE a.result_run_id=?1 AND a.result_content IS NOT NULL AND json_extract(s.value,'$.id')=?2)",
                params![run, reference], |r| r.get(0),
            )?;
            if !observed {
                return Err(Error::denied(
                    "motif evidence was not retrieved by this run; read the cited evidence before submitting",
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_frontier(frontier: &Map<String, Value>) -> Result<()> {
    if frontier.len() > 128 {
        return Err(Error::invalid("motif frontier exceeds 128 producers"));
    }
    for value in frontier.values() {
        counter(
            value
                .as_str()
                .ok_or_else(|| Error::invalid("motif frontier values must be sequence strings"))?,
        )?;
    }
    Ok(())
}

pub(crate) fn within_frontier(event: &Event, frontier: &Map<String, Value>) -> Result<()> {
    let sequence = frontier
        .get(&format!("{}/{}", event.run_id, event.producer))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::invalid("motif frontier must include each cited event producer"))?;
    if counter(&event.sequence)? > counter(sequence)? {
        return Err(Error::invalid(
            "motif evidence lies after the declared frontier",
        ));
    }
    Ok(())
}
