import { execFileSync } from 'node:child_process';

import { leasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import { resolveSpacetimeModuleLayout } from '../../runtime/spacetime-layout.js';
import { CODING_CONTAINER_SPACETIME_CLI, codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../../runtime/coding-container-policy.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = (value: unknown): string => `'${String(value).replaceAll("'", "''")}'`;
const sqlIdentifier = (value: unknown): string => {
  const name = String(value);
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
    throw new Error(`SpacetimeDB schema contains an unsupported identifier: ${name}`);
  }
  return name;
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

type SpacetimeLease = { resources: { module: string; serverUri: string } };
const agentExec = () => ['exec', ...codingContainerAgentExecOptions()];

function stringColumns(schema: unknown): Array<{ table: string; column: string }> {
  if (!record(schema) || !Array.isArray(schema.sections)) {
    throw new Error('SpacetimeDB describe returned an invalid schema');
  }
  const typespace = schema.sections.find(section => record(section) && record(section.Typespace));
  const tablesSection = schema.sections.find(section => record(section) && Array.isArray(section.Tables));
  const types = record(typespace) && record(typespace.Typespace) && Array.isArray(typespace.Typespace.types)
    ? typespace.Typespace.types : null;
  const tables = record(tablesSection) && Array.isArray(tablesSection.Tables) ? tablesSection.Tables : null;
  if (!types || !tables) throw new Error('SpacetimeDB describe omitted tables or types');
  const columns: Array<{ table: string; column: string }> = [];
  for (const table of tables) {
    if (!record(table) || typeof table.source_name !== 'string'
      || !Number.isInteger(table.product_type_ref)) continue;
    const type = types[Number(table.product_type_ref)];
    const elements = record(type) && record(type.Product) && Array.isArray(type.Product.elements)
      ? type.Product.elements : [];
    for (const element of elements) {
      if (!record(element) || !record(element.name) || typeof element.name.some !== 'string'
        || !record(element.algebraic_type) || !('String' in element.algebraic_type)) continue;
      columns.push({ table: sqlIdentifier(table.source_name), column: sqlIdentifier(element.name.some) });
    }
  }
  return columns;
}

export function proveSpacetimeUse({ lease, marker, exec = execFileSync }:
  { lease: SpacetimeLease; marker: unknown; exec?: TextCommandExecutor }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  if (typeof marker !== 'string' || !marker) {
    throw new Error('SpacetimeDB provenance requires a non-empty application marker');
  }
  const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
  const container = target.buildContainer;
  if (!container) throw new Error('leased build container is not active');
  if (target.mod !== lease.resources.module || target.uri !== lease.resources.serverUri) {
    throw new Error('SpacetimeDB provenance target does not match the authenticated lease');
  }
  const run = (args: string[]): string => exec('docker', [...agentExec(), container.id,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI, args)],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  const schema = JSON.parse(run(['describe', '--json', target.mod, '-s', target.containerUri]));
  let matches = 0;
  for (const { table, column } of stringColumns(schema)) {
    const output = run(['sql', target.mod, '-s', target.containerUri,
      `select ${column} from ${table} where ${column} = ${sqlString(marker)}`]);
    if (output.includes(`"${marker}"`)) matches += 1;
  }
  return { ok: matches > 0, verified: true, matches,
    reason: matches
      ? 'the application marker exists in the leased SpacetimeDB module'
      : 'the application marker is absent from the leased SpacetimeDB module' };
}

export function resetSpacetime({ lease, app, exec = execFileSync }:
  { lease: SpacetimeLease; app: string; exec?: TextCommandExecutor }): string {
  const layout = resolveSpacetimeModuleLayout(app);
  const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
  const container = target.buildContainer;
  if (!container) throw new Error('leased build container is not active');
  const containerModule = layout.containerPath;
  try {
    exec('docker', [...agentExec(), container.id,
      ...codingContainerAgentCommand('test', ['-d', containerModule])],
      { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    exec('docker', [...agentExec(), '-w', containerModule, container.id,
      ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,
        ['publish', lease.resources.module, '--module-path', containerModule,
          '-s', target.containerUri, '--delete-data', '-y'])],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const detail = streams(error, 'stdout', 'stderr').trim() || streams(error, 'message');
    throw new Error(`SpacetimeDB reset publish failed in leased container: ${detail}`, { cause: error });
  }
  return `reset spacetime module ${lease.resources.module} on ${lease.resources.serverUri}`;
}

export function setSpacetimeStock({ item, warehouse, quantity, spacetime,
  exec = execFileSync }: {
  item: string; warehouse: string; quantity: number; exec?: TextCommandExecutor;
  spacetime?: { buildContainer?: { id: string; name: string } | null; mod: string; containerUri: string };
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = spacetime?.buildContainer;
  if (!container) {
    throw new Error('SpacetimeDB build container is unavailable for direct SQL');
  }
  const query = (sql: string): string => exec('docker', [...agentExec(), container.id,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,
      ['sql', spacetime.mod, '-s', spacetime.containerUri, sql])],
  { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  const idOf = (table: string, name: string): string => {
    const output = query(`select id from ${table} where name = ${sqlString(name)}`);
    const match = output.match(/^\s*(\d+)\s*$/m);
    if (!match?.[1]) throw new Error(`no ${table} named "${name}" — is the schema as the spec requires?`);
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
  cli, expectedServerUri, expectedModule }: {
  lease: SpacetimeLease; name: string; wipe: boolean; exec?: TextCommandExecutor;
  cli: string; expectedServerUri: string; expectedModule: string;
}): string {
  if (lease.resources.serverUri !== expectedServerUri
    || lease.resources.module !== expectedModule) {
    throw new Error('SpacetimeDB lease target does not match the harness run');
  }
  if (!wipe) return name;
  try {
    exec(cli, ['delete', lease.resources.module, '-s', lease.resources.serverUri, '-y'],
      { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    console.error(`  deleted module ${lease.resources.module} — a build starts with none published`);
  } catch (error) {
    const detail = streams(error, 'stdout', 'stderr', 'message');
    if (!/(?:404\s+Not Found|no such database|failed to find database)/i.test(detail)) {
      throw new Error(`could not delete prior module ${lease.resources.module}: ${detail.trim().split('\n')[0]}`,
        { cause: error });
    }
  }
  return name;
}
