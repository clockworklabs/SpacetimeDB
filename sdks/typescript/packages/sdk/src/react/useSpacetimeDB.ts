import { createContext, useContext, type Context } from "react";
import type { DbConnectionImpl } from "../index";

export const SpacetimeDBContext: Context<DbConnectionImpl | undefined> = createContext<DbConnectionImpl | undefined>(undefined);

// Throws an error if used outside of a SpacetimeDBProvider
// Error is caught by other hooks like useTable so they can provide better error messages
export function useSpacetimeDB<DbConnection extends DbConnectionImpl>(): DbConnection {
  const context = useContext(SpacetimeDBContext) as DbConnection | undefined;
  if (!context) {
    throw new Error("useSpacetimeDB must be used within a SpacetimeDBProvider component. Did you forget to add a `SpacetimeDBProvider` to your component tree?");
  }
  return context;
}