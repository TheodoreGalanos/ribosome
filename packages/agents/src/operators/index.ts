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
    version: '1', profiles: ['curator'], outputKinds: ['definition','occurrence'],
    instructions: `Inspect a bounded evidence window. Search existing motif definitions, compare positive and benign counterexamples, and propose a new definition when no existing function fits. Describe intent, applicability, recognition instructions and observable obligations. A motif can occur without succeeding. Distinguish tentative recognition from supported recognition and open obligations from violated ones. Occurrences may overlap and cross producers; timestamps do not prove causality. Preserve useful local behavior even when the enclosing run failed.`,
    completion: 'Submit distinct definition and occurrence records with source evidence, operator version and explicit assumptions, or abstain when the window is insufficient.',
  },
  extraction: {
    version: '1', profiles: ['curator'], outputKinds: ['definition','occurrence','implementation'],
    instructions: `Find a useful behavioral fragment, including one inside a globally failed run. Recover dependencies, entry/exit conditions, capabilities, effects and failure behavior. Inspect existing definitions; reuse an applicable one or save a candidate definition first. Save an occurrence referencing that definition and the donor events, then link the implementation's motifs to the actual definition ID/version. Parameterize the fragment as agent instructions or a host-registered tool. Explain interface bindings and applicability. Do not embed donor-specific successful tool responses as observations for later recipients. Keep observed, reexecuted and synthetic provenance distinct. Package a candidate; extraction does not admit it for reuse.`,
    completion: 'Submit an implementation linked to its motifs and source evidence, with explicit interfaces, assumptions and evaluation needs.',
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
    instructions: `Form a falsifiable hypothesis about a pinned candidate and baseline. Design matched cases and comparable budgets, declared repetitions, independent checks and isolated starting memory. Use ablation, rescue/substitution, interaction, stress or transfer as appropriate. With an explicitly enabled development policy, author development_cases containing inputs, source_mechanism, synthetic/development provenance with source_refs and scenario_family, and candidate_checks. Use exactly those IDs as case_ids and retain their families in scenario_families. The host evaluator and required checks remain fixed; these candidate checks cannot establish admission. Submit a separate host-owned acceptance study for admission. Development scenarios retain mechanism and family lineage; related scenarios are not independent transfer evidence. Freeze selection before protected evaluation. Request the registered evaluator through Rust and inspect released results. Missing measurements remain unknown. Produce an evidence-backed recommendation; never author evaluation results or admissions. A separately configured policy controls acceptance.`,
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
