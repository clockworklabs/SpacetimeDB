import { ClientDB } from "./client_db";
import { SpacetimeDBClient } from "./spacetimedb";

export type SpacetimeDBGlobals = {
  clientDB: ClientDB;
  spacetimeDBClient: SpacetimeDBClient | undefined;
};

declare global {
  interface Window {
    __SPACETIMEDB__: SpacetimeDBGlobals;
  }
  var __SPACETIMEDB__: SpacetimeDBGlobals;
}
