import { describe, expect, it, vi } from 'vitest';

// Break the runtime/procedures import cycle; see index_prefix_filter.test.ts.
vi.mock('../src/server/procedures', () => ({
  callProcedure: () => {
    throw new Error('callProcedure is not stubbed for this test');
  },
}));

// The mocked host reports the database's identity as zero,
// and presents `jwt.payload` as the JWT of every connection.
const jwt = vi.hoisted(() => ({ payload: '' }));
vi.mock('spacetime:sys@2.0', async importOriginal => ({
  ...(await importOriginal<object>()),
  get_jwt_payload: () => new TextEncoder().encode(jwt.payload),
}));

import { ConnectionId } from '../src/lib/connection_id';
import { Identity } from '../src/lib/identity';
import { Timestamp } from '../src/lib/timestamp';
import { ReducerCtxImpl } from '../src/server/runtime';

function senderAuth(sender: Identity, connectionId: ConnectionId | null) {
  return new ReducerCtxImpl(sender, Timestamp.UNIX_EPOCH, connectionId, {})
    .senderAuth;
}

describe('senderAuth', () => {
  const database = Identity.zero();
  const client = new Identity(1n);
  const connectionId = new ConnectionId(5n);

  it('is internal exactly when the sender is the database', () => {
    expect(senderAuth(database, null).isInternal).toBe(true);
    expect(senderAuth(database, connectionId).isInternal).toBe(true);
    expect(senderAuth(client, null).isInternal).toBe(false);
    expect(senderAuth(client, connectionId).isInternal).toBe(false);
  });

  it('withholds the JWT from internal invocations', () => {
    jwt.payload = '{"iss":"https://example.com","sub":"alice"}';

    const external = senderAuth(client, connectionId);
    expect(external.hasJWT).toBe(true);
    expect(external.jwt?.subject).toBe('alice');
    expect(external.jwt?.identity.isEqual(client)).toBe(true);

    const internal = senderAuth(database, connectionId);
    expect(internal.hasJWT).toBe(false);
    expect(internal.jwt).toBeNull();
  });
});
