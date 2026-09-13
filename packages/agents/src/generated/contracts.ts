// Generated from contracts/schema.json. Run npm run generate.
export type Mode = "observe" | "sandbox" | "apply";
export type Profile = "caretaker" | "curator" | "experimenter";
export type Origin = "observed" | "reexecuted" | "synthetic";
export type Split = "development" | "evaluation" | "holdout";
export type EffectStatus = "started" | "succeeded" | "failed" | "unknown" | "denied" | "stale";
export type Disposition = "completed" | "abstained" | "failed" | "cancelled" | "exhausted" | "interrupted";
export type RecordKind = "definition" | "occurrence" | "implementation" | "finding" | "intervention" | "memory" | "experiment" | "recommendation" | "admission" | "evaluation" | "regulation" | "transplant" | "obligation" | "discovery";
export type MemoryKind = "working" | "episodic" | "aggregated" | "procedural" | "failure";
export type Recognition = "tentative" | "supported" | "rejected";
export type ObligationState = "open" | "satisfied" | "violated" | "unknown";
export type AdmissionDecision = "accepted" | "rejected" | "inconclusive" | "retired";
export type ExperimentTemplate = "ablation" | "rescue" | "interaction" | "stress" | "transfer";
export interface Scope {
  client: string;
  project: string;
}
export interface VersionRef {
  id: string;
  version: string;
}
export interface ArtifactRef {
  path: string;
  version: string;
}
export interface Provenance {
  origin: Origin;
  source_refs: Array<string>;
  scenario_family: string;
  split: Split;
  limitations: Array<string>;
}
export interface Budget {
  max_calls: number;
  max_tokens: string;
  max_cost_microusd: string;
  max_actions: number;
  max_work_items: number;
  max_depth: number;
  deadline_ms: string;
}
export interface Grant {
  id: string;
  scope: Scope;
  mode: Mode;
  paths: Array<string>;
  tools: Array<string>;
  profiles: Array<Profile>;
  budget: Budget;
  context: string;
  visible_splits: Array<Split>;
  allow_export: boolean;
  required_checks?: Array<string>;
  writable_paths?: Array<string>;
  discovery_corpus?: VersionRef;
  prepared_run?: string;
}
export interface Event {
  id: string;
  scope: Scope;
  run_id: string;
  producer: string;
  sequence: string;
  kind: string;
  timestamp_ms: string;
  parents: Array<string>;
  correlation: string;
  artifacts: Array<ArtifactRef>;
  payload: Record<string, unknown>;
  provenance: Provenance;
}
export interface Dependency {
  source: ArtifactRef;
  dependent: ArtifactRef;
  basis: "host" | "observed" | "inferred";
  evidence_refs: Array<string>;
}
export interface Definition {
  name: string;
  version: string;
  intent: string;
  applicability: string;
  recognition_instructions: string;
  obligations: Array<string>;
  positive_examples: Array<string>;
  counterexamples: Array<string>;
  checks: Array<string>;
  functional_contract?: FunctionalContract;
}
export interface ObligationResult {
  obligation: string;
  state: ObligationState;
  evidence_refs: Array<string>;
}
export interface Occurrence {
  definition: VersionRef;
  execution: string;
  event_refs: Array<string>;
  artifacts: Array<ArtifactRef>;
  frontier: Record<string, unknown>;
  recognition: Recognition;
  obligations: Array<ObligationResult>;
  assumptions: Array<string>;
  operator: string;
  grounding?: OccurrenceGrounding;
}
export interface Implementation {
  name: string;
  version: string;
  motifs: Array<VersionRef>;
  format: "instructions" | "registered_tool";
  material: string;
  parameters: Record<string, unknown>;
  required_capabilities: Array<string>;
  state_assumptions: Array<string>;
  possible_effects: Array<string>;
  failure_behavior: string;
  evaluation_refs: Array<string>;
  instruction_contract?: InstructionContract;
  function?: string;
}
export interface Finding {
  subject: string;
  observation: string;
  interpretation: string;
  evidence_refs: Array<string>;
  uncertainty: Array<string>;
  operator: string;
}
export interface Intervention {
  kind: "repair" | "regeneration" | "recombination" | "chaperone";
  subject: string;
  finding_ref: string;
  read_versions: Array<ArtifactRef>;
  preserve: Array<string>;
  replace: Array<string>;
  invalidate: Array<string>;
  recompute: Array<string>;
  required_checks: Array<string>;
  bindings: Record<string, unknown>;
  requested_effects: Array<string>;
  assumptions: Array<string>;
  fallback: string;
  operator: string;
}
export interface Memory {
  kind: MemoryKind;
  content: string;
  applicability: string;
  evidence_refs: Array<string>;
  counterexamples: Array<string>;
  responses: Array<string>;
  regression_cases: Array<string>;
  expires_ms?: string;
  conflicts: Array<string>;
  supersedes: Array<string>;
}
export interface Experiment {
  name: string;
  template: ExperimentTemplate;
  candidate: VersionRef;
  baseline: VersionRef;
  hypothesis: string;
  case_ids: Array<string>;
  scenario_families: Array<string>;
  feedback: "aggregate" | "full_development";
  model_version: string;
  tool_versions: Array<string>;
  memory_start_refs: Array<string>;
  repetitions: number;
  budget: Budget;
  metrics: Array<string>;
  policy_id: string;
  selection_frozen: boolean;
  variants: Array<Variant>;
  development_cases?: Array<DevelopmentCase>;
  study_objective?: "function" | "system_benefit";
  learning_cost?: LearningCost;
}
export interface Measurement {
  name: string;
  value?: number;
  unit: string;
}
export interface Evaluation {
  experiment_ref: string;
  implementation: VersionRef;
  case_id: string;
  arm: string;
  repetition: number;
  evaluator: string;
  evaluator_version: string;
  policy_id: string;
  passed?: boolean;
  measurements: Array<Measurement>;
  checks: Array<string>;
  memory_namespace: string;
  output: string;
  artifact_refs: Array<ArtifactRef>;
  descriptor?: string;
  allocation_id?: string;
  execution_status?: "completed" | "failed" | "cancelled" | "exhausted" | "not_started";
  usage_complete?: boolean;
  run_refs?: Array<string>;
}
export interface Recommendation {
  implementation: VersionRef;
  context: string;
  decision: AdmissionDecision;
  evaluation_refs: Array<string>;
  rationale: string;
  restrictions: Array<string>;
}
export interface Admission {
  implementation: VersionRef;
  context: string;
  decision: AdmissionDecision;
  evaluation_refs: Array<string>;
  restrictions: Array<string>;
  authority: string;
  policy_id: string;
  supersedes: Array<string>;
}
export interface Regulation {
  conditions: string;
  selected_behavior: string;
  scope: Scope;
  reconsider_when: string;
  evidence_refs: Array<string>;
}
export interface Transplant {
  donor: VersionRef;
  recipient: string;
  bindings: Record<string, unknown>;
  adaptations: Array<string>;
  incompatibilities: Array<string>;
  checks: Array<string>;
  fallback: string;
  invocation_run?: string;
  recipient_entry_refs?: Array<string>;
  result_refs?: Array<string>;
}
export interface Obligation {
  description: string;
  owner: string;
  subject: string;
  state: ObligationState;
  affected_outputs: Array<string>;
  created_ms: string;
  consequence_boundary: string;
  evidence_refs: Array<string>;
}
export interface RecordEnvelope {
  schema_version: "1";
  id: string;
  scope: Scope;
  kind: RecordKind;
  version: string;
  created_ms: string;
  updated_ms: string;
  retired: boolean;
  provenance: Provenance;
  body: Record<string, unknown>;
}
export interface RecordSubmission {
  kind: RecordKind;
  provenance: Provenance;
  body: Record<string, unknown>;
  id?: string;
  expected_version?: string;
}
export interface Action {
  operation_id: string;
  kind: "edit" | "check" | "branch" | "apply" | "execute";
  path?: string;
  expected_version?: string;
  content?: string;
  tool?: string;
  branch_id?: string;
  intervention_ref?: string;
  implementation?: VersionRef;
}
export interface ActionReceipt {
  operation_id: string;
  run_id: string;
  status: EffectStatus;
  action: Action;
  before: Array<ArtifactRef>;
  after: Array<ArtifactRef>;
  output: string;
  side_effects: Array<string>;
  started_ms: string;
  finished_ms?: string;
  elapsed_ms?: string;
  reconciled: boolean;
  evidence_ref?: string;
  outcome_basis?: EffectOutcomeBasis;
  validations?: Array<ValidationEvidence>;
  restored_validity?: Array<ArtifactRef>;
  restored_properties?: Array<ValidatedProperty>;
  settlement?: EffectSettlement;
  content_available?: boolean;
}
export interface Checkpoint {
  format: string;
  profile: Profile;
  operator: string;
  provider: string;
  model: string;
  messages: Array<Record<string, unknown>>;
  pending_operations: Array<string>;
  event_cursor: string;
  context?: ContextState;
}
export interface AgentRunRequest {
  run_id: string;
  profile: Profile;
  operator: string;
  prompt: string;
  provider: string;
  model: string;
  checkpoint?: Checkpoint;
  parent_allocation_id?: string;
  discovery_corpus?: VersionRef;
  invocation?: ImplementationInvocation;
}
export interface AgentResult {
  disposition: Disposition;
  summary: string;
}
export interface Handshake {
  protocol: "ribosome/1";
  build: "0.1.0";
  pi: "0.85.1";
  session: string;
  capabilities: Array<string>;
}
export interface Empty {
}
export interface IdRequest {
  id: string;
}
export interface Ok {
  ok: boolean;
}
export interface EvidenceRequest {
  cursor: string;
  limit: number;
  run_id?: string;
  event_refs?: Array<string>;
  neighbors?: boolean;
  kind?: string;
  artifact?: ArtifactRef;
  query?: string;
  through_cursor?: string;
}
export interface EvidencePage {
  events: Array<Event>;
  cursor: string;
  frontier: Record<string, unknown>;
  dependencies: Array<Dependency>;
  invalidated_paths: Array<string>;
}
export interface SearchRequest {
  query: string;
  kind?: RecordKind;
  inventory: "evidence" | "usable";
  limit: number;
  offset: number;
  query_mode?: "all_terms" | "any_terms";
  order?: "id" | "relevance";
  eligible?: boolean;
  function?: string;
  after?: string;
}
export interface RecordPage {
  records: Array<RecordEnvelope>;
  next_offset: number;
  next?: string;
  complete?: boolean;
}
export interface ArtifactRead {
  path: string;
  offset: number;
  length: number;
  branch_id?: string;
  snapshot_id?: string;
  required_freshness?: Freshness;
}
export interface ArtifactChunk {
  artifact: ArtifactRef;
  content: string;
  offset: number;
  total_bytes: string;
  eof: boolean;
  snapshot_id?: string;
  required_freshness?: Freshness;
}
export interface WorkRequest {
  subject: string;
  profile: Profile;
  operator: string;
  reason: string;
  evidence_refs: Array<string>;
}
export interface WorkItem {
  id: string;
  scope: Scope;
  subject: string;
  profile: Profile;
  operator: string;
  reason: string;
  evidence_refs: Array<string>;
  root_id: string;
  parent_id: string;
  depth: number;
  status: WorkItemStatus;
  attempts: number;
  lease_until_ms: string;
  owner: string;
}
export interface MessageSend {
  recipient: string;
  topic: string;
  body: string;
  correlation: string;
}
export interface Message {
  id: string;
  sender: string;
  recipient: string;
  topic: string;
  body: string;
  correlation: string;
  sequence: string;
  attempts: number;
}
export interface Inbox {
  messages: Array<Message>;
}
export interface RetireRequest {
  id: string;
  expected_version: string;
  delete: boolean;
}
export interface PermitRequest {
  max_output_tokens: number;
  input_tokens_bound: string;
  cost_microusd_bound: string;
  compaction_id?: string;
  call_id?: string;
}
export interface Permit {
  id: string;
  max_output_tokens: number;
}
export interface Usage {
  permit_id: string;
  input_tokens: string;
  output_tokens: string;
  cost_microusd: string;
  complete: boolean;
}
export interface ExperimentRequest {
  id: string;
}
export interface ExperimentResult {
  evaluation_refs: Array<string>;
  decision: AdmissionDecision;
  summary: string;
  allocation_id?: string;
  complete?: boolean;
  planned_evaluations?: number;
  usage_complete?: boolean;
  report?: Record<string, unknown>;
}
export interface AdmissionRequest {
  recommendation_id: string;
}
export interface ExportRequest {
  record_ids: Array<string>;
  product: Origin;
}
export interface ExportResult {
  path: string;
  count: number;
  artifact: ArtifactRef;
}
export interface Variant {
  arm: string;
  implementation: VersionRef;
}
export interface EvaluationTask {
  experiment_id: string;
  implementation: Implementation;
  case_id: string;
  case_input: Record<string, unknown>;
  arm: string;
  repetition: number;
  memory_namespace: string;
  budget: Budget;
  memory_start: Array<RecordEnvelope>;
  allocation_id: string;
  implementation_ref?: VersionRef;
}
export interface EvaluationObservation {
  measurements: Array<Measurement>;
  checks: Array<string>;
  output: string;
  passed?: boolean;
  descriptor?: string;
}
export interface ArchiveCell {
  context: string;
  policy_id: string;
  cell: string;
  implementation: RecordEnvelope;
  quality: number;
  evaluation_id: string;
  admission_ref?: string;
  evaluation_refs?: Array<string>;
  implementation_version?: string;
  limitations?: Array<string>;
}
export interface ArchivePage {
  cells: Array<ArchiveCell>;
}
export interface AgentActivity {
  sequence: string;
  role: "user" | "assistant" | "tool_result";
  timestamp_ms: string;
  content: Record<string, unknown>;
  source_refs: Array<string>;
}
export interface AgentActivityBatch {
  entries: Array<AgentActivity>;
}
export interface DevelopmentCase {
  id: string;
  source_mechanism: string;
  provenance: Provenance;
  input: Record<string, unknown>;
  candidate_checks: Array<string>;
}
export type AttachmentCapability = "observe" | "steer" | "coordinated_write";
export type AttachmentState = "active" | "interrupted" | "detached" | "completed";
export type FeedbackState = "pending" | "delivered" | "acknowledged" | "rejected" | "expired" | "unknown";
export interface HostHelloRequest {
  protocol: "ribosome-host/1";
}
export interface HostHello {
  protocol: "ribosome-host/1";
  scope: Scope;
  capabilities: Array<AttachmentCapability>;
}
export interface AttachmentOpen {
  id: string;
  execution_id: string;
  connector: string;
  connector_version: string;
  start: "now" | "history";
  capabilities: Array<AttachmentCapability>;
}
export interface RepairHandoff {
  generation: string;
  work_id: string;
  deadline_ms: string;
  state: "held" | "released" | "unknown";
}
export interface Attachment {
  id: string;
  grant_id: string;
  scope: Scope;
  execution_id: string;
  connector: string;
  connector_version: string;
  capabilities: Array<AttachmentCapability>;
  state: AttachmentState;
  source_status: "running" | "completed" | "failed" | "cancelled" | "unknown";
  start_cursor: string;
  cursor: string;
  frontier: Record<string, unknown>;
  coverage: Array<string>;
  created_ms: string;
  updated_ms: string;
  last_error: string;
  handoff?: RepairHandoff;
}
export interface HarnessEvent {
  id: string;
  producer: string;
  sequence: string;
  kind: string;
  timestamp_ms: string;
  parents: Array<string>;
  correlation: string;
  artifacts: Array<ArtifactRef>;
  payload: Record<string, unknown>;
}
export interface AttachmentEvents {
  attachment_id: string;
  events: Array<HarnessEvent>;
}
export interface IngestionReceipt {
  inserted: number;
  duplicates: number;
  cursor: string;
}
export interface AttachmentRequest {
  attachment_id: string;
}
export interface AttachmentStatus {
  attachment: Attachment;
  queued_work: number;
  running_work: number;
  pending_feedback: number;
  usage: Record<string, unknown>;
  feedback_states: Record<string, unknown>;
}
export interface AttachmentFeedback {
  id: string;
  attachment_id: string;
  run_id: string;
  kind: "finding" | "proposal";
  state: FeedbackState;
  summary: string;
  disposition: Disposition;
  record_refs: Array<string>;
  evidence_refs: Array<string>;
  artifact_versions: Array<ArtifactRef>;
  expires_ms: string;
  attempts: number;
  detail: string;
}
export interface AttachmentFeedbackPage {
  items: Array<AttachmentFeedback>;
}
export interface FeedbackAcknowledgement {
  attachment_id: string;
  feedback_id: string;
  outcome: "acknowledged" | "rejected" | "unknown";
  detail: string;
}
export interface FeedbackRequest {
  attachment_id: string;
  feedback_id: string;
}
export interface AttachmentRecordRequest {
  attachment_id: string;
  record_id: string;
}
export interface AttachmentInterruption {
  attachment_id: string;
  reason: string;
}
export interface AttachmentRelease {
  attachment_id: string;
  generation: string;
}
export interface ContextSource {
  kind: "record" | "event" | "artifact";
  id: string;
  version: string;
}
export interface ContextState {
  segment_id: string;
  count: string;
  generation: string;
  rebuilt: boolean;
  tail_after: string;
  summary_ref?: string;
  summary_through?: string;
}
export interface ContextEntry {
  message: Record<string, unknown>;
  sources: Array<ContextSource>;
}
export interface ContextAppend {
  segment_id: string;
  after: string;
  entries: Array<ContextEntry>;
}
export interface ContextRead {
  segment_id: string;
  after: string;
  limit: number;
}
export interface ContextPage {
  messages: Array<Record<string, unknown>>;
  next: string;
  complete: boolean;
}
export type Freshness = "current" | "historical";
export interface CompactionPlan {
  id: string;
  segment_id: string;
  after: string;
  through: string;
}
export interface CompactionPreparation {
  plan?: CompactionPlan;
}
export interface ContextSummary {
  id: string;
  through: string;
  text: string;
  sources: Array<ContextSource>;
}
export interface CompactionInput {
  plan: CompactionPlan;
  owner_task: string;
  previous_summary?: ContextSummary;
  messages: Array<Record<string, unknown>>;
}
export interface CompactionCommit {
  id: string;
  text: string;
  permit_id: string;
}
export interface ToolCall {
  call_id: string;
  method: "evidence.read" | "search.query" | "record.read" | "artifact.read" | "action.execute" | "action.lookup" | "record.submit" | "record.retire" | "work.request" | "message.send" | "message.inbox" | "message.ack" | "experiment.run" | "inventory.admission_request" | "inventory.archive" | "training.export" | "artifact.validity" | "work.wait" | "work.status" | "evidence.corpus" | "continuation.read";
  arguments: Record<string, unknown>;
}
export interface ToolObservation {
  content: string;
  artifact: ArtifactRef;
  sources: Array<ContextSource>;
  total_bytes: string;
  cursor?: string;
}
export type EffectOutcomeBasis = "execution_established" | "current_postcondition_observed" | "unresolved" | "not_dispatched";
export interface ValidationEvidence {
  check_ref: string;
  checker_version: string;
  policy_version: string;
  authority: string;
  receipt_ref: string;
  inputs: Array<ArtifactRef>;
  targets: Array<ArtifactRef>;
  outcome: "passed" | "failed" | "stale";
  branch_id?: string;
  generation: string;
  properties?: Array<ValidatedProperty>;
}
export interface PropertyBinding {
  obligation: VersionRef;
  path: string;
}
export interface ValidatedProperty {
  obligation: VersionRef;
  artifact: ArtifactRef;
}
export interface PropertyAssessment {
  obligation: VersionRef;
  state: "unproven" | "validated" | "stale";
  evidence_refs: Array<string>;
}
export interface ArtifactValidityRequest {
  path: string;
}
export interface ArtifactValidity {
  artifact: ArtifactRef;
  snapshot_id: string;
  observed_ms: string;
  generation: string;
  properties: Array<PropertyAssessment>;
}
export interface EffectSettlementRequest {
  operation_id: string;
  expected_receipt_version: string;
  executor_stopped: true;
  workspace_versions: Array<ArtifactRef>;
  reason: string;
  source_refs: Array<string>;
}
export interface EffectSettlement {
  request: EffectSettlementRequest;
  evidence_ref: string;
  recorded_ms: string;
}
export interface EffectInspection {
  receipt: ActionReceipt;
  workspace_versions: Array<ArtifactRef>;
  receipt_version: string;
}
export interface BudgetAllocationRequest {
  id: string;
  parent_id: string;
  cause_id: string;
  purpose: string;
  budget: Budget;
}
export interface BudgetAllocation {
  id: string;
  grant_id: string;
  parent_id?: string;
  cause_id: string;
  purpose: string;
  budget: Budget;
  disposition?: Disposition;
}
export interface BudgetUsage {
  model_calls: number;
  undispatched_calls: number;
  unknown_calls: number;
  settled_tokens: string;
  reserved_tokens: string;
  settled_cost_microusd: string;
  reserved_cost_microusd: string;
  actions: number;
  work_items: number;
}
export interface BudgetStatus {
  allocation: BudgetAllocation;
  usage: BudgetUsage;
  remaining: Budget;
}
export type WorkItemStatus = "queued" | "running" | "completed" | "failed" | "cancelled" | "exhausted" | "interrupted";
export interface WorkWaitRequest {
  work_ids: Array<string>;
}
export interface WorkWaitResult {
  work_ids: Array<string>;
  wait_required: boolean;
}
export interface WorkStatus {
  work_id: string;
  status: WorkItemStatus;
  result_available: boolean;
  result?: AgentResult;
  source?: ContextSource;
}
export interface SessionParkRequest {
  work_ids: Array<string>;
  checkpoint: Checkpoint;
}
export interface MotifEntryContract {
  inputs: Array<string>;
  unresolved_state: Array<string>;
  prerequisites: Array<string>;
  not_assumed: Array<string>;
}
export interface MotifRole {
  name: string;
  description: string;
}
export interface MotifDecisionPoint {
  condition: string;
  evidence_required: Array<string>;
  responses: Array<string>;
}
export interface MotifExitContract {
  results: Array<string>;
  required_reports: Array<string>;
}
export interface DefinitionRelation {
  kind: "specializes" | "composes" | "replaces";
  definition: VersionRef;
  explanation: string;
}
export interface MotifEvaluationQuestion {
  kind: "recognition" | "function" | "causal" | "transfer";
  question: string;
}
export interface FunctionalContract {
  entry: MotifEntryContract;
  roles: Array<MotifRole>;
  decision_points: Array<MotifDecisionPoint>;
  exit: MotifExitContract;
  abstention_conditions: Array<string>;
  relations: Array<DefinitionRelation>;
  evaluation_questions: Array<MotifEvaluationQuestion>;
}
export interface MotifRoleBinding {
  role: string;
  event_refs: Array<string>;
}
export interface MotifDependencyEvidence {
  source_event_ref: string;
  target_event_ref: string;
  basis: "source_reported" | "inferred";
  evidence_refs: Array<string>;
  explanation: string;
}
export interface MotifLocalOutcome {
  state: "satisfied" | "violated" | "unresolved";
  evidence_refs: Array<string>;
  limitations: Array<string>;
}
export interface MotifAnnotator {
  run_id: string;
  operator: string;
}
export interface OccurrenceGrounding {
  role_bindings: Array<MotifRoleBinding>;
  incoming_context_refs: Array<string>;
  dependency_evidence: Array<MotifDependencyEvidence>;
  local_outcome: MotifLocalOutcome;
  annotator: MotifAnnotator;
  recognition_visibility: "retrospective" | "online";
}
export interface DiscoveryWindow {
  execution: string;
  event_refs: Array<string>;
  frontier: Record<string, unknown>;
}
export interface DiscoveryAlternative {
  claim: string;
  evidence_refs: Array<string>;
}
export interface DiscoveryHypothesis {
  claim: string;
  entry_boundary: string;
  exit_boundary: string;
  conditions: Array<string>;
  support_refs: Array<string>;
  contradiction_refs: Array<string>;
  alternatives: Array<DiscoveryAlternative>;
  decision: "propose_definition" | "recognize_existing" | "specialize" | "compose" | "reject" | "inconclusive";
  uncertainty: string;
}
export interface Discovery {
  work_ref?: string;
  corpus: VersionRef;
  source_windows: Array<DiscoveryWindow>;
  hypotheses: Array<DiscoveryHypothesis>;
  definition_refs: Array<VersionRef>;
  occurrence_refs: Array<string>;
  decision: "supported" | "rejected" | "inconclusive" | "no_motif";
  open_questions: Array<string>;
  run_refs: Array<string>;
}
export interface DiscoveryCorpus {
  id: string;
  version: string;
  visibility: "retrospective" | "online";
  source_windows: Array<DiscoveryWindow>;
  definition_refs: Array<VersionRef>;
  limitations: Array<string>;
  artifacts: Array<CorpusArtifact>;
  dependencies: Array<Dependency>;
}
export interface CorpusArtifact {
  artifact: ArtifactRef;
  snapshot_id: string;
}
export type ContinuationKind = "obligation" | "effect" | "work";
export interface ContinuationRead {
  kind: ContinuationKind;
  after: string;
  limit: number;
}
export interface ContinuationReference {
  id: string;
  version?: string;
}
export interface ContinuationPage {
  kind: ContinuationKind;
  references: Array<ContinuationReference>;
  next: string;
  complete: boolean;
  evidence_cursor: string;
}
export interface BindingSlot {
  name: string;
  kind: "string" | "number" | "boolean" | "object" | "array" | "artifact_path" | "tool";
  required: boolean;
}
export interface InstructionContract {
  inputs: Array<BindingSlot>;
  outputs: Array<string>;
  entry_obligations: Array<string>;
  exit_obligations: Array<string>;
  limitations: Array<string>;
  discovery_refs: Array<VersionRef>;
}
export interface ImplementationInvocation {
  implementation: VersionRef;
  bindings: Record<string, unknown>;
  recipient_refs: Array<string>;
  purpose: "production" | "experimental";
}
export interface InvocationMaterial {
  invocation: ImplementationInvocation;
  implementation: Implementation;
}
export interface LearningCost {
  cost_microusd: string;
  reuse_count: number;
  complete: boolean;
}
export interface RpcMethods {
  "bridge.hello": { input: Handshake; output: Handshake };
  "agent.run": { input: AgentRunRequest; output: AgentResult };
  "agent.cancel": { input: Empty; output: Ok };
  "agent.steer": { input: MessageSend; output: Ok };
  "evidence.read": { input: EvidenceRequest; output: EvidencePage };
  "search.query": { input: SearchRequest; output: RecordPage };
  "record.read": { input: IdRequest; output: RecordEnvelope };
  "artifact.read": { input: ArtifactRead; output: ArtifactChunk };
  "action.execute": { input: Action; output: ActionReceipt };
  "action.lookup": { input: IdRequest; output: ActionReceipt };
  "work.request": { input: WorkRequest; output: WorkItem };
  "record.submit": { input: RecordSubmission; output: RecordEnvelope };
  "record.retire": { input: RetireRequest; output: Ok };
  "session.grant": { input: Empty; output: Grant };
  "session.checkpoint": { input: Checkpoint; output: Ok };
  "model.permit": { input: PermitRequest; output: Permit };
  "model.usage": { input: Usage; output: Ok };
  "message.send": { input: MessageSend; output: Message };
  "message.inbox": { input: Empty; output: Inbox };
  "message.ack": { input: IdRequest; output: Ok };
  "experiment.run": { input: ExperimentRequest; output: ExperimentResult };
  "inventory.admission_request": { input: AdmissionRequest; output: RecordEnvelope };
  "training.export": { input: ExportRequest; output: ExportResult };
  "inventory.archive": { input: Empty; output: ArchivePage };
  "session.events": { input: AgentActivityBatch; output: Ok };
  "session.context": { input: Empty; output: ContextState };
  "session.context.append": { input: ContextAppend; output: ContextState };
  "session.context.read": { input: ContextRead; output: ContextPage };
  "session.compaction.prepare": { input: Empty; output: CompactionPreparation };
  "session.compaction.read": { input: IdRequest; output: CompactionInput };
  "session.compaction.commit": { input: CompactionCommit; output: ContextSummary };
  "session.summary": { input: IdRequest; output: ContextSummary };
  "tool.call": { input: ToolCall; output: ToolObservation };
  "tool.result": { input: IdRequest; output: ToolObservation };
  "artifact.validity": { input: ArtifactValidityRequest; output: ArtifactValidity };
  "model.dispatch": { input: IdRequest; output: Ok };
  "model.release": { input: IdRequest; output: Ok };
  "budget.status": { input: Empty; output: BudgetStatus };
  "model.permit.lookup": { input: IdRequest; output: Permit };
  "work.wait": { input: WorkWaitRequest; output: WorkWaitResult };
  "work.status": { input: IdRequest; output: WorkStatus };
  "session.park": { input: SessionParkRequest; output: Ok };
  "evidence.corpus": { input: Empty; output: DiscoveryCorpus };
  "continuation.read": { input: ContinuationRead; output: ContinuationPage };
  "invocation.read": { input: Empty; output: InvocationMaterial };
}
export interface HostRpcMethods {
  "host.hello": { input: HostHelloRequest; output: HostHello };
  "attachment.open": { input: AttachmentOpen; output: Attachment };
  "attachment.events": { input: AttachmentEvents; output: IngestionReceipt };
  "attachment.status": { input: AttachmentRequest; output: AttachmentStatus };
  "attachment.feedback": { input: AttachmentRequest; output: AttachmentFeedbackPage };
  "attachment.ack": { input: FeedbackAcknowledgement; output: Ok };
  "attachment.steer": { input: FeedbackRequest; output: Ok };
  "attachment.record": { input: AttachmentRecordRequest; output: RecordEnvelope };
  "attachment.complete": { input: AttachmentRequest; output: Ok };
  "attachment.detach": { input: AttachmentRequest; output: Ok };
  "attachment.interrupt": { input: AttachmentInterruption; output: Ok };
  "attachment.repair": { input: AttachmentRequest; output: RepairHandoff };
  "attachment.release": { input: AttachmentRelease; output: Ok };
  "effect.inspect": { input: IdRequest; output: EffectInspection };
  "effect.settle": { input: EffectSettlementRequest; output: ActionReceipt };
}
