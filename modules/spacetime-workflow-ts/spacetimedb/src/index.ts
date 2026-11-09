import { schema, table, t } from 'spacetimedb/server';
import { Identity, Timestamp } from 'spacetimedb';
import { client } from '@spacetimedb/workflow';

const workflows = client();
const { workflowRun, workflowHistory } = workflows.tables;

const stepLog = table(
  { name: 'step_log', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    runId: t.u64().index(),
    step: t.string(),
    attempt: t.u32(),
    note: t.string(),
  }
);

const spacetimedb = schema({ ...workflows.tables, stepLog });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  workflows.install(ctx);
});

function blockUntilHostStops(): never {
  for (;;) {
    // The recovery test kills the isolated host after observing the marker
    // written before this point.
  }
}

export const workflowRunAttempt = workflows.register(spacetimedb);

// Covers retries, rollback, results, sleep, and events.
const demo = workflows.define(
  spacetimedb,
  'demo',
  {
    args: t.object('DemoArgs', { label: t.string(), failTimes: t.u32() }),
    retry: { maxAttempts: 3, backoffSeconds: 1 },
  },
  (step, args) => {
    const value = step.tx('flaky', (ctx, run) => {
      ctx.db.stepLog.insert({
        id: 0n,
        runId: run.runId,
        step: run.step,
        attempt: run.attempt,
        note: run.idempotencyKey,
      });
      // The insert above must roll back with the failed attempt.
      if (run.attempt <= args.failTimes) throw new Error('flaky.failure');
      return { big: 7n, text: 'saved', at: ctx.timestamp, who: ctx.sender };
    });
    step.sleep(1);
    step.waitFor('go');
    step.tx('finish', (ctx, run) => {
      ctx.db.stepLog.insert({
        id: 0n,
        runId: run.runId,
        step: run.step,
        attempt: run.attempt,
        // Read after a sleep and an event, so it came from the saved result.
        note: `${value.text}:${value.big + 1n}:${value.at instanceof Timestamp}:${value.who instanceof Identity}`,
      });
    });
  }
);

// Blocks until the recovery test kills the host.
const crash = workflows.define(
  spacetimedb,
  'crash',
  {
    args: t.object('CrashArgs', { label: t.string() }),
    retry: { maxAttempts: 3, backoffSeconds: 1 },
    stepTimeoutSeconds: 3,
  },
  step => {
    step.call('block', (ctx, run) => {
      ctx.withTx(tx => {
        tx.db.stepLog.insert({
          id: 0n,
          runId: run.runId,
          step: run.step,
          attempt: run.attempt,
          note: run.idempotencyKey,
        });
      });
      if (run.attempt === 1) blockUntilHostStops();
    });
    step.tx('after', () => {});
  }
);

// Catches a step that gives up, and cleans up in another step.
const caught = workflows.define(
  spacetimedb,
  'caught',
  {
    args: t.object('CaughtArgs', { label: t.string() }),
    retry: { maxAttempts: 2, backoffSeconds: 1 },
  },
  step => {
    try {
      step.tx('boom', () => {
        throw new Error('boom');
      });
    } catch (error) {
      step.tx('compensate', (ctx, run) => {
        ctx.db.stepLog.insert({
          id: 0n,
          runId: run.runId,
          step: run.step,
          attempt: run.attempt,
          note: error instanceof Error ? error.message : String(error),
        });
      });
    }
  }
);

// Each item waits until all have started, so it only finishes if they overlap.
const overlap = workflows.define(
  spacetimedb,
  'overlap',
  {
    args: t.object('OverlapArgs', { label: t.string() }),
    stepTimeoutSeconds: 30,
  },
  step => {
    const results = step.parallel('overlap', [0, 1, 2], (ctx, item, run) => {
      const started = () =>
        ctx.withTx(
          tx =>
            [...tx.db.stepLog.runId.filter(run.runId)].filter(
              row => row.note === 'started'
            ).length
        );
      ctx.withTx(tx => {
        tx.db.stepLog.insert({
          id: 0n,
          runId: run.runId,
          step: run.step,
          attempt: run.attempt,
          note: 'started',
        });
      });
      for (let i = 0; i < 200_000; i++) {
        if (started() >= 3) return item;
      }
      throw new Error('items did not run at the same time');
    });
    step.tx('after', (ctx, run) => {
      ctx.db.stepLog.insert({
        id: 0n,
        runId: run.runId,
        step: run.step,
        attempt: run.attempt,
        note: results.map(r => (r.ok ? r.value : r.error)).join(','),
      });
    });
  }
);

const uncaught = workflows.define(
  spacetimedb,
  'uncaught',
  { args: t.object('UncaughtArgs', { label: t.string() }) },
  step => {
    step.tx('boom', () => {
      throw new Error('boom');
    });
  }
);

// Waits for an event that never arrives, so the test can cancel it.
const parked = workflows.define(
  spacetimedb,
  'parked',
  { args: t.object('ParkedArgs', { label: t.string() }) },
  step => {
    step.waitFor('never');
  }
);

export const startDemo = spacetimedb.reducer(
  { label: t.string(), failTimes: t.u32() },
  (ctx, args) => {
    workflows.start(ctx, demo, args);
  }
);

export const startCrash = spacetimedb.reducer(
  { label: t.string() },
  (ctx, args) => {
    workflows.start(ctx, crash, args);
  }
);

export const startCaught = spacetimedb.reducer(
  { label: t.string() },
  (ctx, args) => {
    workflows.start(ctx, caught, args);
  }
);

export const startOverlap = spacetimedb.reducer(
  { label: t.string() },
  (ctx, args) => {
    workflows.start(ctx, overlap, args);
  }
);

export const startUncaught = spacetimedb.reducer(
  { label: t.string() },
  (ctx, args) => {
    workflows.start(ctx, uncaught, args);
  }
);

export const startParked = spacetimedb.reducer(
  { label: t.string() },
  (ctx, args) => {
    workflows.start(ctx, parked, args);
  }
);

export const signalRun = spacetimedb.reducer(
  { runId: t.u64(), event: t.string() },
  (ctx, { runId, event }) => {
    workflows.signal(ctx, runId, event);
  }
);

export const cancelWorkflowRun = spacetimedb.reducer(
  workflows.reducers.cancelWorkflowRun.params,
  workflows.reducers.cancelWorkflowRun.handler
);

export const addWorkflowAdminIdentity = spacetimedb.reducer(
  workflows.reducers.addWorkflowAdminIdentity.params,
  workflows.reducers.addWorkflowAdminIdentity.handler
);

export const removeWorkflowAdminIdentity = spacetimedb.reducer(
  workflows.reducers.removeWorkflowAdminIdentity.params,
  workflows.reducers.removeWorkflowAdminIdentity.handler
);

export const workflowRunsAdmin = spacetimedb.view(
  { name: 'workflow_runs_admin', public: true },
  t.array(workflowRun.rowType),
  workflows.views.workflowRunsAdmin
);

export const workflowHistoryAdmin = spacetimedb.view(
  { name: 'workflow_history_admin', public: true },
  t.array(workflowHistory.rowType),
  workflows.views.workflowHistoryAdmin
);
