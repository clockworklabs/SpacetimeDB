import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { AGENT_ADAPTER_SCHEMA_VERSION, createAgentAdapterRegistry } from './agent-adapter-contract.mjs';
import { sha256 } from './provenance.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const CLAUDE_SUBSCRIPTION_STATUS_COMMAND = ['node', '-e',
  "const {spawnSync}=require('node:child_process');"
  + "const r=spawnSync('claude',['auth','status','--json'],{encoding:'utf8'});"
  + "let s=null;try{s=JSON.parse(r.stdout)}catch{};"
  + "process.exit(r.status===0&&s?.loggedIn===true&&s?.authMethod==='claude.ai'?0:1)"];
const adapter = (id, entrypoint, defaultModel,
  { modes = ['build', 'upgrade', 'fix'], apiKeyEnvironmentVariable = null,
    credentialFiles = [], outboundDestinations = [], requiredExecutables = [],
    credentialStatusCommand = null, usesStackSkills = false,
    costLimit = 'unsupported', version = '1.0.0' } = {}) => ({
  schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION,
  id, version, entrypoint: join(ROOT, entrypoint), modes, defaultModel,
  apiKeyEnvironmentVariable, credentialFiles, outboundDestinations, requiredExecutables,
  credentialStatusCommand, usesStackSkills, costLimit,
  deadlineMs: 75 * 60_000,
});

export const AGENT_ADAPTER_REGISTRY = createAgentAdapterRegistry([
  adapter('claude-code', 'agent.mjs', 'claude-sonnet-5',
    { apiKeyEnvironmentVariable: 'ANTHROPIC_API_KEY',
      costLimit: 'native',
      credentialFiles: [join('.claude', '.credentials.json')],
      outboundDestinations: ['https://api.anthropic.com'], requiredExecutables: ['claude'],
      // `claude auth status --json` exits zero even when it reports
      // `loggedIn:false`; the adapter command must turn semantic logout into a
      // failed preflight without making a provider request.
      credentialStatusCommand: CLAUDE_SUBSCRIPTION_STATUS_COMMAND,
      usesStackSkills: true, version: '1.4.0' }),
  adapter('deterministic', join('fixtures', 'stub-agent.mjs'), 'deterministic', { costLimit: 'non-billable' }),
  adapter('fault-injection', join('fixtures', 'fault-agent.mjs'), 'fault-injection', { costLimit: 'non-billable' }),
  adapter('reference-fixture', 'reference-agent.mjs', 'reference-fixture',
    { modes: ['build'], costLimit: 'non-billable' }),
]);

export function agentAdapterIdentity(value) {
  return {
    id: value.id,
    version: value.version,
    sha256: sha256(Buffer.concat([
      Buffer.from(`${JSON.stringify({ schemaVersion: value.schemaVersion, id: value.id,
        version: value.version, modes: value.modes, deadlineMs: value.deadlineMs,
        defaultModel: value.defaultModel,
        costLimit: value.costLimit,
        apiKeyEnvironmentVariable: value.apiKeyEnvironmentVariable,
        credentialFiles: value.credentialFiles,
        outboundDestinations: value.outboundDestinations,
        requiredExecutables: value.requiredExecutables,
        credentialStatusCommand: value.credentialStatusCommand,
        usesStackSkills: value.usesStackSkills })}\0`),
      readFileSync(value.entrypoint),
    ])),
  };
}
