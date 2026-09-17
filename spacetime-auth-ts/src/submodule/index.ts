import { schema, t, table } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { installAuth } from './install';
import {
  authAccountTable as authAccount,
  authAdminIdentityTable as authAdminIdentity,
  authConfigTable as authConfig,
  authConnectionBindingTable as authConnectionBinding,
  authOauthStateTable as authOauthState,
  authSessionTable as authSession,
  authUserTable as authUser,
  authVerificationTable as authVerification,
} from '../tables';
import * as auth from '../index';

const authSweeperTick = table(
  { name: 'auth_sweeper_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({
  rateLimit,
  authUser,
  authSession,
  authAccount,
  authVerification,
  authOauthState,
  authConfig,
  authConnectionBinding,
  authAdminIdentity,
  authSweeperTick,
});
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  installAuth(ctx);
});

// Generate an ES256 keypair when the first configuration has no PEM.
export const setAuthConfig = spacetimedb.reducer(
  auth.setAuthConfigParams,
  (ctx, args) => {
    auth.setAuthConfig(ctx, args);
  }
);

export const getAuthPublicKey = spacetimedb.procedure(
  auth.getPublicKeyPemParams,
  t.object('AuthPubKey', {
    publicKeyPem: t.string(),
    keyId: t.string(),
    issuerUrl: t.string(),
  }),
  auth.getPublicKeyPem
);

export const linkConnection = spacetimedb.reducer(
  auth.linkConnectionParams,
  (ctx, args) => {
    auth.linkConnection(ctx, args);
  }
);

export const unlinkConnection = spacetimedb.reducer(
  auth.unlinkConnectionParams,
  (ctx, args) => {
    auth.unlinkConnection(ctx, args);
  }
);

export const updateProfile = spacetimedb.reducer(
  auth.updateProfileParams,
  auth.updateProfile
);

export const revokeSession = spacetimedb.reducer(
  auth.revokeSessionParams,
  (ctx, args) => {
    auth.revokeSession(ctx, args);
  }
);

export const listMySessions = spacetimedb.procedure(
  auth.listMySessionsParams,
  t.object('MySessions', {
    sessions: t.array(
      t.object('MySession', {
        sessionId: t.string(),
        expiresAt: t.timestamp(),
        createdAt: t.timestamp(),
        ipAddress: t.option(t.string()),
        userAgent: t.option(t.string()),
        isCurrent: t.bool(),
      })
    ),
  }),
  auth.listMySessions
);

export const revokeMySession = spacetimedb.reducer(
  auth.revokeMySessionParams,
  (ctx, args) => {
    auth.revokeMySession(ctx, args);
  }
);

export const authSweep = spacetimedb.reducer(
  { onSchedule: authSweeperTick },
  { arg: authSweeperTick.rowType },
  (ctx, _arg) => {
    auth.authSweep(ctx);
  }
);

export const myAuthUser = spacetimedb.view(
  { name: 'my_auth_user', public: true },
  t.array(authUser.rowType),
  ctx => {
    const binding = ctx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) return [];
    const row = ctx.db.authUser.userId.find(binding.userId);
    return row ? [row] : [];
  }
);

export const whoami = spacetimedb.procedure(
  {},
  t.object('WhoAmI', {
    userId: t.option(t.string()),
    senderIdentityHex: t.string(),
  }),
  (ctx, _args) => {
    const userId = auth.getCallerUserId(ctx);
    return {
      userId: userId ?? undefined,
      senderIdentityHex: ctx.sender.toHexString(),
    };
  }
);
