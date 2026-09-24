import { beforeAll, describe, expect, it, vi } from 'vitest';

vi.mock(
  'spacetime:sys@2.0',
  () => ({
    moduleHooks: Symbol('moduleHooks'),
  }),
  { virtual: true }
);

vi.mock('../src/server/runtime', () => ({
  makeHooks: () => ({}),
  callProcedure: () => new Uint8Array(),
  callUserFunction: (fn: (...args: any[]) => any, ...args: any[]) =>
    fn(...args),
  ReducerCtxImpl: class {},
  sys: {
    row_iter_bsatn_close: () => {},
  },
}));

describe('scoped views', () => {
  let schema: typeof import('../src/server/schema').schema;
  let table: typeof import('../src/lib/table').table;
  let t: typeof import('../src/lib/type_builders').t;

  beforeAll(async () => {
    ({ schema } = await import('../src/server/schema'));
    ({ table } = await import('../src/lib/table'));
    ({ t } = await import('../src/lib/type_builders'));
  });

  it('registers the body as an anonymous view with a key param and emits its scope', () => {
    const players = table(
      { name: 'players' },
      { identity: t.identity().primaryKey(), teamId: t.u64() }
    );
    const chatMessages = table(
      { name: 'chat_messages' },
      { id: t.u64().primaryKey(), teamId: t.u64().index(), text: t.string() }
    );
    const spacetimedb = schema({ players, chatMessages });

    const global = spacetimedb.anonymousView(
      { name: 'global', public: true },
      t.array(chatMessages.rowType),
      () => []
    );
    const team_chat = spacetimedb.scopedView(
      { name: 'team_chat', public: true, scope: t.u64() },
      t.array(chatMessages.rowType),
      ctx => ctx.db.players.identity.find(ctx.sender)?.teamId,
      (ctx, teamId) => Array.from(ctx.db.chatMessages.teamId.filter(teamId))
    );

    const raw = spacetimedb.buildRawModuleDefV10({ global, team_chat });

    const views = raw.sections.find(section => section.tag === 'Views')?.value;
    const teamChat = views?.find(view => view.sourceName === 'team_chat');
    expect(teamChat).toEqual(
      expect.objectContaining({ isAnonymous: true, index: 1 })
    );
    expect(teamChat?.params.elements).toEqual([
      expect.objectContaining({ name: 'key', algebraicType: { tag: 'U64' } }),
    ]);

    const scopedViews = raw.sections.find(
      section => section.tag === 'ScopedViews'
    )?.value;
    expect(scopedViews).toEqual([
      { viewSourceName: 'team_chat', resolverIndex: 0 },
    ]);
  });

  it('omits the ScopedViews section for modules without scoped views', () => {
    const things = table({ name: 'things' }, { id: t.u32().primaryKey() });
    const spacetimedb = schema({ things });
    const all = spacetimedb.anonymousView(
      { name: 'all', public: true },
      t.array(things.rowType),
      ctx => Array.from(ctx.db.things.iter())
    );

    const raw = spacetimedb.buildRawModuleDefV10({ all });
    expect(
      raw.sections.find(section => section.tag === 'ScopedViews')
    ).toBeUndefined();
  });
});
