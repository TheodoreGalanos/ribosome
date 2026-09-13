use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::validate,
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

impl Store {
    pub(crate) fn validate_transplant(
        &self,
        grant: &Grant,
        submission: &RecordSubmission,
    ) -> Result<()> {
        if submission.kind != RecordKind::Transplant {
            return Ok(());
        }
        let transplant: Transplant =
            serde_json::from_value(Value::Object(submission.body.clone()))?;
        if let Some(run) = &transplant.invocation_run {
            if self.run_grant(run)?.id != grant.id {
                return Err(Error::denied(
                    "transplant invocation belongs to another grant",
                ));
            }
            let invocation = self
                .run_invocation(run)?
                .ok_or_else(|| Error::invalid("transplant run has no pinned invocation"))?;
            if invocation.implementation != transplant.donor
                || invocation.bindings != transplant.bindings
            {
                return Err(Error::invalid(
                    "transplant must match the invoked implementation and bindings",
                ));
            }
            for reference in transplant
                .recipient_entry_refs
                .iter()
                .flatten()
                .chain(transplant.result_refs.iter().flatten())
            {
                self.require_reference(&self.run_grant(run)?, reference)?;
            }
        } else if transplant
            .result_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_empty())
        {
            return Err(Error::invalid(
                "execution result references require an invocation run",
            ));
        }
        Ok(())
    }

    pub(crate) fn run_invocation(&self, run: &str) -> Result<Option<ImplementationInvocation>> {
        let body: Option<String> = self
            .db
            .query_row(
                "SELECT json_extract(request,'$.invocation') FROM runs WHERE id=?1",
                [run],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        body.map(|body| Ok(serde_json::from_str(&body)?))
            .transpose()
    }

    pub(crate) fn validate_invocation(
        &self,
        grant: &Grant,
        run: &str,
        invocation: &ImplementationInvocation,
    ) -> Result<Implementation> {
        validate(
            "ImplementationInvocation",
            &serde_json::to_value(invocation)?,
        )?;
        // The owner may deliver prepared material without opening its donor
        // history. Ancestry availability, scope and split checks still apply.
        let mut prepared = grant.clone();
        prepared.prepared_run = Some(run.into());
        let record = self.record(&prepared, &invocation.implementation.id)?;
        if record.kind != RecordKind::Implementation {
            return Err(Error::invalid("invocation requires an implementation"));
        }
        let implementation: Implementation =
            serde_json::from_value(Value::Object(record.body.clone()))?;
        if implementation.version != invocation.implementation.version {
            return Err(Error::conflict(
                "implementation version does not match invocation",
            ));
        }
        if implementation.format != ImplementationFormat::Instructions {
            return Err(Error::invalid("registered tools use action.execute"));
        }
        if !implementation
            .required_capabilities
            .iter()
            .all(|tool| grant.tools.contains(tool))
            || (grant.mode == Mode::Observe && !implementation.possible_effects.is_empty())
        {
            return Err(Error::denied(
                "implementation capabilities or effects are not granted",
            ));
        }
        match invocation.purpose {
            ImplementationInvocationPurpose::Production
                if !self.is_admitted(&prepared, &record)? =>
            {
                return Err(Error::denied(
                    "implementation is not admitted in this context",
                ));
            }
            ImplementationInvocationPurpose::Experimental if grant.mode != Mode::Sandbox => {
                return Err(Error::denied(
                    "experimental invocation requires a host sandbox grant",
                ));
            }
            _ => {}
        }
        let contract = implementation
            .instruction_contract
            .as_ref()
            .ok_or_else(|| {
                Error::invalid("instruction invocation requires instruction_contract")
            })?;
        let mut names = std::collections::HashSet::new();
        for slot in &contract.inputs {
            if !names.insert(&slot.name) {
                return Err(Error::invalid("instruction input names must be unique"));
            }
            let Some(value) = invocation.bindings.get(&slot.name) else {
                if slot.required {
                    return Err(Error::invalid("required invocation binding is missing"));
                }
                continue;
            };
            let valid = match slot.kind {
                BindingSlotKind::String => value.is_string(),
                BindingSlotKind::Number => value.is_number(),
                BindingSlotKind::Boolean => value.is_boolean(),
                BindingSlotKind::Object => value.is_object(),
                BindingSlotKind::Array => value.is_array(),
                BindingSlotKind::ArtifactPath => value
                    .as_str()
                    .is_some_and(|path| grant.paths.iter().any(|allowed| allowed == path)),
                BindingSlotKind::Tool => value
                    .as_str()
                    .is_some_and(|tool| grant.tools.iter().any(|allowed| allowed == tool)),
            };
            if !valid {
                return Err(Error::invalid(
                    "invocation binding has the wrong shape or capability",
                ));
            }
        }
        if invocation.bindings.keys().any(|name| !names.contains(name))
            || serde_json::to_vec(&invocation.bindings)?.len() > 65536
        {
            return Err(Error::invalid(
                "invocation contains undeclared or oversized bindings",
            ));
        }
        let mut owner = grant.clone();
        owner.prepared_run = None;
        for reference in &invocation.recipient_refs {
            self.require_reference(&owner, reference)?;
        }
        // Prepared-record source traversal already checks lineage availability.
        // Verify the pinned metadata without granting donor record delivery.
        for reference in &contract.discovery_refs {
            let source: Option<(String, String)> = self
                .db
                .query_row(
                    "SELECT kind,version FROM records WHERE id=?1 AND client=?2 AND project=?3",
                    params![
                        reference.id,
                        self.evaluation_source_grant(grant, "record", &reference.id, false)?
                            .scope
                            .client,
                        self.evaluation_source_grant(grant, "record", &reference.id, false)?
                            .scope
                            .project
                    ],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if !source.is_some_and(|(kind, version)| {
                matches!(kind.as_str(), "definition" | "occurrence" | "discovery")
                    && version == reference.version
            }) {
                return Err(Error::invalid(
                    "instruction discovery linkage must pin an available motif record version",
                ));
            }
        }
        Ok(implementation)
    }

    pub(crate) fn invocation_material(
        &self,
        run: &str,
        grant: &Grant,
    ) -> Result<InvocationMaterial> {
        let invocation = self
            .run_invocation(run)?
            .ok_or_else(|| Error::missing("run has no implementation invocation"))?;
        let implementation = self.validate_invocation(grant, run, &invocation)?;
        Ok(InvocationMaterial {
            invocation,
            implementation,
        })
    }

    pub(crate) fn require_prepared_sources(
        &self,
        grant: &Grant,
        sources: &[(String, String)],
    ) -> Result<()> {
        let Some(run) = &grant.prepared_run else {
            return Ok(());
        };
        let invocation = self.run_invocation(run)?;
        for (kind, id) in sources {
            if invocation
                .as_ref()
                .is_some_and(|i| i.recipient_refs.contains(id))
            {
                continue;
            }
            let allowed = match kind.as_str() {
                "record" => self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM records WHERE id=?1 AND kind IN ('implementation','definition','memory','admission'))", [id], |r| r.get::<_, bool>(0))?,
                "event" => self.db.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE id=?1 AND run_id=?2)", params![id,run], |r| r.get(0))?,
                "artifact" => self.db.query_row("SELECT EXISTS(SELECT 1 FROM artifact_snapshots WHERE id=?1 AND result_run_id=?2)", params![id,run], |r| r.get(0))?,
                _ => false,
            };
            // Fresh tool observations can be revisited. Other runs' retained
            // results, source transcripts and historical snapshots cannot.
            let observed: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM artifact_snapshots a,json_each(a.result_sources) s WHERE a.result_run_id=?1 AND json_extract(s.value,'$.kind')=?2 AND json_extract(s.value,'$.id')=?3)",
                params![run,kind,id], |r| r.get(0))?;
            if !allowed && !observed {
                return Err(Error::denied(
                    "source is outside prepared reuse and recipient observations",
                ));
            }
        }
        Ok(())
    }
}
