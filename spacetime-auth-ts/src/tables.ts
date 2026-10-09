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

export const authUserTable = table(
  { name: 'auth_user', public: false },
  authUserRow
);

export const authSessionTable = table(
  { name: 'auth_session', public: false },
  authSessionRow
);

export const authAccountTable = table(
  { name: 'auth_account', public: false },
  authAccountRow
);

export const authVerificationTable = table(
  { name: 'auth_verification', public: false },
  authVerificationRow
);

export const authOauthStateTable = table(
  { name: 'auth_oauth_state', public: false },
  authOauthStateRow
);

export const authConfigTable = table(
  { name: 'auth_config', public: false },
  authConfigRow
);

export const authConnectionBindingTable = table(
  { name: 'auth_connection_binding', public: false },
  authConnectionBindingRow
);

export const authAdminIdentityTable = table(
  { name: 'auth_admin_identity', public: false },
  authAdminIdentityRow
);
