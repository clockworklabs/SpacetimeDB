// SpacetimeDB chat-app client wrapper for the perf benchmark.
//
// The Level 12 generated module exposes:
//   reducer set_name({ name })             — register the calling identity as a user
//   reducer create_room({ name, isPrivate }) — create a public room (auto-joins creator)
//   reducer join_room({ roomId })            — join an existing public room
//   reducer send_message({ roomId, text })   — insert a message into a room
//
// Bindings live in ./module_bindings (regenerate via `spacetime generate ...`
// — see README). Each connection gets its own anonymous identity, so N writers =
// N independent connections, no per-user rate limiting.

import { DbConnection } from '../module_bindings/index.ts';

export interface StdbConfig {
  uri: string; // ws://localhost:3000
  moduleName: string; // chat-app-<timestamp>
}

export interface StdbHandle {
  conn: InstanceType<typeof DbConnection>;
  close(): void;
}

export async function connectStdb(
  cfg: StdbConfig,
  opts: {
    onMessage?: (row: { id: bigint; roomId: bigint; text: string }) => void;
    subscriptions?: string[];
  } = {},
): Promise<StdbHandle> {
  const subscriptions = opts.subscriptions ?? [
    'SELECT * FROM user',
    'SELECT * FROM room',
    'SELECT * FROM room_member',
    'SELECT * FROM message',
  ];

  const conn = await new Promise<InstanceType<typeof DbConnection>>((resolve, reject) => {
    const c = DbConnection.builder()
      .withUri(cfg.uri)
      .withDatabaseName(cfg.moduleName)
      .onConnect((connection) => {
        if (subscriptions.length === 0) {
          resolve(connection);
          return;
        }
        connection
          .subscriptionBuilder()
          .onApplied(() => resolve(connection))
          .onError((ctx: { event?: Error }) => reject(ctx.event ?? new Error('subscription error')))
          .subscribe(subscriptions);
      })
      .onConnectError((_ctx: unknown, err: Error) => reject(err))
      .build();
    setTimeout(() => reject(new Error('stdb connect timeout')), 15_000);
    void c;
  });

  if (opts.onMessage) {
    // The accessor is the camelCase table name; the row type comes from the bindings.
    (conn.db as any).message.onInsert((_ctx: unknown, row: { id: bigint; roomId: bigint; text: string }) => {
      opts.onMessage!(row);
    });
  }

  return {
    conn,
    close: () => {
      try { (conn as any).disconnect?.(); } catch { /* ignore */ }
    },
  };
}

export async function stdbSetName(h: StdbHandle, name: string): Promise<void> {
  // 20260406 uses `setName`, 20260403 uses `register`
  const reducers = h.conn.reducers as any;
  if (typeof reducers.setName === 'function') {
    await reducers.setName({ name });
  } else if (typeof reducers.register === 'function') {
    await reducers.register({ name });
  } else {
    throw new Error('No setName or register reducer found');
  }
}

export async function stdbCreateRoom(h: StdbHandle, name: string): Promise<void> {
  await (h.conn.reducers as any).createRoom({ name, isPrivate: false });
}

export async function stdbJoinRoom(h: StdbHandle, roomId: bigint): Promise<void> {
  await (h.conn.reducers as any).joinRoom({ roomId });
}

export async function stdbSendMessage(h: StdbHandle, roomId: bigint, text: string): Promise<void> {
  await (h.conn.reducers as any).sendMessage({ roomId, text });
}

// Look up a room id by name (after subscribing to the room table).
export function stdbFindRoomIdByName(h: StdbHandle, name: string): bigint | null {
  const rows = [...((h.conn.db as any).room.iter() as Iterable<{ id: bigint; name: string }>)];
  const match = rows.find((r) => r.name === name);
  return match ? match.id : null;
}
