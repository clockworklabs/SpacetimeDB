import { IdentityBuilder, type ElementsObj } from './type_builders';
import type { Identity } from './identity';

declare const receiverIdentityBrand: unique symbol;
/** An Identity with a compile-time receiver-module brand and the unchanged wire type. */
export type ReceiverIdentity<Module extends symbol> = Identity & {
  readonly [receiverIdentityBrand]: Module;
};

// These brands belong to binding instances, not to the serialized Identity type.
const receiverModules = new WeakMap<RemoteReducer, symbol>();
const identityModules = new WeakMap<object, symbol>();

/** A receiver reducer's canonical wire name, argument schema, and signature. */
export interface RemoteReducer<Params extends ElementsObj = ElementsObj> {
  /** The receiver's canonical reducer name, used for delivery. */
  readonly name: string;
  /** Argument builders keyed by TypeScript names, in receiver parameter order. */
  readonly params: Params;
  /** Hash of the canonical receiver signature, computed by shared codegen. */
  readonly signatureHash: bigint;
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
  return { reducers: handles, identity };
}
