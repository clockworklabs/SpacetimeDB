import * as assert from 'node:assert/strict';
import {
  isOkStatus,
  toStatusCode,
  truncateForLog,
} from '../src/submodule/value-utils.ts';
import {
  MAX_DELIVERY_ATTEMPTS,
  claimHasExpired,
  claimOutboxRow,
  releaseExpiredClaim,
  retryDelayMicros,
  settleOutboxClaim,
} from '../src/submodule/outbox-state.ts';

assert.equal(isOkStatus(200), true);
assert.equal(isOkStatus(299), true);
assert.equal(isOkStatus(300), false);
assert.equal(toStatusCode(65535), 65535);
assert.equal(toStatusCode(65536), 0);
assert.equal(truncateForLog('x'.repeat(3000)).length, 2051);

const timestamp = { microsSinceUnixEpoch: 10_000_000n };
const queued = {
  outboxId: 'event-1',
  status: { tag: 'Queued' },
  attempts: 0,
  claimId: undefined,
  claimExpiresAtMicros: 0n,
  nextAttemptAt: timestamp,
  lastStatusCode: undefined,
  lastError: undefined,
  updatedAt: timestamp,
  deliveredAt: undefined,
};

const claimed = claimOutboxRow(queued, 'claim-1', 15_000_000n, timestamp);
assert.equal(claimed.status.tag, 'Processing');
assert.equal(claimed.claimId, 'claim-1');
assert.equal(claimHasExpired(claimed, 14_999_999n), false);
assert.equal(claimHasExpired(claimed, 15_000_000n), true);

const released = releaseExpiredClaim(claimed, timestamp);
assert.equal(released.status.tag, 'Queued');
assert.equal(released.claimId, undefined);
assert.equal(released.claimExpiresAtMicros, 0n);
assert.equal(retryDelayMicros(1), 1_000_000n);
assert.equal(retryDelayMicros(2), 2_000_000n);
assert.equal(retryDelayMicros(20), 300_000_000n);

let retrying = claimed;
for (let attempt = 1; attempt < MAX_DELIVERY_ATTEMPTS; attempt++) {
  const settled = settleOutboxClaim(
    retrying,
    { ok: false, statusCode: 503, responseBody: 'unavailable' },
    timestamp,
    { microsSinceUnixEpoch: 11_000_000n }
  );
  assert.equal(settled.row.attempts, attempt);
  assert.equal(settled.terminal, false);
  assert.equal(settled.row.status.tag, 'Queued');
  retrying = claimOutboxRow(
    settled.row,
    `claim-${attempt + 1}`,
    15_000_000n,
    timestamp
  );
}

const exhausted = settleOutboxClaim(
  retrying,
  { ok: false, statusCode: 503, responseBody: 'unavailable' },
  timestamp,
  { microsSinceUnixEpoch: 11_000_000n }
);
assert.equal(exhausted.row.attempts, MAX_DELIVERY_ATTEMPTS);
assert.equal(exhausted.terminal, true);
assert.equal(exhausted.row.status.tag, 'Failed');
assert.equal(exhausted.row.lastError, 'unavailable');

const delivered = settleOutboxClaim(
  claimed,
  { ok: true, statusCode: 200, responseBody: 'ok' },
  timestamp,
  timestamp
);
assert.equal(delivered.terminal, true);
assert.equal(delivered.row.status.tag, 'Delivered');
assert.equal(delivered.row.deliveredAt, timestamp);
assert.equal(delivered.row.lastError, undefined);

console.log('posthog tests passed');
