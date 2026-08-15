#!/usr/bin/env node
// Captures the exact prompt bytes that the coding agent would receive for the
// qualified ecommerce ladder. This is intentionally model-free and Docker-free:
// release review must be able to see a prompt change before spending money.

import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { resolveGuidanceProfile } from './condition-compiler.mjs';
import { canonicalDefinitionJson } from './definition-plan.mjs';
import { sha256 } from './provenance.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const AGENT = join(ROOT, 'agent.mjs');
export const PROMPT_SNAPSHOT = join(ROOT, 'conditions', 'qualification',
  'ecommerce-l1-l2-prompts.json');
const STACKS = ['mongodb', 'postgres', 'spacetime'];
const PROFILES = [
  { reference: 'neutral@1.0.0', mode: 'neutral' },
  { reference: 'prescribed@1.0.0', mode: 'prescribed' },
];
const ROUNDS = [
  { mode: 'build', level: 1 },
  { mode: 'fix', level: 1 },
  { mode: 'upgrade', level: 2 },
];

function section(prompt, start, ends) {
  const from = prompt.indexOf(start);
  if (from < 0 || prompt.indexOf(start, from + start.length) >= 0) {
    throw new Error(`prompt has a missing or repeated section marker: ${start}`);
  }
  const bodyStart = from + start.length;
  const candidates = ends.map(end => prompt.indexOf(end, bodyStart)).filter(index => index >= 0);
  const to = candidates.length ? Math.min(...candidates) : prompt.length;
  return prompt.slice(bodyStart, to).trim();
}

function promptBytes({ profile, stack, round }) {
  const args = [AGENT, '--mode', round.mode, '--backend', stack, '--track', 'ecommerce',
    '--level', String(round.level), '--run-index', '0', '--app', '/prompt-review/app',
    '--guidance', profile.mode,
    '--guidance-document-json', JSON.stringify(profile.guidance.documents[stack]),
    '--skills-json', JSON.stringify(profile.guidance.skills[stack].ids), '--print-prompt'];
  return execFileSync(process.execPath, args, {
    encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, STACK_BENCH_APPLIANCE: '1',
      STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

export function capturePromptSnapshot() {
  const guidance = new Map(PROFILES.map(profile => [profile.reference,
    resolveGuidanceProfile(profile.reference, STACKS)]));
  const prompts = [];
  for (const profile of PROFILES) {
    const resolved = guidance.get(profile.reference);
    for (const stack of STACKS) {
      for (const round of ROUNDS) {
        const prompt = promptBytes({ profile: { ...profile, guidance: resolved }, stack, round });
        const backendMaterial = section(prompt, '## Backend access and API material',
          ['## Selected API reference', '## Requested application']);
        prompts.push({
          profile: profile.reference,
          mode: profile.mode,
          stack,
          round,
          document: resolved.documents[stack],
          skills: resolved.skills[stack],
          backendMaterial: { sha256: sha256(backendMaterial),
            bytes: Buffer.byteLength(backendMaterial) },
          prompt: { sha256: sha256(prompt), bytes: Buffer.byteLength(prompt) },
        });
      }
    }
  }
  const content = {
    schemaVersion: 1,
    kind: 'prompt-snapshot',
    id: 'ecommerce-l1-l2-guidance',
    version: '1.0.0',
    topology: { platform: 'linux/amd64', hostAlias: '127.0.0.1', runIndex: 0 },
    prompts,
  };
  return { ...content, contentSha256: sha256(canonicalDefinitionJson(content)) };
}

export function verifyPromptSnapshot(expected = JSON.parse(readFileSync(PROMPT_SNAPSHOT, 'utf8'))) {
  const actual = capturePromptSnapshot();
  if (canonicalDefinitionJson(actual) !== canonicalDefinitionJson(expected)) {
    throw new Error('rendered prompts changed; inspect the diff and deliberately refresh with '
      + '`node prompt-snapshot.mjs --write`');
  }
  return actual;
}

function main() {
  if (process.argv.length > 3 || (process.argv[2] && process.argv[2] !== '--write')) {
    throw new Error('usage: node prompt-snapshot.mjs [--write]');
  }
  if (process.argv[2] === '--write') {
    const snapshot = capturePromptSnapshot();
    mkdirSync(dirname(PROMPT_SNAPSHOT), { recursive: true });
    writeFileSync(PROMPT_SNAPSHOT, `${JSON.stringify(snapshot, null, 2)}\n`);
    console.log(`wrote ${PROMPT_SNAPSHOT}\n${snapshot.contentSha256}`);
    return;
  }
  const snapshot = verifyPromptSnapshot();
  console.log(`prompt snapshot verified: ${snapshot.contentSha256} (${snapshot.prompts.length} prompts)`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
