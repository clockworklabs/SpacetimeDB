import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { Script } from 'node:vm';
import { z } from 'zod';
import { hashAppSource } from '../../runtime/source-snapshot.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { CODING_CONTAINER_SPACETIME_CLI, codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../../runtime/coding-container-policy.js';
import { assertLeasedContainer } from '../backend-reset-guard.js';
import { orderCheckoutStateSchema } from '../checkout-state.js';

const mappingSchema = z.strictObject({ sourceSha256: z.string().regex(/^[a-f0-9]{64}$/),
  tables: z.array(z.string().regex(/^[A-Za-z_][A-Za-z0-9_]*$/)).nonempty(), convert: z.string().min(1) });
const tableSchema = z.strictObject({ inserts: z.array(z.record(z.string(), z.unknown())), deletes: z.array(z.unknown()).length(0) });
const resultSchema = z.strictObject({ accountMatches: z.literal(1), itemMatches: z.literal(1), state: orderCheckoutStateSchema });

// The operator's reviewed conversion program is trusted, like the saved SQL
// reader. The VM timeout bounds mistakes; it is not a security boundary.
export function getSavedSpacetimeCheckoutState({ account, item, app, reader, spacetime, exec = execFileSync }: {
  account: string; item: string; app: string; reader: { path: string; sha256: string }; exec?: TextCommandExecutor;
  spacetime?: { buildContainer?: { id: string; name: string } | null; mod: string; containerUri: string };
}) {
  const bytes = readFileSync(reader.path), readerSha256 = createHash('sha256').update(bytes).digest('hex');
  if (readerSha256 !== reader.sha256) throw new Error('saved SpacetimeDB reader hash mismatch');
  const mapping = mappingSchema.parse(JSON.parse(bytes.toString('utf8')));
  if (hashAppSource(app).sha256 !== mapping.sourceSha256) throw new Error('saved SpacetimeDB source hash mismatch');
  if (!account || !item || account.includes('\0') || item.includes('\0')) throw new Error('invalid checkout account or item');
  if (!spacetime?.buildContainer) throw new Error('SpacetimeDB build container is unavailable for saved snapshot');
  if (new Set(mapping.tables).size !== mapping.tables.length) throw new Error('duplicate saved snapshot tables');
  const container = assertLeasedContainer(spacetime.buildContainer, exec, 60_000, 'saved checkout state read');
  const output = exec('docker', ['exec', ...codingContainerAgentExecOptions(), container,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI, ['subscribe', spacetime.mod, '-s', spacetime.containerUri,
      '--print-initial-update', '--num-updates', '0', '--timeout', '30', ...mapping.tables.map(table => `SELECT * FROM ${table}`)])],
  { encoding: 'utf8', stdio: 'pipe', timeout: 60_000 });
  const snapshot = z.record(z.string(), tableSchema).parse(JSON.parse(output.trim()));
  if (Object.keys(snapshot).some(table => !mapping.tables.includes(table))) throw new Error('unexpected saved snapshot table');
  // Successful initial subscriptions can omit empty tables. Unknown table names
  // make the native CLI fail, rather than silently becoming empty state.
  const tables = Object.fromEntries(mapping.tables.map(table => [table, snapshot[table]?.inserts ?? []]));
  const converted = new Script(`(${mapping.convert})(tables, account, item)`).runInNewContext({ tables, account, item }, { timeout: 1000 });
  const { state } = resultSchema.parse(converted);
  return { state, schemaSha256: { source: mapping.sourceSha256, reader: readerSha256 }, scope: 'orders' as const };
}
