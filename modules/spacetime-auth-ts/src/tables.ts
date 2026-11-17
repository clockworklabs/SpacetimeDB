import { table, t } from 'spacetimedb/server';

export const authUserRow = {
  userId: t.string().primaryKey(),
  email: t.string().unique(),
  emailVerified: t.bool(),
  name: t.option(t.string()),
  image: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const authSessionRow = {
  sessionId: t.string().primaryKey(),
  userId: t.string().index(),
  expiresAt: t.timestamp().index(),
  ipAddress: t.option(t.string()),
  userAgent: t.option(t.string()),
  createdAt: t.timestamp(),
};

// providerId: 'password' | 'google' | 'github'. providerAccountId: email or provider sub.
export const authAccountRow = {
  accountId: t.string().primaryKey(),
  userId: t.string().index(),
  providerId: t.string().index(),
  providerAccountId: t.string().index(),
  passwordHash: t.option(t.string()),
  accessToken: t.option(t.string()),
  refreshToken: t.option(t.string()),
  accessTokenExpiresAt: t.option(t.timestamp()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const authVerificationRow = {
  verificationId: t.string().primaryKey(),
  identifier: t.string().index(),
  value: t.string().unique(),
  purpose: t.string(),
  expiresAt: t.timestamp().index(),
  createdAt: t.timestamp(),
};

export const authOauthStateRow = {
  state: t.string().primaryKey(),
  provider: t.string(),
  codeVerifier: t.string(),
  redirectTo: t.string(),
  expiresAt: t.timestamp().index(),
  createdAt: t.timestamp(),
};

// Private singleton; populated by setAuthConfig.
export const authConfigRow = {
  singleton: t.bool().primaryKey(),
  issuerUrl: t.string(),
  baseUrl: t.string(),
  cookieName: t.string(),
  sessionTtlSeconds: t.u64(),
  es256PrivateKeyPem: t.string(),
  es256PublicKeyPem: t.string(),
  keyId: t.string(),
  // Advances on every secret draw. See tokens.ts.
  tokenCounter: t.u64(),
  googleClientId: t.option(t.string()),
  googleClientSecret: t.option(t.string()),
  githubClientId: t.option(t.string()),
  githubClientSecret: t.option(t.string()),
  updatedAt: t.timestamp(),
};

// Maps an STDB Identity to the session it presented to link_connection.
// Deleting a session deletes its bindings.
export const authConnectionBindingRow = {
  stdbIdentity: t.identity().primaryKey(),
  userId: t.string().index(),
  sessionId: t.string().index(),
  linkedAt: t.timestamp(),
};

// Operator allowlist. install seeds the publishing identity; privileged calls
// (set_auth_config, revoke_session, admin changes) must come from an admin.
export const authAdminIdentityRow = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};

const authUserOptions = { name: 'auth_user', public: false };
export const authUserTable: ReturnType<
  typeof table<typeof authUserRow, typeof authUserOptions>
> = table(authUserOptions, authUserRow);

const authSessionOptions = { name: 'auth_session', public: false };
export const authSessionTable: ReturnType<
  typeof table<typeof authSessionRow, typeof authSessionOptions>
> = table(authSessionOptions, authSessionRow);

const authAccountOptions = { name: 'auth_account', public: false };
export const authAccountTable: ReturnType<
  typeof table<typeof authAccountRow, typeof authAccountOptions>
> = table(authAccountOptions, authAccountRow);

const authVerificationOptions = { name: 'auth_verification', public: false };
export const authVerificationTable: ReturnType<
  typeof table<typeof authVerificationRow, typeof authVerificationOptions>
> = table(authVerificationOptions, authVerificationRow);

const authOauthStateOptions = { name: 'auth_oauth_state', public: false };
export const authOauthStateTable: ReturnType<
  typeof table<typeof authOauthStateRow, typeof authOauthStateOptions>
> = table(authOauthStateOptions, authOauthStateRow);

const authConfigOptions = { name: 'auth_config', public: false };
export const authConfigTable: ReturnType<
  typeof table<typeof authConfigRow, typeof authConfigOptions>
> = table(authConfigOptions, authConfigRow);

const authConnectionBindingOptions = {
  name: 'auth_connection_binding',
  public: false,
};
export const authConnectionBindingTable: ReturnType<
  typeof table<
    typeof authConnectionBindingRow,
    typeof authConnectionBindingOptions
  >
> = table(authConnectionBindingOptions, authConnectionBindingRow);

const authAdminIdentityOptions = { name: 'auth_admin_identity', public: false };
export const authAdminIdentityTable: ReturnType<
  typeof table<typeof authAdminIdentityRow, typeof authAdminIdentityOptions>
> = table(authAdminIdentityOptions, authAdminIdentityRow);
