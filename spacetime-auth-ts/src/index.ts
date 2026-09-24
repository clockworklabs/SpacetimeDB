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
} from './tables';

export {
  authAdminVerdict,
  denyIfNotAdmin,
  seedAuthAdmin,
  type AdminVerdict,
} from './admin';

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
} from './handlers/index';

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
} from './handlers/http';

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
} from './procedures';

export {
  signJwt,
  verifyJwt,
  decodeJwtPayloadUnsafe,
  type JwtClaims,
  type JwtHeader,
  type VerifyResult,
  type VerifyJwtOptions,
} from './jwt';

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
} from './crypto';

export {
  generateEs256Keypair,
  fromPrivateKeyBytes,
  privateKeyFromPem,
  publicKeyFromPem,
  type Es256Keypair,
  type PublicKeyJwk,
} from './keys';

export { getCallerUserId, findCallerUser, requireCallerUserId } from './caller';

export {
  AUTH_RATE_LIMITS,
  clientKey,
  enforceIpRateLimit,
  enforceRateLimits,
  rateLimitResponse,
  type AuthHttpOptions,
  type AuthRateLimitPolicy,
  type TrustedProxyHeader,
} from './rate_limit';

export {
  MailerNotConfiguredError,
  buildVerifyEmail,
  buildPasswordResetEmail,
  type SendMailFn,
  type MailParams,
} from './mailer';

export type {
  AuthUser,
  AuthSession,
  AuthAccount,
  AuthVerification,
  AuthOauthState,
  AuthConfig,
  AuthConnectionBinding,
} from './types';
