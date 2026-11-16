import * as assert from 'node:assert/strict';
import { Timestamp } from 'spacetimedb';
import {
  isOkStatus,
  toStatusCode,
  truncateForLog,
  featureFlagValue,
  posthogFetch,
} from '../src/submodule/http';
import {
  expireOutboxRow,
  claimHasExpired,
  claimOutboxRow,
  isPermanentFailure,
  releaseExpiredClaim,
  requeueFailedRow,
  settleOutboxClaim,
} from '../src/submodule/outbox-state';

assert.equal(isOkStatus(200), true);
assert.equal(isOkStatus(299), true);
assert.equal(isOkStatus(300), false);
assert.equal(toStatusCode(65535), 65535);
assert.equal(toStatusCode(65536), 0);
assert.equal(truncateForLog('x'.repeat(3000)).length, 2051);

const flagBody = JSON.stringify({
  flags: {
    enabled: { enabled: true },
    disabled: { enabled: false },
    experiment: { enabled: true, variant: 'control' },
    invalid: { enabled: 'true' },
  },
  requestId: 'x'.repeat(3000),
});
const cfg = { host: 'https://us.i.posthog.com', projectApiKey: 'test' };
const flagResult = posthogFetch(
  { http: { fetch: () => ({ status: 200, text: () => flagBody }) } },
  cfg,
  '/flags?v=2',
  {}
);
assert.equal(
  flagResult.responseBody,
  flagBody,
  'parse complete responses before trimming logs'
);
assert.equal(featureFlagValue(flagResult.responseBody, 'enabled'), true);
assert.equal(featureFlagValue(flagResult.responseBody, 'disabled'), false);
assert.equal(
  featureFlagValue(flagResult.responseBody, 'experiment'),
  'control'
);
for (const key of ['invalid', 'missing', 'toString']) {
  assert.equal(featureFlagValue(flagBody, key), undefined);
}
assert.equal(featureFlagValue('not json', 'enabled'), undefined);

assert.deepEqual(
  posthogFetch(
    {
      http: {
        fetch: () => {
          throw new Error('connection refused');
        },
      },
    },
    cfg,
    '/batch',
    {}
  ),
  { ok: false, statusCode: 0, responseBody: 'connection refused' },
  'network errors become retryable results'
);

for (const status of [400, 401, 403, 404, 413]) {
  assert.equal(isPermanentFailure(status), true, `${status} is permanent`);
}
for (const status of [0, 408, 429, 500, 503]) {
  assert.equal(isPermanentFailure(status), false, `${status} is retryable`);
}

const timestamp = new Timestamp(10_000_000n);
const day = 86_400_000_000n;
const queued = {
  outboxId: 'event-1',
  status: { tag: 'Queued' },
  attempts: 0,
  claimId: undefined,
  claimExpiresAt: Timestamp.UNIX_EPOCH,
  nextAttemptAt: timestamp,
  lastStatusCode: undefined,
  lastError: undefined,
  updatedAt: timestamp,
  deliveredAt: undefined,
  uuid: 'stable-event-uuid',
  createdAt: timestamp,
  retryExpiresAt: new Timestamp(timestamp.microsSinceUnixEpoch + day),
};

const expiresAt = new Timestamp(15_000_000n);
const claimed = claimOutboxRow(queued, 'claim-1', expiresAt, timestamp);
assert.equal(claimed.status.tag, 'Processing');
assert.equal(claimed.claimId, 'claim-1');
assert.equal(claimHasExpired(claimed, new Timestamp(14_999_999n)), false);
assert.equal(claimHasExpired(claimed, expiresAt), true);

const released = releaseExpiredClaim(claimed, timestamp);
assert.equal(released.status.tag, 'Queued');
assert.equal(released.claimId, undefined);
assert.equal(released.claimExpiresAt, Timestamp.UNIX_EPOCH);
let retrying = claimed;
let completedAt = timestamp;
for (let attempt = 1; attempt <= 8; attempt++) {
  // Include slow requests: the delay must start after the response.
  completedAt = new Timestamp(completedAt.microsSinceUnixEpoch + 60_000_000n);
  const settled = settleOutboxClaim(
    retrying,
    { ok: false, statusCode: 503, responseBody: 'unavailable' },
    completedAt
  );
  assert.equal(settled.attempts, attempt);
  assert.equal(settled.status.tag, 'Queued');
  assert.equal(
    settled.nextAttemptAt.microsSinceUnixEpoch,
    completedAt.microsSinceUnixEpoch +
      [5n, 10n, 20n, 40n, 80n, 300n, 300n, 300n][attempt - 1] * 1_000_000n
  );
  completedAt = settled.nextAttemptAt;
  retrying = claimOutboxRow(
    settled,
    `claim-${attempt + 1}`,
    expiresAt,
    completedAt
  );
}

const recovered = settleOutboxClaim(
  retrying,
  { ok: true, statusCode: 200, responseBody: 'ok' },
  completedAt
);
assert.equal(
  recovered.status.tag,
  'Delivered',
  'an outage longer than five attempts can recover'
);

const nearExpiry = settleOutboxClaim(
  retrying,
  { ok: false, statusCode: 503, responseBody: 'unavailable' },
  new Timestamp(queued.retryExpiresAt.microsSinceUnixEpoch - 1n)
);
assert.equal(nearExpiry.nextAttemptAt, queued.retryExpiresAt);
const exhausted = settleOutboxClaim(
  retrying,
  { ok: false, statusCode: 503, responseBody: 'unavailable' },
  queued.retryExpiresAt
);
assert.equal(exhausted.attempts, 9);
assert.equal(exhausted.status.tag, 'Failed');
assert.equal(exhausted.lastError, 'unavailable');
const unsentExpired = expireOutboxRow(queued, queued.retryExpiresAt);
assert.equal(unsentExpired.status.tag, 'Failed');
assert.equal(
  unsentExpired.attempts,
  0,
  'expiry does not count as an HTTP attempt'
);
assert.equal(unsentExpired.lastError, 'posthog.retry_expired');
const lostClaim = releaseExpiredClaim(retrying, queued.retryExpiresAt);
assert.equal(
  lostClaim.retryExpiresAt,
  queued.retryExpiresAt,
  'claim recovery does not extend the retry window'
);
assert.equal(
  expireOutboxRow(lostClaim, queued.retryExpiresAt).status.tag,
  'Failed'
);
assert.equal(
  settleOutboxClaim(
    retrying,
    { ok: true, statusCode: 200, responseBody: 'ok' },
    queued.retryExpiresAt
  ).status.tag,
  'Delivered',
  'a successful in-flight request is still delivered at the deadline'
);
assert.equal(
  settleOutboxClaim(
    claimed,
    {
      ok: false,
      statusCode: 503,
      responseBody: 'x'.repeat(3000),
    },
    timestamp
  ).lastError,
  truncateForLog('x'.repeat(3000))
);

const unauthorized = settleOutboxClaim(
  claimed,
  { ok: false, statusCode: 401, responseBody: 'invalid api key' },
  timestamp
);
assert.equal(unauthorized.attempts, 1);
assert.equal(unauthorized.status.tag, 'Failed', 'auth errors fail at once');

const requeuedAt = new Timestamp(timestamp.microsSinceUnixEpoch + 2n * day);
const requeued = requeueFailedRow(unauthorized, requeuedAt);
assert.equal(requeued.status.tag, 'Queued');
assert.equal(requeued.attempts, 0);
assert.equal(
  requeued.retryExpiresAt.microsSinceUnixEpoch,
  requeuedAt.microsSinceUnixEpoch + day
);
assert.equal(requeued.uuid, queued.uuid);
assert.equal(requeued.createdAt, queued.createdAt);
assert.equal(
  settleOutboxClaim(
    requeued,
    { ok: false, statusCode: 429, responseBody: 'limited' },
    requeuedAt
  ).status.tag,
  'Queued'
);

const delivered = settleOutboxClaim(
  claimed,
  { ok: true, statusCode: 200, responseBody: 'ok' },
  timestamp
);
assert.equal(delivered.status.tag, 'Delivered');
assert.equal(delivered.deliveredAt, timestamp);
assert.equal(delivered.lastError, undefined);

console.log('posthog tests passed');
