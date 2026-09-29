import { schema, t, table } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { install } from './install';
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
import * as auth from '../procedures';
import { findCallerUser, getCallerUserId } from '../caller';

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
  install(ctx);
});

export const setAuthConfig = spacetimedb.reducer(
  auth.setAuthConfigParams,
  auth.setAuthConfig
);

export const addAuthAdmin = spacetimedb.reducer(
  auth.adminParams,
  auth.addAuthAdmin
);

export const removeAuthAdmin = spacetimedb.reducer(
  auth.adminParams,
  auth.removeAuthAdmin
);

export const getAuthPublicKey = spacetimedb.procedure(
  {},
  auth.authPublicKey,
  auth.getAuthPublicKey
);

export const linkConnection = spacetimedb.reducer(
  auth.linkConnectionParams,
  auth.linkConnection
);

export const unlinkConnection = spacetimedb.reducer({}, auth.unlinkConnection);

export const updateProfile = spacetimedb.reducer(
  auth.updateProfileParams,
  auth.updateProfile
);

export const revokeSession = spacetimedb.reducer(
  auth.sessionIdParams,
  auth.revokeSession
);

export const listMySessions = spacetimedb.procedure(
  {},
  auth.mySessions,
  auth.listMySessions
);

export const revokeMySession = spacetimedb.reducer(
  auth.sessionIdParams,
  auth.revokeMySession
);

export const authSweep = spacetimedb.reducer(
  { onSchedule: authSweeperTick },
  { arg: authSweeperTick.rowType },
  auth.authSweep
);

export const myAuthUser = spacetimedb.view(
  { name: 'my_auth_user', public: true },
  t.array(authUser.rowType),
  ctx => {
    const row = findCallerUser(ctx);
    return row ? [row] : [];
  }
);

export const whoami = spacetimedb.procedure(
  {},
  t.object('WhoAmI', {
    userId: t.option(t.string()),
    senderIdentityHex: t.string(),
  }),
  ctx => ({
    userId: getCallerUserId(ctx) ?? undefined,
    senderIdentityHex: ctx.sender.toHexString(),
  })
);
