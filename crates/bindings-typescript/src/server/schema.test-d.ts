import { schema } from './schema';
import { table } from '../lib/table';
import t from '../lib/type_builders';

const person = table(
  {
    // name: 'person',
    indexes: [
      {
        accessor: 'id_name_idx',
        algorithm: 'btree',
        columns: ['id', 'name'] as const,
      },
      {
        accessor: 'id_name2_idx',
        algorithm: 'btree',
        columns: ['id', 'name2'] as const,
      },
      {
        accessor: 'name_idx',
        algorithm: 'btree',
        columns: ['name'] as const,
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
    name2: t.string().unique(),
    married: t.bool(),
    id2: t.identity(),
    age: t.u32(),
    age2: t.u16(),
  }
);

const spacetimedb = schema({ person });

spacetimedb.init(ctx => {
  ctx.db.person.id_name_idx.filter(1);
  ctx.db.person.id_name_idx.filter([1, 'aname']);
  // ctx.db.person.id_name2_idx.find

  // @ts-expect-error id2 is not indexed, so this should not exist at all.
  const _id2 = ctx.db.person.id2;

  ctx.db.person.id.find(2);

  // update() is allowed on primary key indexes
  ctx.db.person.id.update({
    id: 1,
    name: '',
    name2: '',
    married: false,
    id2: '' as any,
    age: 0,
    age2: 0,
  });

  // @ts-expect-error update() is NOT allowed on non-PK unique indexes
  const _update = ctx.db.person.name2.update;
});

/**
 * Regression coverage for the declared-vs-resolved index split:
 * - declared table-level indexes must still produce typed accessors
 * - field-level indexes must still produce typed accessors
 * - non-indexed fields must not accidentally become index accessors
 */
const account = table(
  {
    indexes: [
      {
        accessor: 'byEmailAndOrg',
        algorithm: 'btree',
        columns: ['email', 'orgId'] as const,
      },
    ] as const,
  },
  {
    accountId: t.u32(),
    email: t.string().index('hash'),
    orgId: t.u32(),
    nickname: t.string(),
  }
);

const spacetimedbIndexSplit = schema({ account });

spacetimedbIndexSplit.init(ctx => {
  // Explicit table-level index accessor from `table({ indexes: [...] })`.
  ctx.db.account.byEmailAndOrg.filter(['a@example.com', 1]);

  // Field-level index accessor derived from column metadata.
  ctx.db.account.email.filter('a@example.com');

  // @ts-expect-error `nickname` is not indexed, so no index accessor should exist.
  const _nickname = ctx.db.account.nickname;
});
