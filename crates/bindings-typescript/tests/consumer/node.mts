// Node consumers resolve the package from an ES module here and from CommonJS
// in node.cts. The config uses node16, which unlike nodenext rejects CommonJS
// types that are really ES modules.
import { Identity } from 'spacetimedb';
import { useTable } from 'spacetimedb/react';

export const zero: Identity = Identity.zero();
export const hook: typeof useTable = useTable;
