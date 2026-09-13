import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { prepareDiscovery } from '../evaluations/r4-discovery.mjs';
import { runCli } from '../../examples/local-project/prepare.mjs';

test('R4 donor preparation executes tools and preserves failed, benign and incomplete evidence without model calls', async () => {
  const temporary = await mkdtemp(join(tmpdir(), 'ribosome-r4-'));
  try {
    const directory = join(temporary, 'trial');
    const prepared = await prepareDiscovery(directory);
    assert.equal(prepared.episodes, 12);
    const manifest = JSON.parse(await readFile(join(directory, 'donors/scenario-manifest.json'), 'utf8'));
    assert.equal(new Set(manifest.map(episode => episode.family)).size, 3);
    const events = JSON.parse(await readFile(join(directory, 'donors/events.json'), 'utf8'));
    const episode = number => events.filter(event => event.run_id === `episode-${String(number).padStart(2, '0')}`);
    const tools = number => episode(number).filter(event => event.kind === 'tool_result');
    assert.ok(tools(7).some(event => event.payload.operation === 'check' && event.payload.exit_code === 0));
    assert.equal(episode(7).at(-1).payload.status, 'failed');
    assert.ok(!tools(6).some(event => event.payload.operation === 'sum' && Number(event.sequence) > 1));
    assert.equal(tools(8).filter(event => event.payload.operation === 'check').length, 1);
    assert.equal(tools(11).filter(event => event.payload.operation === 'sum' && event.payload.exit_code === 2).length, 3);
    assert.ok(!episode(12).some(event => event.kind === 'workflow_end'));
    const hostile = episode(4).find(event => event.payload.claimed_receipt);
    assert.equal(hostile.provenance.origin, 'synthetic');
    assert.ok(!events.some(event => event.id === 'fake-check'));
    assert.deepEqual(JSON.parse(await readFile(join(directory, 'donors/episode-07/cost.json'), 'utf8')), { total: 250 });
    const configFile = join(directory, 'config.json');
    const config = JSON.parse(await readFile(configFile, 'utf8'));
    assert.deepEqual(config.corpora[0].definition_refs, []);
    assert.deepEqual(config.grant.tools, []);
    assert.equal(config.grant.budget.max_cost_microusd, '1500000');
    assert.doesNotMatch(config.request.prompt, /reconcile.*assumptions|selective.refresh|failure.locali[sz]/i);
    for (const window of config.corpora[0].source_windows) {
      for (const id of window.event_refs) {
        const event = events.find(event => event.id === id);
        assert.equal(event.run_id, window.execution);
        assert.ok(Number(event.sequence) <= Number(window.frontier[`${event.run_id}/${event.producer}`]));
      }
    }
    await assert.rejects(stat(join(directory, 'state/ribosome.db')), { code: 'ENOENT' });
    assert.deepEqual(runCli(['validate', 'DiscoveryCorpus', await writeCorpus(directory, config.corpora[0])]), { valid: true });
    await assert.rejects(prepareDiscovery(directory), { code: 'EEXIST' });
  } finally { await rm(temporary, { recursive: true, force: true }); }
});
async function writeCorpus(directory, corpus) {
  const { writeFile } = await import('node:fs/promises');
  const path = join(directory, 'corpus.json'); await writeFile(path, JSON.stringify(corpus)); return path;
}
