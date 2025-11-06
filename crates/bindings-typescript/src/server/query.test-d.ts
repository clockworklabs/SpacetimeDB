import type { U32 } from '../lib/autogen/algebraic_type_variants';
import type { Indexes } from './indexes';
import type { ColumnExpr, IndexExprs, IndexNameUnion, RowExpr } from './query';
import { schema } from './schema';
import { table, type TableIndexes } from './table';
import t from './type_builders';

const person = table(
  {
    name: 'person',
    indexes: [
      {
        name: 'id_name_idx',
        algorithm: 'btree',
        columns: ['id', 'name'] as const,
      },
      {
        name: 'name_idx',
        algorithm: 'btree',
        columns: ['name'] as const,
      },
    ] as const,
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
    married: t.bool(),
    id2: t.identity(),
    age: t.u32(),
    age2: t.u16(),
  }
);

const spacetimedb = schema(person);

type PersonDef = {
  name: typeof person.tableName;
  columns: typeof person.rowType.row;
  indexes: typeof person.idxs;
};

declare const row: RowExpr<PersonDef>;
const x: U32 = row.age.spacetimeType;

declare const idxs: IndexExprs<PersonDef>;

declare const xyz: IndexNameUnion<PersonDef>;

declare const xx: TableIndexes<PersonDef>;

declare const idxs2: Indexes<PersonDef, TableIndexes<PersonDef>>;
idxs2.name_idx;
