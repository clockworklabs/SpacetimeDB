import assert from 'node:assert/strict';
import test from 'node:test';

import { verifyPromptSnapshot } from '../prompt-snapshot.mjs';

test('the exact model prompts match the reviewed, symmetric L1/L2 guidance snapshot without Docker', () => {
  const snapshot = verifyPromptSnapshot();
  assert.equal(snapshot.prompts.length, 27);
  for (const stack of ['mongodb', 'postgres', 'spacetime']) {
    for (const mode of ['build', 'fix', 'upgrade']) {
      const entries = snapshot.prompts.filter(entry => entry.stack === stack
        && entry.round.mode === mode);
      assert.equal(entries.length, 3);
      const byProfile = Object.fromEntries(entries.map(entry => [entry.profile, entry]));
      assert.deepEqual(Object.keys(byProfile).sort(),
        ['neutral@1.0.0', 'neutral@1.1.0', 'prescribed@1.0.0']);

      if (stack === 'spacetime') {
        assert.notEqual(byProfile['neutral@1.0.0'].backendMaterial.sha256,
          byProfile['neutral@1.1.0'].backendMaterial.sha256);
        assert.notEqual(byProfile['neutral@1.0.0'].prompt.sha256,
          byProfile['neutral@1.1.0'].prompt.sha256);
      } else {
        assert.equal(byProfile['neutral@1.0.0'].backendMaterial.sha256,
          byProfile['neutral@1.1.0'].backendMaterial.sha256);
        assert.equal(byProfile['neutral@1.0.0'].prompt.sha256,
          byProfile['neutral@1.1.0'].prompt.sha256);
      }

      assert.notEqual(byProfile['neutral@1.1.0'].backendMaterial.sha256,
        byProfile['prescribed@1.0.0'].backendMaterial.sha256);
      assert.notEqual(byProfile['neutral@1.1.0'].prompt.sha256,
        byProfile['prescribed@1.0.0'].prompt.sha256);
    }
  }
});
