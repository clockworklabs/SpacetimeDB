import type { RawOutboxDefV10 } from './autogen/types';
import { table } from './table';
import {
  t,
  type ElementsObj,
  type RowBuilder,
  type RowObj,
} from './type_builders';
import { deepEqual } from './util';

/** A receiver reducer's canonical wire name, argument schema, and signature. */
export interface RemoteReducer<Params extends ElementsObj = ElementsObj> {
  readonly name: string;
  readonly params: Params;
  readonly paramNames: { readonly [K in keyof Params]: string };
  readonly signatureHash: bigint;
}

function outboxTable<Params extends ElementsObj>(
  receiver: RemoteReducer<Params>,
  opts: { name?: string; accessor?: string } = {}
) {
  if ('msgId' in receiver.params || 'target' in receiver.params) {
    throw new TypeError(
      'Reducer parameters conflict with outbox msgId/target columns; declare the outbox explicitly'
    );
  }
  const row = {
    msgId: t.u64().primaryKey().autoInc(),
    target: t.identity().target(),
    ...receiver.params,
  };
  return table({ name: opts.name ?? opts.accessor, outbox: { receiver } }, row);
}

/** Used by generated receiver bindings. */
export function remoteModule<
  const Reducers extends Record<string, RemoteReducer>,
>(reducers: Reducers) {
  const outbox = Object.fromEntries(
    Object.entries(reducers).map(([name, reducer]) => [
      name,
      (opts?: { name?: string; accessor?: string }) =>
        outboxTable(reducer, opts),
    ])
  ) as {
    [K in keyof Reducers]: (opts?: {
      name?: string;
      accessor?: string;
    }) => ReturnType<typeof outboxTable<Reducers[K]['params']>>;
  };
  return { reducers, identity: t.identity, outbox };
}

/** Resolve receiver parameters by name, independently of sender column order. */
export function makeOutbox(
  receiver: RemoteReducer,
  row: RowBuilder<RowObj>,
  primaryKey: number[],
  scheduledOrEvent: boolean
): Omit<RawOutboxDefV10, 'tableSourceName' | 'onResultReducer'> {
  const columns = Object.entries(row.row);
  if (scheduledOrEvent)
    throw new TypeError('Outbox tables cannot be scheduled or event tables');
  const pk = primaryKey.length === 1 ? columns[primaryKey[0]][1] : undefined;
  if (
    !pk?.columnMetadata.isAutoIncrement ||
    pk.typeBuilder.algebraicType.tag !== 'U64'
  ) {
    throw new TypeError(
      'Outbox tables require a u64 primary key with autoInc()'
    );
  }
  const targets = columns.flatMap(([, col], i) =>
    col.columnMetadata.isTarget ? [i] : []
  );
  if (
    targets.length !== 1 ||
    !deepEqual(
      columns[targets[0]][1].typeBuilder.algebraicType,
      t.identity().algebraicType
    )
  ) {
    throw new TypeError(
      'Outbox tables require exactly one identity().target() column'
    );
  }
  const argColumns = Object.entries(receiver.params).map(([name, type]) => {
    const canonical = receiver.paramNames[name];
    const matches = columns.flatMap(([key, col], i) =>
      (col.columnMetadata.name ?? key) === canonical ||
      (col.columnMetadata.name === undefined && key === name)
        ? [i]
        : []
    );
    if (matches.length !== 1)
      throw new TypeError(
        `Outbox requires exactly one column for receiver parameter '${canonical}'`
      );
    const index = matches[0];
    if (
      !deepEqual(
        columns[index][1].typeBuilder.algebraicType,
        type.algebraicType
      )
    ) {
      throw new TypeError(
        `Outbox column for '${canonical}' has the wrong type`
      );
    }
    return index;
  });
  return {
    remoteReducer: receiver.name,
    targetColumn: targets[0],
    argColumns,
    signatureHash: receiver.signatureHash,
  };
}
