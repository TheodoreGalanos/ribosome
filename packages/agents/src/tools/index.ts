import type { AgentTool } from '@earendil-works/pi-agent-core';
import { Type } from 'typebox';
import type { AgentResult, RecordKind } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { contract, schema, toolSchema, validate, RpcError } from '../client/validation.js';

const descriptions = {
  'continuation.read': 'List prior obligation, effect or direct child-work references for this run in bounded pages. Start with after="0"; use next as after until complete, including after empty advancing pages. Obligations are previously observed, still accessible records whose declared state is not satisfied. These IDs establish neither completion nor current validity: retrieve record_read, action_lookup or work_status. evidence_cursor is the last checkpointed evidence-read position, not proof that all relevant evidence was inspected.',
  'evidence.corpus': 'Read the host-assigned immutable discovery corpus: visibility mode, selected source windows, pinned definitions, retained artifact snapshot IDs and frozen dependency entries. This returns the assignment, not the source payloads; retrieve cited evidence separately. If no corpus is assigned, this reports that absence.',
  'evidence.read': 'Read source workflow events, frontier and dependencies. Start with cursor="0", limit=20. Omit run_id to inspect all producers in scope; it is a source-workflow filter, not a grant ID. Select exact event_refs, kind, an artifact {path,version}, or query for a literal substring of event JSON. With event_refs, neighbors=true includes one hop of declared parents and children; that relationship is source-reported, not proven causal. Filters intersect. through_cursor caps this read; it does not establish online-only visibility. Continue from the returned cursor, including after an empty advancing page. Targeted pages include only artifact dependencies touching returned events. search_query searches records separately.',
  'search.query': 'Defaults remain literal all_terms matching and ID order. Set query_mode=any_terms for alternatives, order=relevance for lexical ranking, eligible=true for tool/effect-compatible implementation candidates, or function for an exact authored category. These filters do not establish semantic compatibility. For subsequent pages use next as after with offset=0 and unchanged query/filters; complete marks the end. If the index changed, restart the query. Search record body text. Use inventory=evidence for definitions, occurrences or memory; usable contains only admitted implementations. Use record_read for a known record ID: IDs and ID@version strings are not full-text queries.',
  'record.read': 'Read a record by its exact envelope ID. Do not append @version. For {id,version}, pass only id; the result contains version metadata.',
  'artifact.read': 'Read a UTF-8 artifact chunk with its exact version and retained snapshot_id. Workspace reads default to required_freshness=current. Assigned corpus reads require snapshot_id and default to historical; use historical for evidence that may remain usable after revision. snapshot_id reads only that prior observation window. Large tool outputs return an excerpt and a ribosome-result: artifact path: read that path for further bytes of the original JSON result. Result artifacts and ribosome-export: training products return at most 8,192 bytes per read. These retained results are historical only; inspect original workspace artifacts for current state. Deleted or unauthorized sources deny their retained copies. Byte offsets advance by returned UTF-8 byte length.',
  'action.execute': 'Request a host effect. branch creates a copy; edit writes content with an expected_version; check runs tool without admission; execute requires tool AND an admitted implementation {id,version}; apply needs branch_id, path, original live expected_version, checked branch content and intervention_ref. Inspect the returned receipt.',
  'action.lookup': 'Look up or reconcile a previous operation; never repeat an unknown effect.',
  'artifact.validity': 'Assess the current host validation evidence for visible Obligation properties of a live workspace artifact. Returns exact obligation and artifact versions with validated, stale or unproven assessments. A retained result is historical; request a fresh assessment before relying on it. Declared obligation states and host validation are separate. An empty property list establishes no semantic coverage.',
  'record.submit': 'Submit an interpretation, candidate, memory or recommendation; protected observations/evaluations/admissions are rejected.',
  'record.retire': 'Retire or delete a scoped unprotected record at its expected version.',
  'work.wait': 'Wait for one to sixteen direct child work IDs returned by work_request. The host saves this completed turn, releases the worker slot and runs the children. After continuation resumes, use work_status to inspect their actual outcomes. This does not declare success or create more budget.',
  'work.status': 'Inspect a direct child work item and its source-authorized completion. Missing completion text is unavailable evidence, not success. A prior status result is historical; read again for current status.',
  'work.request': 'Request bounded follow-up work for a subject, retaining root budget and causal origin.',
  'message.send': 'Deliver a message to another participant inside the communication grant.',
  'message.inbox': 'Read bounded unacknowledged messages. Acknowledge after interpreting them.',
  'message.ack': 'Acknowledge a message addressed to this run.',
  'experiment.run': 'Execute a frozen comparison using the separately configured evaluator and acceptance policy.',
  'inventory.admission_request': 'Request policy-controlled admission from an evidence-backed recommendation.',
  'inventory.archive': 'Inspect the scope-filtered diversity cells and their admitted implementations.',
  'training.export': 'Export supported material with provenance and scope/split checks. Returns a host delivery path and an immutable ribosome-export: artifact reference. Read the reference with artifact_read for current source authorization; products are historical and limited to 16 MiB.',
} as const;

export function createTools(peer: RpcPeer, runId: string, operator: string, outputKinds: readonly RecordKind[], finish: (result: AgentResult) => void, wait?: (workIds: string[]) => void): AgentTool[] {
  // Prepared reuse consumes the inventory and recipient artifacts. Source
  // history investigation belongs in a separately budgeted curator run.
  const prepared = ['recombination@1', 'execute-motif@1'].includes(operator);
  const separateRecords = operator === 'discovery@1' || operator === 'contrast-motif@1';
  const methods = Object.entries(descriptions).filter(([method]) =>
    (!prepared || !['evidence.read', 'evidence.corpus', 'message.inbox', 'message.send', 'training.export', 'experiment.run', 'work.request'].includes(method)) &&
    (operator !== 'contrast-motif@1' || method !== 'work.request')).flatMap<{ method: string; description: string; recordKind: RecordKind | undefined }>(([method, description]) =>
      method === 'record.submit' && separateRecords
        ? outputKinds.map(kind => ({ method, description: `Save a ${kind} record. ${description}`, recordKind: kind }))
        : [{ method, description, recordKind: undefined }]);
  const tools: AgentTool[] = methods.map(({method, description, recordKind}) => {
    const rpcMethod = method as keyof typeof descriptions;
    const parameters = toolSchema(contract(method)[0]);
    if (method === 'record.submit') {
      // Providers require an object at the parameter root. Describe the body
      // alternatives here, then enforce the selected kind before RPC dispatch.
      const bodies = (recordKind ? [recordKind] : outputKinds).map(kind => {
        const body = recordBodySchema(kind);
        const required = operator === 'discovery@1' ? discoveryField(kind) : undefined;
        if (required) body.required = [...body.required as string[], required];
        return { ...body, description: `Body for kind="${kind}".` };
      });
      (parameters.properties as Record<string, unknown>).body = recordKind ? bodies[0] : { anyOf: bodies };
      (parameters.properties as Record<string, unknown>).kind = { type: 'string', enum: outputKinds };
      if (recordKind) {
        delete (parameters.properties as Record<string, unknown>).kind;
        parameters.required = (parameters.required as string[]).filter(field => field !== 'kind');
      }
    }
    if (method === 'work.request' && operator === 'discovery@1') {
      const properties = parameters.properties as Record<string, unknown>;
      properties.profile = { type: 'string', enum: ['curator'] };
      properties.operator = { type: 'string', enum: ['contrast-motif@1'], description: 'A fresh contrast investigation. It cannot request another reviewer.' };
    }
    if (method === 'action.execute') {
      delete (parameters.properties as Record<string, unknown>).operation_id;
      parameters.required = (parameters.required as string[]).filter(k => k !== 'operation_id');
    }
    return {
      name: recordKind ? `${recordKind}_submit` : method.replaceAll('.', '_'), label: method, description,
      parameters: Type.Unsafe(parameters), replay: method === 'action.execute' ? 'never' : 'safe', executionMode: 'sequential',
      async execute(callId, args, signal) {
        const input = method === 'action.execute' ? { ...(args as Record<string, unknown>), operation_id: `${runId}/${callId}` }
          : recordKind ? { ...(args as Record<string, unknown>), kind: recordKind } : args;
        if (method === 'record.submit') {
          const record = input as { kind: RecordKind; body: unknown };
          if (!outputKinds.includes(record.kind)) throw new RpcError(-32602, 'record kind is not an operator output');
          validate(schema['x-records'][record.kind], withHostMetadata(record.kind, record.body, runId, operator));
          const required = operator === 'discovery@1' ? discoveryField(record.kind) : undefined;
          if (required && !(required in (record.body as Record<string, unknown>))) throw new RpcError(-32602, `discovery requires ${required}`);
        }
        if (method === 'work.request' && operator === 'discovery@1' &&
            ((input as Record<string, unknown>).operator !== 'contrast-motif@1' || (input as Record<string, unknown>).profile !== 'curator')) {
          throw new RpcError(-32602, 'discovery follow-up must use curator contrast-motif@1');
        }
        const { content, ...details } = await peer.call('tool.call', { call_id: callId, method: rpcMethod, arguments: input as Record<string, unknown> }, signal);
        if (method === 'work.wait') {
          const result = JSON.parse(content) as { work_ids: string[]; wait_required: boolean };
          validate('WorkWaitResult', result);
          if (result.wait_required) wait?.(result.work_ids);
        }
        return { content: [{ type: 'text', text: content }], details };
      },
    };
  });
  tools.push({
    name: 'finish', label: 'Finish investigation', description: 'End with a concise evidence-linked disposition. Actual host receipts remain authoritative.',
    parameters: Type.Unsafe(toolSchema('AgentResult')), executionMode: 'sequential', replay: 'safe',
    async execute(_callId, args) { validate('AgentResult', args); finish(args as AgentResult); return { content: [{ type: 'text', text: 'Terminal disposition recorded for host return.' }], details: args, terminate: true }; },
  });
  return tools;
}

function discoveryField(kind: RecordKind): string | undefined {
  return kind === 'definition' ? 'functional_contract' : kind === 'occurrence' ? 'grounding' : undefined;
}

function recordBodySchema(kind: RecordKind): Record<string, unknown> {
  const body = toolSchema(schema['x-records'][kind]);
  const properties = body.properties as Record<string, Record<string, unknown>>;
  const optional = (value: Record<string, unknown>, fields: string[]) => {
    value.required = (value.required as string[]).filter(field => !fields.includes(field));
    const properties = value.properties as Record<string, Record<string, unknown>>;
    for (const field of fields) properties[field]!.description = 'Optional: Rust supplies this from the submitting run and selected events. Explicit values are validated.';
  };
  if (kind === 'discovery') {
    delete properties.work_ref;
    body.required = (body.required as string[]).filter(field => field !== 'work_ref');
    optional(body, ['run_refs']);
    optional(properties.source_windows!.items as Record<string, unknown>, ['frontier']);
  }
  if (kind === 'occurrence') {
    optional(body, ['frontier', 'operator']);
    optional(properties.grounding!, ['annotator']);
  }
  return body;
}

// Check the authored fields against the canonical persisted contract. Rust
// computes these omitted fields; this validation copy is never sent or stored.
function withHostMetadata(kind: RecordKind, value: unknown, runId: string, operator: string): unknown {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
  const body = structuredClone(value) as Record<string, unknown>;
  if (kind === 'discovery') {
    body.run_refs ??= [runId];
    if (Array.isArray(body.source_windows)) for (const window of body.source_windows) {
      if (window && typeof window === 'object' && !Array.isArray(window)) window.frontier ??= {};
    }
  }
  if (kind === 'occurrence') {
    body.frontier ??= {}; body.operator ??= operator;
    if (body.grounding && typeof body.grounding === 'object' && !Array.isArray(body.grounding)) {
      const grounding = body.grounding as Record<string, unknown>;
      grounding.annotator ??= { run_id: runId, operator };
    }
  }
  return body;
}
