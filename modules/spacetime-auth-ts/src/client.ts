import type { Request, SyncResponse } from 'spacetimedb/server';
import type { AuthHandlerCtx } from './context.js';
import type { SendMailFn } from './mailer.js';
import type { TrustedProxyHeader } from './request-trust.js';
import { errorResponse, HttpError } from './handlers/http.js';
import { passwordLogin, passwordSignup } from './handlers/password.js';
import { forgotPassword, resetPassword } from './handlers/password_reset.js';
import { emailVerify, emailVerifyRequest } from './handlers/email_verify.js';
import { logout, me, refresh } from './handlers/session.js';
import {
  oauthCallback,
  oauthStart,
  type OAuthProviderSpec,
} from './handlers/oauth.js';
import { google } from './handlers/google.js';
import { github } from './handlers/github.js';

export interface AuthHttpOptions {
  /** Header set by a trusted proxy after it removes any client-supplied value. */
  trustedProxyHeader?: TrustedProxyHeader;
  /** Defaults to true. Set false only for local HTTP development. */
  secureCookies?: boolean;
  /** Delivers verification and reset mail. Those routes return `mailer_not_configured` without it. */
  sendMail?: SendMailFn;
  /** Names the application in outgoing mail. */
  appName?: string;
  /** Where the email verification link lands. Default '/'. */
  emailVerifiedRedirect?: string;
}

type Handler = (
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
) => SyncResponse;

/** Configure the HTTP handlers once. Register each on the host router with `ctx.as.auth`. */
export function client(options: AuthHttpOptions = {}) {
  const route =
    (handler: Handler) =>
    (ctx: AuthHandlerCtx, req: Request): SyncResponse => {
      try {
        return handler(ctx, req, options);
      } catch (e) {
        if (e instanceof HttpError) return errorResponse(e.code, e.status);
        throw e;
      }
    };
  return {
    passwordSignup: route(passwordSignup),
    passwordLogin: route(passwordLogin),
    me: route(me),
    refresh: route(refresh),
    logout: route(logout),
    forgotPassword: route(forgotPassword),
    resetPassword: route(resetPassword),
    emailVerifyRequest: route(emailVerifyRequest),
    emailVerify: route(emailVerify),
    googleStart: route(oauthStart(google)),
    googleCallback: route(oauthCallback(google)),
    githubStart: route(oauthStart(github)),
    githubCallback: route(oauthCallback(github)),
    /** Handlers for a provider other than Google or GitHub. */
    oauthStart: (provider: OAuthProviderSpec) => route(oauthStart(provider)),
    oauthCallback: (provider: OAuthProviderSpec) =>
      route(oauthCallback(provider)),
  };
}
