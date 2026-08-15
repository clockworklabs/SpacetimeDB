import assert from 'node:assert/strict';
import test from 'node:test';

import { verifyPromptSnapshot } from '../prompt-snapshot.mjs';

test('the exact model prompts match the reviewed, symmetric L1/L2 guidance snapshot without Docker', () => {
  const snapshot = verifyPromptSnapshot();
  assert.equal(snapshot.prompts.length, 18);
  for (const stack of ['mongodb', 'postgres', 'spacetime']) {
    for (const mode of ['build', 'fix', 'upgrade']) {
      const pair = snapshot.prompts.filter(entry => entry.stack === stack && entry.round.mode === mode);
      assert.equal(pair.length, 2);
      assert.notEqual(pair[0].backendMaterial.sha256, pair[1].backendMaterial.sha256);
      assert.notEqual(pair[0].prompt.sha256, pair[1].prompt.sha256);
    }
  }
});
