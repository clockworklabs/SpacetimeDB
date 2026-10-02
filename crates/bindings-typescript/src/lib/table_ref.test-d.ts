import type { TableRef, TypedTableDecl } from './query';
import type { TableDecl } from './schema';
import { table } from './table';
import { t } from './type_builders';

type Extends<A, B> = [A] extends [B] ? true : false;
type Assert<T extends true> = T;

// eslint-disable-next-line @typescript-eslint/no-unused-vars
const person = table(
  { name: 'person' },
  { id: t.u32().primaryKey(), name: t.string() }
);
type PersonDecl = TableDecl<'person', typeof person>;

// A `TableRef` must be usable wherever the query builder expects a table
// declaration.
type _TableRef = Assert<Extends<TableRef<PersonDecl>, TypedTableDecl>>;
