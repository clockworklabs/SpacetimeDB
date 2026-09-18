import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { leaseFromEnv, readBackendLease } from './backend-lease.js';
import type { BackendLease } from './backend-lease.js';
import { attemptDocker, createAttemptContainer, requireAttemptNetwork } from './docker-network.js';
import { loadTrack, portsFor } from '../composition/tracks.js';
import { applyCredentialAliases } from '../composition/credential-aliases.js';

export const AUTHENTICATION_ISSUER = 'http://127.0.0.1:9090/realms/stack-bench';
export const AUTHENTICATION_CLIENT_ID = 'storefront';
export const AUTHENTICATION_IMAGE = 'quay.io/keycloak/keycloak@sha256:357829ec7c4693397533035092ad13b0644bcc95ded311f33a3738c4d9e9bdba';

export function authenticationBrowserConfiguration(env: NodeJS.ProcessEnv = process.env):
  { provider: 'keycloak'; issuer: string } | undefined {
  if (!env.STACK_BENCH_LEASE && !env.STACK_BENCH_LEASE_TOKEN) return undefined;
  const { lease } = leaseFromEnv(env, { active: true });
  const provider = lease.resources.authenticationContainer;
  if (!provider?.owned || provider.removedAt) return undefined;
  return { provider: 'keycloak', issuer: AUTHENTICATION_ISSUER };
}

export function authenticationEnvironment(lease: Pick<BackendLease, 'backend' | 'track' | 'runIndex'> & {
  resources: Pick<BackendLease['resources'], 'authenticationContainer'>;
}): Record<string, string> {
  if (!lease.resources.authenticationContainer?.owned || lease.resources.authenticationContainer.removedAt) return {};
  const ports = portsFor(loadTrack(lease.track), lease.backend, lease.runIndex);
  const redirectUri = `http://127.0.0.1:${ports.vite}/`;
  return { OIDC_ISSUER: AUTHENTICATION_ISSUER, OIDC_CLIENT_ID: AUTHENTICATION_CLIENT_ID,
    OIDC_REDIRECT_URI: redirectUri, VITE_OIDC_ISSUER: AUTHENTICATION_ISSUER,
    VITE_OIDC_CLIENT_ID: AUTHENTICATION_CLIENT_ID, VITE_OIDC_REDIRECT_URI: redirectUri };
}

export function authenticationRealm(redirectUri: string, credentialAliases?: unknown) {
  const roles = ['admin', 'staff', 'customer'];
  return { realm: 'stack-bench', enabled: true, sslRequired: 'none', registrationAllowed: true,
    registrationEmailAsUsername: false, duplicateEmailsAllowed: false, editUsernameAllowed: false,
    roles: { realm: roles.map(role => ({ name: `app-${role}` })) },
    clients: [{ clientId: AUTHENTICATION_CLIENT_ID, publicClient: true, standardFlowEnabled: true,
      directAccessGrantsEnabled: false, redirectUris: [redirectUri], webOrigins: [new URL(redirectUri).origin],
      protocolMappers: [{ name: 'application-roles', protocol: 'openid-connect',
        protocolMapper: 'oidc-usermodel-realm-role-mapper', config: {
          'claim.name': 'realm_access.roles', 'jsonType.label': 'String', multivalued: 'true',
          'id.token.claim': 'true', 'access.token.claim': 'true', 'userinfo.token.claim': 'false',
        } }],
      attributes: { 'pkce.code.challenge.method': 'S256', 'post.logout.redirect.uris': redirectUri } }],
    users: roles.map(role => ({ username: role, enabled: true, email: `${role}@example.test`,
      emailVerified: true, firstName: role, lastName: 'Account', realmRoles: [`app-${role}`],
      credentials: [{ type: 'password', temporary: false,
        value: applyCredentialAliases(`stackbench-${role}-2026`, credentialAliases) }] })),
  };
}

// Self-contained: the compiled function runs inside the owned browser namespace.
export async function resetAuthenticationRealm(input: {
  password: string; realm: ReturnType<typeof authenticationRealm>;
}, fetchImpl: typeof fetch = fetch): Promise<void> {
  const base = 'http://127.0.0.1:9090';
  const deadline = Date.now() + 60_000;
  let response: Response | undefined;
  while (Date.now() < deadline) {
    try {
      response = await fetchImpl(`${base}/realms/master/protocol/openid-connect/token`, {
        method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ client_id: 'admin-cli', grant_type: 'password',
          username: 'operator', password: input.password }), signal: AbortSignal.timeout(2000),
      });
      if (response.ok) break;
      if (response.status === 401) throw new Error('Identity administration credentials were refused');
    } catch (error) {
      if (response?.status === 401) throw error;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  if (!response?.ok) throw new Error('Owned identity provider did not become ready');
  const token = (await response.json()).access_token;
  if (typeof token !== 'string' || !token) throw new Error('Identity administration returned no token');
  const request = async (path: string, method = 'GET', body?: unknown) => {
    const result = await fetchImpl(`${base}/admin/${path}`, { method,
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }), signal: AbortSignal.timeout(10_000) });
    if (!result.ok && !(method === 'GET' && path === 'realms/stack-bench' && result.status === 404)) {
      throw new Error(`Identity administration ${method} ${path} failed: ${result.status}`);
    }
    return result;
  };
  const previous = await request('realms/stack-bench');
  if (previous.status === 404) {
    await request('realms', 'POST', input.realm);
  } else {
    // Preserve issuer keys. Destroying a realm rotates keys under the database's JWKS cache.
    await request('realms/stack-bench/logout-all', 'POST');
    for (;;) {
      const users: Array<{ id: string }> = await (await request('realms/stack-bench/users?first=0&max=100')).json();
      if (!Array.isArray(users)) throw new Error('Identity reset returned an invalid user list');
      if (!users.length) break;
      for (const user of users) {
        if (typeof user.id !== 'string' || !user.id) throw new Error('Identity reset returned no user id');
        await request(`realms/stack-bench/users/${encodeURIComponent(user.id)}`, 'DELETE');
      }
    }
    await request('realms/stack-bench/partialImport', 'POST', { ifResourceExists: 'FAIL', users: input.realm.users });
  }
  // The product asks for username/password only. Do not invent email or personal names in the driver.
  const profile = await (await request('realms/stack-bench/users/profile')).json();
  if (!Array.isArray(profile.attributes)) throw new Error('Identity provider returned no user profile');
  for (const attribute of profile.attributes) {
    if (['email', 'firstName', 'lastName'].includes(attribute.name)) {
      delete attribute.required;
      attribute.permissions = { view: ['admin'], edit: ['admin'] };
    }
    if (attribute.name === 'username') {
      attribute.validations = { length: { min: 1, max: 48 }, pattern: { pattern: '^[a-zA-Z0-9-]+$' } };
    }
  }
  await request('realms/stack-bench/users/profile', 'PUT', profile);
}

function administrationPassword(lease: BackendLease): string {
  return createHash('sha256').update(`authentication:${lease.ownershipToken}`).digest('hex');
}

export function resetAuthenticationService(lease: BackendLease, credentialAliases?: unknown): void {
  if (!lease.resources.authenticationContainer) return;
  requireAttemptNetwork(lease);
  const browser = lease.resources.browserContainer;
  if (!browser?.owned) throw new Error('Identity provider reset requires the owned browser container');
  const expected = authenticationEnvironment(lease).OIDC_REDIRECT_URI!;
  const script = `const chunks=[]; for await (const chunk of process.stdin) chunks.push(chunk);`
    + `await (${resetAuthenticationRealm.toString()})(JSON.parse(Buffer.concat(chunks).toString()));`;
  execFileSync('docker', ['exec', '-i', browser.id, 'node', '--input-type=module', '-e', script], {
    encoding: 'utf8', stdio: 'pipe', windowsHide: true, timeout: 120_000,
    input: JSON.stringify({ password: administrationPassword(lease), realm: authenticationRealm(expected, credentialAliases) }),
  });
}

export function startAuthenticationService(leasePath: string, lease: BackendLease, selection: 'none' | 'keycloak' = 'none',
  credentialAliases?: Readonly<Record<string, string>>): void {
  if (selection === 'none') return;
  const namespace = requireAttemptNetwork(lease);
  try { attemptDocker(['image', 'inspect', AUTHENTICATION_IMAGE]); }
  catch { attemptDocker(['pull', AUTHENTICATION_IMAGE]); }
  const image = attemptDocker(['image', 'inspect', '--format', '{{.Id}}', AUTHENTICATION_IMAGE]);
  const provider = createAttemptContainer(leasePath, lease, 'authentication', image, namespace,
    ['-e', 'KC_BOOTSTRAP_ADMIN_USERNAME=operator', '-e', `KC_BOOTSTRAP_ADMIN_PASSWORD=${administrationPassword(lease)}`]);
  attemptDocker(['exec', '-d', provider.id, '/opt/keycloak/bin/kc.sh', 'start-dev',
    '--http-port', '9090', '--hostname', 'http://127.0.0.1:9090']);
  resetAuthenticationService(readBackendLease(leasePath, { token: lease.ownershipToken }), credentialAliases);
}
