// The production worker and pinned Azure SDK run unchanged. Only fetch is a
// test double; no request can leave this process or reach a paid provider.
import assert from 'node:assert/strict';

let step = 0;
const fixtureFetch = async (input, options) => {
  const request = new Request(input, options);
  const url = new URL(request.url);
  assert.equal(url.origin, 'https://ribosome-test.openai.azure.com');
  assert.equal(url.pathname, '/openai/v1/responses');
  assert.equal(url.searchParams.get('api-version'), 'v1');
  assert.equal(process.env.AZURE_OPENAI_API_VERSION, 'v1');
  assert.equal(request.headers.get('api-key'), 'azure-fixture-key');
  for (const name of ['OPENAI_API_KEY', 'ANTHROPIC_API_KEY', 'UNRELATED_SECRET']) assert.equal(process.env[name], undefined);
  const body = await request.json();
  const authority = body.input.find(item => ['system', 'developer'].includes(item.role));
  const instruction = typeof authority?.content === 'string' ? authority.content : authority?.content.map(part => part.text ?? '').join('');
  assert.ok(instruction?.includes('"mode":"apply"'), 'Pi must receive the run-bound host grant');
  assert.ok(instruction?.includes('"writable_paths":[]'), 'Pi must see the exact write restriction');
  assert.ok(!instruction?.includes('azure-fixture-key'), 'Provider credentials must stay outside model context');
  assert.ok(['measurement-deployment', 'reasoning-deployment'].includes(body.model));
  if (body.model === 'reasoning-deployment') assert.equal(body.reasoning?.effort, 'medium');
  else assert.equal(body.reasoning, undefined);
  assert.equal(body.stream, true);
  assert.equal(body.store, false);
  assert.equal(body.tool_choice, 'required', 'An agent turn must use a tool, including finish for its terminal disposition');
  assert.equal(body.max_output_tokens, body.model === 'reasoning-deployment' ? 16384 : 4096);
  const calls = [
    ['artifact_read', { path: 'report.txt', offset: 0, length: 1000 }],
    ['action_execute', { kind: 'check', tool: 'credential-check' }],
    ['finish', { disposition: 'completed', summary: 'Azure transport fixture read an artifact and received a credential-isolated host check.' }],
  ];
  assert.ok(step < calls.length, 'Unexpected extra model request');
  if (step > 0) {
    const observation = body.input.findLast(item => item.type === 'function_call_output');
    assert.ok(observation, 'The next Azure request must contain the Rust tool result');
    assert.match(observation.output, step === 1 ? /original/ : /succeeded/);
  }
  const [name, args] = calls[step];
  const item = { type: 'function_call', id: `fc_${step}`, call_id: `call_${step}`, name, arguments: JSON.stringify(args), status: 'completed' };
  step++;
  const events = [
    { type: 'response.output_item.added', output_index: 0, item: { ...item, arguments: '', status: 'in_progress' } },
    { type: 'response.function_call_arguments.done', output_index: 0, item_id: item.id, arguments: item.arguments },
    { type: 'response.output_item.done', output_index: 0, item },
    { type: 'response.completed', response: { id: `response_${step}`, status: 'completed', output: [item], usage: { input_tokens: 100, output_tokens: 20, total_tokens: 120, input_tokens_details: { cached_tokens: 0 } } } },
  ];
  return new Response(events.map(event => `event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`).join(''), { headers: { 'content-type': 'text/event-stream' } });
};

globalThis.fetch = async (input, options) => {
  try { return await fixtureFetch(input, options); }
  catch (error) { console.error('Azure transport fixture failed:', error.message); throw error; }
};

await import('../../packages/agents/dist/worker.js');
