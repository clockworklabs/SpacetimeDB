import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { agentSkillPaths, normalizePromptText, readAgentSkillDocuments,
  selectAgentSkills } from '../src/agents/agent-materials.js';

test('stack defaults and explicit agent skill selections resolve predictably', () => {
  assert.deepEqual(selectAgentSkills(['typescript-server'], null), ['typescript-server']);
  assert.deepEqual(selectAgentSkills(['typescript-server'], []), []);
  assert.deepEqual(selectAgentSkills([], ['typescript-server']), ['typescript-server']);
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

test('benchmark workflows resolve separately from public SDK skills in selected order', () => {
  const ids = ['typescript-server', 'spacetime-dev', 'spacetime-managed-dev'];
  const paths = [join('/repo', 'skills', 'typescript-server', 'SKILL.md'),
    join('/repo', 'tools', 'stack-bench', 'backends', 'workflows', 'spacetime-dev.md'),
    join('/repo', 'tools', 'stack-bench', 'backends', 'workflows', 'spacetime-managed-dev.md')];
  assert.deepEqual(agentSkillPaths('/repo', ids), paths);
  assert.equal(readAgentSkillDocuments('/repo', ids, {
    read: path => `---\nname: ignored\n---\n${ids[paths.indexOf(path)]}`,
  }), ids.join('\n\n---\n\n'));
});

test('prompt material is identical across platform line endings', () => {
  assert.equal(normalizePromptText('one\r\ntwo\rthree\n'), 'one\ntwo\nthree\n');
  const text = readAgentSkillDocuments('/repo', ['typescript-server'], {
    read: () => '---\r\nname: ignored\r\n---\r\nreference\r\nline\r\n',
  });
  assert.equal(text, 'reference\nline\n');
});
