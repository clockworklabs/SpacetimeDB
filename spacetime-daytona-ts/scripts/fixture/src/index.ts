import {
  schema,
  t,
  table,
  type InferSchema,
  type ProcedureCtx,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as daytona from '../../../src/submodule';
import { client } from '../../../src/client';
import { type Http } from '../../../src/api';

const remote = table(
  { public: true },
  {
    id: t.u32().primaryKey(),
    creates: t.u32(),
    submits: t.u32(),
    removed: t.bool(),
    mode: t.string(),
    data: t.string(),
  }
);
const db = schema({ daytona, remote });
export default db;
type Schema = InferSchema<typeof db>;
const sandbox = client({
  submodule: (tx: TransactionCtx<Schema>) => tx.as.daytona,
  namespace: 'test',
  snapshot: 'daytona-small',
  ttlMinutes: 10,
  maxSandboxesPerOwner: 1,
});
type Command = { id: string; command: string; exitCode: number };
type State = {
  labels: Record<string, string>;
  sessions: Record<string, Command[]>;
};
export const init = db.init(ctx => {
  ctx.db.remote.insert({
    id: 0,
    creates: 0,
    submits: 0,
    removed: false,
    mode: 'normal',
    data: JSON.stringify({ labels: {}, sessions: {} }),
  });
});
export const create = db.reducer(
  { requestKey: t.string() },
  (ctx, { requestKey }) => {
    sandbox.createSandbox(ctx, { owner: ctx.sender.toHexString(), requestKey });
  }
);
export const run = db.reducer(
  { requestKey: t.string(), command: t.string() },
  (ctx, args) => {
    sandbox.runCommand(ctx, {
      owner: ctx.sender.toHexString(),
      sandboxId: [...ctx.as.daytona.db.sandbox.iter()].find(
        row => row.state.tag !== 'Deleted'
      )!.id,
      ...args,
    });
  }
);
export const otherOwner = db.reducer(ctx => {
  sandbox.deleteSandbox(ctx, { owner: 'another-owner', sandboxId: 1n });
});
export const remove = db.reducer(ctx => {
  sandbox.deleteSandbox(ctx, {
    owner: ctx.sender.toHexString(),
    sandboxId: [...ctx.as.daytona.db.sandbox.iter()].find(
      row => row.state.tag !== 'Deleted'
    )!.id,
  });
});
export const mode = db.reducer({ mode: t.string() }, (ctx, args) => {
  const row = ctx.db.remote.id.find(0)!;
  ctx.db.remote.id.update({ ...row, mode: args.mode });
});

function provider(ctx: ProcedureCtx<Schema>): Http {
  return {
    fetch(url, options) {
      const outcome = ctx.withTx(tx => {
        const row = tx.db.remote.id.find(0)!;
        const state = JSON.parse(row.data) as State;
        const path = url.replace(/^https:\/\/[^/]+/, '');
        const body = JSON.parse(options?.body ?? '{}') as Record<
          string,
          string
        >;
        const result = (
          data: object,
          status = 200,
          lost = false,
          hold = false
        ) => {
          tx.db.remote.id.update({ ...row, data: JSON.stringify(state) });
          return { data, status, lost, hold };
        };
        if (path === '/api/sandbox' && options?.method === 'POST') {
          const input = JSON.parse(options.body!) as {
            labels: Record<string, string>;
          };
          state.labels = input.labels;
          row.creates++;
          row.removed = false;
          if (row.mode === 'late_delete') {
            const parent = [...tx.as.daytona.db.sandbox.iter()].find(
              parent => parent.state.tag !== 'Deleted'
            )!;
            sandbox.deleteSandbox(tx, {
              owner: parent.owner,
              sandboxId: parent.id,
            });
          }
        } else if (
          path.startsWith('/api/sandbox/') &&
          options?.method === 'DELETE'
        ) {
          row.removed = true;
          return result({}, 200, row.mode === 'lost_delete');
        }
        if (path.startsWith('/api/sandbox')) {
          if (row.removed) return result({}, 404);
          return result(
            {
              id: 'remote-1',
              state: 'started',
              labels: state.labels,
              toolboxProxyUrl: 'https://proxy.app.daytona.io/toolbox',
              autoDestroyAt: '2030-01-01T00:00:00Z',
            },
            200,
            row.mode === 'lost_create' && options?.method === 'POST'
          );
        }
        if (path.endsWith('/process/session') && options?.method === 'POST') {
          state.sessions[body.sessionId] = [];
          return result({});
        }
        const parts = path.split('/');
        const sessionId = decodeURIComponent(parts[5]);
        const commands = state.sessions[sessionId];
        if (!commands) return result({}, 404);
        if (path.endsWith('/exec')) {
          if (row.mode === 'before_send') return result({}, 200, true);
          row.submits++;
          const id = `command-${row.submits}`;
          commands.push({
            id,
            command: body.command,
            exitCode: Number(/exit (\d+)/.exec(body.command)?.[1]),
          });
          return result(
            { cmdId: id },
            200,
            row.mode === 'lost_reply',
            row.mode === 'hold_reply'
          );
        }
        if (parts.includes('command'))
          return result(
            commands.find(command => command.id === parts.at(-1)) ?? {},
            200
          );
        return result({ sessionId, commands });
      });
      // Hold an accepted command outside its transaction until the test releases
      // the response or stops the host. Other procedures can still make progress.
      if (outcome.hold) {
        while (
          ctx.withTx(tx => tx.db.remote.id.find(0)!.mode === 'hold_reply')
        ) {
          // The test controls the release through the mode reducer.
        }
      }
      if (outcome.lost) throw new Error('simulated lost response');
      return { status: outcome.status, json: () => outcome.data };
    },
  };
}

export const reconcile = db.procedure(t.unit(), ctx => {
  sandbox.reconcile(ctx, 'fixture');
  return {};
});

// Only this test module advances due times and substitutes the remote service.
export const poll = db.procedure(t.unit(), ctx => {
  ctx.withTx(tx => {
    for (const row of tx.as.daytona.db.sandbox.iter()) {
      if (row.state.tag !== 'Deleted')
        tx.as.daytona.db.sandbox.id.update({ ...row, checkAt: tx.timestamp });
    }
    for (const row of tx.as.daytona.db.execution.iter()) {
      if (!row.finishedAt)
        tx.as.daytona.db.execution.id.update({ ...row, checkAt: tx.timestamp });
    }
  });
  sandbox.reconcile(
    {
      sender: ctx.databaseIdentity,
      databaseIdentity: ctx.databaseIdentity,
      withTx: body => ctx.withTx(body),
      http: provider(ctx),
    },
    'fixture'
  );
  return {};
});
