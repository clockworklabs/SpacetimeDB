import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { z } from 'zod';
import { hashAppSource } from '../../runtime/source-snapshot.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { assertLeasedContainer, type LeasedDatabase } from '../backend-reset-guard.js';
import { checkoutId, checkoutMinor, orderCheckoutStateSchema } from '../checkout-state.js';
import { attemptDatabaseIdentity } from '../hosted-database-identity.js';
import { redactCredentials } from '../../evidence/diagnostic-sanitizer.js';

const mappingSchema = z.strictObject({ sourceSha256: z.string().regex(/^[a-f0-9]{64}$/), script: z.string().min(1) });
const resultSchema = z.strictObject({ accountMatches: z.literal(1), itemMatches: z.literal(1), state: orderCheckoutStateSchema });

// Trusted operator code bound to reviewed source, never a generated-app mapper.
export function getSavedMongoDbCheckoutState({ account, item, app, reader, lease, exec = execFileSync }: {
  account: string; item: string; app: string; reader: { path: string; sha256: string };
  lease: LeasedDatabase; exec?: TextCommandExecutor;
}) {
  const bytes = readFileSync(reader.path);
  const readerSha256 = createHash('sha256').update(bytes).digest('hex');
  if (readerSha256 !== reader.sha256) throw new Error('saved MongoDB reader hash mismatch');
  const mapping = mappingSchema.parse(JSON.parse(bytes.toString('utf8')));
  if (hashAppSource(app).sha256 !== mapping.sourceSha256) throw new Error('saved MongoDB source hash mismatch');
  if (!account || !item || account.includes('\0') || item.includes('\0')) throw new Error('invalid checkout account or item');
  const container = assertLeasedContainer(lease.resources.container, exec, 60_000, 'saved checkout state read');
  const identity = lease.resources.network ? attemptDatabaseIdentity(lease.ownershipToken ?? '') : null;
  const authentication = identity ? ['--username', identity.user, '--password', identity.password,
    '--authenticationDatabase', lease.resources.database] : [];
  const script = `const account=${JSON.stringify(account)}, item=${JSON.stringify(item)};
    const minor=${checkoutMinor.toString()}, exactId=${checkoutId.toString()};
    const key=value=>exactId(value && typeof value.toHexString==='function' ? value.toHexString() : value);
    const session=db.getMongo().startSession();
    try {
      session.startTransaction({readConcern:{level:'snapshot'}});
      const store=session.getDatabase(db.getName());
      const result=(()=>{${mapping.script}\n})();
      print(JSON.stringify(result));
    } finally { try { session.abortTransaction(); } finally { session.endSession(); } }
  `;
  let output: string;
  try {
    output = exec('docker', ['exec', container, 'mongosh', lease.resources.database,
      ...authentication, '--quiet', '--eval', script], { encoding: 'utf8', stdio: 'pipe', timeout: 60_000 });
  } catch (error) {
    const stderr = error && typeof error === 'object' && 'stderr' in error ? String(error.stderr ?? '') : '';
    const withoutProgram = stderr.replaceAll(script, '[reader program]');
    const safe = redactCredentials(identity ? withoutProgram.replaceAll(identity.password, '[redacted credential]') : withoutProgram)
      .trim().slice(0, 600);
    throw new Error(`saved MongoDB checkout read failed: ${safe || 'database command failed without stderr'}`);
  }
  const { state } = resultSchema.parse(JSON.parse(output.trim()));
  return { state, schemaSha256: { source: mapping.sourceSha256, reader: readerSha256 }, scope: 'orders' as const };
}
