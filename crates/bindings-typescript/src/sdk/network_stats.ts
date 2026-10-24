import { Timestamp } from '../lib/timestamp';

/**
 * A snapshot of a connection's network statistics, derived from Ping/Pong
 * round trips. Durations are in milliseconds.
 */
export type NetworkStats = Readonly<{
  /** Smoothed round-trip time (RFC 6298 SRTT), in milliseconds. */
  rtt: number;
  /** The most recent round-trip time sample, in milliseconds. */
  rttLatest: number;
  /** The lowest round-trip time over the recent sample window, in milliseconds. */
  rttMin: number;
  /** Round-trip time variation (RFC 6298 RTTVAR), in milliseconds. */
  jitter: number;
  /**
   * The estimated current time on the server, on the same clock as a
   * reducer's `ctx.timestamp`. Adjusts gradually and never goes backwards.
   */
  serverNow: Timestamp;
  /**
   * When the server received the most recent Ping, as reported in its Pong.
   * This is the raw measurement `serverNow` is derived from: it doesn't
   * advance between Pongs.
   */
  serverTimeLatest: Timestamp;
  /**
   * How long the oldest unanswered Ping has been waiting for its Pong, in
   * milliseconds, or `0` when every Ping has been answered.
   *
   * On a healthy connection this stays below `rtt`. The other fields only
   * change when a Pong arrives, so they keep showing the last good values
   * while the network is silently down; this is the field that keeps rising.
   */
  pongWait: number;
}>;

/**
 * The result of a single Ping sent with `DbConnection.ping()`. Durations are
 * in milliseconds.
 */
export type PingResult = Readonly<{
  /**
   * The round-trip time for this Ping, excluding the time the server held it:
   * `roundTrip - serverHold`, floored at zero. This is the sample that feeds
   * `NetworkStats`.
   */
  rtt: number;
  /** Wall time from sending the Ping to receiving its Pong, in milliseconds. */
  roundTrip: number;
  /** How long the server held the Ping before replying, in milliseconds. */
  serverHold: number;
  /** When the server received the Ping, on the reducer `ctx.timestamp` clock. */
  serverReceiveTime: Timestamp;
  /** When the Ping was sent, as a `performance.now()` reading. */
  sentAt: number;
  /** When the Pong arrived, as a `performance.now()` reading. */
  receivedAt: number;
}>;

/** How many recent samples feed `rttMin` and the clock offset estimate. */
export const NETWORK_STATS_WINDOW = 10;

/**
 * The fastest the applied clock offset may move towards a new estimate, as a
 * fraction of the monotonic time that has elapsed.
 */
export const NETWORK_STATS_MAX_SLEW = 0.05;

// RFC 6298 smoothing factors and clock granularity.
const ALPHA = 1 / 8;
const BETA = 1 / 4;
const K = 4;
const GRANULARITY_US = 1_000;

/** How long Pings may go unanswered before the connection is treated as lost. */
export const DEFAULT_PING_TIMEOUT_MS = 10_000;

/**
 * How many RTOs a Ping may go unanswered before the connection is considered
 * unresponsive, when that is longer than the configured Ping timeout.
 */
export const PING_TIMEOUT_RTO_MULTIPLE = 4;

type Sample = { r: number; theta: number };

/**
 * Tracks round-trip time and server clock offset from Pong replies.
 *
 * Pure: every time value is a `number` of microseconds supplied by the caller,
 * so the maths can be tested deterministically. `now` values are on the
 * client's monotonic clock (`performance.now()`); server times are
 * microseconds since the Unix epoch.
 */
export class NetworkStatsTracker {
  #srtt = 0;
  #rttvar = 0;
  #latestR = 0;
  #latestServerReceiveUs = 0;
  #samples: Sample[] = [];
  #estimate = 0;
  #appliedOffset?: number = undefined;
  #lastSlewAt = 0;
  #lastServerNowUs = -Infinity;
  // Send times of Pings with no Pong yet, oldest first.
  #unansweredSendTimesUs: number[] = [];

  /**
   * Record that a Ping went out.
   *
   * @param sentUs The Ping's `clientSendTime`, on the monotonic clock.
   */
  recordPing(sentUs: number): void {
    this.#unansweredSendTimesUs.push(sentUs);
  }

  /**
   * Record a Pong.
   *
   * @param nowUs When the Pong arrived, on the monotonic clock.
   * @param clientSendTimeUs The Ping's send time echoed back by the server.
   * @param serverReceiveTimeUs When the server received the Ping.
   * @param serverHoldUs How long the server held the Ping before replying.
   */
  recordPong(
    nowUs: number,
    clientSendTimeUs: number,
    serverReceiveTimeUs: number,
    serverHoldUs: number
  ): void {
    // A Pong proves the link was alive when its Ping arrived, so it answers
    // every Ping sent up to then. Pongs can overtake one another on the
    // server, so this doesn't assume they arrive in order.
    this.#unansweredSendTimesUs = this.#unansweredSendTimesUs.filter(
      sentUs => sentUs > clientSendTimeUs
    );

    const r = Math.max(0, nowUs - clientSendTimeUs - serverHoldUs);
    if (this.#samples.length === 0) {
      this.#srtt = r;
      this.#rttvar = r / 2;
    } else {
      // RFC 6298 requires RTTVAR to be updated using the previous SRTT.
      this.#rttvar =
        (1 - BETA) * this.#rttvar + BETA * Math.abs(this.#srtt - r);
      this.#srtt = (1 - ALPHA) * this.#srtt + ALPHA * r;
    }
    this.#latestR = r;
    this.#latestServerReceiveUs = serverReceiveTimeUs;

    const theta = serverReceiveTimeUs - (clientSendTimeUs + r / 2);
    this.#samples.push({ r, theta });
    if (this.#samples.length > NETWORK_STATS_WINDOW) {
      this.#samples.shift();
    }

    if (this.#appliedOffset === undefined) {
      this.#estimate = this.#minSample().theta;
      this.#appliedOffset = this.#estimate;
      this.#lastSlewAt = nowUs;
    } else {
      // Slew up to now towards the old estimate before switching to the new one.
      this.#advance(nowUs);
      this.#estimate = this.#minSample().theta;
    }
  }

  /**
   * A fresh, frozen snapshot of the statistics at monotonic time `nowUs`, or
   * `undefined` if no Pong has been recorded since construction or `reset()`.
   */
  snapshot(nowUs: number): NetworkStats | undefined {
    if (this.#samples.length === 0) {
      return undefined;
    }
    this.#advance(nowUs);
    const serverNowUs = Math.max(
      nowUs + this.#appliedOffset!,
      this.#lastServerNowUs
    );
    this.#lastServerNowUs = serverNowUs;
    return Object.freeze({
      rtt: this.#srtt / 1000,
      rttLatest: this.#latestR / 1000,
      rttMin: this.#minSample().r / 1000,
      jitter: this.#rttvar / 1000,
      serverNow: new Timestamp(BigInt(Math.round(serverNowUs))),
      serverTimeLatest: new Timestamp(
        BigInt(Math.round(this.#latestServerReceiveUs))
      ),
      pongWait: this.pongWaitUs(nowUs) / 1000,
    });
  }

  /**
   * Whether the connection has stopped answering Pings at monotonic time
   * `nowUs`: the oldest unanswered Ping has waited longer than `timeoutUs` (or
   * than `PING_TIMEOUT_RTO_MULTIPLE` RTOs, if longer), and at least one more
   * Ping has gone out since it.
   *
   * Requiring a second Ping means a single stale send time, such as one from
   * before the page was frozen or the machine slept, can't trip the timeout
   * on its own: the connection must also miss a Ping sent since.
   */
  isUnresponsive(nowUs: number, timeoutUs: number): boolean {
    if (this.#unansweredSendTimesUs.length < 2) {
      return false;
    }
    const limitUs = Math.max(
      timeoutUs,
      PING_TIMEOUT_RTO_MULTIPLE * this.#rtoUs()
    );
    return this.pongWaitUs(nowUs) > limitUs;
  }

  /** Forget all samples, unanswered Pings, the clock offset and the never-backwards clamp. */
  reset(): void {
    this.#srtt = 0;
    this.#rttvar = 0;
    this.#latestR = 0;
    this.#latestServerReceiveUs = 0;
    this.#samples = [];
    this.#estimate = 0;
    this.#appliedOffset = undefined;
    this.#lastSlewAt = 0;
    this.#lastServerNowUs = -Infinity;
    this.#unansweredSendTimesUs = [];
  }

  /**
   * How long the oldest unanswered Ping has waited at monotonic time `nowUs`,
   * in microseconds, or `0` if every Ping has been answered. Unlike
   * `snapshot()`, this is available before the first Pong.
   */
  pongWaitUs(nowUs: number): number {
    const oldest = this.#unansweredSendTimesUs[0];
    return oldest === undefined ? 0 : Math.max(0, nowUs - oldest);
  }

  // The RFC 6298 retransmission timeout, measured from the samples alone.
  // RFC 6298's 1 s initial value and floor are left out, so that the
  // configured timeout governs until a slow link shows it needs longer.
  #rtoUs(): number {
    if (this.#samples.length === 0) {
      return 0;
    }
    return this.#srtt + Math.max(GRANULARITY_US, K * this.#rttvar);
  }

  // The lowest-R sample in the window. Ties go to the most recent sample.
  #minSample(): Sample {
    let min = this.#samples[0]!;
    for (const sample of this.#samples) {
      if (sample.r <= min.r) {
        min = sample;
      }
    }
    return min;
  }

  // Move the applied offset towards the estimate, by at most
  // `NETWORK_STATS_MAX_SLEW` of the monotonic time elapsed since the last move.
  #advance(nowUs: number): void {
    if (this.#appliedOffset === undefined) {
      return;
    }
    const step = NETWORK_STATS_MAX_SLEW * Math.max(0, nowUs - this.#lastSlewAt);
    const delta = this.#estimate - this.#appliedOffset;
    this.#appliedOffset += Math.max(-step, Math.min(step, delta));
    this.#lastSlewAt = Math.max(this.#lastSlewAt, nowUs);
  }
}
