// Generated from contracts/schema.json. Run npm run generate.
export type Mode = "observe" | "sandbox" | "apply";
export type Profile = "caretaker" | "curator" | "experimenter";
export type Origin = "observed" | "reexecuted" | "synthetic";
export type Split = "development" | "evaluation" | "holdout";
export type EffectStatus = "started" | "succeeded" | "failed" | "unknown" | "denied" | "stale";
export type Disposition = "completed" | "abstained" | "failed" | "cancelled" | "exhausted" | "interrupted";
export type RecordKind = "definition" | "occurrence" | "implementation" | "finding" | "intervention" | "memory" | "experiment" | "recommendation" | "admission" | "evaluation" | "regulation" | "transplant" | "obligation";
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
}
export interface Checkpoint {
  format: "pi-0.85.1/1";
  profile: Profile;
  operator: string;
  provider: string;
  model: string;
  messages: Array<Record<string, unknown>>;
  pending_operations: Array<string>;
  event_cursor: string;
}
export interface AgentRunRequest {
  run_id: string;
  profile: Profile;
  operator: string;
  prompt: string;
  provider: string;
  model: string;
  checkpoint?: Checkpoint;
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
}
export interface RecordPage {
  records: Array<RecordEnvelope>;
  next_offset: number;
}
export interface ArtifactRead {
  path: string;
  offset: number;
  length: number;
  branch_id?: string;
}
export interface ArtifactChunk {
  artifact: ArtifactRef;
  content: string;
  offset: number;
  total_bytes: string;
  eof: boolean;
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
  status: "queued" | "running" | "completed" | "failed" | "cancelled" | "exhausted" | "interrupted";
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
}
