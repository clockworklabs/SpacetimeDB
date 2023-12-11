import { SpacetimeDBClient } from "./spacetimedb";

// Helper function for creating a proxy for a table class
export function _tableProxy<T>(t: any, client: SpacetimeDBClient): T {
  return new Proxy(t, {
    get: (target, prop: keyof typeof t) => {
      if (typeof target[prop] === "function") {
        return (...args: any[]) => {
          const originalDb = t.db;
          t.db = client.db;
          const result = (t[prop] as unknown as Function)(...args);
          t.db = originalDb;
          return result;
        };
      } else {
        return t[prop];
      }
    },
  }) as unknown as typeof t;
}
