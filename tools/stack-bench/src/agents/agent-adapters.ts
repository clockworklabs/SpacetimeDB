import { readFileSync } from 'node:fs';
import { join, sep } from 'node:path';

import { AGENT_ADAPTER_SCHEMA_VERSION, createAgentAdapterRegistry } from './agent-adapter-contract.js';
import type {
  AgentAdapter,
  AgentCostLimit,
  AgentMode,
} from './agent-adapter-contract.js';
import { AGENT_PROCESS_TIMEOUT_MS } from './coding-session-timeouts.js';
import { DEFAULT_THROTTLE_MAX_WAIT_MS } from './coding-session-recovery.js';
import { sha256 } from '../evidence/provenance.js';

import { compiledEntrypoint } from '../package-root.js';

interface AdapterOptions {
  modes?: AgentMode[];
  apiKeyEnvironmentVariable?: string | null;
  credentialEnvironmentVariables?: string[];
  credentialFiles?: string[];
  outboundDestinations?: string[];
  requiredExecutables?: string[];
  credentialStatusCommand?: string[] | null;
  usesStackSkills?: boolean;
  costLimit?: AgentCostLimit;
  version?: string;
  deadlineMs?: number;
}

export interface AgentAdapterIdentity extends Record<string, unknown> {
  id: string;
  version: string;
  sha256: string;
}
const CLAUDE_SUBSCRIPTION_STATUS_COMMAND = ['node', '-e',
  "const {spawnSync}=require('node:child_process');"
  + "const r=spawnSync('claude',['auth','status','--json'],{encoding:'utf8'});"
  + "let s=null;try{s=JSON.parse(r.stdout)}catch{};"
  + "process.exit(r.status===0&&s?.loggedIn===true&&['claude.ai','oauth_token'].includes(s?.authMethod)?0:1)"];
const adapter = (id: string, entrypoint: string, defaultModel: string,
  { modes = ['build', 'upgrade', 'resume', 'fix'], apiKeyEnvironmentVariable = null,
    credentialEnvironmentVariables = [], credentialFiles = [], outboundDestinations = [],
    requiredExecutables = [],
    credentialStatusCommand = null, usesStackSkills = false,
    costLimit = 'unsupported', version = '1.0.0',
    deadlineMs = 75 * 60_000 }: AdapterOptions = {}): AgentAdapter => ({
  schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION,
  id, version, entrypoint: compiledEntrypoint(...entrypoint.split(sep)), modes, defaultModel,
  apiKeyEnvironmentVariable, credentialEnvironmentVariables, credentialFiles,
  outboundDestinations, requiredExecutables, credentialStatusCommand, usesStackSkills, costLimit,
  deadlineMs,
});

export const AGENT_ADAPTER_REGISTRY = createAgentAdapterRegistry([
  adapter('claude-code', join('commands', 'agent.js'), 'claude-sonnet-5',
    { apiKeyEnvironmentVariable: 'ANTHROPIC_API_KEY',
      credentialEnvironmentVariables: ['CLAUDE_CODE_OAUTH_TOKEN'],
      costLimit: 'native',
      outboundDestinations: ['https://api.anthropic.com'], requiredExecutables: ['claude'],
      // `claude auth status --json` exits zero even when it reports
      // `loggedIn:false`; the adapter command must turn semantic logout into a
      // failed preflight without making a provider request.
      credentialStatusCommand: CLAUDE_SUBSCRIPTION_STATUS_COMMAND,
      usesStackSkills: true, version: '1.17.0',
      // Claude can wait through an account throttle. Local adapters keep the
      // shorter default deadline because they have no provider wait state.
      deadlineMs: AGENT_PROCESS_TIMEOUT_MS + DEFAULT_THROTTLE_MAX_WAIT_MS + 10 * 60_000 }),
  adapter('deterministic', join('fixtures', 'stub-agent.js'), 'deterministic',
    { costLimit: 'non-billable', version: '1.3.0' }),
  adapter('fault-injection', join('fixtures', 'fault-agent.js'), 'fault-injection',
    { modes: ['build'], costLimit: 'non-billable', version: '1.2.0' }),
  adapter('reference-fixture', join('src', 'references', 'reference-agent.js'), 'reference-fixture',
    { modes: ['build', 'upgrade', 'fix'], costLimit: 'non-billable', version: '1.4.0' }),
]);

export function agentAdapterIdentity(value: AgentAdapter): AgentAdapterIdentity {
  return {
    id: value.id,
    version: value.version,
    sha256: sha256(Buffer.concat([
      Buffer.from(`${JSON.stringify({ schemaVersion: value.schemaVersion, id: value.id,
        version: value.version, modes: value.modes, deadlineMs: value.deadlineMs,
        defaultModel: value.defaultModel,
        costLimit: value.costLimit,
        apiKeyEnvironmentVariable: value.apiKeyEnvironmentVariable,
        credentialEnvironmentVariables: value.credentialEnvironmentVariables,
        credentialFiles: value.credentialFiles,
        outboundDestinations: value.outboundDestinations,
        requiredExecutables: value.requiredExecutables,
        credentialStatusCommand: value.credentialStatusCommand,
        usesStackSkills: value.usesStackSkills })}\0`),
      readFileSync(value.entrypoint),
    ])),
  };
}
