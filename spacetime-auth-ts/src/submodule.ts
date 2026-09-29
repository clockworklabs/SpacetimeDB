export { default } from './submodule/index';
export { install } from './submodule/install';
export {
  addAuthAdmin,
  authSweep,
  getAuthPublicKey,
  linkConnection,
  listMySessions,
  myAuthUser,
  removeAuthAdmin,
  revokeMySession,
  revokeSession,
  setAuthConfig,
  unlinkConnection,
  updateProfile,
  whoami,
} from './submodule/index';
export {
  client,
  errors,
  findCallerUser,
  getCallerUserId,
  requestUserId,
  requireCallerUserId,
  type AuthConfig,
  type AuthHttpOptions,
  type AuthUser,
  type MailParams,
  type OAuthProfile,
  type OAuthProviderSpec,
  type SendMailFn,
  type TrustedProxyHeader,
} from './index';
