import { ChatClient, ChatClientFactory, ChatMessage } from "./chatClient.js";

export interface CheckResult {
  name: string;
  pass: boolean;
  detail?: string;
}

export interface ScenarioResult {
  passed: number;
  total: number;
  checks: CheckResult[];
}

const DELIVERY_TIMEOUT_MS = Number(process.env.DELIVERY_TIMEOUT_MS ?? 10_000);

/** Poll `pred` until true or timeout. Returns whether it became true in time. */
function waitFor(pred: () => boolean, timeoutMs: number, intervalMs = 100): Promise<boolean> {
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

/**
 * The backend-neutral behavioral test for a minimal real-time chat app.
 *
 * Scoring is non-binary (Harbor supports fractional rewards): the reward is the
 * fraction of checks that pass, so a backend that gets real-time delivery right
 * but botches history still scores partial credit.
 */
export async function runChatScenario(makeClient: ChatClientFactory): Promise<ScenarioResult> {
  const checks: CheckResult[] = [];
  const record = (name: string, pass: boolean, detail?: string) => checks.push({ name, pass, detail });

  const a = makeClient();
  const b = makeClient();
  const bReceived: ChatMessage[] = [];

  try {
    await a.connect();
    await b.connect();
    await b.subscribe((m) => bReceived.push(m));

    // 1) Real-time delivery: B is subscribed; a message A sends must arrive (pushed).
    await a.send("alice", "hello");
    const got1 = await waitFor(() => bReceived.some((m) => m.text === "hello"), DELIVERY_TIMEOUT_MS);
    record("realtime_delivery", got1, got1 ? undefined : "B never received 'hello' within timeout");

    // 2) Ordering: a second message arrives, after the first.
    await a.send("alice", "world");
    const got2 = await waitFor(
      () => bReceived.some((m) => m.text === "world"),
      DELIVERY_TIMEOUT_MS,
    );
    const texts = bReceived.map((m) => m.text);
    const ordered =
      texts.indexOf("hello") !== -1 &&
      texts.indexOf("world") !== -1 &&
      texts.indexOf("hello") < texts.indexOf("world");
    record("ordering", got2 && ordered, ordered ? undefined : `unexpected order: ${JSON.stringify(texts)}`);

    // 3) Sender attribution survives the round trip.
    const helloMsg = bReceived.find((m) => m.text === "hello");
    record("sender_attribution", helloMsg?.sender === "alice", `sender=${helloMsg?.sender ?? "<none>"}`);

    // 4) History persistence: a freshly-connected client must receive prior messages on subscribe.
    const c = makeClient();
    const cReceived: ChatMessage[] = [];
    await c.connect();
    await c.subscribe((m) => cReceived.push(m));
    const gotHistory = await waitFor(
      () => cReceived.some((m) => m.text === "hello") && cReceived.some((m) => m.text === "world"),
      DELIVERY_TIMEOUT_MS,
    );
    record(
      "history_persistence",
      gotHistory,
      gotHistory ? undefined : `fresh client saw: ${JSON.stringify(cReceived.map((m) => m.text))}`,
    );
    await c.close();
  } catch (err) {
    record("scenario_error", false, String(err));
  } finally {
    await a.close().catch(() => {});
    await b.close().catch(() => {});
  }

  const passed = checks.filter((c) => c.pass).length;
  return { passed, total: checks.length, checks };
}
