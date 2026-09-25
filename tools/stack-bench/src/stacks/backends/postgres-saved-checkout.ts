import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { z } from 'zod';
import { hashAppSource } from '../../runtime/source-snapshot.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { assertLeasedContainer, type LeasedDatabase } from '../backend-reset-guard.js';
import { orderCheckoutStateSchema } from '../checkout-state.js';
import { POSTGRES_APPLICATION_IDENTITY } from '../hosted-database-identity.js';

const mappingSchema = z.strictObject({ sourceSha256: z.string().regex(/^[a-f0-9]{64}$/), sql: z.string().min(1) });
const resultSchema = z.strictObject({ accountMatches: z.literal(1), itemMatches: z.literal(1), state: orderCheckoutStateSchema });

// The caller authenticates a PostgreSQL lease. This is trusted audit SQL bound
// to reviewed source, never SQL supplied by the generated application.
export function getSavedPostgresCheckoutState({ account, item, app, reader, lease, exec = execFileSync }: {
  account: string; item: string; app: string; reader: { path: string; sha256: string };
  lease: LeasedDatabase; exec?: TextCommandExecutor;
}) {
  const bytes = readFileSync(reader.path);
  const readerSha256 = createHash('sha256').update(bytes).digest('hex');
  if (readerSha256 !== reader.sha256) throw new Error('saved PostgreSQL reader hash mismatch');
  const mapping = mappingSchema.parse(JSON.parse(bytes.toString('utf8')));
  if (hashAppSource(app).sha256 !== mapping.sourceSha256) throw new Error('saved PostgreSQL source hash mismatch');
  if (!account || !item || account.includes('\0') || item.includes('\0')) throw new Error('invalid checkout account or item');
  const container = assertLeasedContainer(lease.resources.container, exec, 60_000, 'saved checkout state read');
  const output = exec('docker', ['exec', '-i', container, 'psql', '-X', '-qAt',
    '-U', POSTGRES_APPLICATION_IDENTITY.user, '-d', lease.resources.database,
    '-v', 'ON_ERROR_STOP=1', '-v', `account=${account}`, '-v', `item=${item}`], {
    encoding: 'utf8', stdio: 'pipe', timeout: 60_000,
    input: `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\n${mapping.sql}\n;\nCOMMIT;\n`,
  });
  const { state } = resultSchema.parse(JSON.parse(output.trim()));
  return { state, schemaSha256: { source: mapping.sourceSha256, reader: readerSha256 },
    scope: 'orders' as const };
}
