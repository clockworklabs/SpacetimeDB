import {
  schema,
  table,
  t,
  ScheduleAt,
  SenderError,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as daytona from '@spacetimedb/daytona/submodule';
import { client, daytonaTick, sandbox, execution } from '@spacetimedb/daytona';

const operator = table(
  { name: 'operator' },
  { identity: t.identity().primaryKey() }
);
const spacetimedb = schema(
  { daytona, daytonaTick, operator },
  {
    env: { DAYTONA_API_KEY: t.string() },
  }
);
export default spacetimedb;

const daytonaClient = client({
  submodule: (tx: TransactionCtx<InferSchema<typeof spacetimedb>>) =>
    tx.as.daytona,
  namespace: 'daytona',
  snapshot: 'daytona-small',
  ttlMinutes: 10,
  maxSandboxesPerOwner: 1,
});

export const init = spacetimedb.init(ctx => {
  ctx.db.operator.insert({ identity: ctx.sender });
  ctx.db.daytonaTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(3_000_000n),
  });
});

export const grantAccess = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    if (!ctx.db.operator.identity.find(ctx.sender))
      throw new SenderError('daytona.operator_only');
    if (!ctx.db.operator.identity.find(identity))
      ctx.db.operator.insert({ identity });
  }
);

function owner(ctx: TransactionCtx<InferSchema<typeof spacetimedb>>): string {
  if (!ctx.db.operator.identity.find(ctx.sender))
    throw new SenderError('daytona.operator_only');
  return ctx.sender.toHexString();
}

export const createSandbox = spacetimedb.reducer(
  { requestKey: t.string() },
  (ctx, { requestKey }) => {
    daytonaClient.createSandbox(ctx, { owner: owner(ctx), requestKey });
  }
);
export const runCommand = spacetimedb.reducer(
  { sandboxId: t.u64(), requestKey: t.string(), command: t.string() },
  (ctx, args) => {
    daytonaClient.runCommand(ctx, { owner: owner(ctx), ...args });
  }
);
export const deleteSandbox = spacetimedb.reducer(
  { sandboxId: t.u64() },
  (ctx, args) => {
    daytonaClient.deleteSandbox(ctx, { owner: owner(ctx), ...args });
  }
);
export const reconcileDaytona = spacetimedb.procedure(
  { onSchedule: daytonaTick },
  { tick: daytonaTick.rowType },
  t.unit(),
  ctx => {
    daytonaClient.reconcile(ctx, ctx.env.DAYTONA_API_KEY);
    return {};
  }
);

export const canManage = spacetimedb.view(
  { public: true },
  t.array(operator.rowType),
  ctx => {
    const row = ctx.db.operator.identity.find(ctx.sender);
    return row ? [row] : [];
  }
);
export const mySandboxes = spacetimedb.view(
  { public: true },
  t.array(sandbox.rowType),
  ctx => [...ctx.db.daytona.sandbox.owner.filter(ctx.sender.toHexString())]
);
export const myExecutions = spacetimedb.view(
  { public: true },
  t.array(execution.rowType),
  ctx => {
    const rows = [];
    for (const row of ctx.db.daytona.sandbox.owner.filter(
      ctx.sender.toHexString()
    )) {
      rows.push(...ctx.db.daytona.execution.sandboxId.filter(row.id));
    }
    return rows;
  }
);
