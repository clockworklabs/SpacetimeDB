import { tableToSchema, type UntypedSchemaDef } from '../lib/schema';
import type { UntypedTableDef } from '../lib/table';
import type { UntypedTableSchema } from '../lib/table_schema';
import { hostBackend } from './backend';
import type { DbView, ReadonlyDbView } from './db_view';
import { makeTableView } from './runtime';
import {
  processSchemaEntries,
  Schema,
  SchemaInner,
  type SchemaDefForEntries,
} from './schema';

export type MigrationFn<
  S extends UntypedSchemaDef = UntypedSchemaDef,
  Dropped extends UntypedSchemaDef = UntypedSchemaDef,
> = (ctx: MigrationContext<S, Dropped>) => void;

export interface MigrationContext<
  S extends UntypedSchemaDef = UntypedSchemaDef,
  Dropped extends UntypedSchemaDef = UntypedSchemaDef,
> {
  readonly db: DbView<S>;
  readonly dropped: ReadonlyDbView<Dropped>;
}

export class MigrationContextImpl<
  S extends UntypedSchemaDef = UntypedSchemaDef,
  Dropped extends UntypedSchemaDef = UntypedSchemaDef,
> implements MigrationContext<S, Dropped> {
  #dbView: DbView<S>;
  #dropped: ReadonlyDbView<Dropped>;

  constructor(dbView: DbView<S>, dropped: DroppedSchema<Dropped>) {
    this.#dbView = dbView;
    const rootTables = Object.values(dropped.schemaType.tables).map(table => [
      table.accessorName,
      makeTableView(
        dropped.typespace,
        table.tableDef,
        hostBackend,
        '',
        table.isOutbox
      ),
    ]);
    this.#dropped = Object.freeze(
      Object.fromEntries(rootTables)
    ) as ReadonlyDbView<Dropped>;
  }

  get db() {
    return this.#dbView;
  }

  get dropped() {
    return this.#dropped;
  }
}

export function droppedSchema<
  const H extends Record<string, UntypedTableSchema>,
>(hash: bigint, entries: H): DroppedSchema<SchemaDefForEntries<H>> {
  const hashTrunc = BigInt.asUintN(256, hash);
  if (hashTrunc !== hash)
    throw new RangeError('passed bigint is too large to be a hash');
  const inner = new SchemaInner<SchemaDefForEntries<H>>(ctx => {
    const tables = processSchemaEntries(entries, ctx);
    return { tables } as SchemaDefForEntries<H>;
  });
  return new DroppedSchema(inner, hash);
}

// export interface Dro

class DroppedSchema<S extends UntypedSchemaDef> extends Schema<S> {
  public readonly hash: bigint;
  constructor(ctx: SchemaInner<S>, hash: bigint) {
    super(ctx);
    this.hash = hash;
  }
}
