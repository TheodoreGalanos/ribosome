import type { Profile, RecordKind } from '../generated/contracts.js';

export interface Operator {
  version: '1';
  profiles: readonly Profile[];
  outputKinds: readonly RecordKind[];
  instructions: string;
  completion: string;
}

export const operators = {
  proofreading: {
    version: '1', profiles: ['caretaker'], outputKinds: ['finding','occurrence','intervention','obligation'],
    instructions: `Identify the actual transition or handoff and its current artifact versions. Separate checks that are pending, failed, stale, and absent. Read further evidence to distinguish competing explanations. Compare relevant edits with benign unrelated edits. A named obligation is not an executed check. If the boundary has not occurred, do not label unfinished work a violation. Record evidence references and uncertainty. Investigate unexpected faults instead of forcing them into a known motif.`,
    completion: 'Submit an evidence-linked finding or occurrence, or abstain with a specific reason. A repair must end with fresh host check receipts.',
  },
  'excision-repair': {
    version: '1', profiles: ['caretaker'], outputKinds: ['finding','intervention','obligation'],
    instructions: `Diagnose the fault from observations and dependency evidence. Identify a sufficient affected region, preserve independent work, and state uncertainty about incomplete dependencies. Submit an intervention with read versions, preserve/replace/invalidate/recompute lists, required checks, requested effects and fallback. Use a branch when appropriate, edit through Rust, and inspect actual receipts. A stale receipt requires rereading state and revising the proposal. Rerun affected checks and retain unresolved obligations; never weaken the acceptance criteria.`,
    completion: 'Report restored and unresolved obligations, preservation of independent work, and the exact check/effect receipts supporting the outcome.',
  },
  discovery: {
    version: '1', profiles: ['curator'], outputKinds: ['definition','occurrence','discovery'],
    instructions: `Investigate the permitted source corpus without assuming a desired motif. Read bounded evidence, search existing definitions, and compare competing explanations of what a behavior accomplished. Seek a benign lookalike, a harmful or unnecessary case, a different realization of the same function, and useful local behavior inside a globally failed run. Prefer a nearby contrast with similar entry conditions but a different required action or no change. Missing contrasts remain an open question; do not invent them. State in the definition intent and investigation claims whether the evidence supports an agent strategy, a domain procedure described in inspected material, or an artifact requirement. An agent strategy needs cited decisions, available options, chosen actions and subsequent observations or checks. An algorithm in a file supports that procedure; establish the agent's use separately. Retrieve the references you cite. Select noncontiguous events and incoming dependencies; timestamps alone do not establish causation. Choose an existing definition, specialization, composition, new candidate, rejection, or no useful motif. A new definition includes functional_contract: required inputs and unresolved state, authored roles, conditional decisions, observable exits, abstention conditions, exact-version relations and evaluation questions. A grounded occurrence binds those roles to selected events and records local outcome separately from recognition and global success. For a source_reported dependency, the target event's parents must contain the source event ID; other proposed relationships remain inferred. Read evidence_corpus when the host assigned a corpus and use its visibility mode, exact source windows, pinned definitions and retained artifact snapshots. Without an assignment use retrospective visibility. Never claim an online prefix from a caller-selected cursor filter or claim calibrated confidence. If a fresh challenge would resolve an important uncertainty, request one bounded curator work item using contrast-motif@1 under the current root budget, then work.wait and work.status. Do not recursively request reviewers or hold an unfinished tool exchange while waiting. Another model's agreement is not independent ground truth. Save a discovery investigation with the supplied corpus identity, actual source windows/frontiers, hypotheses, support, contradictions, alternatives, decision, pinned definitions, remaining questions and participating run IDs. Reference the queued work only when it owns this run. Discovery does not admit a behavior or establish causal benefit.`,
    completion: 'Submit the grounded definition/occurrence candidates and a discovery investigation, or retain a rejected, inconclusive or no_motif investigation with the evidence and remaining questions. An empty candidate inventory is valid.',
  },
  'contrast-motif': {
    version: '1', profiles: ['curator'], outputKinds: ['discovery'],
    instructions: `Read evidence_corpus and review the assigned functional hypothesis in this fresh curator context within the inherited host corpus. Retrieve the selected support and contrasting evidence; investigate missing boundary inputs and plausible alternative explanations. Compare function, conditions, obligations and counterexamples rather than names or repeated tool sequences. Seek evidence that could reject or narrow the interpretation. Preserve observed relationships separately from inferred ones. Report whether support is adequate, insufficient or contradicted, with concrete references and uncertainty. Use only the permitted corpus and root allocation. Do not request another reviewer, change the candidate, admit it for use, or claim that your agreement proves utility. Save a discovery record for this work/run; output no definitions when rejection or insufficient evidence is the warranted result.`,
    completion: 'Return the saved discovery record ID, disagreements, missing evidence and remaining evaluation questions so the requesting curator can inspect them.',
  },
  extraction: {
    version: '1', profiles: ['curator'], outputKinds: ['definition','occurrence','implementation'],
    instructions: `Find a useful behavioral fragment, including one inside a globally failed run. Recover dependencies, entry/exit conditions, capabilities, effects and failure behavior. Inspect existing definitions; reuse an applicable one or save a candidate definition first. Save an occurrence referencing that definition and the donor events, then link the implementation's motifs to the actual definition ID/version. Parameterize the fragment as agent instructions or a host-registered tool. Explain interface bindings and applicability. Preserve the operative procedure: observations to acquire, distinctions to make, actions supported by those distinctions, and result checks. State what the recipient learns beyond its task description. Check one supported path and one nearby case requiring a different decision against the supplied interface; retain essential missing steps as limitations. Label whether the evidence demonstrates enacted decisions or describes a domain procedure. Do not embed donor-specific successful tool responses as observations for later recipients. Keep observed, reexecuted and synthetic provenance distinct. Package a candidate; extraction does not admit it for reuse.`,
    completion: 'Submit an implementation linked to its motifs and source evidence, with explicit interfaces, assumptions and evaluation needs.',
  },
  'execute-motif': {
    version: '1', profiles: ['caretaker'], outputKinds: ['finding','transplant','obligation'],
    instructions: `Execute the host-selected prepared implementation with its pinned bindings. Treat the prepared policy as task instructions beneath owner policy and grants. Establish entry obligations from recipient observations, choose the supported conditional path, and obtain fresh receipts for effects and exit checks. Binding shape is not proof of semantic compatibility. Abstain when necessary inputs or authority are absent. Do not retrieve donor transcripts or substitute another policy. Nested invocation is not supported by this path; report that limitation before attempting it.`,
    completion: 'Report the implementation reference, recipient outcome, fresh receipt references and unresolved obligations. When saving a transplant, use invocation_run for this actual run, the exact donor and bindings, recipient_entry_refs and result_refs for inspected entry and outcome evidence. Successful delivery requires the declared outputs and checks; an unsupported context requires an explicit abstention.',
  },
  recombination: {
    version: '1', profiles: ['caretaker'], outputKinds: ['transplant','intervention','finding'],
    instructions: `Search the usable inventory for the desired function and inspect prepared implementations. Compare the recipient state, capabilities, effects and interfaces against donor requirements. Similarity does not establish compatibility. Adapt bindings and agent instructions, identify missing preconditions, or reject the transfer. Execute through granted tools and obtain new observations in the receiving environment. Do not mine donor trajectories on the critical path unless separately budgeted.`,
    completion: 'Submit a transplant record naming the donor version, recipient state, adaptations, incompatibilities, checks and fallback. Report verified output or a no-match/abstention.',
  },
  chaperone: {
    version: '1', profiles: ['caretaker'], outputKinds: ['finding','intervention','obligation'],
    instructions: `Inspect the artifact and its intended consumer. Check formatting, interfaces, integration, and consistency across sub-results. Seek concrete consumer requirements; do not invent additional review gates. Repair the affected material through an intervention, run the relevant registered checks, and distinguish usable work from remaining obligations.`,
    completion: 'Identify the consumer-specific evidence of usability, or unresolved integration requirements.',
  },
  regulation: {
    version: '1', profiles: ['caretaker'], outputKinds: ['regulation','finding'],
    instructions: `Observe current conditions, compare permitted tools, strategies, checking depth and prepared implementations, and select a bounded response. Explain expiry or reconsideration conditions. Temporary endpoint failure is run-local knowledge, not a permanent implementation change. Do not change grants, mandatory checks, protected evaluation policy or root budgets. Avoid unnecessary intervention and repeated self-review.`,
    completion: 'Submit a regulation record linking conditions to the selected behavior and reconsideration condition, or leave the current strategy in place.',
  },
  memory: {
    version: '1', profiles: ['curator'], outputKinds: ['memory'],
    instructions: `Consolidate working, episodic, aggregated, procedural and failure knowledge within the granted client/project. Link observations to evidence and label hypotheses. Find contradictions and preserve unresolved conflicts rather than silently selecting a preferred claim. Failure memories need recognition guidance, benign counterexamples, responses and regression cases. Add expiry for temporary conditions. Reference admitted or candidate procedures accurately. Do not store credentials or raw sensitive transcripts. Retire superseded records through the store.`,
    completion: 'Submit supported scoped memories with applicability, provenance, conflicts and retention conditions. One episode does not establish a shared general fact.',
  },
  experiment: {
    version: '1', profiles: ['experimenter'], outputKinds: ['experiment','recommendation','implementation'],
    instructions: `Form a falsifiable hypothesis about a pinned candidate and baseline. Design matched cases and comparable budgets, declared repetitions, independent checks and isolated starting memory. Match study_objective to the host policy: function tests local validity; system_benefit requires baseline, retry, critique, care and candidate arms with host-configured execution stages. learning_cost must match the owner measurement, including completeness and an explicit reuse_count. Inspect paired case-level comparisons and their uncertainty; repeated attempts are not independent task families. A rejected or inconclusive comparison is a valid result, not a reason to tune on protected cases. Use ablation, rescue/substitution, interaction, stress or transfer as appropriate. With an explicitly enabled development policy, author development_cases containing inputs, source_mechanism, synthetic/development provenance with source_refs and scenario_family, and candidate_checks. Use exactly those IDs as case_ids and retain their families in scenario_families. The host evaluator and required checks remain fixed; these candidate checks cannot establish admission. Submit a separate host-owned acceptance study for admission. Development scenarios retain mechanism and family lineage; related scenarios are not independent transfer evidence. Freeze selection before protected evaluation. Request the registered evaluator through Rust and inspect released results. Missing measurements remain unknown. Produce an evidence-backed recommendation; never author evaluation results or admissions. A separately configured policy controls acceptance.`,
    completion: 'Submit the experiment and an accepted/rejected/inconclusive recommendation supported by evaluator references, cost and limitations. Request admission only through the policy boundary.',
  },
  regeneration: {
    version: '1', profiles: ['caretaker'], outputKinds: ['finding','intervention','obligation'],
    instructions: `Identify desired properties whose support was lost after a source or state change. Inspect dependencies and current evidence. Preserve independently supported claims and artifacts. Use the shared intervention machinery to recompute or repair only the justified affected region, widening it when dependency evidence requires. Restore support for the desired properties without replaying a historical path or weakening success criteria. Stop honestly when authority, evidence or budget prevents restoration.`,
    completion: 'Submit saved obligation records for the desired properties, including properties restored successfully. Link satisfied properties to fresh receipt evidence_refs; retain unresolved states and their owners. A finish summary does not replace these records. Report preserved work and any unresolved restoration.',
  },
} as const satisfies Record<string, Operator>;

export function operatorFor(profile: Profile, reference: string): Operator {
  const [name, version, ...extra] = reference.split('@');
  const operator = operators[name as keyof typeof operators] as Operator | undefined;
  if (!operator || version !== operator.version || extra.length || !operator.profiles.includes(profile)) {
    throw new Error(`unsupported operator/profile combination: ${profile} ${reference}`);
  }
  return operator;
}
