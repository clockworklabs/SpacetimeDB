import { DBConnectionBase } from './spacetimedb.ts';

// Helper function for creating a proxy for a table class
export function _tableProxy<T>(t: any, client: DBConnectionBase<any, any>): T {
  return new Proxy(t, {
    get: (target, prop: keyof typeof t) => {
      if (typeof target[prop] === 'function') {
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

export function toPascalCase(s: string): string {
  const str = s.replace(/([-_][a-z])/gi, $1 => {
    return $1.toUpperCase().replace('-', '').replace('_', '');
  });

  return str.charAt(0).toUpperCase() + str.slice(1);
}
