import { z } from 'zod';
import { addressEntriesSchema, type AddressEntry } from './migration-state.js';
import type { SpacetimeTarget } from './stack-grading-operations.js';

export type AddressBookRead = { status: number; text: string; entries: AddressEntry[] | null };

// This is the ordinary owner interface, not a reader for a physical table.
// Native SQL runs as the actor on a new connection; it never uses publisher auth.
export async function readAddressBook({ backend, url, spacetime }: {
  backend: string; url: string; spacetime?: SpacetimeTarget | null;
}, credentials: Readonly<Record<string, string>>, signal: AbortSignal,
  recordResponse: (text: string) => void, fetchImpl: typeof fetch = fetch): Promise<AddressBookRead> {
  const native = backend === 'spacetime';
  if (native ? !spacetime : !['postgres', 'mongodb'].includes(backend)) {
    throw new Error('address-book reader has no supported stack target');
  }
  const target = native ? `${spacetime!.uri}/v1/database/${spacetime!.mod}/sql`
    : `${url.replace(/\/$/, '')}/api/addresses`;
  const response = await fetchImpl(target, { method: native ? 'POST' : 'GET', signal,
    headers: { ...credentials, 'Content-Type': native ? 'text/plain' : 'application/json' },
    ...(native ? { body: 'SELECT id, name, address, is_default FROM my_addresses' } : {}) });
  const text = await response.text();
  recordResponse(text);
  if ([401, 403].includes(response.status)) return { status: response.status, text, entries: null };
  if (!response.ok) throw new Error(`address-book read returned HTTP ${response.status}`);
  const json: unknown = JSON.parse(text);
  const entries = native
    ? z.tuple([z.object({ rows: z.array(z.tuple([z.string().min(1), z.string(), z.string(), z.boolean()])) })])
      .parse(json)[0].rows.map(([id, name, address, isDefault]) => ({ id, name, address, isDefault }))
    : z.object({ entries: addressEntriesSchema }).parse(json).entries;
  return { status: response.status, text, entries };
}
