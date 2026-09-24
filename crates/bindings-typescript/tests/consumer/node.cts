// The CommonJS half of node.mts: `require` resolves the .d.cts types.
import { Identity, type TokenProvider } from 'spacetimedb';
import { useTable } from 'spacetimedb/react';

export const zero: Identity = Identity.zero();
export const hook: typeof useTable = useTable;
export const provider: TokenProvider = async () => undefined;
