// Local integration suite for @spacetimedb/cron. Publishes the demo module
// (spacetimedb/) and the example module (example/spacetimedb/) against a
// running local SpacetimeDB and drives the full fire semantics:
// single-stage scheduled handlers, volatile failure recovery, lost-fire repair,
// payload rollback, auto-disable, procedures, generations, and cancellation.
//
// Requires: `spacetime start` running locally.
import * as assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const database = `cron-release-${process.pid}-${Date.now()}`;
const exampleDatabase = `${database}-example`;
const serverArgs = ['--server', 'local'];
let published = false;
let examplePublished = false;

function run(args, options = {}) {
  const result = spawnSync('spacetime', args, {
    cwd: new URL('..', import.meta.url),
    encoding: 'utf8',
    shell: false,
  });
  if (options.expectFailure) {
    assert.notEqual(
      result.status,
      0,
      `command unexpectedly succeeded: spacetime ${args.join(' ')}`
    );
    return result;
  }
  assert.equal(
    result.status,
    0,
    `command failed: spacetime ${args.join(' ')}\n${result.stdout}\n${result.stderr}`
  );
  return result;
}

function call(name, ...args) {
  return run(['call', ...serverArgs, database, name, ...args]);
}

function callExpectFailure(name, ...args) {
  return run(['call', ...serverArgs, database, name, ...args], {
    expectFailure: true,
  });
}

function sql(query, target = database) {
  const result = run(['sql', ...serverArgs, '--format', 'json', target, query]);
  return JSON.parse(result.stdout)[0].rows;
}

function sqlExpectFailure(query, target = database) {
  return run(['sql', ...serverArgs, '--format', 'json', target, query], {
    expectFailure: true,
  });
}

function count(table, where = '') {
  return Number(sql(`SELECT COUNT(*) AS count FROM ${table} ${where}`)[0][0]);
}

function jobRow(name) {
  const rows = sql(
    `SELECT enabled, fireCount, consecutiveFailures, disabledReason, generation FROM cron_job WHERE name = '${name}'`
  );
  assert.equal(rows.length, 1, `expected one cron_job row for ${name}`);
  const [enabled, fireCount, consecutiveFailures, disabledReason, generation] =
    rows[0];
  return {
    enabled,
    fireCount,
    consecutiveFailures,
    disabledReason,
    generation,
  };
}

function publicDisabledReason(name) {
  const rows = sql(
    `SELECT disabledReason FROM cron_jobs WHERE name = '${name}'`
  );
  assert.equal(rows.length, 1, `expected one cron_jobs row for ${name}`);
  const option = rows[0][0];
  assert.deepEqual(option?.[0], 0, `expected a disabled reason for ${name}`);
  return option[1];
}

function wait(milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds));
}

async function poll(description, check, timeoutMilliseconds = 15_000) {
  const deadline = Date.now() + timeoutMilliseconds;
  let lastError;

  while (Date.now() < deadline) {
    try {
      const result = check();
      if (result) return result;
      lastError = undefined;
    } catch (error) {
      lastError = error;
    }
    await wait(200);
  }

  const detail = lastError instanceof Error ? `: ${lastError.message}` : '';
  throw new Error(`timed out waiting for ${description}${detail}`);
}

try {
  run([
    'publish',
    ...serverArgs,
    '--yes',
    '--module-path',
    'spacetimedb',
    database,
  ]);
  published = true;

  // init seeded the heartbeat job (every 30s) declaratively.
  assert.equal(count('cron_job'), 1, 'init must seed exactly one job');
  assert.equal(
    count('cron_jobs'),
    1,
    'public job view must expose seeded state'
  );
  assert.equal(
    sql("SELECT name, enabled FROM cron_jobs WHERE name = 'heartbeat'").length,
    1,
    'public job view must support filtered operational queries'
  );
  sqlExpectFailure('SELECT args FROM cron_jobs');
  assert.equal(
    count('heartbeat_fire', "WHERE jobName = 'heartbeat'"),
    1,
    'seeded job must have exactly one pending fire'
  );
  assert.equal(
    count('cron_reconcile_tick'),
    1,
    'configured reconciliation must keep one native interval row'
  );

  // Management operations opportunistically repair an enabled job whose fire
  // disappeared, record the loss, and preserve the one-row invariant.
  call('drop_heartbeat_fire_for_test');
  assert.equal(count('heartbeat_fire'), 0);
  assert.equal(jobRow('heartbeat').enabled, true);
  call(
    'schedule_report',
    JSON.stringify('0 0 1 1 *'),
    JSON.stringify('UTC'),
    '0',
    JSON.stringify('reconcile-probe'),
    '1'
  );
  assert.equal(
    count('heartbeat_fire', "WHERE jobName = 'heartbeat'"),
    1,
    'management scheduling must repair another enabled job'
  );
  assert.equal(
    count('cron_run', "WHERE jobName = 'heartbeat'"),
    1,
    'opportunistic repair must record lost_fire'
  );
  assert.deepEqual(
    sql("SELECT error FROM cron_run WHERE jobName = 'heartbeat'")[0][0],
    [0, 'lost_fire'],
    'repair history must preserve the lost_fire reason'
  );

  // The optional interval sweep repairs the same invariant without a
  // management call.
  call('schedule_every', JSON.stringify('heartbeat'), '30', '0');
  call('drop_heartbeat_fire_for_test');
  await poll('the reconciliation sweep to restore heartbeat', () => {
    return (
      count('heartbeat_fire', "WHERE jobName = 'heartbeat'") === 1 &&
      count('cron_run', "WHERE jobName = 'heartbeat'") >= 2
    );
  });
  assert.equal(
    count('heartbeat_fire', "WHERE jobName = 'heartbeat'"),
    1,
    'reconciliation sweep must restore a missing fire'
  );
  assert.equal(
    count('cron_run', "WHERE jobName = 'heartbeat'"),
    2,
    'sweep repair must record lost_fire'
  );

  // Lost fires participate in the normal consecutive-failure policy.
  call('schedule_every', JSON.stringify('heartbeat'), '30', '1');
  call('drop_heartbeat_fire_for_test');
  await poll(
    'the lost-fire policy to disable heartbeat',
    () => jobRow('heartbeat').enabled === false
  );
  const lostFireDisabled = jobRow('heartbeat');
  assert.equal(lostFireDisabled.enabled, false);
  assert.match(
    String(lostFireDisabled.disabledReason),
    /failed_1_consecutive_times:lost_fire/
  );
  assert.equal(
    publicDisabledReason('heartbeat'),
    'lost_fire_threshold_reached',
    'public view must expose a safe lost-fire reason code'
  );
  assert.equal(count('heartbeat_fire'), 0);
  call('schedule_every', JSON.stringify('heartbeat'), '30', '0');

  // Chain job fires on cadence and keeps the single-pending-fire invariant.
  call(
    'schedule_report',
    JSON.stringify('*/1 * * * * *'),
    JSON.stringify('UTC'),
    '0',
    JSON.stringify('primary'),
    '25'
  );
  await poll(
    'at least two report fires',
    () => count('tick_log', "WHERE jobName = 'report'") >= 2
  );
  const reportTicks = count('tick_log', "WHERE jobName = 'report'");
  assert.ok(
    reportTicks >= 2,
    `expected at least two report fires, got ${reportTicks}`
  );
  assert.equal(
    count('report_fire', "WHERE jobName = 'report'"),
    1,
    'chain job must keep exactly one pending fire'
  );
  const report = jobRow('report');
  assert.equal(report.enabled, true);
  // Fires keep landing between queries, so bound rather than equate.
  assert.ok(
    Number(report.fireCount) >= reportTicks,
    `fireCount ${report.fireCount} must cover observed ticks ${reportTicks}`
  );
  const reportArgs = sql(
    "SELECT value, count FROM argument_log WHERE jobName = 'report'"
  );
  assert.ok(reportArgs.length >= 2, 'report arguments must reach every fire');
  assert.ok(
    reportArgs.every(
      ([value, batchSize]) => value === 'primary' && batchSize === 25
    ),
    'report fires must receive the scheduled typed arguments'
  );

  const reportGeneration = BigInt(report.generation);
  call(
    'schedule_report',
    JSON.stringify('*/1 * * * * *'),
    JSON.stringify('UTC'),
    '0',
    JSON.stringify('replacement'),
    '7'
  );
  assert.equal(
    BigInt(jobRow('report').generation),
    reportGeneration + 1n,
    'rescheduling typed work must start a new generation'
  );
  await poll(
    'the replacement report arguments',
    () =>
      count(
        'argument_log',
        "WHERE jobName = 'report' AND value = 'replacement' AND count = 7"
      ) >= 1
  );
  assert.ok(
    count(
      'argument_log',
      "WHERE jobName = 'report' AND value = 'replacement' AND count = 7"
    ) >= 1,
    'rescheduling must replace the durable arguments'
  );

  // A failed payload rolls back its writes. Volatile recovery rearms the
  // calendar chain, records the failure, and disables at the threshold.
  call(
    'schedule_cron',
    JSON.stringify('flaky'),
    JSON.stringify('*/1 * * * * *'),
    JSON.stringify('UTC'),
    '2'
  );
  call('set_flaky_failing', 'true');
  await poll('the flaky job to reach its failure threshold', () => {
    const state = jobRow('flaky');
    return !state.enabled && Number(state.consecutiveFailures) >= 2;
  });
  assert.equal(
    count('tick_log', "WHERE jobName = 'flaky'"),
    0,
    'failed payload writes must roll back'
  );
  const flaky = jobRow('flaky');
  assert.equal(flaky.enabled, false, 'flaky must auto-disable');
  assert.equal(Number(flaky.consecutiveFailures), 2);
  assert.match(
    String(flaky.disabledReason),
    /failed_2_consecutive_times:flaky\.failure/
  );
  assert.equal(
    publicDisabledReason('flaky'),
    'failure_threshold_reached',
    'public view must not expose handler errors'
  );
  assert.equal(
    count('flaky_fire', "WHERE jobName = 'flaky'"),
    0,
    'auto-disabled job must have no pending fire'
  );
  assert.equal(
    count('cron_run', "WHERE jobName = 'flaky'"),
    2,
    'each failed invocation must have one durable run record'
  );

  // Native interval rows persist after a failed transaction. The explicit
  // recovery payload must still record the failure and apply maxFailures.
  const failuresBeforeInterval = count('cron_run', "WHERE jobName = 'flaky'");
  call('schedule_every', JSON.stringify('flaky'), '1', '1');
  await poll('the failed interval job to disable itself', () => {
    const state = jobRow('flaky');
    return !state.enabled && Number(state.consecutiveFailures) >= 1;
  });
  assert.equal(
    count('cron_run', "WHERE jobName = 'flaky'"),
    failuresBeforeInterval + 1,
    'interval recovery must commit one failed run record'
  );
  assert.equal(
    count('flaky_fire', "WHERE jobName = 'flaky'"),
    0,
    'an interval job disabled by maxFailures must remove its persistent fire'
  );

  // Recovery: fix the payload and reschedule; the chain resumes cleanly.
  call('set_flaky_failing', 'false');
  call('schedule_every', JSON.stringify('flaky'), '1', '0');
  await poll(
    'the rescheduled flaky job to fire',
    () => count('tick_log', "WHERE jobName = 'flaky'") >= 1
  );
  assert.ok(
    count('tick_log', "WHERE jobName = 'flaky'") >= 1,
    'rescheduled job must fire again'
  );
  assert.equal(jobRow('flaky').enabled, true);

  // Rescheduling creates a new generation so stale fire or recovery work cannot
  // mutate the replacement schedule.
  const recoveredGeneration = BigInt(jobRow('flaky').generation);
  call('schedule_every', JSON.stringify('flaky'), '2', '0');
  assert.equal(
    BigInt(jobRow('flaky').generation),
    recoveredGeneration + 1n,
    'reschedule must increment the generation'
  );
  assert.equal(
    count('flaky_fire', "WHERE jobName = 'flaky'"),
    1,
    'reschedule must atomically replace the pending trigger'
  );

  // Procedure job: fires through the two-transaction middleware.
  call(
    'schedule_probe',
    JSON.stringify('*/1 * * * * *'),
    JSON.stringify('UTC'),
    '0',
    JSON.stringify('health-check')
  );
  await poll('the procedure job to fire with its arguments', () => {
    return (
      count('tick_log', "WHERE jobName = 'probe'") >= 1 &&
      count(
        'argument_log',
        "WHERE jobName = 'probe' AND value = 'health-check'"
      ) >= 1
    );
  });
  assert.ok(
    count('tick_log', "WHERE jobName = 'probe'") >= 1,
    'procedure job must fire'
  );
  assert.ok(
    count(
      'argument_log',
      "WHERE jobName = 'probe' AND value = 'health-check'"
    ) >= 1,
    'procedure job must receive its typed arguments'
  );
  assert.equal(count('probe_fire', "WHERE jobName = 'probe'"), 1);

  // Unschedule disarms and disables without deleting state.
  call('unschedule_job', JSON.stringify('probe'));
  assert.equal(
    count('probe_fire', "WHERE jobName = 'probe'"),
    0,
    'unschedule must disarm'
  );
  assert.equal(jobRow('probe').enabled, false);

  // Procedure failures are recorded in a committed follow-up transaction and
  // participate in the same automatic disable policy.
  call('unschedule_job', JSON.stringify('flaky'));
  call('set_flaky_failing', 'true');
  const successfulProbeTicks = count('tick_log', "WHERE jobName = 'probe'");
  call(
    'schedule_probe',
    JSON.stringify('*/1 * * * * *'),
    JSON.stringify('UTC'),
    '1',
    JSON.stringify('failure-check')
  );
  await poll(
    'the failed procedure to disable itself',
    () => jobRow('probe').enabled === false
  );
  const failedProbe = jobRow('probe');
  assert.equal(
    failedProbe.enabled,
    false,
    'failed procedure must auto-disable'
  );
  assert.equal(Number(failedProbe.consecutiveFailures), 1);
  assert.match(String(failedProbe.disabledReason), /probe\.failure/);
  assert.equal(
    count('tick_log', "WHERE jobName = 'probe'"),
    successfulProbeTicks,
    'failed procedure must not report successful application work'
  );
  call('set_flaky_failing', 'false');

  // History stays capped (default 5 per job) despite many report fires.
  const reportFireCountBeforeHistoryCheck = Number(jobRow('report').fireCount);
  await poll(
    'additional report fires for history pruning',
    () =>
      Number(jobRow('report').fireCount) >=
      reportFireCountBeforeHistoryCheck + 2
  );
  assert.ok(
    count('cron_run', "WHERE jobName = 'report'") <= 5,
    'completed history must stay bounded'
  );

  // Validation rejections surface as failed calls before any row changes.
  callExpectFailure(
    'schedule_cron',
    JSON.stringify('report'),
    JSON.stringify('not a cron'),
    JSON.stringify('UTC'),
    '0'
  );
  callExpectFailure(
    'schedule_cron',
    JSON.stringify('report'),
    JSON.stringify('* * * * *'),
    JSON.stringify('Mars/Olympus'),
    '0'
  );
  callExpectFailure('schedule_every', JSON.stringify('report'), '0', '0');
  callExpectFailure('schedule_every', JSON.stringify('nope'), '5', '0');

  // Scheduled job reducers reject direct client calls.
  callExpectFailure('forge_report_fire_for_test');

  // The example module publishes and seeds its declared jobs.
  run([
    'publish',
    ...serverArgs,
    '--yes',
    '--module-path',
    'example/spacetimedb',
    exampleDatabase,
  ]);
  examplePublished = true;
  assert.equal(
    Number(
      sql('SELECT COUNT(*) AS count FROM cron_job', exampleDatabase)[0][0]
    ),
    2,
    'example init must seed digest and cleanup'
  );

  console.log('cron module local test passed');
} finally {
  if (published) {
    run(['delete', ...serverArgs, '--yes', database]);
  }
  if (examplePublished) {
    run(['delete', ...serverArgs, '--yes', exampleDatabase]);
  }
}
