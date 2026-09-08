// Raw host ABI used to verify that SDK context changes cannot grant authority.
declare module 'spacetime:sys@2.3' {
  export function env_get(key: string): string | null;
}
