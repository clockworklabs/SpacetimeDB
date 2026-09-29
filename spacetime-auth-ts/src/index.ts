export { client, type AuthHttpOptions } from './client';
export { errors } from './errors';
export { findCallerUser, getCallerUserId, requireCallerUserId } from './caller';
export { requestUserId } from './sessions';
export type { OAuthProfile, OAuthProviderSpec } from './handlers/oauth';
export type { MailParams, SendMailFn } from './mailer';
export type { TrustedProxyHeader } from './request-trust';
export type { AuthConfig, AuthUser } from './types';
