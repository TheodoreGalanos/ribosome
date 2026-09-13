# R4 — Agent-driven discovery of behavioral motifs

**Priority:** Central product development in this pass.\
**Outcome:** A Pi curator discovers meaningful conditional behavior from execution evidence, actively tests its interpretation, and prepares evidence-backed definitions and occurrences for executable extraction.\
**Dependencies:** R1 for safe context; shared contracts. Corpus work can begin in parallel with correctness fixes.

## 1. What must change

The repository already has motif definitions, occurrences, implementations, discovery/extraction prompts, and candidate admission boundaries. This is a strong starting point, not missing scaffolding. [S2][S9]

The required advance is from recognizing or documenting a behavior to investigating **what function it performs, when it applies, which evidence actually demonstrates it, and how another agent could perform that function without replaying the original transcript**.

The current reference evaluator distinguishes two installed procedures by `implementation.material`. Retain it as a procedure test. It does not qualify the new discovery capability. [S10]

### Definition of a behavioral motif

A motif is a recurring or potentially reusable **functional pattern in decision-making and action**. It has an entry situation, a purpose, a conditional policy, observable obligations, and boundaries of applicability. Its realization can vary in tool names, number of steps, agent count, and ordering of independent actions.

Examples of the abstraction level, not a required taxonomy:

- Reconcile incompatible assumptions before combining independently produced results.
- Localize whether failure belongs to the input, tool, or procedure before repeating expensive work.
- Refresh only the evidence affected by a source revision before publishing dependent claims.
- Preserve an independently useful partial result while replacing a failed branch.

Do not define motifs as `read -> edit -> test`, as counts of verification turns, or as a synonym for `normalize-measurements`. Those can be observations, retrieval hints, or mechanical subroutines. They do not specify the function we want to reuse.

A previously unseen, single well-supported occurrence can justify a provisional hypothesis. Recurrence is evidence for generality, not a prerequisite that prevents discovery from rare events. Conversely, repeated behavior can be useless or harmful. Frequency is not admission.

## 2. Separate five outputs

| Output | Question |
| --- | --- |
| Discovery investigation | What hypotheses were considered and what evidence supports or contradicts them? |
| Definition | What function/policy is being described, and where should it apply? |
| Occurrence | Which observed actions/relationships instantiate it in this execution? |
| Implementation candidate | What instructions or procedure can another agent execute to realize it? |
| Evaluation/admission | Where has executing that candidate actually helped or satisfied its contract? |

Extend existing records rather than replacing them. One additional `discovery` record kind is justified for investigation results linking hypotheses, source windows, comparisons, proposed definitions, and unresolved questions. The Rust `work` item still owns scheduling and progress; the discovery record does not introduce another workflow engine.

Do not give an occurrence a single status that conflates recognition, local function, global outcome, and transfer. Keep the existing recognition/obligation distinctions. Add local-outcome attribution and observed/inferred dependency evidence where the current shape is insufficient.

## 3. Inputs: evidence, not leading answers

A discovery assignment gives the curator a task-level objective such as "identify reusable behavior in these permitted runs," its source scope, a budget, and evidence access. It does not give the intended motif name, a known repair rule, or a shortlist of only successful matching examples.

The source corpus must include successful and failed executions, partial successes, benign lookalikes, incomplete observations, and multi-agent interleaving. Retain actual artifact versions, tool observations, source messages, and outcome provenance where available. Visible narratives are evidence of what was said, not proof that a claimed check ran.

A failure can donate good behavior. A successful result can be accidental. The curator must investigate both possibilities.

Maintain these separate information boundaries:

- **Retrospective discovery:** can use the permitted completed source run and its released outcomes.
- **Online recognition:** can use only the observed prefix and current evidence; future outcome information is excluded.
- **Development evaluation:** can use declared feedback to revise the hypothesis or implementation.
- **Protected recipient evaluation:** cannot feed back into source selection or candidate revision during that qualification.

A supplied corpus manifest identifies which partition a source belongs to. Rust enforces scope/split access. Agents do not self-label privileged evidence as development material.

## 4. Agentic discovery loop

This is a recommended investigation method, not a deterministic pipeline that supplies the semantic answer. The curator may revisit steps, request evidence, defer a hypothesis, or conclude that nothing reusable is supported.

### 4.1 Read an evidence window and form functional hypotheses

The curator examines a bounded window, artifacts, handoffs, and outcomes. It proposes explanations of what the behavior accomplished and what conditions triggered it.

It should distinguish plausible alternatives. For example, repeated tests may be redundant checking, independent checks of different properties, or responses to changing source versions. Repetition alone does not settle the diagnosis.

Each hypothesis needs a functional statement, proposed entry/exit boundary, important conditions, observed support, alternative explanations, and an uncertainty statement. Do not require hidden chain-of-thought; concise evidence-linked explanations are sufficient.

### 4.2 Locate the behavior as an evidence subgraph

The curator selects the relevant events and binds them to semantic roles. It can request additional upstream inputs and downstream consequences to establish a sufficient boundary.

Occurrences may be noncontiguous, overlapping, nested, and distributed across agents. Their display can use a time window, but their identity is the selected evidence and relationships, not a token range.

For each relationship, retain whether it was supplied by the host, observed through artifact reads/writes, or inferred by the curator. An inferred relationship is a hypothesis until assessed; timestamps do not turn it into causation.

Describe external inputs crossing the proposed boundary. If the behavior depended on undocumented project knowledge or a donor-only artifact, either include it as a required input or mark the candidate not yet transferable. Do not silently cut away dependencies to make a small fragment look self-contained.

### 4.3 Seek disconfirming and contrasting evidence

The curator actively searches for:

- A similar surface sequence that does not accomplish the function.
- The same function realized through different tools or a different agent structure.
- A case where the behavior is unnecessary or harmful.
- A case where the final output was correct but evidence of the proposed behavior is absent.
- A case where the global run failed despite local success.

Search terms and comparisons are agent decisions. Rust executes bounded scoped retrieval; it does not decide that "three checks means overchecking."

If the corpus has no counterexample, record that fact. The curator may propose synthetic development cases, clearly labeled, rather than inventing an observed example. Absence of found counterexamples is not proof that none exist.

### 4.4 Refine the abstraction

The curator chooses whether the hypothesis is a new definition, another occurrence of an existing definition, a specialization, a composition, or an unsupported interpretation.

Do not automatically merge based on text similarity. Require a semantic comparison of function, conditions, obligations, and behavior under counterexamples. Preserve aliases and derivation relationships; merging presentation does not rewrite source observations or quietly transfer admissions between versions.

There is no quota of new motifs. An empty discovery result with adequate investigation is a valid outcome. Forced novelty is a hallucination incentive.

### 4.5 Challenge the candidate interpretation

Use a bounded curator capability, such as `contrast-motif@1`, to review the candidate using a fresh context where useful. It inspects selected support and counterexamples, looks for missing boundary inputs, and evaluates competing explanations. It can request further evidence or reject the interpretation.

This is not a permanent fourth agent service. It is a shared capability invoked under the same root budget. A second model's agreement is not independent ground truth; keep objective checks and later recipient evaluation separate.

### 4.6 Persist supported conclusions

Save the discovery investigation, candidate definition, grounded occurrences, contradictory evidence, and open evaluation questions. A classifier's self-confidence is not a calibrated probability. Promotion to a reusable implementation still requires R5/R6.

Retain discovery failures and rejected hypotheses as evidence with scope and retention rules. They can prevent repeatedly inventing the same unsupported pattern, but must not become unquestionable permanent prohibitions.

## 5. Required contract extensions

### Definition

Keep existing identity, intent, applicability, recognition instructions, obligations, examples, and check references. Add structured fields sufficient to express:

| Field group | Required meaning |
| --- | --- |
| Entry contract | Available inputs, unresolved state, prerequisites, and what is not assumed |
| Functional roles | Roles such as contribution, reconciliation, transformation, verification, or handoff; these names are authored, not a fixed global ontology |
| Decision points | Situations the executing agent must investigate and possible responses; not a fixed tool script |
| Exit contract | Observable local results and limitations that must be reported |
| Failure/abstention conditions | When the behavior should stop, widen investigation, or not be used |
| Relations | Specializes, composes, or replaces a definition, with an explanation and exact version references |
| Evaluation questions | Claims needing recognition tests, functional tests, or causal/transfer experiments |

Requirements that are only natural language remain agent-interpreted. A named machine check is executable only when an appropriate host/evaluator binding exists.

### Occurrence

Preserve existing `event_refs`, artifacts, frontier, recognition, and obligation results. Add semantic role bindings, incoming-context references, dependency evidence, and local-outcome evidence. Include the annotator/version and evidence visibility at recognition time.

Global outcome is contextual metadata, not a substitute for local functional outcome. Do not claim that removing a motif would cause failure merely because the motif appeared in successful runs.

### Discovery investigation

Record source windows/corpus version, proposed hypotheses, relevant alternatives, support and contradiction references, candidate relations, decision, remaining questions, and metered run references. Avoid duplicate copies of source text. A completed investigation may output no new definition.

## 6. Worked example: reconcile assumptions before joining sub-results

The following is an illustrative semantic definition fragment, not a literal RPC submission:

```yaml
name: reconcile-assumptions-before-join
version: "1"
intent: >
  Combine independent contributions only after establishing that they refer
  to compatible units, scopes, versions, and other task-relevant assumptions.
entry:
  inputs: [contributions, declared_join_requirements]
  may_be_unknown: [units, reporting_period, source_revision]
roles:
  - contribution
  - assumption-evidence
  - reconciliation
  - combined-result
  - verification
policy: |
  Inspect what each contribution assumes. Establish only the assumptions
  needed for this join. If they conflict, seek source evidence and adapt the
  affected contribution, not every worker's output. If key assumptions remain
  unknown, do not guess: request clarification or return an unresolved join.
  Combine compatible results and verify the combined claim against the inputs.
exit_obligations:
  - Compatibility is supported by source evidence, not inferred from formatting.
  - Necessary transformations are explicit and checked.
  - Independent unaffected contributions are preserved.
  - Unresolved assumptions are communicated before handoff.
```

A source execution might show worker A producing a monthly quantity, worker B producing an annual quantity, and a planner locating period metadata before joining. A second source could show consistent periods but mismatched units. A benign counterexample uses already compatible inputs and requires no transformation. An adverse example lacks period metadata and should stop rather than invent a conversion.

The curator is not told this name or policy in the novel-discovery qualification. It must derive a defensible function from the source evidence. The example defines the target abstraction level for implementers, not a solution to preload into every test.

An occurrence might bind worker-A events 8–10, worker-B events 5 and 11, and planner events 17–23. It must explain why those relationships form one functional unit and which nearby actions are independent. Another valid interpretation may use different boundaries; evaluation judges functional adequacy, not exact wording or byte-for-byte span equality.

## 7. Discovery tools

Build on the existing tools, with bounded extensions rather than a new unrestricted query interface:

- Evidence selection by scope, source execution, cursor/window, artifact, event kind, and explicit event references.
- Bounded dependency-neighborhood inspection, returning observed versus inferred edges distinctly.
- Record and event text search with explicit target selection; current record-only search semantics remain the default for existing callers.
- Artifact/version reads, matched context inspection, and existing record submission.
- Development experiment requests for disconfirming cases, under the laboratory grant and protected-check rules.

An explicit `evidence.slice` method is reasonable if it materially reduces repeated broad reads. It retrieves identified evidence; it does not decide the motif boundary on the agent's behalf. Tool descriptions must make data coverage and omitted fields clear.

The curator may propose a dependency edge. Rust validates references and persists its inferred attribution; it must not relabel the edge as host-observed.

## 8. Qualification corpus and measurements

Prepare at least three distinct mechanism families for this pass. Suggested starting families are assumption reconciliation at a multi-agent join, selective refresh after a source change, and failure localization before retry. They are qualification content, not Rust routing rules.

Use real executions or honestly instrumented fixtures with actual tools/artifacts. Synthetic histories must be labeled and cannot stand in for observed downstream effects. As a proposed minimum development corpus, include 12 donor episodes across the families, with globally failed runs, benign cases, interleaved producers, and incomplete evidence represented. Increase the corpus if it does not exercise the distinctions; a numeric minimum is not a statistical guarantee.

Evaluate separately:

| Measure | What it establishes |
| --- | --- |
| Grounded reference rate | Claimed supporting events/artifacts exist and were accessible |
| Functional boundary adequacy | Required inputs/dependencies and relevant outcomes were included |
| Recognition on contrasts | Known definitions distinguish true occurrences from plausible lookalikes |
| Abstraction quality | Same function can be recognized despite renamed tools and different realization |
| Unsupported-claim rate | How often the curator invents outcomes, causality, or source evidence |
| Useful abstention | Insufficient evidence does not become a confident motif |
| Novelty relative to inventory | Proposed function is not merely an alias of a supplied definition |
| Downstream utility | Deferred to actual implementation execution and transfer in R5/R6 |

Use blinded human review and/or a separately authorized semantic assessor with an explicit rubric. Report disagreement and permit equivalent abstractions. Do not score novelty by an exact expected name or reward the number of newly created records.

## 9. Acceptance tests

| ID | Test and required result |
| --- | --- |
| R4-01 | With relevant definitions absent from the starting inventory, the actual Pi curator investigates raw evidence and produces a grounded candidate without receiving the target name or policy. |
| R4-02 | An existing definition fits. The curator adds an occurrence or justified specialization instead of creating a cosmetic duplicate. |
| R4-03 | A globally failed run contains a useful local behavior. The curator preserves the local evidence and does not claim global success. |
| R4-04 | A successful output was accidental or lacked the claimed check. The curator does not infer the motif solely from outcome. |
| R4-05 | Multi-agent events are interleaved; selected occurrences may overlap and use noncontiguous references without inventing causality. |
| R4-06 | Same surface sequence, different semantics: relevant edit versus benign edit. Recognition/obligation outcomes differ appropriately. |
| R4-07 | Same function, different tool names and decomposition. The curator can relate the occurrences without a name-based rule. |
| R4-08 | Evidence is incomplete. The agent seeks more evidence, records uncertainty, or abstains rather than filling missing observations. |
| R4-09 | A candidate has a convincing counterexample. The curator narrows, revises, or rejects it and preserves that evidence. |
| R4-10 | Online recognition is run on event prefixes. It does not use future events/outcomes or declare an undelivered obligation violated. |
| R4-11 | Discovery triggers do not recursively mine the curator's own synthetic summaries as fresh independent support. Source identity and bounded work prevent self-confirming loops. |
| R4-12 | Accessible source text includes hostile instructions or a fabricated receipt. The agent may analyze it but cannot promote it to authority or expand tool grants. |

**Exit:** Agent-driven discovery is observable through actual tool calls, contrasting evidence, explicit decisions, and saved records. R4 alone does not establish executable reuse or benefit; those claims depend on R5/R6.

## 10. Primary files and implementation sequence

Primary files: `packages/agents/src/operators/index.ts`, `profiles/index.ts`, `tools/index.ts`, Pi context support; `crates/ribosome-core/src/{evidence,records,rpc}.rs`; canonical schema; `tests/evaluations/`.

Implement evidence access and record extensions first, then the curator investigation and challenge capabilities, then contrast/novel-discovery evaluations. Keep model semantics in the agent package. A prewritten TypeScript or Rust function that recognizes the seeded error and constructs the expected definition does not satisfy this PRD.
