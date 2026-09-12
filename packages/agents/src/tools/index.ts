import type { AgentTool } from '@earendil-works/pi-agent-core';
import { Type } from 'typebox';
import type { AgentResult, RecordKind, RpcMethods } from '../generated/contracts.js';
import { RpcPeer } from '../client/rpc.js';
import { contract, schema, toolSchema, validate } from '../client/validation.js';

const descriptions = {
  'evidence.read': 'Read actual workflow events, frontier and dependencies. Start with cursor="0", limit=20. Omit run_id to inspect all producers in scope; run_id is an optional source-workflow filter, not your grant ID or maintenance run ID. Search queries do not search these events.',
  'search.query': 'Search record body text. Use inventory=evidence for definitions, occurrences or memory; usable contains only admitted implementations. Use record_read for a known record ID: IDs and ID@version strings are not full-text queries.',
  'record.read': 'Read a record by its exact envelope ID. Do not append @version. For {id,version}, pass only id; the result contains version metadata.',
  'artifact.read': 'Read a UTF-8 artifact chunk with its exact current version. Byte offsets advance by returned UTF-8 byte length.',
  'action.execute': 'Request a host effect. branch creates a copy; edit writes content with an expected_version; check runs tool without admission; execute requires tool AND an admitted implementation {id,version}; apply needs branch_id, path, original live expected_version, checked branch content and intervention_ref. Inspect the returned receipt.',
  'action.lookup': 'Look up or reconcile a previous operation; never repeat an unknown effect.',
  'record.submit': 'Submit an interpretation, candidate, memory or recommendation; protected observations/evaluations/admissions are rejected.',
  'record.retire': 'Retire or delete a scoped unprotected record at its expected version.',
  'work.request': 'Request bounded follow-up work for a subject, retaining root budget and causal origin.',
  'message.send': 'Deliver a message to another participant inside the communication grant.',
  'message.inbox': 'Read bounded unacknowledged messages. Acknowledge after interpreting them.',
  'message.ack': 'Acknowledge a message addressed to this run.',
  'experiment.run': 'Execute a frozen comparison using the separately configured evaluator and acceptance policy.',
  'inventory.admission_request': 'Request policy-controlled admission from an evidence-backed recommendation.',
  'inventory.archive': 'Inspect the scope-filtered diversity cells and their admitted implementations.',
  'training.export': 'Export supported material with provenance and scope/split checks.',
} as const;

export function createTools(peer: RpcPeer, runId: string, operator: string, outputKinds: readonly RecordKind[], finish: (result: AgentResult) => void): AgentTool[] {
  // Prepared reuse consumes the inventory and recipient artifacts. Source
  // history investigation belongs in a separately budgeted curator run.
  const methods = Object.entries(descriptions).filter(([method]) => operator !== 'recombination@1' || method !== 'evidence.read');
  const tools: AgentTool[] = methods.map(([method, description]) => {
    const rpcMethod = method as keyof typeof descriptions;
    const parameters = toolSchema(contract(method)[0]);
    if (method === 'record.submit') {
      // Present the actual body contract at the tool boundary. A free-form
      // object encouraged plausible records that the store could not use.
      parameters.oneOf = outputKinds.map(kind => ({
        properties: { kind: { const: kind }, body: toolSchema(schema['x-records'][kind]) },
      }));
      (parameters.properties as Record<string, unknown>).kind = { type: 'string', enum: outputKinds };
    }
    if (method === 'action.execute') {
      delete (parameters.properties as Record<string, unknown>).operation_id;
      parameters.required = (parameters.required as string[]).filter(k => k !== 'operation_id');
    }
    return {
      name: method.replaceAll('.', '_'), label: method, description,
      parameters: Type.Unsafe(parameters), replay: method === 'action.execute' ? 'never' : 'safe', executionMode: 'sequential',
      async execute(callId, args, signal) {
        const input = method === 'action.execute' ? { ...(args as Record<string, unknown>), operation_id: `${runId}/${callId}` } : args;
        const result = await peer.call(rpcMethod, input as RpcMethods[typeof rpcMethod]['input'], signal);
        return { content: [{ type: 'text', text: JSON.stringify(result) }], details: result };
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
