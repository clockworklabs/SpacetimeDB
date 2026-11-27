import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

export interface AgentSkillFile { id: string; text: string }

type Run = (command: string, args: string[], options: { input: string; encoding: 'utf8';
  env: NodeJS.ProcessEnv; timeout: number }) => { status: number | null; stdout: string };

// Prints the written file's hash, so a truncated or failed copy cannot pass.
const INSTALL_SKILL = 'set -e; umask 077; mkdir -p "$1"; cat > "$1/SKILL.md"; sha256sum "$1/SKILL.md"';

// Installs each skill as <directory>/<id>/SKILL.md, owned by the agent, where its CLI lists it.
export function installAgentSkills({ containerName, user, directory, skills, env, timeoutMs,
  run = spawnSync as unknown as Run }: { containerName: string; user: string; directory: string;
  skills: readonly AgentSkillFile[]; env: NodeJS.ProcessEnv; timeoutMs: number; run?: Run }): void {
  for (const { id, text } of skills) {
    const result = run('docker', ['exec', '-i', '--user', user, containerName, 'sh', '-c', INSTALL_SKILL,
      'install-skill', `${directory}/${id}`], { input: text, encoding: 'utf8', env, timeout: timeoutMs });
    const expected = createHash('sha256').update(text, 'utf8').digest('hex');
    if (result.status !== 0 || result.stdout.trim().split(/\s+/)[0] !== expected) {
      throw new Error(`could not install the ${id} skill in ${directory}`);
    }
  }
}
