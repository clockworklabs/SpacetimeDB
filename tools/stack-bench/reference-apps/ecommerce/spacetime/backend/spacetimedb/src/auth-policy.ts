// The runtime supplies this provider in each attempt's isolated network namespace.
export const AUTH_ISSUER = 'http://127.0.0.1:9090/realms/stack-bench';
export const AUTH_CLIENT = 'storefront';

export function providerUsername(jwt: {
  issuer: string;
  audience: readonly string[];
  fullPayload: Record<string, unknown>;
}): string {
  if (jwt.issuer !== AUTH_ISSUER || !jwt.audience.includes(AUTH_CLIENT)) {
    throw new Error('Invalid login issuer or audience.');
  }
  const username = jwt.fullPayload.preferred_username;
  if (typeof username !== 'string' || !/^[A-Za-z0-9-]{1,48}$/.test(username)) {
    throw new Error('Invalid username.');
  }
  if (username === 'admin' || username === 'staff' || username === 'customer') {
    const realm = jwt.fullPayload.realm_access;
    const roles = realm && typeof realm === 'object' && 'roles' in realm ? realm.roles : undefined;
    if (!Array.isArray(roles) || !roles.includes(`app-${username}`)) {
      throw new Error('This account requires a trusted provider role.');
    }
  }
  return username;
}
