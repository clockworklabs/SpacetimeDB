import { describe, expect, test } from 'vitest';
import { Timestamp } from '../src';
import {
  NETWORK_STATS_WINDOW,
  NetworkStatsTracker,
  PING_TIMEOUT_RTO_MULTIPLE,
  type NetworkStats,
} from '../src/sdk/network_stats';

// A plausible server clock offset: monotonic client time vs. Unix epoch micros.
const T0 = 1_700_000_000_000_000;

// Record a Pong arriving at `now` with round-trip `r` and clock offset `theta`,
// with no server hold time.
function pong(
  tracker: NetworkStatsTracker,
  now: number,
  r: number,
  theta: number
): void {
  const clientSendTime = now - r;
  tracker.recordPong(now, clientSendTime, clientSendTime + r / 2 + theta, 0);
}

function serverNowUs(stats: NetworkStats | undefined): number {
  return Number(stats!.serverNow.microsSinceUnixEpoch);
}

function offsetAt(tracker: NetworkStatsTracker, now: number): number {
  return serverNowUs(tracker.snapshot(now)) - now;
}

describe('NetworkStatsTracker', () => {
  test('is undefined before any Pong', () => {
    expect(new NetworkStatsTracker().snapshot(1_000_000)).toBeUndefined();
  });

  test('first sample seeds SRTT = R and RTTVAR = R/2', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPong(1_000_000, 900_000, T0, 20_000);
    const stats = tracker.snapshot(1_000_000)!;
    expect(stats.rtt).toBe(80);
    expect(stats.jitter).toBe(40);
    expect(stats.rttLatest).toBe(80);
    expect(stats.rttMin).toBe(80);
  });

  test('clamps a negative round trip to zero', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPong(1_000_000, 990_000, T0, 50_000);
    const stats = tracker.snapshot(1_000_000)!;
    expect(stats.rttLatest).toBe(0);
    expect(stats.rtt).toBe(0);
    expect(stats.jitter).toBe(0);
  });

  test('updates RTTVAR using the previous SRTT, then SRTT (RFC 6298)', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 100_000, T0);
    pong(tracker, 2_000_000, 200_000, T0);
    const stats = tracker.snapshot(2_000_000)!;
    expect(stats.jitter).toBe(62.5);
    expect(stats.rtt).toBe(112.5);
    expect(stats.rttLatest).toBe(200);
  });

  test('rttMin covers only the recent window', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0);
    for (let i = 1; i < NETWORK_STATS_WINDOW; i++) {
      pong(tracker, 1_000_000 + i * 1_000_000, 50_000, T0);
      expect(tracker.snapshot(1_000_000 + i * 1_000_000)!.rttMin).toBe(10);
    }
    // The 10_000 sample is now the oldest of a full window; one more evicts it.
    pong(tracker, 20_000_000, 50_000, T0);
    expect(tracker.snapshot(20_000_000)!.rttMin).toBe(50);
  });

  test('uses the offset of the lowest-R sample, not the latest', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0 + 5_000);
    pong(tracker, 2_000_000, 50_000, T0 + 99_999);
    // Long enough for any slew to converge.
    expect(offsetAt(tracker, 1_000_000_000)).toBe(T0 + 5_000);

    // Evict the low-R sample; ties then go to the most recent sample.
    for (let i = 0; i < NETWORK_STATS_WINDOW - 1; i++) {
      pong(tracker, 1_000_000_001 + i, 50_000, T0 + 7_000);
    }
    expect(offsetAt(tracker, 2_000_000_000)).toBe(T0 + 7_000);
  });

  test('applies the first offset immediately', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0);
    const stats = tracker.snapshot(1_000_000)!;
    expect(stats.serverNow).toBeInstanceOf(Timestamp);
    expect(serverNowUs(stats)).toBe(1_000_000 + T0);
  });

  test("serverTimeLatest is the latest Pong's server receive time", () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPong(1_000_000, 900_000, T0, 0);
    tracker.recordPong(2_000_000, 1_900_000, T0 + 1_000_000, 0);
    // It's the raw measurement, so it doesn't advance between Pongs.
    const stats = tracker.snapshot(2_500_000)!;
    expect(stats.serverTimeLatest.microsSinceUnixEpoch).toBe(
      BigInt(T0 + 1_000_000)
    );
  });

  test('slews towards a new offset at 5% of elapsed time, then stops', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0);
    // Same R, so the new sample wins the tie and the estimate jumps by +1 s.
    pong(tracker, 2_000_000, 10_000, T0 + 1_000_000);
    expect(offsetAt(tracker, 2_000_000)).toBe(T0);
    expect(offsetAt(tracker, 3_000_000)).toBe(T0 + 50_000);
    expect(offsetAt(tracker, 22_000_000)).toBe(T0 + 1_000_000);
    expect(offsetAt(tracker, 30_000_000)).toBe(T0 + 1_000_000);
  });

  test('serverNow never goes backwards', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0);
    const first = serverNowUs(tracker.snapshot(5_000_000));
    expect(serverNowUs(tracker.snapshot(5_000_000 - 5_000))).toBe(first);

    // A large negative estimate pulls the offset down, but never the clock.
    pong(tracker, 6_000_000, 10_000, T0 - 1_000_000_000);
    let previous = first;
    for (const now of [
      6_000_000, 6_000_001, 7_000_000, 6_500_000, 60_000_000,
    ]) {
      const current = serverNowUs(tracker.snapshot(now));
      expect(current).toBeGreaterThanOrEqual(previous);
      previous = current;
    }
  });

  test('reset clears the stats and the never-backwards clamp', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0);
    const before = serverNowUs(tracker.snapshot(5_000_000));

    tracker.reset();
    expect(tracker.snapshot(5_000_000)).toBeUndefined();

    pong(tracker, 1_000_000, 30_000, T0 - 1_000_000);
    const stats = tracker.snapshot(1_000_000)!;
    expect(serverNowUs(stats)).toBe(1_000_000 + T0 - 1_000_000);
    expect(serverNowUs(stats)).toBeLessThan(before);
    expect(stats.rtt).toBe(30);
    expect(stats.rttMin).toBe(30);
  });

  test('returns a distinct frozen snapshot on each access', () => {
    const tracker = new NetworkStatsTracker();
    pong(tracker, 1_000_000, 10_000, T0);
    const a = tracker.snapshot(1_000_000)!;
    const b = tracker.snapshot(1_000_000)!;
    expect(a).not.toBe(b);
    expect(a).toEqual(b);
    expect(Object.isFrozen(a)).toBe(true);
  });
});

describe('NetworkStatsTracker unanswered Pings', () => {
  // Answer the Ping sent at `sentUs`, arriving `r` later.
  function answer(tracker: NetworkStatsTracker, sentUs: number, r: number) {
    tracker.recordPong(sentUs + r, sentUs, T0 + sentUs, 0);
  }

  test('pongWait is 0 when every Ping has been answered', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPing(1_000_000);
    answer(tracker, 1_000_000, 20_000);
    expect(tracker.snapshot(5_000_000)!.pongWait).toBe(0);
  });

  test('pongWait keeps rising while Pongs stop arriving', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPing(1_000_000);
    answer(tracker, 1_000_000, 20_000);
    tracker.recordPing(2_000_000);
    tracker.recordPing(3_000_000);

    // The other fields are frozen at their last good values...
    const early = tracker.snapshot(2_500_000)!;
    const late = tracker.snapshot(6_000_000)!;
    expect(late.rtt).toBe(early.rtt);
    // ...but pongWait measures from the oldest unanswered Ping.
    expect(early.pongWait).toBe(500);
    expect(late.pongWait).toBe(4000);
  });

  test('a Pong answers every Ping sent up to its own', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPing(1_000_000);
    tracker.recordPing(2_000_000);
    tracker.recordPing(3_000_000);

    // The Pong for the second Ping overtakes the first's.
    answer(tracker, 2_000_000, 20_000);
    expect(tracker.pongWaitUs(3_500_000)).toBe(500_000);

    // The first Pong arriving late changes nothing.
    answer(tracker, 1_000_000, 2_100_000);
    expect(tracker.pongWaitUs(3_500_000)).toBe(500_000);
  });

  test('pongWaitUs is available before the first Pong', () => {
    const tracker = new NetworkStatsTracker();
    expect(tracker.pongWaitUs(1_000_000)).toBe(0);
    tracker.recordPing(1_000_000);
    expect(tracker.snapshot(4_000_000)).toBeUndefined();
    expect(tracker.pongWaitUs(4_000_000)).toBe(3_000_000);
  });

  test('is unresponsive once two Pings are unanswered past the timeout', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPing(1_000_000);
    tracker.recordPing(2_000_000);
    expect(tracker.isUnresponsive(11_000_000, 10_000_000)).toBe(false);
    expect(tracker.isUnresponsive(11_000_001, 10_000_000)).toBe(true);
  });

  test('a single unanswered Ping is never unresponsive', () => {
    // As after a long sleep: only one Ping went out, and the next tick will
    // send another before deciding.
    const tracker = new NetworkStatsTracker();
    tracker.recordPing(1_000_000);
    expect(tracker.isUnresponsive(1_000_000_000, 10_000_000)).toBe(false);
  });

  test('allows a slow link several RTOs before giving up', () => {
    const tracker = new NetworkStatsTracker();
    // A steady 3 s round trip: RTO = SRTT + 4 * RTTVAR = 3 s + 4 * 1.5 s.
    tracker.recordPing(0);
    answer(tracker, 0, 3_000_000);
    const rtoUs = 9_000_000;
    tracker.recordPing(10_000_000);
    tracker.recordPing(11_000_000);

    const limitUs = PING_TIMEOUT_RTO_MULTIPLE * rtoUs;
    expect(tracker.isUnresponsive(10_000_000 + limitUs, 10_000_000)).toBe(
      false
    );
    expect(tracker.isUnresponsive(10_000_001 + limitUs, 10_000_000)).toBe(true);
  });

  test('reset forgets unanswered Pings', () => {
    const tracker = new NetworkStatsTracker();
    tracker.recordPing(1_000_000);
    tracker.recordPing(2_000_000);
    tracker.reset();
    expect(tracker.pongWaitUs(60_000_000)).toBe(0);
    expect(tracker.isUnresponsive(60_000_000, 10_000_000)).toBe(false);
  });
});
