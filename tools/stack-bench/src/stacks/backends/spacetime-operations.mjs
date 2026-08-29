import { execFileSync } from 'node:child_process';

import { leasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import { resolveSpacetimeModuleLayout } from '../../runtime/spacetime-layout.js';
import { codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../../runtime/coding-container-policy.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = value => `'${String(value).replaceAll("'", "''")}'`;
const agentExec = () => ['exec', ...codingContainerAgentExecOptions()];

export function resetSpacetime({ lease, app, exec = execFileSync }) {
  const layout = resolveSpacetimeModuleLayout(app);
  const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
  const container = target.buildContainer;
  const containerModule = layout.containerPath;
  try {
    exec('docker', [...agentExec(), container.name,
      ...codingContainerAgentCommand('test', ['-d', containerModule])],
      { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    exec('docker', [...agentExec(), '-w', containerModule, container.name,
      ...codingContainerAgentCommand('/deps/spacetimedb-cli',
        ['publish', lease.resources.module, '--module-path', containerModule,
          '-s', target.containerUri, '--delete-data', '-y'])],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const detail = `${error.stdout ?? ''}${error.stderr ?? ''}`.trim() || error.message;
    throw new Error(`SpacetimeDB reset publish failed in leased container: ${detail}`, { cause: error });
  }
  return `reset spacetime module ${lease.resources.module} on ${lease.resources.serverUri}`;
}

export function setSpacetimeStock({ item, warehouse, quantity, spacetime, exec = execFileSync }) {
  if (!spacetime?.buildContainer) {
    throw new Error('SpacetimeDB build container is unavailable for direct SQL');
  }
  const query = sql => exec('docker', [...agentExec(), spacetime.buildContainer.name,
    ...codingContainerAgentCommand('/deps/spacetimedb-cli',
      ['sql', spacetime.mod, '-s', spacetime.containerUri, sql])],
  { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  const idOf = (table, name) => {
    const output = query(`select id from ${table} where name = ${sqlString(name)}`);
    const match = output.match(/^\s*(\d+)\s*$/m);
    if (!match) throw new Error(`no ${table} named "${name}" — is the schema as the spec requires?`);
    return match[1];
  };
  const itemId = idOf('item', item);
  const warehouseId = idOf('warehouse', warehouse);
  query(`update stock set quantity = ${quantity} where item_id = ${itemId} and warehouse_id = ${warehouseId}`);
  const verified = query(`select quantity from stock where item_id = ${itemId} and warehouse_id = ${warehouseId}`);
  if (!new RegExp(`^\\s*${quantity}\\s*$`, 'm').test(verified)) {
    throw new Error(`stock row for ${item} / ${warehouse} did not update to ${quantity}`);
  }
  return { backend: 'spacetime', item, warehouse, quantity };
}

export function prepareSpacetimeDatabase({ lease, name, wipe, exec = execFileSync,
  cli, expectedServerUri, expectedModule }) {
  if (lease.resources.serverUri !== expectedServerUri
    || lease.resources.module !== expectedModule) {
    throw new Error('SpacetimeDB lease target does not match the harness run');
  }
  if (!wipe) return name;
  try {
    exec(cli, ['delete', lease.resources.module, '-s', lease.resources.serverUri, '-y'],
      { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    console.error(`  deleted module ${lease.resources.module} — a build starts with none published`);
  } catch (error) {
    const detail = `${error.stdout ?? ''}${error.stderr ?? ''}${error.message ?? ''}`;
    if (!/(?:404\s+Not Found|no such database|failed to find database)/i.test(detail)) {
      throw new Error(`could not delete prior module ${lease.resources.module}: ${detail.trim().split('\n')[0]}`,
        { cause: error });
    }
  }
  return name;
}
