// Generated from contracts/schema.json. Run npm run generate.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub client: String,
    pub project: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRef {
    pub id: String,
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub path: String,
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub origin: Origin,
    pub source_refs: Vec<String>,
    pub scenario_family: String,
    pub split: Split,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub max_calls: u32,
    pub max_tokens: String,
    pub max_cost_microusd: String,
    pub max_actions: u32,
    pub max_work_items: u32,
    pub max_depth: u32,
    pub deadline_ms: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub id: String,
    pub scope: Scope,
    pub mode: Mode,
    pub paths: Vec<String>,
    pub tools: Vec<String>,
    pub profiles: Vec<Profile>,
    pub budget: Budget,
    pub context: String,
    pub visible_splits: Vec<Split>,
    pub allow_export: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_checks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writable_paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_corpus: Option<VersionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_run: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: String,
    pub scope: Scope,
    pub run_id: String,
    pub producer: String,
    pub sequence: String,
    pub kind: String,
    pub timestamp_ms: String,
    pub parents: Vec<String>,
    pub correlation: String,
    pub artifacts: Vec<ArtifactRef>,
    pub payload: serde_json::Map<String, serde_json::Value>,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub source: ArtifactRef,
    pub dependent: ArtifactRef,
    pub basis: DependencyBasis,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub version: String,
    pub intent: String,
    pub applicability: String,
    pub recognition_instructions: String,
    pub obligations: Vec<String>,
    pub positive_examples: Vec<String>,
    pub counterexamples: Vec<String>,
    pub checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub functional_contract: Option<FunctionalContract>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationResult {
    pub obligation: String,
    pub state: ObligationState,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occurrence {
    pub definition: VersionRef,
    pub execution: String,
    pub event_refs: Vec<String>,
    pub artifacts: Vec<ArtifactRef>,
    pub frontier: serde_json::Map<String, serde_json::Value>,
    pub recognition: Recognition,
    pub obligations: Vec<ObligationResult>,
    pub assumptions: Vec<String>,
    pub operator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding: Option<OccurrenceGrounding>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Implementation {
    pub name: String,
    pub version: String,
    pub motifs: Vec<VersionRef>,
    pub format: ImplementationFormat,
    pub material: String,
    pub parameters: serde_json::Map<String, serde_json::Value>,
    pub required_capabilities: Vec<String>,
    pub state_assumptions: Vec<String>,
    pub possible_effects: Vec<String>,
    pub failure_behavior: String,
    pub evaluation_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_contract: Option<InstructionContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub subject: String,
    pub observation: String,
    pub interpretation: String,
    pub evidence_refs: Vec<String>,
    pub uncertainty: Vec<String>,
    pub operator: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intervention {
    pub kind: InterventionKind,
    pub subject: String,
    pub finding_ref: String,
    pub read_versions: Vec<ArtifactRef>,
    pub preserve: Vec<String>,
    pub replace: Vec<String>,
    pub invalidate: Vec<String>,
    pub recompute: Vec<String>,
    pub required_checks: Vec<String>,
    pub bindings: serde_json::Map<String, serde_json::Value>,
    pub requested_effects: Vec<String>,
    pub assumptions: Vec<String>,
    pub fallback: String,
    pub operator: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub kind: MemoryKind,
    pub content: String,
    pub applicability: String,
    pub evidence_refs: Vec<String>,
    pub counterexamples: Vec<String>,
    pub responses: Vec<String>,
    pub regression_cases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_ms: Option<String>,
    pub conflicts: Vec<String>,
    pub supersedes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Experiment {
    pub name: String,
    pub template: ExperimentTemplate,
    pub candidate: VersionRef,
    pub baseline: VersionRef,
    pub hypothesis: String,
    pub case_ids: Vec<String>,
    pub scenario_families: Vec<String>,
    pub feedback: ExperimentFeedback,
    pub model_version: String,
    pub tool_versions: Vec<String>,
    pub memory_start_refs: Vec<String>,
    pub repetitions: u32,
    pub budget: Budget,
    pub metrics: Vec<String>,
    pub policy_id: String,
    pub selection_frozen: bool,
    pub variants: Vec<Variant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_cases: Option<Vec<DevelopmentCase>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study_objective: Option<ExperimentStudyObjective>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learning_cost: Option<LearningCost>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    pub unit: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub experiment_ref: String,
    pub implementation: VersionRef,
    pub case_id: String,
    pub arm: String,
    pub repetition: u32,
    pub evaluator: String,
    pub evaluator_version: String,
    pub policy_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<bool>,
    pub measurements: Vec<Measurement>,
    pub checks: Vec<String>,
    pub memory_namespace: String,
    pub output: String,
    pub artifact_refs: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_status: Option<EvaluationExecutionStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_refs: Option<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recommendation {
    pub implementation: VersionRef,
    pub context: String,
    pub decision: AdmissionDecision,
    pub evaluation_refs: Vec<String>,
    pub rationale: String,
    pub restrictions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    pub implementation: VersionRef,
    pub context: String,
    pub decision: AdmissionDecision,
    pub evaluation_refs: Vec<String>,
    pub restrictions: Vec<String>,
    pub authority: String,
    pub policy_id: String,
    pub supersedes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regulation {
    pub conditions: String,
    pub selected_behavior: String,
    pub scope: Scope,
    pub reconsider_when: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transplant {
    pub donor: VersionRef,
    pub recipient: String,
    pub bindings: serde_json::Map<String, serde_json::Value>,
    pub adaptations: Vec<String>,
    pub incompatibilities: Vec<String>,
    pub checks: Vec<String>,
    pub fallback: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient_entry_refs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_refs: Option<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    pub description: String,
    pub owner: String,
    pub subject: String,
    pub state: ObligationState,
    pub affected_outputs: Vec<String>,
    pub created_ms: String,
    pub consequence_boundary: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordEnvelope {
    pub schema_version: String,
    pub id: String,
    pub scope: Scope,
    pub kind: RecordKind,
    pub version: String,
    pub created_ms: String,
    pub updated_ms: String,
    pub retired: bool,
    pub provenance: Provenance,
    pub body: serde_json::Map<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSubmission {
    pub kind: RecordKind,
    pub provenance: Provenance,
    pub body: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub operation_id: String,
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intervention_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<VersionRef>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReceipt {
    pub operation_id: String,
    pub run_id: String,
    pub status: EffectStatus,
    pub action: Action,
    pub before: Vec<ArtifactRef>,
    pub after: Vec<ArtifactRef>,
    pub output: String,
    pub side_effects: Vec<String>,
    pub started_ms: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<String>,
    pub reconciled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_basis: Option<EffectOutcomeBasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validations: Option<Vec<ValidationEvidence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_validity: Option<Vec<ArtifactRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_properties: Option<Vec<ValidatedProperty>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement: Option<EffectSettlement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_available: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub format: String,
    pub profile: Profile,
    pub operator: String,
    pub provider: String,
    pub model: String,
    pub messages: Vec<serde_json::Map<String, serde_json::Value>>,
    pub pending_operations: Vec<String>,
    pub event_cursor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextState>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRunRequest {
    pub run_id: String,
    pub profile: Profile,
    pub operator: String,
    pub prompt: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Checkpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_allocation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_corpus: Option<VersionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<ImplementationInvocation>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentResult {
    pub disposition: Disposition,
    pub summary: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handshake {
    pub protocol: String,
    pub build: String,
    pub pi: String,
    pub session: String,
    pub capabilities: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdRequest {
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ok {
    pub ok: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRequest {
    pub cursor: String,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_refs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neighbors: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePage {
    pub events: Vec<Event>,
    pub cursor: String,
    pub frontier: serde_json::Map<String, serde_json::Value>,
    pub dependencies: Vec<Dependency>,
    pub invalidated_paths: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<RecordKind>,
    pub inventory: SearchRequestInventory,
    pub limit: u32,
    pub offset: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_mode: Option<SearchRequestQueryMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<SearchRequestOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eligible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordPage {
    pub records: Vec<RecordEnvelope>,
    pub next_offset: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRead {
    pub path: String,
    pub offset: u32,
    pub length: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_freshness: Option<Freshness>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactChunk {
    pub artifact: ArtifactRef,
    pub content: String,
    pub offset: u32,
    pub total_bytes: String,
    pub eof: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_freshness: Option<Freshness>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkRequest {
    pub subject: String,
    pub profile: Profile,
    pub operator: String,
    pub reason: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItem {
    pub id: String,
    pub scope: Scope,
    pub subject: String,
    pub profile: Profile,
    pub operator: String,
    pub reason: String,
    pub evidence_refs: Vec<String>,
    pub root_id: String,
    pub parent_id: String,
    pub depth: u32,
    pub status: WorkItemStatus,
    pub attempts: u32,
    pub lease_until_ms: String,
    pub owner: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageSend {
    pub recipient: String,
    pub topic: String,
    pub body: String,
    pub correlation: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub recipient: String,
    pub topic: String,
    pub body: String,
    pub correlation: String,
    pub sequence: String,
    pub attempts: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inbox {
    pub messages: Vec<Message>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetireRequest {
    pub id: String,
    pub expected_version: String,
    pub delete: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermitRequest {
    pub max_output_tokens: u32,
    pub input_tokens_bound: String,
    pub cost_microusd_bound: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permit {
    pub id: String,
    pub max_output_tokens: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub permit_id: String,
    pub input_tokens: String,
    pub output_tokens: String,
    pub cost_microusd: String,
    pub complete: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentRequest {
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentResult {
    pub evaluation_refs: Vec<String>,
    pub decision: AdmissionDecision,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_evaluations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<serde_json::Map<String, serde_json::Value>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRequest {
    pub recommendation_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub record_ids: Vec<String>,
    pub product: Origin,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportResult {
    pub path: String,
    pub count: u32,
    pub artifact: ArtifactRef,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub arm: String,
    pub implementation: VersionRef,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationTask {
    pub experiment_id: String,
    pub implementation: Implementation,
    pub case_id: String,
    pub case_input: serde_json::Map<String, serde_json::Value>,
    pub arm: String,
    pub repetition: u32,
    pub memory_namespace: String,
    pub budget: Budget,
    pub memory_start: Vec<RecordEnvelope>,
    pub allocation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_ref: Option<VersionRef>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationObservation {
    pub measurements: Vec<Measurement>,
    pub checks: Vec<String>,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveCell {
    pub context: String,
    pub policy_id: String,
    pub cell: String,
    pub implementation: RecordEnvelope,
    pub quality: f64,
    pub evaluation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_refs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limitations: Option<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivePage {
    pub cells: Vec<ArchiveCell>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentActivity {
    pub sequence: String,
    pub role: AgentActivityRole,
    pub timestamp_ms: String,
    pub content: serde_json::Map<String, serde_json::Value>,
    pub source_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentActivityBatch {
    pub entries: Vec<AgentActivity>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentCase {
    pub id: String,
    pub source_mechanism: String,
    pub provenance: Provenance,
    pub input: serde_json::Map<String, serde_json::Value>,
    pub candidate_checks: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostHelloRequest {
    pub protocol: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostHello {
    pub protocol: String,
    pub scope: Scope,
    pub capabilities: Vec<AttachmentCapability>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentOpen {
    pub id: String,
    pub execution_id: String,
    pub connector: String,
    pub connector_version: String,
    pub start: AttachmentOpenStart,
    pub capabilities: Vec<AttachmentCapability>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairHandoff {
    pub generation: String,
    pub work_id: String,
    pub deadline_ms: String,
    pub state: RepairHandoffState,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub id: String,
    pub grant_id: String,
    pub scope: Scope,
    pub execution_id: String,
    pub connector: String,
    pub connector_version: String,
    pub capabilities: Vec<AttachmentCapability>,
    pub state: AttachmentState,
    pub source_status: AttachmentSourceStatus,
    pub start_cursor: String,
    pub cursor: String,
    pub frontier: serde_json::Map<String, serde_json::Value>,
    pub coverage: Vec<String>,
    pub created_ms: String,
    pub updated_ms: String,
    pub last_error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<RepairHandoff>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessEvent {
    pub id: String,
    pub producer: String,
    pub sequence: String,
    pub kind: String,
    pub timestamp_ms: String,
    pub parents: Vec<String>,
    pub correlation: String,
    pub artifacts: Vec<ArtifactRef>,
    pub payload: serde_json::Map<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentEvents {
    pub attachment_id: String,
    pub events: Vec<HarnessEvent>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestionReceipt {
    pub inserted: u32,
    pub duplicates: u32,
    pub cursor: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRequest {
    pub attachment_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentStatus {
    pub attachment: Attachment,
    pub queued_work: u32,
    pub running_work: u32,
    pub pending_feedback: u32,
    pub usage: serde_json::Map<String, serde_json::Value>,
    pub feedback_states: serde_json::Map<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentFeedback {
    pub id: String,
    pub attachment_id: String,
    pub run_id: String,
    pub kind: AttachmentFeedbackKind,
    pub state: FeedbackState,
    pub summary: String,
    pub disposition: Disposition,
    pub record_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub artifact_versions: Vec<ArtifactRef>,
    pub expires_ms: String,
    pub attempts: u32,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentFeedbackPage {
    pub items: Vec<AttachmentFeedback>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackAcknowledgement {
    pub attachment_id: String,
    pub feedback_id: String,
    pub outcome: FeedbackAcknowledgementOutcome,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRequest {
    pub attachment_id: String,
    pub feedback_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRecordRequest {
    pub attachment_id: String,
    pub record_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentInterruption {
    pub attachment_id: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRelease {
    pub attachment_id: String,
    pub generation: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSource {
    pub kind: ContextSourceKind,
    pub id: String,
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextState {
    pub segment_id: String,
    pub count: String,
    pub generation: String,
    pub rebuilt: bool,
    pub tail_after: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_through: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEntry {
    pub message: serde_json::Map<String, serde_json::Value>,
    pub sources: Vec<ContextSource>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextAppend {
    pub segment_id: String,
    pub after: String,
    pub entries: Vec<ContextEntry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRead {
    pub segment_id: String,
    pub after: String,
    pub limit: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPage {
    pub messages: Vec<serde_json::Map<String, serde_json::Value>>,
    pub next: String,
    pub complete: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionPlan {
    pub id: String,
    pub segment_id: String,
    pub after: String,
    pub through: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionPreparation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<CompactionPlan>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSummary {
    pub id: String,
    pub through: String,
    pub text: String,
    pub sources: Vec<ContextSource>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionInput {
    pub plan: CompactionPlan,
    pub owner_task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_summary: Option<ContextSummary>,
    pub messages: Vec<serde_json::Map<String, serde_json::Value>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactionCommit {
    pub id: String,
    pub text: String,
    pub permit_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub call_id: String,
    pub method: ToolCallMethod,
    pub arguments: serde_json::Map<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolObservation {
    pub content: String,
    pub artifact: ArtifactRef,
    pub sources: Vec<ContextSource>,
    pub total_bytes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationEvidence {
    pub check_ref: String,
    pub checker_version: String,
    pub policy_version: String,
    pub authority: String,
    pub receipt_ref: String,
    pub inputs: Vec<ArtifactRef>,
    pub targets: Vec<ArtifactRef>,
    pub outcome: ValidationEvidenceOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    pub generation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<ValidatedProperty>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyBinding {
    pub obligation: VersionRef,
    pub path: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatedProperty {
    pub obligation: VersionRef,
    pub artifact: ArtifactRef,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyAssessment {
    pub obligation: VersionRef,
    pub state: PropertyAssessmentState,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactValidityRequest {
    pub path: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactValidity {
    pub artifact: ArtifactRef,
    pub snapshot_id: String,
    pub observed_ms: String,
    pub generation: String,
    pub properties: Vec<PropertyAssessment>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectSettlementRequest {
    pub operation_id: String,
    pub expected_receipt_version: String,
    pub executor_stopped: bool,
    pub workspace_versions: Vec<ArtifactRef>,
    pub reason: String,
    pub source_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectSettlement {
    pub request: EffectSettlementRequest,
    pub evidence_ref: String,
    pub recorded_ms: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectInspection {
    pub receipt: ActionReceipt,
    pub workspace_versions: Vec<ArtifactRef>,
    pub receipt_version: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetAllocationRequest {
    pub id: String,
    pub parent_id: String,
    pub cause_id: String,
    pub purpose: String,
    pub budget: Budget,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetAllocation {
    pub id: String,
    pub grant_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub cause_id: String,
    pub purpose: String,
    pub budget: Budget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposition: Option<Disposition>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetUsage {
    pub model_calls: u32,
    pub undispatched_calls: u32,
    pub unknown_calls: u32,
    pub settled_tokens: String,
    pub reserved_tokens: String,
    pub settled_cost_microusd: String,
    pub reserved_cost_microusd: String,
    pub actions: u32,
    pub work_items: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetStatus {
    pub allocation: BudgetAllocation,
    pub usage: BudgetUsage,
    pub remaining: Budget,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkWaitRequest {
    pub work_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkWaitResult {
    pub work_ids: Vec<String>,
    pub wait_required: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkStatus {
    pub work_id: String,
    pub status: WorkItemStatus,
    pub result_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<AgentResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ContextSource>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionParkRequest {
    pub work_ids: Vec<String>,
    pub checkpoint: Checkpoint,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifEntryContract {
    pub inputs: Vec<String>,
    pub unresolved_state: Vec<String>,
    pub prerequisites: Vec<String>,
    pub not_assumed: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifRole {
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifDecisionPoint {
    pub condition: String,
    pub evidence_required: Vec<String>,
    pub responses: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifExitContract {
    pub results: Vec<String>,
    pub required_reports: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionRelation {
    pub kind: DefinitionRelationKind,
    pub definition: VersionRef,
    pub explanation: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifEvaluationQuestion {
    pub kind: MotifEvaluationQuestionKind,
    pub question: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionalContract {
    pub entry: MotifEntryContract,
    pub roles: Vec<MotifRole>,
    pub decision_points: Vec<MotifDecisionPoint>,
    pub exit: MotifExitContract,
    pub abstention_conditions: Vec<String>,
    pub relations: Vec<DefinitionRelation>,
    pub evaluation_questions: Vec<MotifEvaluationQuestion>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifRoleBinding {
    pub role: String,
    pub event_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifDependencyEvidence {
    pub source_event_ref: String,
    pub target_event_ref: String,
    pub basis: MotifDependencyEvidenceBasis,
    pub evidence_refs: Vec<String>,
    pub explanation: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifLocalOutcome {
    pub state: MotifLocalOutcomeState,
    pub evidence_refs: Vec<String>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifAnnotator {
    pub run_id: String,
    pub operator: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceGrounding {
    pub role_bindings: Vec<MotifRoleBinding>,
    pub incoming_context_refs: Vec<String>,
    pub dependency_evidence: Vec<MotifDependencyEvidence>,
    pub local_outcome: MotifLocalOutcome,
    pub annotator: MotifAnnotator,
    pub recognition_visibility: OccurrenceGroundingRecognitionVisibility,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryWindow {
    pub execution: String,
    pub event_refs: Vec<String>,
    pub frontier: serde_json::Map<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryAlternative {
    pub claim: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryHypothesis {
    pub claim: String,
    pub entry_boundary: String,
    pub exit_boundary: String,
    pub conditions: Vec<String>,
    pub support_refs: Vec<String>,
    pub contradiction_refs: Vec<String>,
    pub alternatives: Vec<DiscoveryAlternative>,
    pub decision: DiscoveryHypothesisDecision,
    pub uncertainty: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discovery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_ref: Option<String>,
    pub corpus: VersionRef,
    pub source_windows: Vec<DiscoveryWindow>,
    pub hypotheses: Vec<DiscoveryHypothesis>,
    pub definition_refs: Vec<VersionRef>,
    pub occurrence_refs: Vec<String>,
    pub decision: DiscoveryDecision,
    pub open_questions: Vec<String>,
    pub run_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryCorpus {
    pub id: String,
    pub version: String,
    pub visibility: DiscoveryCorpusVisibility,
    pub source_windows: Vec<DiscoveryWindow>,
    pub definition_refs: Vec<VersionRef>,
    pub limitations: Vec<String>,
    pub artifacts: Vec<CorpusArtifact>,
    pub dependencies: Vec<Dependency>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusArtifact {
    pub artifact: ArtifactRef,
    pub snapshot_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationRead {
    pub kind: ContinuationKind,
    pub after: String,
    pub limit: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationReference {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationPage {
    pub kind: ContinuationKind,
    pub references: Vec<ContinuationReference>,
    pub next: String,
    pub complete: bool,
    pub evidence_cursor: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingSlot {
    pub name: String,
    pub kind: BindingSlotKind,
    pub required: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionContract {
    pub inputs: Vec<BindingSlot>,
    pub outputs: Vec<String>,
    pub entry_obligations: Vec<String>,
    pub exit_obligations: Vec<String>,
    pub limitations: Vec<String>,
    pub discovery_refs: Vec<VersionRef>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementationInvocation {
    pub implementation: VersionRef,
    pub bindings: serde_json::Map<String, serde_json::Value>,
    pub recipient_refs: Vec<String>,
    pub purpose: ImplementationInvocationPurpose,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationMaterial {
    pub invocation: ImplementationInvocation,
    pub implementation: Implementation,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningCost {
    pub cost_microusd: String,
    pub reuse_count: u32,
    pub complete: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[serde(rename = "observe")]
    Observe,
    #[serde(rename = "sandbox")]
    Sandbox,
    #[serde(rename = "apply")]
    Apply,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
    #[serde(rename = "caretaker")]
    Caretaker,
    #[serde(rename = "curator")]
    Curator,
    #[serde(rename = "experimenter")]
    Experimenter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    #[serde(rename = "observed")]
    Observed,
    #[serde(rename = "reexecuted")]
    Reexecuted,
    #[serde(rename = "synthetic")]
    Synthetic,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Split {
    #[serde(rename = "development")]
    Development,
    #[serde(rename = "evaluation")]
    Evaluation,
    #[serde(rename = "holdout")]
    Holdout,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectStatus {
    #[serde(rename = "started")]
    Started,
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "denied")]
    Denied,
    #[serde(rename = "stale")]
    Stale,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposition {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "abstained")]
    Abstained,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "exhausted")]
    Exhausted,
    #[serde(rename = "interrupted")]
    Interrupted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordKind {
    #[serde(rename = "definition")]
    Definition,
    #[serde(rename = "occurrence")]
    Occurrence,
    #[serde(rename = "implementation")]
    Implementation,
    #[serde(rename = "finding")]
    Finding,
    #[serde(rename = "intervention")]
    Intervention,
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "experiment")]
    Experiment,
    #[serde(rename = "recommendation")]
    Recommendation,
    #[serde(rename = "admission")]
    Admission,
    #[serde(rename = "evaluation")]
    Evaluation,
    #[serde(rename = "regulation")]
    Regulation,
    #[serde(rename = "transplant")]
    Transplant,
    #[serde(rename = "obligation")]
    Obligation,
    #[serde(rename = "discovery")]
    Discovery,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    #[serde(rename = "working")]
    Working,
    #[serde(rename = "episodic")]
    Episodic,
    #[serde(rename = "aggregated")]
    Aggregated,
    #[serde(rename = "procedural")]
    Procedural,
    #[serde(rename = "failure")]
    Failure,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recognition {
    #[serde(rename = "tentative")]
    Tentative,
    #[serde(rename = "supported")]
    Supported,
    #[serde(rename = "rejected")]
    Rejected,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObligationState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "satisfied")]
    Satisfied,
    #[serde(rename = "violated")]
    Violated,
    #[serde(rename = "unknown")]
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionDecision {
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "inconclusive")]
    Inconclusive,
    #[serde(rename = "retired")]
    Retired,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentTemplate {
    #[serde(rename = "ablation")]
    Ablation,
    #[serde(rename = "rescue")]
    Rescue,
    #[serde(rename = "interaction")]
    Interaction,
    #[serde(rename = "stress")]
    Stress,
    #[serde(rename = "transfer")]
    Transfer,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyBasis {
    #[serde(rename = "host")]
    Host,
    #[serde(rename = "observed")]
    Observed,
    #[serde(rename = "inferred")]
    Inferred,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImplementationFormat {
    #[serde(rename = "instructions")]
    Instructions,
    #[serde(rename = "registered_tool")]
    RegisteredTool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterventionKind {
    #[serde(rename = "repair")]
    Repair,
    #[serde(rename = "regeneration")]
    Regeneration,
    #[serde(rename = "recombination")]
    Recombination,
    #[serde(rename = "chaperone")]
    Chaperone,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentFeedback {
    #[serde(rename = "aggregate")]
    Aggregate,
    #[serde(rename = "full_development")]
    FullDevelopment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentStudyObjective {
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "system_benefit")]
    SystemBenefit,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvaluationExecutionStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "exhausted")]
    Exhausted,
    #[serde(rename = "not_started")]
    NotStarted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    #[serde(rename = "edit")]
    Edit,
    #[serde(rename = "check")]
    Check,
    #[serde(rename = "branch")]
    Branch,
    #[serde(rename = "apply")]
    Apply,
    #[serde(rename = "execute")]
    Execute,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchRequestInventory {
    #[serde(rename = "evidence")]
    Evidence,
    #[serde(rename = "usable")]
    Usable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchRequestQueryMode {
    #[serde(rename = "all_terms")]
    AllTerms,
    #[serde(rename = "any_terms")]
    AnyTerms,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchRequestOrder {
    #[serde(rename = "id")]
    Id,
    #[serde(rename = "relevance")]
    Relevance,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentActivityRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool_result")]
    ToolResult,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentCapability {
    #[serde(rename = "observe")]
    Observe,
    #[serde(rename = "steer")]
    Steer,
    #[serde(rename = "coordinated_write")]
    CoordinatedWrite,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentState {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "interrupted")]
    Interrupted,
    #[serde(rename = "detached")]
    Detached,
    #[serde(rename = "completed")]
    Completed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackState {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "delivered")]
    Delivered,
    #[serde(rename = "acknowledged")]
    Acknowledged,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "expired")]
    Expired,
    #[serde(rename = "unknown")]
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentOpenStart {
    #[serde(rename = "now")]
    Now,
    #[serde(rename = "history")]
    History,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairHandoffState {
    #[serde(rename = "held")]
    Held,
    #[serde(rename = "released")]
    Released,
    #[serde(rename = "unknown")]
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentSourceStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "unknown")]
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentFeedbackKind {
    #[serde(rename = "finding")]
    Finding,
    #[serde(rename = "proposal")]
    Proposal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackAcknowledgementOutcome {
    #[serde(rename = "acknowledged")]
    Acknowledged,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "unknown")]
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextSourceKind {
    #[serde(rename = "record")]
    Record,
    #[serde(rename = "event")]
    Event,
    #[serde(rename = "artifact")]
    Artifact,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Freshness {
    #[serde(rename = "current")]
    Current,
    #[serde(rename = "historical")]
    Historical,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCallMethod {
    #[serde(rename = "evidence.read")]
    EvidenceRead,
    #[serde(rename = "search.query")]
    SearchQuery,
    #[serde(rename = "record.read")]
    RecordRead,
    #[serde(rename = "artifact.read")]
    ArtifactRead,
    #[serde(rename = "action.execute")]
    ActionExecute,
    #[serde(rename = "action.lookup")]
    ActionLookup,
    #[serde(rename = "record.submit")]
    RecordSubmit,
    #[serde(rename = "record.retire")]
    RecordRetire,
    #[serde(rename = "work.request")]
    WorkRequest,
    #[serde(rename = "message.send")]
    MessageSend,
    #[serde(rename = "message.inbox")]
    MessageInbox,
    #[serde(rename = "message.ack")]
    MessageAck,
    #[serde(rename = "experiment.run")]
    ExperimentRun,
    #[serde(rename = "inventory.admission_request")]
    InventoryAdmissionRequest,
    #[serde(rename = "inventory.archive")]
    InventoryArchive,
    #[serde(rename = "training.export")]
    TrainingExport,
    #[serde(rename = "artifact.validity")]
    ArtifactValidity,
    #[serde(rename = "work.wait")]
    WorkWait,
    #[serde(rename = "work.status")]
    WorkStatus,
    #[serde(rename = "evidence.corpus")]
    EvidenceCorpus,
    #[serde(rename = "continuation.read")]
    ContinuationRead,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectOutcomeBasis {
    #[serde(rename = "execution_established")]
    ExecutionEstablished,
    #[serde(rename = "current_postcondition_observed")]
    CurrentPostconditionObserved,
    #[serde(rename = "unresolved")]
    Unresolved,
    #[serde(rename = "not_dispatched")]
    NotDispatched,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationEvidenceOutcome {
    #[serde(rename = "passed")]
    Passed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "stale")]
    Stale,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyAssessmentState {
    #[serde(rename = "unproven")]
    Unproven,
    #[serde(rename = "validated")]
    Validated,
    #[serde(rename = "stale")]
    Stale,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkItemStatus {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "exhausted")]
    Exhausted,
    #[serde(rename = "interrupted")]
    Interrupted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefinitionRelationKind {
    #[serde(rename = "specializes")]
    Specializes,
    #[serde(rename = "composes")]
    Composes,
    #[serde(rename = "replaces")]
    Replaces,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifEvaluationQuestionKind {
    #[serde(rename = "recognition")]
    Recognition,
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "causal")]
    Causal,
    #[serde(rename = "transfer")]
    Transfer,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifDependencyEvidenceBasis {
    #[serde(rename = "source_reported")]
    SourceReported,
    #[serde(rename = "inferred")]
    Inferred,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifLocalOutcomeState {
    #[serde(rename = "satisfied")]
    Satisfied,
    #[serde(rename = "violated")]
    Violated,
    #[serde(rename = "unresolved")]
    Unresolved,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OccurrenceGroundingRecognitionVisibility {
    #[serde(rename = "retrospective")]
    Retrospective,
    #[serde(rename = "online")]
    Online,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryHypothesisDecision {
    #[serde(rename = "propose_definition")]
    ProposeDefinition,
    #[serde(rename = "recognize_existing")]
    RecognizeExisting,
    #[serde(rename = "specialize")]
    Specialize,
    #[serde(rename = "compose")]
    Compose,
    #[serde(rename = "reject")]
    Reject,
    #[serde(rename = "inconclusive")]
    Inconclusive,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryDecision {
    #[serde(rename = "supported")]
    Supported,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "inconclusive")]
    Inconclusive,
    #[serde(rename = "no_motif")]
    NoMotif,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryCorpusVisibility {
    #[serde(rename = "retrospective")]
    Retrospective,
    #[serde(rename = "online")]
    Online,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContinuationKind {
    #[serde(rename = "obligation")]
    Obligation,
    #[serde(rename = "effect")]
    Effect,
    #[serde(rename = "work")]
    Work,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingSlotKind {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "array")]
    Array,
    #[serde(rename = "artifact_path")]
    ArtifactPath,
    #[serde(rename = "tool")]
    Tool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImplementationInvocationPurpose {
    #[serde(rename = "production")]
    Production,
    #[serde(rename = "experimental")]
    Experimental,
}
