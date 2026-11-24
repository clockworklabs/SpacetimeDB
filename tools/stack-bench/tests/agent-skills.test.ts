import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';

import { installAgentSkills } from '../container/agent-skills.js';
import { CODING_PROVIDERS } from '../container/coding-providers.js';

const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const skills = [{ id: 'typescript-client', text: '---\nname: typescript-client\n---\nclient\n' },
  { id: 'cli', text: '---\nname: cli\n---\ncli\n' }];

test('each skill is written as the agent into its own folder and checked by hash', () => {
  const calls: { args: string[]; input: string }[] = [];
  installAgentSkills({ containerName: 'build', user: '10001:10001', directory: '/home/developer/.codex/skills',
    skills, env: {}, timeoutMs: 1_000, run: (_command, args, { input }) => {
      calls.push({ args, input });
      return { status: 0, stdout: `${sha(input)}  ${args.at(-1)}/SKILL.md\n` };
    } });
  assert.deepEqual(calls.map(call => [call.args.slice(0, 5), call.args.at(-1), call.input]), [
    [['exec', '-i', '--user', '10001:10001', 'build'], '/home/developer/.codex/skills/typescript-client', skills[0]!.text],
    [['exec', '-i', '--user', '10001:10001', 'build'], '/home/developer/.codex/skills/cli', skills[1]!.text],
  ]);
});

test('a failed or altered copy stops the session before it starts', () => {
  const install = (run: Parameters<typeof installAgentSkills>[0]['run']) => installAgentSkills({ containerName: 'build',
    user: '10001:10001', directory: '/home/developer/.grok/skills', skills, env: {}, timeoutMs: 1_000, run });
  assert.throws(() => install(() => ({ status: 1, stdout: '' })), /could not install the typescript-client skill/);
  assert.throws(() => install(() => ({ status: 0, stdout: `${sha('other')}  SKILL.md\n` })), /typescript-client/);
});

test('every coding provider names a skills folder in the agent home', () => {
  assert.deepEqual(Object.fromEntries(Object.entries(CODING_PROVIDERS).map(([id, value]) => [id, value.skillsDirectory])), {
    anthropic: '/home/developer/.claude/skills',
    openai: '/home/developer/.codex/skills',
    openrouter: '/home/developer/.codex/skills',
    xai: '/home/developer/.grok/skills',
    google: '/home/developer/.gemini/config/skills',
  });
});
