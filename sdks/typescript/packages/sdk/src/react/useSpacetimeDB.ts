import { createContext, useContext } from "react";
import type { ConnectionStatus } from "./SpacetimeDBProvider";
import type { Identity } from "@clockworklabs/spacetimedb-sdk";

export const SpacetimeDBContext = createContext<any>(null);

export function useSpacetimeDB<T>(): { status: ConnectionStatus; client: T; identity: Identity; token: string } {
  return useContext(SpacetimeDBContext);
}
