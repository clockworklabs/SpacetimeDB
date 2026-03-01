import { createContext, useContext } from "solid-js";
import type { ConnectionState } from "./connection_state";


export const SpacetimeDBContext = createContext<ConnectionState | undefined>(
  undefined
);

export function useSpacetimeDB(): ConnectionState {
    const context = useContext(SpacetimeDBContext) as ConnectionState | undefined;

    if (!context) {
        throw new Error(
            "useSpacetimeDB must be used within a SpacetimeDBProvider component. Did you forget to add a `SpacetimeDBProvider` to your component tree?"
        );
    }   

    return context;
}