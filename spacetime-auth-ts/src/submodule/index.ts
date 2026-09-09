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
import {
  setAuthConfigParams,
  setAuthConfig,
  authSweep,
  getPublicKeyPemParams,
  getPublicKeyPem,
  linkConnectionParams,
  linkConnection,
  unlinkConnectionParams,
  unlinkConnection,
  updateProfileParams,
  updateProfile,
  revokeSessionParams,
  revokeSession,
  listMySessionsParams,
  listMySessions,
  revokeMySessionParams,
  revokeMySession,
  getCallerUserId,
} from '../index';

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

// On the first set_auth_config call, setAuthConfig generates an ES256 keypair when no PEM is supplied.
export const set_auth_config = spacetimedb.reducer(
  setAuthConfigParams,
  (ctx, args) => {
    setAuthConfig(ctx, args);
  }
);

export const get_auth_public_key = spacetimedb.procedure(
  getPublicKeyPemParams,
  t.object('AuthPubKey', {
    publicKeyPem: t.string(),
    keyId: t.string(),
    issuerUrl: t.string(),
  }),
  getPublicKeyPem
);

export const link_connection = spacetimedb.reducer(
  linkConnectionParams,
  (ctx, args) => {
    linkConnection(ctx, args);
  }
);

export const unlink_connection = spacetimedb.reducer(
  unlinkConnectionParams,
  (ctx, args) => {
    unlinkConnection(ctx, args);
  }
);

export const update_profile = spacetimedb.reducer(
  updateProfileParams,
  updateProfile
);

export const revoke_session = spacetimedb.reducer(
  revokeSessionParams,
  (ctx, args) => {
    revokeSession(ctx, args);
  }
);

export const list_my_sessions = spacetimedb.procedure(
  listMySessionsParams,
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
  listMySessions
);

export const revoke_my_session = spacetimedb.reducer(
  revokeMySessionParams,
  (ctx, args) => {
    revokeMySession(ctx, args);
  }
);

export const auth_sweep = spacetimedb.reducer(
  { onSchedule: authSweeperTick },
  { arg: authSweeperTick.rowType },
  (ctx, _arg) => {
    authSweep(ctx);
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
    const userId = getCallerUserId(ctx);
    return {
      userId: userId ?? undefined,
      senderIdentityHex: ctx.sender.toHexString(),
    };
  }
);
