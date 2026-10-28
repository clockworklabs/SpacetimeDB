import type { OutboxMetadata } from './table_schema';
import { table } from './table';
import {
  IdentityBuilder,
  t,
  type ElementsObj,
  type RowBuilder,
  type RowObj,
} from './type_builders';
import { deepEqual } from './util';
import type { Identity } from './identity';

declare const receiverIdentityBrand: unique symbol;
/** An Identity with a compile-time receiver-module brand and the unchanged wire type. */
export type ReceiverIdentity<Module extends symbol> = Identity & {
  readonly [receiverIdentityBrand]: Module;
};

// These brands belong to binding instances, not to the serialized Identity type.
const receiverModules = new WeakMap<RemoteReducer, symbol>();
const identityModules = new WeakMap<object, symbol>();

/** A receiver reducer's canonical name, argument schema, and signature. */
export interface RemoteReducer<Params extends ElementsObj = ElementsObj> {
  /** The receiver's canonical reducer name, used for delivery. */
  readonly name: string;
  /** Argument builders keyed by TypeScript names, in receiver parameter order. */
  readonly params: Params;
  /** Hash of the canonical receiver signature, computed by shared codegen. */
  readonly signatureHash: bigint;
}

function outboxTable<Params extends ElementsObj, Module extends symbol>(
  receiver: RemoteReducer<Params>,
  identity: () => IdentityBuilder<ReceiverIdentity<Module>>,
  opts: { name?: string } = {}
) {
  for (const name of ['msgId', 'target']) {
    if (name in receiver.params) {
      throw new TypeError(
        `Receiver parameter '${name}' conflicts with outbox metadata; declare the outbox with table() and choose different metadata column names`
      );
    }
  }
  const row = {
    msgId: t.u64().primaryKey().autoInc(),
    target: identity().target(),
    ...receiver.params,
  };
  return table({ name: opts.name, outbox: { receiver } }, row);
}

/** Used by generated receiver bindings with a unique symbol declared in that module. */
export function remoteModule<
  const Reducers extends Record<string, RemoteReducer>,
  const Module extends symbol,
>(reducers: Reducers, module: Module) {
  const identity = (): IdentityBuilder<ReceiverIdentity<Module>> => {
    const builder = new IdentityBuilder<ReceiverIdentity<Module>>();
    identityModules.set(builder, module);
    return builder;
  };
  // Do not mutate caller-owned reducer metadata or share a handle's module brand.
  const handles = Object.fromEntries(
    Object.entries(reducers).map(([name, reducer]) => {
      const handle = { ...reducer };
      receiverModules.set(handle, module);
      return [name, handle];
    })
  ) as Reducers;
  const outbox = Object.fromEntries(
    Object.entries(handles).map(([name, reducer]) => [
      name,
      (opts?: { name?: string }) => outboxTable(reducer, identity, opts),
    ])
  ) as {
    [K in keyof Reducers]: (opts?: {
      name?: string;
    }) => ReturnType<typeof outboxTable<Reducers[K]['params'], Module>>;
  };
  return { reducers: handles, identity, outbox };
}

/** Resolve receiver parameters by name, independently of sender column order. */
export function makeOutbox(
  receiver: RemoteReducer,
  row: RowBuilder<RowObj>,
  primaryKey: number[],
  scheduledOrEvent: boolean
): OutboxMetadata {
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
    const matches = columns.flatMap(([key, col], i) =>
      (col.columnMetadata.paramName ?? key) === name ? [i] : []
    );
    if (matches.length !== 1)
      throw new TypeError(
        `Outbox requires exactly one column for receiver parameter '${name}'`
      );
    const index = matches[0];
    if (
      !deepEqual(
        columns[index][1].typeBuilder.algebraicType,
        type.algebraicType
      )
    ) {
      throw new TypeError(`Outbox column for '${name}' has the wrong type`);
    }
    return index;
  });
  const module = receiverModules.get(receiver);
  const target = columns[targets[0]][1].typeBuilder;
  if (module === undefined || identityModules.get(target) !== module) {
    throw new TypeError(
      'Outbox target must use the selected receiver module identity() builder'
    );
  }
  return {
    targetColumn: targets[0],
    argColumns,
    remoteReducer: receiver.name,
    signatureHash: receiver.signatureHash,
  };
}
