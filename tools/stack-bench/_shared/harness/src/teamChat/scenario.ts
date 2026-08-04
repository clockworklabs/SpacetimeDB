// The backend-neutral behavioral scenario for the team-chat task.
//
// ~44 named checks in 4 weighted groups (correctness/realtime/durability/perf).
// The scenario is the only writer, so it maintains an exact expected-state
// model (τ-bench pattern) and compares backend snapshots against it at
// quiescent points. Concurrency checks (gapless seq, unread invariant, tip
// conservation) exercise transactional behavior; the restart phase exercises
// durability including durable counters and idempotency records.

import type { AppClient, AppClientFactory, MessageRecord } from "../appClient.js";
import { TeamChatModel } from "./model.js";

export type CheckGroup = "correctness" | "realtime" | "durability" | "perf";

export interface CheckResult {
  group: CheckGroup;
  name: string;
  pass: boolean;
  detail?: string;
}

export interface ScenarioMetrics {
  latency_p50_ms?: number;
  latency_p95_ms?: number;
  burst_throughput_msgs_per_sec?: number;
}

export interface ScenarioResult {
  checks: CheckResult[];
  metrics: ScenarioMetrics;
}

export const GROUP_WEIGHTS: Record<CheckGroup, number> = {
  correctness: 0.4,
  realtime: 0.3,
  durability: 0.2,
  perf: 0.1,
};

const T = Number(process.env.DELIVERY_TIMEOUT_MS ?? 15_000);
const SETTLE_TIMEOUT_MS = Number(process.env.SETTLE_TIMEOUT_MS ?? 20_000);

function waitFor(pred: () => boolean, timeoutMs = T, intervalMs = 100): Promise<boolean> {
  return new Promise((resolve) => {
    const start = Date.now();
    const tick = () => {
      if (pred()) return resolve(true);
      if (Date.now() - start >= timeoutMs) return resolve(false);
      setTimeout(tick, intervalMs);
    };
    tick();
  });
}

/** Key-order-insensitive canonical serialization for deep equality. */
function canon(x: unknown): string {
  if (Array.isArray(x)) return "[" + x.map(canon).join(",") + "]";
  if (x !== null && typeof x === "object") {
    const keys = Object.keys(x as object).sort();
    return "{" + keys.map((k) => JSON.stringify(k) + ":" + canon((x as any)[k])).join(",") + "}";
  }
  return JSON.stringify(x) ?? "undefined";
}

/** Poll an async snapshot until it deep-equals `expected` (handles propagation lag). */
async function settleEqual<X>(
  snap: () => Promise<X>,
  expected: X,
  timeoutMs = SETTLE_TIMEOUT_MS,
): Promise<{ ok: boolean; last: X | undefined }> {
  const want = canon(expected);
  const start = Date.now();
  let last: X | undefined;
  for (;;) {
    try {
      last = await snap();
      if (canon(last) === want) return { ok: true, last };
    } catch {
      /* backend may still be settling/restarting */
    }
    if (Date.now() - start >= timeoutMs) return { ok: false, last };
    await new Promise((r) => setTimeout(r, 250));
  }
}

/** Did `op` reject? (expected-failure probe) */
async function rejects(op: () => Promise<void>): Promise<boolean> {
  try {
    await op();
    return false;
  } catch {
    return true;
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const percentile = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)];
};

/** Recorder for one room subscription: collects message events in arrival order. */
class RoomRecorder {
  events: MessageRecord[] = [];
  /** newest event per clientMsgId (edits/deletes overwrite) */
  get latest(): Map<string, MessageRecord> {
    const m = new Map<string, MessageRecord>();
    for (const e of this.events) m.set(e.clientMsgId, e);
    return m;
  }
  seqsSeen(): number[] {
    // first arrival per clientMsgId only (ignore edit/delete re-deliveries)
    const seen = new Set<string>();
    const out: number[] = [];
    for (const e of this.events) {
      if (!seen.has(e.clientMsgId)) {
        seen.add(e.clientMsgId);
        out.push(e.seq);
      }
    }
    return out;
  }
  handlers() {
    return { onMessage: (m: MessageRecord) => this.events.push(m) };
  }
}

export interface ScenarioEnv {
  makeClient: AppClientFactory;
  /** Restart the backend process; resolves when the restart command exits. */
  restartBackend?: () => Promise<void>;
}

export async function runTeamChatScenario(env: ScenarioEnv): Promise<ScenarioResult> {
  const { makeClient } = env;
  const checks: CheckResult[] = [];
  const metrics: ScenarioMetrics = {};
  const rec = (group: CheckGroup, name: string, pass: boolean, detail?: string) => {
    checks.push({ group, name, pass, detail: pass ? undefined : detail });
    console.log(`  [${pass ? "PASS" : "FAIL"}] ${group}/${name}${!pass && detail ? " — " + detail : ""}`);
  };

  const model = new TeamChatModel();
  const open: AppClient[] = [];
  const client = async () => {
    const c = makeClient();
    await c.connect();
    open.push(c);
    return c;
  };

  let driver!: AppClient;
  try {
    driver = await client();

    // =========================================================================
    // CORRECTNESS
    // =========================================================================
    for (const u of ["alice", "bob", "carol", "dave", "mallory"]) {
      await driver.register(u);
      model.register(u);
    }
    {
      const a = await settleEqual(() => driver.getUser("alice"), model.expectedUser("alice"));
      rec("correctness", "register_creates_user", a.ok, `got ${JSON.stringify(a.last)}`);
    }

    await driver.setStatus("alice", "away");
    model.setStatus("alice", "away");
    await driver.register("alice"); // must be a no-op
    {
      const a = await settleEqual(() => driver.getUser("alice"), model.expectedUser("alice"));
      rec("correctness", "register_idempotent_preserves_state", a.ok, `got ${JSON.stringify(a.last)}`);
    }
    {
      const bad = await rejects(() => driver.setStatus("alice", "invisible"));
      const unknown = await rejects(() => driver.setStatus("nobody", "online"));
      rec("correctness", "set_status_validation", bad && unknown, `invalid-status rejected=${bad}, unknown-user rejected=${unknown}`);
    }

    await driver.createRoom("alice", "general");
    model.createRoom("alice", "general");
    {
      const owner = await driver.getRoomOwner("general");
      const mem = await settleEqual(() => driver.getMembers("general"), model.expectedMembers("general"));
      rec("correctness", "create_room_owner_and_membership", owner === "alice" && mem.ok, `owner=${owner} members=${JSON.stringify(mem.last)}`);
    }
    rec("correctness", "duplicate_room_rejected", await rejects(() => driver.createRoom("bob", "general")));

    for (const u of ["bob", "carol", "dave"]) {
      await driver.joinRoom(u, "general");
      model.joinRoom(u, "general");
    }
    await driver.leaveRoom("dave", "general");
    model.removeMember("dave", "general");
    {
      const mem = await settleEqual(() => driver.getMembers("general"), model.expectedMembers("general"));
      rec("correctness", "join_leave_membership", mem.ok, `members=${JSON.stringify(mem.last)}`);
    }

    rec("correctness", "nonmember_send_rejected", await rejects(() => driver.sendMessage("mallory", "general", "hi", "mal-1")));
    rec("correctness", "empty_text_rejected", await rejects(() => driver.sendMessage("bob", "general", "", "bob-empty")));
    rec("correctness", "oversize_text_rejected", await rejects(() => driver.sendMessage("bob", "general", "x".repeat(4001), "bob-big")));
    rec("correctness", "unknown_room_send_rejected", await rejects(() => driver.sendMessage("bob", "nowhere", "hi", "bob-lost")));

    await driver.sendMessage("bob", "general", "first!", "g-1");
    model.sendMessage("bob", "general", "first!", "g-1");
    await driver.sendMessage("bob", "general", "retry of first", "g-1"); // idempotent resend: must succeed, no dupe
    {
      const msgs = await settleEqual(() => driver.getMessages("general"), model.expectedMessages("general"));
      rec("correctness", "idempotent_resend_no_duplicate", msgs.ok, `messages=${JSON.stringify(msgs.last)}`);
    }

    for (let i = 2; i <= 5; i++) {
      await driver.sendMessage("alice", "general", `serial ${i}`, `g-${i}`);
      model.sendMessage("alice", "general", `serial ${i}`, `g-${i}`);
    }
    {
      const msgs = await driver.getMessages("general");
      const seqs = msgs.map((m) => m.seq);
      const ok = JSON.stringify(seqs) === JSON.stringify([1, 2, 3, 4, 5]);
      rec("correctness", "seq_serial_gapless", ok, `seqs=${JSON.stringify(seqs)}`);
    }

    // Concurrent sends from 3 separate connections: seq must stay gapless/unique.
    const [cA, cB, cC] = [await client(), await client(), await client()];
    {
      const jobs: Promise<void>[] = [];
      const senders: Array<[AppClient, string]> = [[cA, "alice"], [cB, "bob"], [cC, "carol"]];
      for (const [conn, user] of senders) {
        for (let i = 0; i < 10; i++) {
          jobs.push(conn.sendMessage(user, "general", `conc ${user} ${i}`, `conc-${user}-${i}`));
        }
      }
      const results = await Promise.allSettled(jobs);
      const failed = results.filter((r) => r.status === "rejected").length;
      // Wait until all 35 messages are visible, then apply the backend's chosen order.
      const snapshot = await (async () => {
        let msgs: MessageRecord[] = [];
        const deadline = Date.now() + SETTLE_TIMEOUT_MS;
        for (;;) {
          msgs = await driver.getMessages("general");
          if (msgs.length >= 35 || Date.now() > deadline) break;
          await sleep(250);
        }
        return { done: msgs.length >= 35, msgs };
      })();
      // Apply the 30 concurrent sends to the model in the order the backend chose.
      for (const m of snapshot.msgs.filter((m) => m.clientMsgId.startsWith("conc-"))) {
        model.sendMessage(m.sender, "general", m.text, m.clientMsgId);
      }
      const seqs = snapshot.msgs.map((m) => m.seq);
      const expectSeqs = Array.from({ length: 35 }, (_, i) => i + 1);
      const gapless = JSON.stringify(seqs) === JSON.stringify(expectSeqs);
      rec(
        "correctness",
        "seq_concurrent_gapless",
        failed === 0 && snapshot.done && gapless,
        `failedSends=${failed} count=${snapshot.msgs.length} seqs=${JSON.stringify(seqs.slice(0, 40))}`,
      );
    }
    {
      const mem = await settleEqual(() => driver.getMembers("general"), model.expectedMembers("general"));
      rec("correctness", "unread_exact_after_concurrent_sends", mem.ok, `members=${JSON.stringify(mem.last)} expected=${JSON.stringify(model.expectedMembers("general"))}`);
    }
    {
      await driver.markRead("carol", "general", 35);
      model.markRead("carol", "general", 35);
      const after = await settleEqual(() => driver.getMembers("general"), model.expectedMembers("general"));
      await driver.markRead("carol", "general", 5); // must NOT regress
      model.markRead("carol", "general", 5);
      const mono = await settleEqual(() => driver.getMembers("general"), model.expectedMembers("general"));
      rec("correctness", "mark_read_and_monotonicity", after.ok && mono.ok, `afterMark=${JSON.stringify(after.last)} afterRegress=${JSON.stringify(mono.last)}`);
    }

    await driver.editMessage("bob", "general", "g-1", "first! (edited)");
    model.editMessage("general", "g-1", "first! (edited)");
    {
      const msgs = await settleEqual(() => driver.getMessages("general"), model.expectedMessages("general"));
      rec("correctness", "edit_by_author_applies", msgs.ok, `messages[0]=${JSON.stringify(msgs.last?.[0])}`);
    }
    rec("correctness", "edit_by_other_rejected", await rejects(() => driver.editMessage("carol", "general", "g-1", "hax")));

    await driver.deleteMessage("alice", "general", "g-2"); // author deletes own message
    model.deleteMessage("general", "g-2");
    await driver.deleteMessage("alice", "general", "g-1"); // room owner deletes bob's message
    model.deleteMessage("general", "g-1");
    {
      const msgs = await settleEqual(() => driver.getMessages("general"), model.expectedMessages("general"));
      const g1 = msgs.last?.find((m) => m.clientMsgId === "g-1");
      rec("correctness", "delete_tombstone_by_owner_and_author", msgs.ok && g1?.deleted === true && g1?.text === "", `g-1=${JSON.stringify(g1)}`);
    }
    rec("correctness", "delete_by_other_rejected", await rejects(() => driver.deleteMessage("carol", "general", "g-3")));
    rec("correctness", "delete_already_deleted_rejected", await rejects(() => driver.deleteMessage("bob", "general", "g-1")));
    {
      // Tombstone privacy: a completely fresh client must never see the original text.
      const fresh = await client();
      const msgs = await fresh.getMessages("general");
      const g1 = msgs.find((m) => m.clientMsgId === "g-1");
      rec("correctness", "deleted_text_unrecoverable", g1 !== undefined && g1.deleted && g1.text === "", `freshView g-1=${JSON.stringify(g1)}`);
    }

    // ---- tips ----
    await driver.tip("alice", "bob", 30);
    model.tip("alice", "bob", 30);
    {
      const a = await settleEqual(() => driver.getUser("alice"), model.expectedUser("alice"));
      const b = await settleEqual(() => driver.getUser("bob"), model.expectedUser("bob"));
      rec("correctness", "tip_transfers_balance", a.ok && b.ok, `alice=${JSON.stringify(a.last)} bob=${JSON.stringify(b.last)}`);
    }
    rec("correctness", "tip_overdraft_rejected", await rejects(() => driver.tip("mallory", "alice", 5000)));
    {
      const zero = await rejects(() => driver.tip("alice", "bob", 0));
      const neg = await rejects(() => driver.tip("alice", "bob", -5));
      const self = await rejects(() => driver.tip("alice", "alice", 5));
      const unknown = await rejects(() => driver.tip("alice", "nobody", 5));
      rec("correctness", "tip_invalid_rejected", zero && neg && self && unknown, `zero=${zero} neg=${neg} self=${self} unknown=${unknown}`);
    }
    {
      // Concurrent round-robin tips: all legal under any interleaving, so all
      // must succeed and final balances are deterministic (net zero).
      const before = model.totalBalance("alice", "bob", "carol");
      const jobs: Promise<void>[] = [];
      for (let round = 0; round < 3; round++) {
        jobs.push(cA.tip("alice", "bob", 5));
        jobs.push(cB.tip("bob", "carol", 5));
        jobs.push(cC.tip("carol", "alice", 5));
      }
      const results = await Promise.allSettled(jobs);
      const failed = results.filter((r) => r.status === "rejected").length;
      // net zero: model balances unchanged
      const a = await settleEqual(() => driver.getUser("alice"), model.expectedUser("alice"));
      const b = await settleEqual(() => driver.getUser("bob"), model.expectedUser("bob"));
      const c = await settleEqual(() => driver.getUser("carol"), model.expectedUser("carol"));
      const after = model.totalBalance("alice", "bob", "carol");
      rec(
        "correctness",
        "tip_concurrent_conservation",
        failed === 0 && a.ok && b.ok && c.ok && before === after,
        `failedTips=${failed} alice=${JSON.stringify(a.last)} bob=${JSON.stringify(b.last)} carol=${JSON.stringify(c.last)}`,
      );
    }

    // ---- kick & permissions ----
    {
      const nonOwnerKick = await rejects(() => driver.kick("bob", "general", "carol"));
      const kickOwner = await rejects(() => driver.kick("bob", "general", "alice"));
      const ownerLeave = await rejects(() => driver.leaveRoom("alice", "general"));
      rec("correctness", "kick_and_leave_permissions", nonOwnerKick && kickOwner && ownerLeave, `nonOwnerKick=${nonOwnerKick} kickOwner=${kickOwner} ownerLeave=${ownerLeave}`);
    }
    {
      await driver.kick("alice", "general", "carol");
      model.removeMember("carol", "general");
      const mem = await settleEqual(() => driver.getMembers("general"), model.expectedMembers("general"));
      const blocked = await rejects(() => driver.sendMessage("carol", "general", "still here?", "carol-after-kick"));
      rec("correctness", "kick_removes_membership_and_blocks_send", mem.ok && blocked, `members=${JSON.stringify(mem.last)} sendBlocked=${blocked}`);
    }

    // =========================================================================
    // REALTIME — dedicated room "rt" with three independent subscriber clients
    // =========================================================================
    await driver.createRoom("alice", "rt");
    model.createRoom("alice", "rt");
    for (const u of ["bob", "carol"]) {
      await driver.joinRoom(u, "rt");
      model.joinRoom(u, "rt");
    }

    const obs1 = await client();
    const obs2 = await client();
    const obs3 = await client();
    const r1 = new RoomRecorder();
    const r2 = new RoomRecorder();
    const r3 = new RoomRecorder();
    await obs1.subscribeRoom("rt", r1.handlers());
    await obs2.subscribeRoom("rt", r2.handlers());
    await obs3.subscribeRoom("rt", r3.handlers());

    // A general-room subscriber must not hear rt traffic (cross-room isolation).
    const isoRec = new RoomRecorder();
    const isoClient = await client();
    await isoClient.subscribeRoom("general", isoRec.handlers());
    const generalCount = isoRec.events.length; // history size at subscribe

    {
      const jobs: Promise<void>[] = [];
      const senders: Array<[AppClient, string]> = [[cA, "alice"], [cB, "bob"], [cC, "carol"]];
      for (const [conn, user] of senders) {
        for (let i = 0; i < 5; i++) jobs.push(conn.sendMessage(user, "rt", `rt ${user} ${i}`, `rt-${user}-${i}`));
      }
      await Promise.allSettled(jobs);
      const allArrived = await waitFor(
        () => [r1, r2, r3].every((r) => r.seqsSeen().length >= 15),
        T,
      );
      rec("realtime", "fanout_all_subscribers_receive", allArrived, `counts=${[r1, r2, r3].map((r) => r.seqsSeen().length).join(",")}`);

      // Sync the model with the backend's chosen order.
      const msgs = await driver.getMessages("rt");
      for (const m of msgs) model.sendMessage(m.sender, "rt", m.text, m.clientMsgId);

      const exactlyOnceInOrder = [r1, r2, r3].every((r) => {
        const seqs = r.seqsSeen();
        const uniq = new Set(seqs).size === seqs.length;
        const ascending = seqs.every((s, i) => i === 0 || seqs[i - 1] < s);
        return uniq && ascending && seqs.length === 15;
      });
      rec("realtime", "fanout_exactly_once_in_seq_order", allArrived && exactlyOnceInOrder, `obs1Seqs=${JSON.stringify(r1.seqsSeen())}`);
    }
    {
      await sleep(1000); // give any misrouted events time to arrive
      const leaked = isoRec.events.slice(generalCount).filter((e) => e.clientMsgId.startsWith("rt-"));
      rec("realtime", "cross_room_isolation", leaked.length === 0, `leaked=${JSON.stringify(leaked.map((e) => e.clientMsgId))}`);
    }
    {
      // Late joiner: full history in seq order, then a live message, no dupes/gaps.
      const late = await client();
      const lateRec = new RoomRecorder();
      await late.subscribeRoom("rt", lateRec.handlers());
      const historyArrived = await waitFor(() => lateRec.seqsSeen().length >= 15, T);
      await driver.sendMessage("alice", "rt", "post-join live", "rt-live-1");
      model.sendMessage("alice", "rt", "post-join live", "rt-live-1");
      const liveArrived = await waitFor(() => lateRec.latest.has("rt-live-1"), T);
      const seqs = lateRec.seqsSeen();
      const expected = Array.from({ length: 16 }, (_, i) => i + 1);
      const exact = JSON.stringify(seqs) === JSON.stringify(expected);
      rec("realtime", "late_joiner_history_then_live", historyArrived && liveArrived && exact, `seqs=${JSON.stringify(seqs)}`);
    }
    {
      await driver.editMessage("alice", "rt", "rt-live-1", "post-join live (edited)");
      model.editMessage("rt", "rt-live-1", "post-join live (edited)");
      const saw = await waitFor(() => {
        const m = r1.latest.get("rt-live-1");
        return m?.edited === true && m.text === "post-join live (edited)";
      }, T);
      rec("realtime", "edit_propagates_live", saw, `obs1 sees ${JSON.stringify(r1.latest.get("rt-live-1"))}`);
    }
    {
      await driver.deleteMessage("alice", "rt", "rt-live-1");
      model.deleteMessage("rt", "rt-live-1");
      const saw = await waitFor(() => {
        const m = r1.latest.get("rt-live-1");
        return m?.deleted === true && m.text === "";
      }, T);
      rec("realtime", "delete_propagates_live", saw, `obs1 sees ${JSON.stringify(r1.latest.get("rt-live-1"))}`);
    }
    {
      const users = new Map<string, { status: string; balance: number }>();
      await obs1.subscribeUsers({ onUser: (u) => users.set(u.username, { status: u.status, balance: u.balance }) });
      await waitFor(() => users.has("bob"), T);
      await driver.setStatus("bob", "offline");
      model.setStatus("bob", "offline");
      const saw = await waitFor(() => users.get("bob")?.status === "offline", T);
      rec("realtime", "status_change_propagates", saw, `bob=${JSON.stringify(users.get("bob"))}`);

      const bobBefore = model.users.get("bob")!.balance;
      await driver.tip("alice", "bob", 7);
      model.tip("alice", "bob", 7);
      const sawBal = await waitFor(() => users.get("bob")?.balance === bobBefore + 7, T);
      rec("realtime", "balance_change_propagates", sawBal, `bob=${JSON.stringify(users.get("bob"))}`);
    }
    {
      const members = new Map<string, { lastReadSeq: number; unread: number }>();
      const removed: string[] = [];
      await obs2.subscribeMembers("rt", {
        onMember: (m) => members.set(m.user, { lastReadSeq: m.lastReadSeq, unread: m.unread }),
        onMemberRemoved: (u) => removed.push(u),
      });
      await waitFor(() => members.has("carol"), T);
      await driver.kick("alice", "rt", "carol");
      model.removeMember("carol", "rt");
      const saw = await waitFor(() => removed.includes("carol") || !members.has("carol"), T);
      rec("realtime", "membership_change_propagates", saw, `removed=${JSON.stringify(removed)}`);
    }

    // =========================================================================
    // PERF — dedicated room, generous thresholds; raw numbers reported as metrics
    // =========================================================================
    await driver.createRoom("alice", "perf");
    model.createRoom("alice", "perf");
    for (const u of ["bob", "carol"]) {
      await driver.joinRoom(u, "perf");
      model.joinRoom(u, "perf");
    }
    const perfObs = await client();
    const perfRec = new RoomRecorder();
    const arrivalTimes = new Map<string, number>();
    await perfObs.subscribeRoom("perf", {
      onMessage: (m) => {
        if (!arrivalTimes.has(m.clientMsgId)) arrivalTimes.set(m.clientMsgId, Date.now());
        perfRec.events.push(m);
      },
    });
    {
      const latencies: number[] = [];
      for (let i = 0; i < 30; i++) {
        const id = `lat-${i}`;
        const t0 = Date.now();
        await cB.sendMessage("bob", "perf", `latency probe ${i}`, id);
        model.sendMessage("bob", "perf", `latency probe ${i}`, id);
        const got = await waitFor(() => arrivalTimes.has(id), T);
        if (got) latencies.push(arrivalTimes.get(id)! - t0);
        await sleep(100);
      }
      const p50 = latencies.length ? percentile(latencies, 50) : NaN;
      const p95 = latencies.length ? percentile(latencies, 95) : NaN;
      metrics.latency_p50_ms = p50;
      metrics.latency_p95_ms = p95;
      const ok = latencies.length === 30 && p95 < 1500;
      rec("perf", "delivery_latency_p95", ok, `delivered=${latencies.length}/30 p50=${p50}ms p95=${p95}ms`);
    }
    {
      const t0 = Date.now();
      const jobs: Promise<void>[] = [];
      const senders: Array<[AppClient, string]> = [[cA, "alice"], [cB, "bob"], [cC, "carol"]];
      for (const [conn, user] of senders) {
        for (let i = 0; i < 40; i++) jobs.push(conn.sendMessage(user, "perf", `burst ${user} ${i}`, `burst-${user}-${i}`));
      }
      const results = await Promise.allSettled(jobs);
      const failed = results.filter((r) => r.status === "rejected").length;
      const allArrived = await waitFor(
        () => new Set(perfRec.events.filter((e) => e.clientMsgId.startsWith("burst-")).map((e) => e.clientMsgId)).size >= 120,
        45_000,
      );
      const durS = (Date.now() - t0) / 1000;
      metrics.burst_throughput_msgs_per_sec = allArrived ? Math.round((120 / durS) * 10) / 10 : 0;
      // Sync model with backend order.
      const msgs = await driver.getMessages("perf");
      for (const m of msgs) model.sendMessage(m.sender, "perf", m.text, m.clientMsgId);
      rec("perf", "burst_delivery_within_budget", failed === 0 && allArrived, `failedSends=${failed} delivered in ${durS.toFixed(1)}s (${metrics.burst_throughput_msgs_per_sec} msg/s)`);
    }

    // Final pre-restart settle + full goal-state comparison.
    {
      const parts = await Promise.all([
        settleEqual(() => driver.getMessages("general"), model.expectedMessages("general")),
        settleEqual(() => driver.getMessages("rt"), model.expectedMessages("rt")),
        settleEqual(() => driver.getMessages("perf"), model.expectedMessages("perf")),
        settleEqual(() => driver.getMembers("general"), model.expectedMembers("general")),
        settleEqual(() => driver.getMembers("rt"), model.expectedMembers("rt")),
        settleEqual(() => driver.getMembers("perf"), model.expectedMembers("perf")),
        settleEqual(() => driver.getUser("alice"), model.expectedUser("alice")),
        settleEqual(() => driver.getUser("bob"), model.expectedUser("bob")),
        settleEqual(() => driver.getUser("carol"), model.expectedUser("carol")),
      ]);
      const ok = parts.every((p) => p.ok);
      rec("correctness", "final_state_matches_model", ok, `firstMismatch=${JSON.stringify(parts.find((p) => !p.ok)?.last)?.slice(0, 400)}`);
    }

    // =========================================================================
    // DURABILITY — restart the backend, reconnect fresh, verify everything
    // =========================================================================
    if (!env.restartBackend) {
      for (const name of [
        "restart_users_and_balances_persist",
        "restart_rooms_persist",
        "restart_memberships_persist",
        "restart_messages_persist",
        "restart_seq_continues_gapless",
        "restart_realtime_works",
        "restart_idempotency_persists",
      ]) {
        rec("durability", name, false, "no RESTART_CMD configured — durability unverifiable");
      }
    } else {
      // Drop all live connections first; they die with the backend anyway.
      for (const c of open.splice(0)) await c.close().catch(() => {});
      await env.restartBackend();

      // Fresh client; connect may need retries while the backend comes back.
      let post: AppClient | undefined;
      const reconnectDeadline = Date.now() + 90_000;
      let lastErr: unknown;
      while (Date.now() < reconnectDeadline && !post) {
        try {
          const c = makeClient();
          await c.connect();
          await c.getUser("alice"); // probe a real read
          post = c;
          open.push(c);
        } catch (e) {
          lastErr = e;
          await sleep(1000);
        }
      }
      if (!post) throw new Error(`could not reconnect after restart: ${lastErr}`);

      {
        const users = await Promise.all(
          ["alice", "bob", "carol", "dave", "mallory"].map((u) => settleEqual(() => post!.getUser(u), model.expectedUser(u))),
        );
        rec("durability", "restart_users_and_balances_persist", users.every((r) => r.ok), `mismatch=${JSON.stringify(users.find((r) => !r.ok)?.last)}`);
      }
      {
        const owners = await Promise.all(
          (["general", "rt", "perf"] as const).map(async (r) => (await post!.getRoomOwner(r)) === "alice"),
        );
        rec("durability", "restart_rooms_persist", owners.every(Boolean), `owners ok=${JSON.stringify(owners)}`);
      }
      {
        const mems = await Promise.all(
          (["general", "rt", "perf"] as const).map((r) => settleEqual(() => post!.getMembers(r), model.expectedMembers(r))),
        );
        rec("durability", "restart_memberships_persist", mems.every((m) => m.ok), `mismatch=${JSON.stringify(mems.find((m) => !m.ok)?.last)?.slice(0, 400)}`);
      }
      {
        const msgs = await Promise.all(
          (["general", "rt", "perf"] as const).map((r) => settleEqual(() => post!.getMessages(r), model.expectedMessages(r))),
        );
        rec("durability", "restart_messages_persist", msgs.every((m) => m.ok), `mismatch=${JSON.stringify(msgs.find((m) => !m.ok)?.last)?.slice(0, 400)}`);
      }
      {
        // Durable per-room counter: next seq continues exactly from pre-restart max.
        const expectSeq = model.nextSeq("general");
        await post.sendMessage("alice", "general", "after restart", "g-post-restart");
        model.sendMessage("alice", "general", "after restart", "g-post-restart");
        const settled = await settleEqual(() => post!.getMessages("general"), model.expectedMessages("general"));
        const got = settled.last?.find((m) => m.clientMsgId === "g-post-restart");
        rec("durability", "restart_seq_continues_gapless", settled.ok && got?.seq === expectSeq, `expected seq=${expectSeq} got=${JSON.stringify(got)}`);
      }
      {
        const sub = await client();
        const subRec = new RoomRecorder();
        await sub.subscribeRoom("general", subRec.handlers());
        await post.sendMessage("bob", "general", "live after restart", "g-post-live");
        model.sendMessage("bob", "general", "live after restart", "g-post-live");
        const saw = await waitFor(() => subRec.latest.has("g-post-live"), T);
        rec("durability", "restart_realtime_works", saw, "subscriber did not receive post-restart live message");
      }
      {
        // Idempotency record must be durable: pre-restart clientMsgId resend → no dupe.
        await post.sendMessage("bob", "general", "dupe probe", "g-1");
        const settled = await settleEqual(() => post!.getMessages("general"), model.expectedMessages("general"));
        rec("durability", "restart_idempotency_persists", settled.ok, `messages=${JSON.stringify(settled.last)?.slice(0, 400)}`);
      }
    }
  } catch (err) {
    checks.push({ group: "correctness", name: "scenario_error", pass: false, detail: String(err) });
    console.log(`  [FAIL] correctness/scenario_error — ${String(err)}`);
  } finally {
    for (const c of open) await c.close().catch(() => {});
  }

  return { checks, metrics };
}

/** Weighted reward + per-group subscores from check results. */
export function scoreChecks(checks: CheckResult[]): {
  reward: number;
  groups: Record<CheckGroup, { passed: number; total: number; score: number }>;
} {
  const groups = {} as Record<CheckGroup, { passed: number; total: number; score: number }>;
  for (const g of Object.keys(GROUP_WEIGHTS) as CheckGroup[]) {
    const of = checks.filter((c) => c.group === g);
    const passed = of.filter((c) => c.pass).length;
    groups[g] = { passed, total: of.length, score: of.length ? passed / of.length : 0 };
  }
  let reward = 0;
  for (const g of Object.keys(GROUP_WEIGHTS) as CheckGroup[]) {
    reward += GROUP_WEIGHTS[g] * groups[g].score;
  }
  return { reward, groups };
}
