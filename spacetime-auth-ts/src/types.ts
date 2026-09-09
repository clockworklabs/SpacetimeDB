import type { Infer } from 'spacetimedb/server';
import type {
  authUserRow,
  authSessionRow,
  authAccountRow,
  authVerificationRow,
  authOauthStateRow,
  authConfigRow,
  authConnectionBindingRow,
} from './tables.ts';

export type AuthUser = Infer<typeof authUserRow>;
export type AuthSession = Infer<typeof authSessionRow>;
export type AuthAccount = Infer<typeof authAccountRow>;
export type AuthVerification = Infer<typeof authVerificationRow>;
export type AuthOauthState = Infer<typeof authOauthStateRow>;
export type AuthConfig = Infer<typeof authConfigRow>;
export type AuthConnectionBinding = Infer<typeof authConnectionBindingRow>;
