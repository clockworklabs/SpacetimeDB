export {
  authTables,
  authUserTable,
  authSessionTable,
  authAccountTable,
  authVerificationTable,
  authOauthStateTable,
  authConfigTable,
  authConnectionBindingTable,
  authAdminIdentityTable,
  authUserRow,
  authSessionRow,
  authAccountRow,
  authVerificationRow,
  authOauthStateRow,
  authConfigRow,
  authConnectionBindingRow,
  authAdminIdentityRow,
} from './tables.ts';

export {
  authAdminVerdict,
  denyIfNotAdmin,
  seedAuthAdmin,
  type AdminVerdict,
} from './admin.ts';

export {
  passwordLoginHandler,
  passwordSignupHandler,
  googleStartHandler,
  googleCallbackHandler,
  githubStartHandler,
  githubCallbackHandler,
  meHandler,
  logoutHandler,
  refreshHandler,
  makeOAuthStartHandler,
  makeOAuthCallbackHandler,
  makeEmailVerifyHandler,
  makeEmailVerifyRequestHandler,
  makeForgotPasswordHandler,
  resetPasswordHandler,
  type OAuthProviderSpec,
  type OAuthProfile,
  type VerifyRequestOpts,
  type ForgotPasswordOpts,
} from './handlers/index.ts';

export {
  clearCookie,
  makeCookie,
  parseCookies,
  jsonResponse,
  errorResponse,
  redirectResponse,
  readBearer,
  readSession,
  configKeys,
  type CookieOptions,
} from './handlers/http.ts';

export {
  setAuthConfigParams,
  setAuthConfig,
  authSweep,
  revokeSessionParams,
  revokeSession,
  listMySessionsParams,
  listMySessions,
  revokeMySessionParams,
  revokeMySession,
  getPublicKeyPemParams,
  getPublicKeyPem,
  linkConnectionParams,
  linkConnection,
  unlinkConnectionParams,
  unlinkConnection,
  updateProfileParams,
  updateProfile,
} from './procedures.ts';

export {
  signJwt,
  verifyJwt,
  decodeJwtPayloadUnsafe,
  type JwtClaims,
  type JwtHeader,
  type VerifyResult,
  type VerifyJwtOptions,
} from './jwt.ts';

export {
  hashPassword,
  verifyPassword,
  newSessionToken,
  newPkceVerifier,
  pkceChallenge,
  randomToken,
  randomBytes,
  uuidV7,
  type RandomSource,
  type ScryptParams,
} from './crypto.ts';

export {
  generateEs256Keypair,
  fromPrivateKeyBytes,
  privateKeyFromPem,
  publicKeyFromPem,
  type Es256Keypair,
  type PublicKeyJwk,
} from './keys.ts';

export {
  getCallerUserId,
  findCallerUser,
  requireCallerUserId,
} from './caller.ts';

export {
  consumeRateLimit,
  sweepRateLimits,
  type ConsumeRateLimitOpts,
  type RateLimitResult,
} from '@spacetimedb/rate-limit/submodule';

export {
  AUTH_RATE_LIMITS,
  clientKey,
  enforceIpRateLimit,
  enforceRateLimits,
  rateLimitKey,
  rateLimitResponse,
  type AuthHttpOptions,
  type AuthRateLimitPolicy,
  type TrustedProxyHeader,
} from './rate_limit.ts';

export {
  MailerNotConfiguredError,
  buildVerifyEmail,
  buildPasswordResetEmail,
  type SendMailFn,
  type MailParams,
} from './mailer.ts';

export type {
  AuthUser,
  AuthSession,
  AuthAccount,
  AuthVerification,
  AuthOauthState,
  AuthConfig,
  AuthConnectionBinding,
} from './types.ts';
