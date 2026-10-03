// A module whose case conversion policy is `None`, so the host uses each name
// as written, including a procedure's explicit name. It tests that a client
// importing this source gets the names that the host gives the module.
import { CaseConversionPolicy, schema, t, table } from 'spacetimedb/server';

const scoreEntry = table(
  {
    public: true,
    indexes: [
      {
        accessor: 'byPlayerName',
        algorithm: 'btree',
        columns: ['playerName'],
      },
    ],
  },
  {
    entryId: t.u64().primaryKey().autoInc(),
    playerName: t.string(),
    bestScore: t.u32(),
  }
);

const spacetimedb = schema(
  { scoreEntry },
  { CASE_CONVERSION_POLICY: CaseConversionPolicy.None }
);
export default spacetimedb;

export const submitScore = spacetimedb.reducer(
  { playerName: t.string(), bestScore: t.u32() },
  (ctx, { playerName, bestScore }) => {
    ctx.db.scoreEntry.insert({ entryId: 0n, playerName, bestScore });
  }
);

export const topScore = spacetimedb.procedure(
  { name: 'TopScore' },
  {},
  t.u32(),
  ctx =>
    ctx.withTx(tx =>
      Math.max(0, ...[...tx.db.scoreEntry.iter()].map(row => row.bestScore))
    )
);
