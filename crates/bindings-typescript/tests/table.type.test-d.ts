import { table, t } from '../src/server';
import type { RowExpr, RowTypeWithValue } from '../src/server/table';

const example = table(
  { name: 'example' },
  {
    id: t.u32(),
    name: t.string(),
    active: t.bool(),
    score: t.i64(),
  }
);

type ExampleTableDef = {
  name: typeof example.tableName;
  columns: typeof example.rowType.row;
  indexes: Array<typeof example.idxs[number]>;
};

type StringsOnly = RowTypeWithValue<ExampleTableDef, string>;
type Idk = RowExpr<ExampleTableDef>;

function takesRowExpr(expr: Idk) {
  expr.name
  expr.name
  expr.name
}


const strRecord: StringsOnly = {
  name: 'Ada',
};

const invalid: StringsOnly = {
  name: 'Ada',
  // @ts-expect-error id is not a string column
  id: 1,
};

type ExampleRowExpr = RowExpr<ExampleTableDef>;

const goodExpr: ExampleRowExpr = {
  id: { type: 'column', column: 'id', table: 'example', valueType: 1 },
  name: {
    type: 'column',
    column: 'name',
    table: 'example',
    valueType: 'Ada',
  },
  active: {
    type: 'column',
    column: 'active',
    table: 'example',
    valueType: true,
  },
  score: {
    type: 'column',
    column: 'score',
    table: 'example',
    valueType: 1n,
  },
};

type NameValueType = ExampleRowExpr['name']['valueType'];
const nameValueOk: NameValueType = 'Ada';
// @ts-expect-error name column expects string values
const nameValueBad: NameValueType = 123;
