# Contract evolution ledger

This ledger coordinates the shared schema changes across R1–R6. The tables record the accepted design; implemented additions are identified below. The canonical schema and protocol define the available API. Extend `contracts/schema.json`, regenerate both language bindings, and add conformance fixtures. Do not hand-edit generated types or maintain two rival definitions of a motif.

The names below are design names. New fields on an existing record must have documented legacy defaults or require the negotiated newer record schema. A default may mean unknown/unsupported; it must not invent evidence.

## 1. Existing records to extend

| Existing record | Add or clarify | Owning package |
| --- | --- | --- |
| `Definition` | Structured entry/exit contract, functional role descriptions, decision points, relations to other definitions, and evaluation questions | R4 |
| `Occurrence` | Role-to-event bindings, incoming context, dependency evidence, local functional outcome, and recognition-time visibility | R4 |
| `Implementation` | Explicit execution input/output contract, allowed binding/adaptation slots, discovered-source linkage, and known limitations; keep `instructions` / `registered_tool` | R5 |
| `Transplant` | Exact receiving state and implementation binding snapshot, invocation/run reference, fresh output/check references, unresolved incompatibilities | R5 |
| `Checkpoint` | A negotiated bounded continuation descriptor rather than unlimited embedded messages; source-authorized context references, pinned implementation, and pending operation identity | R1 |
| `ActionReceipt` | Internal finalization phase/evidence, outcome basis, validation references, and separate observed-state reconciliation when occurrence is unproven | R2 |
| `Evaluation` | Exact subject execution/run references, condition/arm identity, usage/allocation references, scenario lineage, and actual artifact/validation evidence | R6 |
| `Experiment` | Study objective, frozen recipient/corpus identity, consumed protected-set identity, allocation reference, complete planned arm/case matrix | R3/R6 |
| `Admission` | Stated claim supported, applicable context, exact implementation version, required evidence set and limitations | R6 |

Do not replace every existing string obligation with a giant formal language. A structured obligation can name the property, required evidence, and optional checker while leaving semantic interpretation agentic. Mechanical validation answers whether the record is well formed; semantic evaluation answers whether it is warranted.

## 2. New internal or exchanged values

### `SourceRef` and `ContextItem`

A source reference discriminates record, event, or host artifact. A record uses its envelope identity/revision; a motif/implementation link additionally pins its semantic version where applicable. An artifact reference includes its owning host/workspace as needed to disambiguate identical relative paths. Neither timestamps nor path strings alone establish source identity.

A context item contains `item_id`, `kind`, `content_ref`, `source_refs`, `derived_from`, `freshness_requirement`, `attribution`, and `coverage`. Rust can resolve its transitive availability under the caller's grant. Its summary text is not authoritative evidence of tool execution.

### Implemented continuation reference lookup

`ContinuationRead`, `ContinuationKind`, `ContinuationReference` and `ContinuationPage` back `continuation.read`. This bounded run-bound lookup reads existing obligation, receipt and work state. Clean context segments retain initial reference pages and the checkpointed evidence cursor; further pages do not copy source content or assert successful completion. The current schema and conformance fixtures define the implemented fields.

### `ValidationEvidence`

Contains `check_ref`, exact checker/config version, input artifact versions, targets/properties actually validated, outcome, source receipt, and owner policy identity. The set of read inputs may be larger than the validated targets. Application must not collapse those sets.

### `ImplementationInvocation`

Contains the implementation reference, typed bindings, recipient state/evidence, parent work, allocated budget, purpose, and resulting run identity. Purpose is supplied through an authorized path. A model cannot switch a production invocation into an unrestricted experiment by editing a flag.

### `Discovery`

One new semantic record links a work item to hypotheses, evidence windows, contrasts, proposed definitions/relations, decision, and unresolved questions. It is evidence about discovery, not its own queue, scheduler, or truth oracle.

### `BudgetAllocation`

Contains identity, parent allocation/root grant, permitted resource ceilings, reservations, settled usage references, deadline, and terminal disposition. Rollups do not charge the same leaf twice. Unknown usage survives restart.

Keep internal executor/finalizer types private unless a consumer actually needs them. Do not turn every phase enum into a public protocol merely because it exists in Rust.

## 3. Worked record chain

The JSON fragments below illustrate semantics and linkage. They intentionally omit current envelope fields and are not copy-paste RPC requests. Human-readable IDs represent future fixture records, not existing repository data.

### Discovery result

```json
{
  "work_ref": "discover-join-pattern",
  "sources": ["donor-join-1", "donor-join-2", "benign-join-1"],
  "hypotheses": [
    {
      "claim": "The planner establishes assumption compatibility before joining contributions.",
      "support": ["donor-join-1/planner-17", "donor-join-2/planner-8"],
      "contrasts": ["benign-join-1/planner-4"],
      "alternative": "The extra steps may be unnecessary checking of already compatible inputs.",
      "decision": "propose_definition",
      "limitations": ["No evidence yet for transfer to contributions with missing metadata."]
    }
  ],
  "definition_refs": [{"id": "assumption-join", "version": "1"}]
}
```

The investigator must have actually retrieved the referenced evidence. Rust validates accessible references; the contrast capability/evaluator assesses whether the evidence supports the interpretation.

### Occurrence extension

```json
{
  "definition": {"id": "assumption-join", "version": "1"},
  "execution": "donor-join-1",
  "role_bindings": [
    {"role": "contribution", "event_refs": ["donor-join-1/worker-a-8", "donor-join-1/worker-b-5"]},
    {"role": "reconciliation", "event_refs": ["donor-join-1/planner-17"]},
    {"role": "verification", "event_refs": ["donor-join-1/checker-3"]}
  ],
  "recognition": "supported",
  "local_outcome": {
    "state": "satisfied",
    "evidence_refs": ["donor-join-1/checker-3"]
  },
  "assumptions": ["Relationship between planner-17 and checker-3 is inferred pending host dependency evidence."]
}
```

An inconsistent combination of claimed success and missing critical dependency evidence must be challenged. The fragment demonstrates fields; it does not certify the hypothesis. Local success can be supported by checker output even when the explanatory causal link remains uncertain; store those claims separately.

### Instruction implementation and invocation

```json
{
  "implementation": {
    "name": "assumption-aware-join",
    "version": "1",
    "format": "instructions",
    "motifs": [{"id": "assumption-join", "version": "1"}],
    "material": "Inspect task-relevant assumptions; reconcile supported mismatches; preserve independent work; return unresolved when critical assumptions cannot be established; verify the combined result.",
    "required_capabilities": ["read-contribution", "read-source-metadata", "write-joined-result", "check-joined-result"],
    "state_assumptions": ["The host supplies the requested join requirements."],
    "possible_effects": ["write joined result in the granted workspace"],
    "failure_behavior": "Return unresolved requirements rather than invent missing metadata."
  },
  "invocation": {
    "implementation": {"id": "assumption-aware-join-record", "version": "1"},
    "recipient": "recipient-join-7",
    "bindings": {"contributions": ["left.json", "right.json"], "output": "combined.json"},
    "parent_work": "repair-recipient-join-7",
    "budget_allocation": "allocation-recipient-join-7"
  }
}
```

Capability names bind to the host's actual tools. A stored string is not proof of a working capability. The test must run this material through Pi with fresh recipient observations and registered low-level task tools, without an all-in-one helper that already implements the policy.

## 4. New or extended operation families

| Operation family | Contract obligation |
| --- | --- |
| Context authorize/resolve | Current access, source lineage, freshness purpose, bounded payload, and a generation to revalidate before provider dispatch |
| Evidence slice/search | Explicit target, bound scope, retained observed/inferred distinctions, limits, and stable pagination |
| Implementation request/status | Exact version/bindings, admission or experimental authority, scheduled child identity, and deadlock-free completion |
| Effect prepare/finalize/reconcile | Stable operation identity, grant/state checks, no uncertain replay, and idempotent bookkeeping |
| Allocation reserve/settle | Atomic hierarchical limits and adapter-attributed usage |
| Qualification reserve/record | Protected corpus consumption independent of a fresh grant ID |

Use existing `record.submit`, `work.request`, and `search.query` when extension is sufficient. Add a new method only when its authority or lifecycle cannot be represented clearly through an existing one. No generic SQL, arbitrary filesystem, or evaluator-policy mutation endpoint is introduced.

## 5. Cross-language conformance

Test absent versus null, exact string versions/counters, enums and unknown values, bounded nested arrays, inaccessible references, legacy record decoding, incompatible checkpoint formats, and all new method grants. Test that generated TypeScript tool schemas present the actual nested record contracts to Pi.

A migration may produce a legacy record with unknown structured fields. It must not convert it to a fully evidenced new definition or invocation merely to satisfy the schema. Old material can remain usable under its existing supported path; stronger new claims require new evidence.

## R6 implemented contract additions

`Experiment.study_objective` selects `function` or `system_benefit`; `learning_cost` carries measured cost, completeness and an explicit reuse count. Both match the host policy. `EvaluationTask.implementation_ref` identifies the exact invoked material. `Evaluation.run_refs` identifies actual stage executions. `ExperimentResult.report` retains aggregate measurements, paired case comparisons, uncertainty, usage and limitations. `ArchiveCell` adds exact implementation version, admission reference, complete evaluation references and limitations. Existing fields remain readable. See [protocol](../../protocol.md#whole-agent-laboratory) for the host configuration, isolation and acceptance rules.
