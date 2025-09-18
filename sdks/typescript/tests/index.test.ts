import { describe, it } from 'vitest';
import type { IdentityTokenMessage } from 'spacetimedb/sdk';
import { ConnectionId, Identity } from 'spacetimedb';

describe('spacetimedb', () => {
  it('imports something from the spacetimedb sdk', () => {
    const msg: IdentityTokenMessage = {
      tag: 'IdentityToken',
      identity: Identity.fromString('0xc200000000000000000000000000000000000000000000000000000000000000'),
      token: 'some-token',
      connectionId: ConnectionId.fromString('0x00000000000000000000000000000000'),
    };
  });
});
