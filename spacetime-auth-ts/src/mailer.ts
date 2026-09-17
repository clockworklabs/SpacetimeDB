import type { AuthHandlerCtx } from './context.ts';

export interface MailParams {
  to: string;
  subject: string;
  text: string;
  html?: string;
}

export type SendMailFn = (ctx: AuthHandlerCtx, params: MailParams) => void;

export class MailerNotConfiguredError extends Error {
  constructor() {
    super('auth.mailer_not_configured');
  }
}

export function buildVerifyEmail(opts: {
  baseUrl: string;
  token: string;
  appName?: string;
}): MailParams {
  const url = `${opts.baseUrl}/auth/email/verify?token=${encodeURIComponent(opts.token)}`;
  const app = opts.appName ?? 'this app';
  return {
    to: '',
    subject: `Verify your email for ${app}`,
    text: `Click to verify your email:\n\n${url}\n\nThis link expires in 24 hours.`,
    html: `<p>Click to verify your email:</p><p><a href="${url}">${url}</a></p><p>This link expires in 24 hours.</p>`,
  };
}

export function buildPasswordResetEmail(opts: {
  baseUrl: string;
  token: string;
  appName?: string;
}): MailParams {
  const url = `${opts.baseUrl}/auth/password/reset?token=${encodeURIComponent(opts.token)}`;
  const app = opts.appName ?? 'this app';
  return {
    to: '',
    subject: `Reset your password for ${app}`,
    text: `Click to reset your password:\n\n${url}\n\nThis link expires in 1 hour. Ignore this email if the request was unexpected.`,
    html: `<p>Click to reset your password:</p><p><a href="${url}">${url}</a></p><p>This link expires in 1 hour. Ignore this email if the request was unexpected.</p>`,
  };
}
