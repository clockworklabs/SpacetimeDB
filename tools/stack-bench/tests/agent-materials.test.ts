import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { agentSkillPaths, normalizePromptText, readAgentSkillDocuments, readAgentSkillFiles,
  selectAgentSkills } from '../src/agents/agent-materials.js';

test('stack defaults and explicit agent skill selections resolve predictably', () => {
  assert.deepEqual(selectAgentSkills(['typescript-server'], null), ['typescript-server']);
  assert.deepEqual(selectAgentSkills(['typescript-server'], []), []);
  assert.deepEqual(selectAgentSkills([], ['typescript-server']), ['typescript-server']);
  assert.throws(() => selectAgentSkills(['../private'], null), /invalid/);
  assert.throws(() => agentSkillPaths('/repo/tools/stack-bench', ['same', 'same']), /invalid/);
});

test('benchmark workflows resolve separately from public SDK skills in selected order', () => {
  const ids = ['typescript-server', 'spacetime-dev'];
  const paths = [join('/repo', 'skills', 'typescript-server', 'SKILL.md'),
    join('/repo', 'tools', 'stack-bench', 'backends', 'workflows', 'spacetime-dev.md')];
  assert.deepEqual(agentSkillPaths('/repo/tools/stack-bench', ids), paths);
  assert.deepEqual(agentSkillPaths('/opt/stack-bench', ids), [
    join('/skills', 'typescript-server', 'SKILL.md'),
    join('/opt/stack-bench/backends/workflows', 'spacetime-dev.md'),
  ]);
  assert.equal(readAgentSkillDocuments('/repo/tools/stack-bench', ids, {
    read: path => `---\nname: ignored\n---\n${ids[paths.indexOf(path)]}`,
  }), ids.join('\n\n---\n\n'));
  // A public skill may discuss tests; a workflow document StackBench wrote may not.
  const mentioning = (path: string) => paths.indexOf(path) === 0 ? 'Run the module tests.' : 'Start the watcher.';
  assert.doesNotThrow(() => readAgentSkillDocuments('/repo/tools/stack-bench', ids, { read: mentioning }));
  assert.throws(() => readAgentSkillDocuments('/repo/tools/stack-bench', ids,
    { read: path => paths.indexOf(path) === 1 ? 'Check the grader first.' : 'SDK notes.' }), /internal language/);
});

test('installed skill files keep their frontmatter in selected order', () => {
  const files = readAgentSkillFiles('/repo/tools/stack-bench', ['typescript-server', 'spacetime-dev'], {
    read: path => `---\r\nname: ${path.includes('workflows') ? 'spacetime-dev' : 'typescript-server'}\r\n---\r\nbody\r\n`,
  });
  assert.deepEqual(files, [
    { id: 'typescript-server', text: '---\nname: typescript-server\n---\nbody\n' },
    { id: 'spacetime-dev', text: '---\nname: spacetime-dev\n---\nbody\n' },
  ]);
  assert.throws(() => readAgentSkillFiles('/repo/tools/stack-bench', ['../private']), /invalid/);
});

test('prompt material is identical across platform line endings', () => {
  assert.equal(normalizePromptText('one\r\ntwo\rthree\n'), 'one\ntwo\nthree\n');
  const text = readAgentSkillDocuments('/repo/tools/stack-bench', ['typescript-server'], {
    read: () => '---\r\nname: ignored\r\n---\r\nreference\r\nline\r\n',
  });
  assert.equal(text, 'reference\nline\n');
});
