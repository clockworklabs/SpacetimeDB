export { client, type AuthHttpOptions } from './client.js';
export { errors } from './errors.js';
export {
  findCallerUser,
  getCallerUserId,
  requireCallerUserId,
} from './caller.js';
export { requestUserId } from './sessions.js';
export type { OAuthProfile, OAuthProviderSpec } from './handlers/oauth.js';
export type { MailParams, SendMailFn } from './mailer.js';
export type { TrustedProxyHeader } from './request-trust.js';
export type { AuthConfig, AuthUser } from './types.js';
