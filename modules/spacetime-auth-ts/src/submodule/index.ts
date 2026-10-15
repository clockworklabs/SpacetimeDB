import {
  schema,
  t,
  table,
  type ArrayBuilder,
  type Infer,
  type ProcedureExport,
  type ReducerExport,
  type ViewCtx,
  type ViewExport,
} from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { install } from './install.js';
import {
  authAccountTable as authAccount,
  authAdminIdentityTable as authAdminIdentity,
  authConfigTable as authConfig,
  authConnectionBindingTable as authConnectionBinding,
  authOauthStateTable as authOauthState,
  authSessionTable as authSession,
  authUserTable as authUser,
  authVerificationTable as authVerification,
} from '../tables.js';
import * as auth from '../procedures.js';
import { findCallerUser, getCallerUserId } from '../caller.js';

const authSweeperTickOptions = { name: 'auth_sweeper_tick' };
const authSweeperTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
const authSweeperTick: ReturnType<
  typeof table<typeof authSweeperTickColumns, typeof authSweeperTickOptions>
> = table(authSweeperTickOptions, authSweeperTickColumns);

const tables: {
  rateLimit: typeof rateLimit;
  authUser: typeof authUser;
  authSession: typeof authSession;
  authAccount: typeof authAccount;
  authVerification: typeof authVerification;
  authOauthState: typeof authOauthState;
  authConfig: typeof authConfig;
  authConnectionBinding: typeof authConnectionBinding;
  authAdminIdentity: typeof authAdminIdentity;
  authSweeperTick: typeof authSweeperTick;
} = {
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
};
const spacetimedb: ReturnType<typeof schema<typeof tables>> = schema(tables);
export default spacetimedb;

export const init: ReturnType<typeof spacetimedb.init> = spacetimedb.init(
  ctx => {
    install(ctx);
  }
);

export const setAuthConfig: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.setAuthConfigParams
> = spacetimedb.reducer(auth.setAuthConfigParams, auth.setAuthConfig);

export const addAuthAdmin: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.adminParams
> = spacetimedb.reducer(auth.adminParams, auth.addAuthAdmin);

export const removeAuthAdmin: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.adminParams
> = spacetimedb.reducer(auth.adminParams, auth.removeAuthAdmin);

export const getAuthPublicKey: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof auth.authPublicKey
> = spacetimedb.procedure({}, auth.authPublicKey, auth.getAuthPublicKey);

export const linkConnection: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.linkConnectionParams
> = spacetimedb.reducer(auth.linkConnectionParams, auth.linkConnection);

export const unlinkConnection: ReducerExport<
  typeof spacetimedb.schemaType,
  {}
> = spacetimedb.reducer({}, auth.unlinkConnection);

export const updateProfile: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.updateProfileParams
> = spacetimedb.reducer(auth.updateProfileParams, auth.updateProfile);

export const revokeSession: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.sessionIdParams
> = spacetimedb.reducer(auth.sessionIdParams, auth.revokeSession);

export const listMySessions: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof auth.mySessions
> = spacetimedb.procedure({}, auth.mySessions, auth.listMySessions);

export const revokeMySession: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof auth.sessionIdParams
> = spacetimedb.reducer(auth.sessionIdParams, auth.revokeMySession);

const authSweepArgs: { arg: typeof authSweeperTick.rowType } = {
  arg: authSweeperTick.rowType,
};
export const authSweep: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof authSweepArgs
> = spacetimedb.reducer(
  { onSchedule: authSweeperTick },
  authSweepArgs,
  auth.authSweep
);

const myAuthUserResult: ArrayBuilder<typeof authUser.rowType> = t.array(
  authUser.rowType
);
export const myAuthUser: ViewExport<
  (
    ctx: ViewCtx<typeof spacetimedb.schemaType>
  ) => Infer<typeof myAuthUserResult>
> = spacetimedb.view(
  { name: 'my_auth_user', public: true },
  myAuthUserResult,
  ctx => {
    const row = findCallerUser(ctx);
    return row ? [row] : [];
  }
);

const whoamiResult = t.object('WhoAmI', {
  userId: t.option(t.string()),
  senderIdentityHex: t.string(),
});
export const whoami: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof whoamiResult
> = spacetimedb.procedure({}, whoamiResult, ctx => ({
  userId: getCallerUserId(ctx) ?? undefined,
  senderIdentityHex: ctx.sender.toHexString(),
}));
