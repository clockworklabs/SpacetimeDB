import type { CompiledStep } from '../src/composition/definition-compiler.js';

// Read the authored form so loops stay visible instead of expanding to thousands of rows.
export interface GuideStep extends CompiledStep {
  repeat?: number;
  forEach?: unknown[];
  steps?: GuideStep[];
  branches?: GuideStep[][];
  alongside?: GuideStep[];
  storage?: { cart?: boolean; warehouses?: boolean };
  swap?: { find: unknown; with: unknown };
  senders?: { actor: string; count: number; prefix: string }[];
}
const q = (value: unknown) => JSON.stringify(value);
const words = (value: unknown) => String(value ?? '').replaceAll('-', ' ');
const actor = (s: GuideStep) => s.actor ? `In the ${s.actor} browser, ` : '';
const target = (s: GuideStep) => `${words(s.testid)}${s.contains ? ` matching ${q(s.contains)}` : ''}${s.in ? ` inside ${words(s.in.testid)}${s.in.contains ? ` for ${q(s.in.contains)}` : ''}` : ''}`;
const seconds = (ms: unknown) => `${Number((Number(ms) / 1000).toFixed(3))} seconds`;
const wanted = (s: GuideStep) => s.relativeTo ? `the saved ${q(s.relativeTo)} value ${Number(s.plus ?? 0) < 0 ? 'minus' : 'plus'} ${Math.abs(s.plus ?? 0)}`
  : s.equals !== undefined ? q(s.equals) : s.value !== undefined ? q(s.value) : s.containsText ? `text containing ${q(s.containsText)}`
  : s.atLeast !== undefined ? `at least ${s.atLeast}` : s.atMost !== undefined ? `at most ${s.atMost}` : 'the required value';
const meanings: Record<string, (s: GuideStep) => string> = {
  signUp: s => `${actor(s)}register ${q(s.name)}${s.password ? ' with the specified test password' : ''}.`,
  signIn: s => `${actor(s)}sign in as ${q(s.name)}${s.exact ? ' using the exact seeded account name' : ''}.`,
  ensureSignedIn: s => `${actor(s)}keep the current session if it is ready; otherwise sign in as ${q(s.name)}.`,
  click: s => `${actor(s)}click ${target(s)}${s.ifAvailable ? ' if available' : ''}${s.unlessVisible ? `, unless ${words(s.unlessVisible)} is already visible` : ''}.`,
  openItem: s => `${actor(s)}open the product ${q(s.item)}${s.unlessVisible ? ` unless ${words(s.unlessVisible)} is already visible` : ''}.`,
  fill: s => `${actor(s)}set ${target(s)} to ${q(s.text)}${s.enter ? ' and press Enter' : ''}.`,
  pressKey: s => `${actor(s)}press ${q(s.key)}.`,
  reload: s => `${actor(s)}reload the page.`,
  freshClient: s => `Open a separate browser for ${s.actor}${s.preserveStorage ? ', copying its cookies, local storage, IndexedDB and session storage to test retained access' : ' with clean storage'}; later steps refer to ${s.actor}-fresh.`,
  openClient: s => `Reopen the ${s.actor} browser.`, closeClient: s => `Close the ${s.actor} browser.`,
  setOffline: s => `${s.offline === false ? 'Restore' : 'Disconnect'} the ${s.actor} browser's network connection.`,
  expect: s => `${actor(s)}${s.absent ? 'watch for and reject any visible' : 'check for visible'} ${target(s)}${s.count !== undefined ? `; require ${s.count} matching elements` : ''}${s.value !== undefined ? `; require value ${q(s.value)}` : ''}${s.nonEmpty ? '; require nonempty content' : ''}${s.notContains ? `; reject text containing ${q(s.notContains)}` : ''}${s.ignoreCase ? ' (ignore letter case)' : ''}.`,
  expectNumber: s => `${actor(s)}check that ${target(s)} is ${s.comparison === 'atMost' ? 'no greater than ' : s.comparison === 'atLeast' ? 'at least ' : ''}${wanted(s)}.`,
  expectElementCount: s => `${actor(s)}count ${target(s)}; require ${wanted(s)}.`,
  expectSequence: s => `${actor(s)}read every ${target(s)} in displayed order; require exactly ${q(s.equals)}.`,
  expectAgreement: s => `Compare ${target(s)} in browsers ${(s.actors ?? []).join(', ')}; require equal ${s.numeric ? 'numbers' : 'values'}.`,
  expectActorsWith: s => `Require exactly ${s.equals} of browsers ${(s.actors ?? []).join(', ')} to show ${target(s)}${s.maxEach !== undefined ? `, with at most ${s.maxEach} per browser` : ''}.`,
  expectUnavailable: s => `${actor(s)}check that ${target(s)} is absent, hidden or disabled.`,
  waitUntilAbsent: s => `${actor(s)}wait until ${target(s)} is absent.`,
  recordNumber: s => `${actor(s)}save the number from ${target(s)} as ${q(s.as)}.`,
  recordTime: s => `Record the current time as ${q(s.as)}.`,
  expectElapsed: s => `Require this observation to start within ${seconds(s.atMost)} of ${q(s.since)}; otherwise the measurement is inconclusive.`,
  wait: s => s.since ? `Wait until ${seconds(s.ms)} after ${q(s.since)}.` : `Wait ${seconds(s.ms)}.`,
  dbSetStock: s => `Set stored stock for ${q(s.item)} in ${q(s.warehouse)} to ${s.quantity}, independently of the app's handlers.`,
  dbRecordStock: s => `Read authoritative stock for ${q(s.item)}${s.warehouse ? ` in ${q(s.warehouse)}` : ' across warehouses'}; save it as ${q(s.as)}.`,
  dbExpectStock: s => `Independently read stored stock for ${q(s.item)}${s.warehouse ? ` in ${q(s.warehouse)}` : ''}; require ${wanted(s)}.`,
  dbRecordCheckout: s => `Read stored account, product and order state for ${q(s.account)} and ${q(s.item)}; save it as ${q(s.as)}${s.storage ? ` (cart: ${!!s.storage.cart}; warehouse detail: ${!!s.storage.warehouses})` : ''}.`,
  dbExpectCatalogItem: s => `Read the database after product creation. Relative to ${q(s.before)}, require exactly one new product named ${q(s.name)} with price ${s.priceMinor} minor units. This is also the committed-write barrier before the next creation.`,
  dbExpectCheckout: s => `Compare stored state with ${q(s.before)} and the prepared cart ${q(s.prepared)}. Require complete checkout effects for quantity ${q(s.quantity)}${s.alongsideAdd ? ` while preserving a concurrent add of ${q(s.alongsideAdd)}` : ''}${s.actor ? `, reconciled with ${s.actor}'s request outcome` : ''}.`,
  dbExpectCancellation: s => `Compare stored state with ${q(s.before)}. Require cancellation to restore the original stock and booked amount exactly once.`,
  dbExpectNoPurchase: s => `Compare stored state with ${q(s.before)}. Require no purchase effects.`,
  dbExpectPurchase: s => `Compare stored orders with ${q(s.before)}${s.stockBefore ? ` and stock with ${q(s.stockBefore)}` : ''}; require the purchase and its accounting effects.`,
  dbExpectPurchases: s => `Reconcile stored orders and warehouse effects against the saved buyer snapshots ${q(s.before)}; require ${s.purchases} complete purchases.`,
  dbExpectPurchaseCount: s => `Read all stored orders against snapshots ${q(s.before)}. Require ${s.purchasesEach} additional purchases for each account, their stored prices, and warehouse effects where those snapshots include warehouses.`,
  dbExpectOperation: s => `Check the complete stored-state transition for ${s.actor}'s ${s.operation}, against ${q(s.before)}${s.otherBefore ? ` and ${q(s.otherBefore)}` : ''}.`,
  callAction: s => `${actor(s)}send the real application ${q(s.action)} request${s.authentication === 'none' ? ' without credentials' : s.authentication === 'tampered-session' ? ' with a tampered session credential' : s.authentication === 'optional' ? ' with whatever credentials this browser actually has; allow it to be signed out' : " with this actor's credentials"}${s.input ? `, taking ${s.input.attribute} from ${words(s.input.testid)}${s.input.contains ? ` matching ${q(s.input.contains)}` : ''} in ${s.from ?? s.actor}'s browser` : ''}.`,
  expectActionOutcome: s => `Require ${s.actor}'s application request to be ${s.outcome}.`,
  callConcurrently: s => `Send ${s.requests ?? (s.actors ?? []).length} ${q(s.action)} requests across browsers ${(s.actors ?? []).join(', ')} concurrently; retain every response or unknown outcome${s.delayMs ? ` (delay this request group by ${seconds(s.delayMs)})` : ''}.`,
  clickConcurrently: s => `In browsers ${(s.actors ?? []).join(', ')}, click ${target(s)} concurrently.`,
  expectCallOutcomes: s => `Check all recorded concurrent request outcomes${s.accepted !== undefined ? `; require exactly ${s.accepted} accepted calls` : ''}; unknown outcomes remain inconclusive rather than assumed refusals.`,
  replayAs: s => `Replay the observed ${s.namedAction?.id ?? s.action ?? s.match ?? 'write'} from ${s.from ?? 'the source actor'} with ${s.actor}'s authority${s.namedTarget ? ', selecting the declared target entity' : ''}.`,
  replayConcurrently: s => `Replay recorded writes concurrently from ${(s.actors ?? []).join(', ')}.`,
  expectReplayRejected: s => `Require the replay by ${s.actor} to be rejected. Later state assertions, where present, check its effects.`,
  expectReplayCompleted: s => `Require ${s.actor}'s replay to complete${s.requireAccepted ? ' and be accepted' : ''}.`,
  forgeWrite: s => `Send a write as ${s.actor} while claiming ${s.fromActor}'s identity.`,
  expectForgeryRejected: s => `Require the forged write by ${s.actor} to be rejected.`,
  prepareResponseLoss: s => `Prepare to intercept ${s.actor}'s checkout reply on its actual transport.`,
  loseCheckoutResponse: s => `Submit ${s.actor}'s checkout, lose its reply, and independently inspect stored effects against ${q(s.before)} and ${q(s.prepared)}.`,
  confirmCheckout: s => `Confirm a working authenticated checkout path for ${s.actor}.`,
  crashCheckout: s => `Start ${s.requests} checkout requests from ${s.actor}; crash the ${s.target} process at offset ${s.offsetMs} ms. Recover it, inspect stored state against ${q(s.before)} and ${q(s.prepared)}, and save ${q(s.as)}.`,
  expectCrashCheckout: s => `Read crash evidence ${q(s.from)} and require ${s.verdict === 'atomicity' ? 'either the prepared cart or one complete order, never partial effects' : 'acknowledged work and earlier orders to survive'}.`,
  restartBackend: () => 'Restart the owned database backend with retained storage and wait for readiness.',
  stopAppServer: () => 'Stop the application server.', startAppServer: () => 'Start the application server and wait for readiness.',
  expectReceived: s => `Require ${s.actor}'s observed incoming data to contain ${q(s.contains)}.`,
  expectNotReceived: s => `Watch ${s.actor}'s observed incoming data; reject ${q(s.contains)}. This is a bounded observation, not a proof against every possible leak.`,
  armScriptCanary: s => `Install a harmless execution marker in ${s.actor}'s browser.`,
  expectNoScriptExecution: s => `Require ${s.actor}'s execution marker to remain unchanged after the supplied content is displayed.`,
  createRoom: s => `${actor(s)}create chat room ${q(s.room)}.`, enterRoom: s => `${actor(s)}enter chat room ${q(s.room)}.`,
  send: s => `${actor(s)}send message ${q(s.text)}.`, typeInto: s => `${actor(s)}type ${q(s.text)} without sending.`,
  clearInput: s => `${actor(s)}clear the message input.`,
  sendMany: s => `${actor(s)}send ${s.count} numbered messages beginning ${q(s.prefix)}, ${seconds(s.delayMs ?? 0)} apart.`,
  sendConcurrently: s => `Send messages concurrently: ${(s.senders ?? []).map(x=>`${x.actor}: ${x.count} beginning ${q(x.prefix)}`).join('; ')}; spacing ${seconds(s.delayMs ?? 0)}.`,
  expectAllPresent: s => `${actor(s)}require all ${s.count} messages beginning ${q(s.prefix)}.`,
  expectOrderMatches: s => `Require browsers ${(s.actors ?? []).join(', ')} to show the same nonempty message order matching ${q(s.prefix)}.`,
  expectStable: s => `${actor(s)}sample ${target(s)} ${s.samples ?? 4} times, waiting ${seconds(s.intervalMs ?? 700)} after each sample; require a stable value.`,
  runScript: s => `Run the application's ${q(s.script)} with arguments ${q(s.args ?? [])}.`,
};
export function stepsText(steps: GuideStep[], depth = 0): string[] {
  const lines = [];
  for (const s of steps) {
    const pad = '  '.repeat(depth);
    if (s.repeat !== undefined || s.forEach) {
      const list = s.forEach;
      lines.push(`${pad}- Repeat ${list ? list.length : s.repeat} times${list ? `, substituting each listed value (first ${q(list[0])}, last ${q(list.at(-1))}; exact list in source)` : ''}:`);
      lines.push(...stepsText(s.steps ?? [], depth + 1)); continue;
    }
    if (s.do === 'race') {
      lines.push(`${pad}- Run these branches concurrently and wait for all of them:`);
      (s.branches ?? []).forEach((branch, i) => { lines.push(`${pad}  - Branch ${i + 1}:`); lines.push(...stepsText(branch, depth + 2)); });
    } else {
      const describe = meanings[s.do];
      lines.push(`${pad}- ${describe ? describe(s) : `Run ${q(s.do)}. See the exact input below for its fields.`}`);
    }
    if (s.input?.overrides) lines.push(`${pad}  Replace submitted fields with ${q(s.input.overrides)}; this is deliberately untrusted client input.`);
    if (s.browserOrigin) lines.push(`${pad}  Send from a ${s.browserOrigin === 'cross-site' ? 'different browser origin' : 'same-site browser origin'}.`);
    if (s.swap) lines.push(`${pad}  In the replayed request, replace ${q(s.swap.find)} with ${q(s.swap.with)}.`);
    if (s.containsText) lines.push(`${pad}  Require displayed text containing ${q(s.containsText)}.`);
    if (s.attribute) lines.push(`${pad}  Inspect the ${q(s.attribute)} attribute.`);
    if (s.provenBy) lines.push(`${pad}  This check uses proof recorded by ${q(s.provenBy)}.`);
    if (s.reuseCombinedFrom) lines.push(`${pad}  If application and database are the same process, reuse ${q(s.reuseCombinedFrom)} rather than counting another crash.`);
    for (const group of s.alongside ?? []) lines.push(`${pad}  At the same time: ${meanings.callConcurrently!(group)}`);
    if (s.within !== undefined) lines.push(`${pad}  Observation deadline: ${seconds(s.within)}${s.absent || s.do === 'expectNotReceived' ? '; this includes an absence observation window' : '; normally completes sooner if the condition is met'}.`);
    if (s.settleMs) lines.push(`${pad}  Always wait another ${seconds(s.settleMs)} after this action.`);

  }
  return lines;
}
