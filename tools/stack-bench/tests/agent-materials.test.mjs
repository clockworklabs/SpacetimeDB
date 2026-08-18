import assert from 'node:assert/strict';
import test from 'node:test';

import { agentSkillPaths, normalizePromptText, readAgentSkillDocuments,
  selectAgentSkills } from '../agent-materials.mjs';

test('stack defaults and explicit agent skill selections resolve predictably', () => {
  assert.deepEqual(selectAgentSkills(['typescript-server'], null), ['typescript-server']);
  assert.deepEqual(selectAgentSkills(['typescript-server'], []), []);
  assert.deepEqual(selectAgentSkills([], ['typescript-server']), []);
  assert.throws(() => selectAgentSkills(['../private'], null), /invalid/);
  assert.throws(() => agentSkillPaths('/repo', ['same', 'same']), /invalid/);
});

test('skill documents are read in selected order with front matter removed', () => {
  const paths = agentSkillPaths('/repo', ['typescript-server', 'typescript-client']);
  assert.equal(paths.length, 2);
  const text = readAgentSkillDocuments('/repo', ['typescript-server', 'typescript-client'], {
    read: path => `---\nname: ignored\n---\n${path.split(/[\\/]/).at(-2)}`,
  });
  assert.equal(text, 'typescript-server\n\n---\n\ntypescript-client');
});

test('prompt material is identical across platform line endings', () => {
  assert.equal(normalizePromptText('one\r\ntwo\rthree\n'), 'one\ntwo\nthree\n');
  const text = readAgentSkillDocuments('/repo', ['typescript-server'], {
    read: () => '---\r\nname: ignored\r\n---\r\nreference\r\nline\r\n',
  });
  assert.equal(text, 'reference\nline\n');
});
