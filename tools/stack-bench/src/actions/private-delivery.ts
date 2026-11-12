import { spacetimeFrameRows } from '../stacks/backends/spacetime-browser-session.js';

// A record one actor received: a JSON object, or a SpacetimeDB row. `kind` groups records of
// one shape (JSON keys or table); `fingerprint` identifies the exact record.
export interface DeliveredRecord {
  readonly atMs: number;
  readonly kind: string;
  readonly fingerprint: string;
  readonly bytes: number;
}

// Records new to the owner while its private write was the only change.
export interface DeliveryMark {
  readonly startedAtMs: number;
  readonly noisyKinds: ReadonlySet<string>;
  readonly seen: ReadonlySet<string>;
  privateRecords: DeliveredRecord[] | null;
}

const canonical = (value: unknown): string => Array.isArray(value)
  ? `[${value.map(canonical).join(',')}]`
  : value && typeof value === 'object'
    ? `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical((value as Record<string, unknown>)[key])}`).join(',')}}`
    : JSON.stringify(value) ?? 'null';

function jsonRecords(text: string, atMs: number, out: DeliveredRecord[]): void {
  // Socket.IO packets carry a numeric prefix; event streams carry data lines.
  const bodies = [text, text.replace(/^\d+/, ''), ...text.split('\n')
    .filter(line => line.startsWith('data:')).map(line => line.slice(5))];
  for (const body of bodies) {
    let value: unknown;
    try { value = JSON.parse(body); } catch { continue; }
    const walk = (item: unknown): void => {
      if (Array.isArray(item)) { item.forEach(walk); return; }
      if (!item || typeof item !== 'object') return;
      const entries = Object.entries(item);
      // One field alone is a status or a flag, not a record.
      if (entries.filter(([, field]) => field === null || typeof field !== 'object').length >= 2) {
        const fingerprint = canonical(item);
        out.push({ atMs, kind: `json:${Object.keys(item).sort().join(',')}`, fingerprint, bytes: Buffer.byteLength(fingerprint) });
      }
      for (const [, field] of entries) if (field && typeof field === 'object') walk(field);
    };
    walk(value);
    return;
  }
}

// Every record in the deliveries. A binary frame that is not a readable SpacetimeDB
// message is counted, so a caller can refuse to treat it as absent.
export async function deliveredRecords(deliveries: readonly { atMs: number; payload: string | Buffer }[]) {
  const records: DeliveredRecord[] = [];
  let unreadable = 0;
  for (const { atMs, payload } of deliveries) {
    if (typeof payload === 'string') { jsonRecords(payload, atMs, records); continue; }
    try {
      for (const { table, row } of await spacetimeFrameRows(payload)) {
        records.push({ atMs, kind: `table:${table}`, fingerprint: `row:${row.toString('hex')}`, bytes: row.length });
      }
    } catch {
      const text = payload.toString('utf8');
      const before = records.length;
      jsonRecords(text, atMs, records);
      if (records.length === before && !/^[\s\x20-\x7e]*$/.test(text)) unreadable++;
    }
  }
  return { records, unreadable };
}

// Small rows of plain integers can repeat by chance across tables; such a row must match
// within the same table. Longer rows and JSON objects match wherever they appear.
export function sameRecord(left: DeliveredRecord, right: DeliveredRecord): boolean {
  return left.fingerprint === right.fingerprint
    && (left.kind === right.kind || !left.kind.startsWith('table:') || left.bytes >= 24);
}

// Private candidates are at least this large: a short value is a flag, not a record.
export const MIN_PRIVATE_RECORD_BYTES = 8;
