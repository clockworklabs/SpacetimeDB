import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { AGENT_ADAPTER_SCHEMA_VERSION, createAgentAdapterRegistry } from './agent-adapter-contract.mjs';
import { sha256 } from './provenance.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const adapter = (id, entrypoint, defaultModel,
  { modes = ['build', 'upgrade', 'fix'], apiKeyEnvironmentVariable = null,
    credentialFiles = [], outboundDestinations = [], costLimit = 'unsupported' } = {}) => ({
  schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION,
  id, version: '1.0.0', entrypoint: join(ROOT, entrypoint), modes, defaultModel,
  apiKeyEnvironmentVariable, credentialFiles, outboundDestinations, costLimit,
  deadlineMs: 75 * 60_000,
});

export const AGENT_ADAPTER_REGISTRY = createAgentAdapterRegistry([
  adapter('claude-code', 'agent.mjs', 'claude-sonnet-5',
    { apiKeyEnvironmentVariable: 'ANTHROPIC_API_KEY',
      costLimit: 'native',
      credentialFiles: [join('.claude', '.credentials.json')],
      outboundDestinations: ['https://api.anthropic.com'] }),
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
        outboundDestinations: value.outboundDestinations })}\0`),
      readFileSync(value.entrypoint),
    ])),
  };
}
