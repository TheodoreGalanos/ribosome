import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeLearning, summarizeStudy } from '../../scripts/qualification-evidence.mjs';
test('qualification export excludes private text and retains incomplete denominators and unknown usage', () => {
  const report = { status: 'executed', qualification: 'not_established', summary: 'PRIVATE-TRANSCRIPT', model: 'PRIVATE-DEPLOYMENT', candidate: { id: 'PRIVATE-KEY', version: 'PRIVATE-VERSION' }, usage: { calls: 7, known_cost_microusd: 100, unknown_calls: 1 }, study: { decision: 'inconclusive', complete: false, usage_complete: false, planned_evaluations: 8, report: { arms: [{ arm: 'candidate', planned: 4, observed: 2, verified_successes: 2, mean_quality: 1, private: 'PRIVATE-ORACLE' }] } } };
  const safe = summarizeLearning(report);
  assert.doesNotMatch(JSON.stringify(safe), /PRIVATE/);
  assert.equal(safe.usage.unknown_calls, 1);
  assert.equal(safe.study.complete, false);
  assert.equal(safe.study.arms[0].verified_success_rate, 0.5);
  assert.equal(summarizeStudy(undefined).complete, null);
  assert.equal(summarizeLearning(undefined).qualification, 'unknown');
});
