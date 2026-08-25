import { DbConnection, type ErrorContext, type EventContext } from './codegen';
import {
  MAX_MACHINE_LEVEL,
  RUSH_CYCLE_TICKS,
  RUSH_LENGTH_TICKS,
  SUPPLY_PRICE,
  maximumQueueLength,
  storageCapacity,
  supplyCost,
  upgradeCost,
} from '../spacetimedb/src/economy';

interface ServerConfig {
  stdbUri: string;
  database: string;
  posthogAppUrl?: string | null;
}

type TableEvents<T> = {
  iter(): Iterable<T>;
  onInsert(cb: (ctx: EventContext, row: T) => void): void;
  onUpdate(cb: (ctx: EventContext, old: T, row: T) => void): void;
  onDelete(cb: (ctx: EventContext, row: T) => void): void;
};

type ProductRow = {
  productId: string;
  name: string;
  category: string;
  description: string;
  baseAppeal: number;
  active: boolean;
};

type VariantRow = {
  variantId: string;
  productId: string;
  name: string;
  flavor: string;
  contextTokens: number;
  reasoning: number;
  latency: number;
  priceCents: number;
  baselinePriceCents: number;
  discountBps: number;
  active: boolean;
  featured: boolean;
};

type ScenarioRow = {
  scenarioId: string;
  name: string;
  description: string;
  trafficPerTick: number;
};

type ConfigRow = {
  scenarioId: string;
  tick: bigint;
  experimentKey: string;
  experimentVariant?: string;
};

type MetricsRow = {
  tick: bigint;
  views: bigint;
  carts: bigint;
  checkouts: bigint;
  purchases: bigint;
  abandons: bigint;
  revenueCents: bigint;
};

type EconRow = {
  cashCents: bigint;
  computeUnits: number;
  contextUnits: number;
  memoryUnits: number;
  suppliesSpentCents: bigint;
  stockouts: number;
  reputation: number;
  workers: number;
  machineLevel: number;
  seats: number;
  storageLevel: number;
  reneged: number;
};

const BUY_UNITS = 50;

type SessionRow = {
  sessionId: bigint;
  botId: string;
  tick: bigint;
  profile: string;
  variantId?: string;
  stage: string;
  revenueCents: number;
  reason: string;
};

type AnalyticsSummaryRow = {
  queued: bigint;
  delivered: bigint;
  failed: bigint;
};

let conn: DbConnection | null = null;
let simTimer: ReturnType<typeof setInterval> | null = null;
let running = false;
let selectedVariantId = '';
let drawerOpen = false;
let speedMs = 900;

let toastTimer: ReturnType<typeof setTimeout> | null = null;

// Track served bots that appeared on the counter and animate each outcome once.
const seenSessionIds = new Set<string>();
let counterReady = false;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function input(id: string): HTMLInputElement {
  return $(id) as HTMLInputElement;
}

function select(id: string): HTMLSelectElement {
  return $(id) as HTMLSelectElement;
}

function setText(id: string, value: string): void {
  $(id).textContent = value;
}

// Ephemeral status line: slides in on a message, auto-dismisses success
// messages and holds errors until the next action clears them.
function showToast(message: string, kind: 'ok' | 'error' = 'ok'): void {
  const el = $('toast');
  el.textContent = message;
  el.className = `toast ${kind} show`;
  if (toastTimer) {
    clearTimeout(toastTimer);
    toastTimer = null;
  }
  if (kind === 'ok') {
    toastTimer = setTimeout(() => el.classList.remove('show'), 3000);
  }
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function money(cents: number | bigint): string {
  const n = typeof cents === 'bigint' ? Number(cents) : cents;
  return `$${(n / 100).toFixed(2)}`;
}

function pct(num: bigint, den: bigint): string {
  if (den === 0n) return '0.0%';
  return `${(Number((num * 1000n) / den) / 10).toFixed(1)}%`;
}

function requireConn(): DbConnection {
  if (!conn) throw new Error('stdb.disconnected');
  return conn;
}

function productsTable(): TableEvents<ProductRow> {
  return requireConn().db.cafeProducts;
}
function variantsTable(): TableEvents<VariantRow> {
  return requireConn().db.cafeVariants;
}
function scenariosTable(): TableEvents<ScenarioRow> {
  return requireConn().db.cafeScenarios;
}
function configTable(): TableEvents<ConfigRow> {
  return requireConn().db.cafeConfig;
}
function metricsTable(): TableEvents<MetricsRow> {
  return requireConn().db.cafeMetrics;
}
function sessionsTable(): TableEvents<SessionRow> {
  return requireConn().db.cafeRecentSessions;
}
function queueTable(): TableEvents<QueueRow> {
  return requireConn().db.cafeQueue;
}
function econTable(): TableEvents<EconRow> {
  return requireConn().db.cafeEcon;
}

function analyticsSummaryTable(): TableEvents<AnalyticsSummaryRow> {
  return requireConn().db.cafeAnalyticsSummary;
}

function rows<T>(source: TableEvents<T>): T[] {
  return [...source.iter()];
}

async function loadServerConfig(): Promise<ServerConfig> {
  const r = await fetch('/api/config');
  if (!r.ok) throw new Error(`/api/config returned ${r.status}`);
  return (await r.json()) as ServerConfig;
}

const TOKEN_KEY = 'context-cafe.stdb-token';

function connect(config: ServerConfig): Promise<DbConnection> {
  const attempt = (token: string | null): Promise<DbConnection> =>
    new Promise((resolve, reject) => {
      let builder = DbConnection.builder()
        .withUri(config.stdbUri)
        .withDatabaseName(config.database)
        // Persist the token so this browser keeps its identity across reloads.
        .onConnect((c, _identity, tok) => {
          try {
            localStorage.setItem(TOKEN_KEY, tok);
          } catch {
            /* ignore */
          }
          resolve(c);
        })
        .onDisconnect((_ctx, err) => {
          stopSimulation();
          showToast(err?.message ?? 'Disconnected.', 'error');
        })
        .onConnectError((_ctx, err) => reject(err));
      if (token) builder = builder.withToken(token);
      builder.build();
    });

  let saved: string | null = null;
  try {
    saved = localStorage.getItem(TOKEN_KEY);
  } catch {
    /* ignore */
  }
  if (!saved) return attempt(null);
  // A token rejected after a --delete-data republish
  // is rejected with a 401. Drop it and reconnect with a fresh identity.
  return attempt(saved).catch(err => {
    console.warn(
      'Stored identity token rejected. Clearing it and reconnecting.',
      err
    );
    try {
      localStorage.removeItem(TOKEN_KEY);
    } catch {
      /* ignore */
    }
    return attempt(null);
  });
}

function effectivePriceCents(row: VariantRow): number {
  return Math.round(row.priceCents * (1 - row.discountBps / 10000));
}

function variantById(id: string): VariantRow | undefined {
  return rows(variantsTable()).find(v => v.variantId === id);
}

function productById(id: string): ProductRow | undefined {
  return rows(productsTable()).find(p => p.productId === id);
}

function currentConfig(): ConfigRow | undefined {
  return rows(configTable())[0];
}

function currentMetrics(): MetricsRow {
  return (
    rows(metricsTable())[0] ?? {
      tick: 0n,
      views: 0n,
      carts: 0n,
      checkouts: 0n,
      purchases: 0n,
      abandons: 0n,
      revenueCents: 0n,
    }
  );
}

// Rendering

function renderKpis(): void {
  const m = currentMetrics();
  const e = currentEcon();

  // Wallet cash is set in renderEcon. Profit equals sales minus supply spend.
  setText('walletRevenue', money(m.revenueCents));
  setText('walletSpent', money(e.suppliesSpentCents));
  const profit = m.revenueCents - e.suppliesSpentCents;
  const profitEl = document.getElementById('walletProfit');
  if (profitEl) {
    const mag = money(profit < 0n ? -profit : profit);
    profitEl.textContent = `${profit < 0n ? '▼' : '▲'} ${mag} profit`;
    profitEl.classList.toggle('up', profit > 0n);
    profitEl.classList.toggle('down', profit < 0n);
  }

  // Ticker.
  setText('kpiConversion', pct(m.purchases, m.views));
  setText(
    'kpiAov',
    m.purchases === 0n ? '$0.00' : money(m.revenueCents / m.purchases)
  );
  setText('kpiTicks', m.tick.toString());
  setText(
    'kpiSent',
    (rows(analyticsSummaryTable())[0]?.delivered ?? 0n).toString()
  );
  setText('kpiReneged', String(e.reneged));
  const rushing =
    Number(m.tick % BigInt(RUSH_CYCLE_TICKS)) < RUSH_LENGTH_TICKS &&
    m.tick > 0n;
  document.getElementById('rushBadge')?.classList.toggle('on', rushing);

  const funnel: Array<[string, bigint, string]> = [
    ['views', m.views, ''],
    ['carts', m.carts, ''],
    ['bought', m.purchases, 'buy'],
    ['walked', m.abandons, 'off'],
  ];
  $('flowSummary').innerHTML = funnel
    .map(
      ([label, val, cls]) =>
        `<span class="f ${cls}"><b>${val.toString()}</b> ${escapeHtml(label)}</span>`
    )
    .join('');
}

function currentEcon(): EconRow {
  return (
    rows(econTable())[0] ?? {
      cashCents: 0n,
      computeUnits: 0,
      contextUnits: 0,
      memoryUnits: 0,
      suppliesSpentCents: 0n,
      stockouts: 0,
      reputation: 0,
      workers: 1,
      machineLevel: 0,
      seats: 0,
      storageLevel: 0,
      reneged: 0,
    }
  );
}

const SUPPLIES: Array<{
  kind: 'compute' | 'context' | 'memory';
  unitsKey: 'computeUnits' | 'contextUnits' | 'memoryUnits';
  id: string;
  fillId: string;
  capId: string;
}> = [
  {
    kind: 'compute',
    unitsKey: 'computeUnits',
    id: 'bankCompute',
    fillId: 'fillCompute',
    capId: 'capCompute',
  },
  {
    kind: 'context',
    unitsKey: 'contextUnits',
    id: 'bankContext',
    fillId: 'fillContext',
    capId: 'capContext',
  },
  {
    kind: 'memory',
    unitsKey: 'memoryUnits',
    id: 'bankMemory',
    fillId: 'fillMemory',
    capId: 'capMemory',
  },
];

function renderEcon(): void {
  const e = currentEcon();
  setText('econCash', money(e.cashCents));
  setText('econStockouts', String(e.stockouts));
  for (const s of SUPPLIES) {
    const units = e[s.unitsKey];
    const cap = storageCapacity(s.kind, e.storageLevel);
    const low = units < 20;
    const el = document.getElementById(s.id);
    if (el) {
      el.textContent = String(units);
      el.classList.toggle('low', low);
    }
    const capEl = document.getElementById(s.capId);
    if (capEl) capEl.textContent = `/${cap}`;
    const fill = document.getElementById(s.fillId);
    if (fill) {
      fill.style.width = `${Math.max(0, Math.min(100, Math.round((units / cap) * 100)))}%`;
      fill.classList.toggle('low', low);
    }
    const btn = document.querySelector(
      `[data-supply="${s.kind}"]`
    ) as HTMLButtonElement | null;
    if (btn) {
      const headroom = cap - units;
      const full = headroom <= 0;
      btn.disabled =
        full ||
        e.cashCents <
          BigInt(Math.min(BUY_UNITS, headroom) * SUPPLY_PRICE[s.kind]);
      btn.textContent = full
        ? 'Full'
        : `+50 · ${money(BUY_UNITS * SUPPLY_PRICE[s.kind])}`;
    }
  }
  renderReputation(e);
  renderUpgrades(e);
}

function renderReputation(e: EconRow): void {
  const filled = Math.round(e.reputation / 20);
  setText('repStars', '★'.repeat(filled) + '☆'.repeat(5 - filled));
  setText('repValue', String(e.reputation));
}

function renderUpgrades(e: EconRow): void {
  setText('upWorkers', `${e.workers}/tick`);
  setText(
    'upMachine',
    e.machineLevel > 0 ? `−${e.machineLevel * 8}% supplies` : 'standard'
  );
  setText('upCounter', `holds ${maximumQueueLength(e)}`);
  setText(
    'upStorage',
    e.storageLevel > 0 ? `+${e.storageLevel * 50}% space` : 'standard'
  );

  const maxed = e.machineLevel >= MAX_MACHINE_LEVEL;
  setBuy('worker', upgradeCost('worker', e), e.cashCents);
  setBuy(
    'machine',
    upgradeCost('machine', e),
    e.cashCents,
    maxed,
    maxed ? 'Maxed' : undefined
  );
  setBuy('counter', upgradeCost('counter', e), e.cashCents);
  setBuy('storage', upgradeCost('storage', e), e.cashCents);
}

function setBuy(
  kind: string,
  costCents: number | bigint,
  cash: bigint,
  force = false,
  label?: string
): void {
  const btn = document.querySelector(
    `[data-upgrade="${kind}"]`
  ) as HTMLButtonElement | null;
  if (!btn) return;
  btn.textContent = label ?? money(costCents);
  btn.disabled = force || cash < BigInt(costCents);
}

function activeScenarioId(): string {
  return (
    currentConfig()?.scenarioId || rows(scenariosTable())[0]?.scenarioId || ''
  );
}

function renderMenu(): void {
  const products = new Map(rows(productsTable()).map(p => [p.productId, p]));
  const variants = rows(variantsTable()).sort((a, b) => {
    const pa = products.get(a.productId)?.name ?? '';
    const pb = products.get(b.productId)?.name ?? '';
    return pa.localeCompare(pb) || a.name.localeCompare(b.name);
  });

  if (variants.length === 0) {
    $('menuGrid').innerHTML =
      '<div class="empty">Waiting for catalog sync.</div>';
    return;
  }

  $('menuGrid').innerHTML = variants
    .map(v => {
      const product = products.get(v.productId);
      const off = !v.active || (product ? !product.active : false);
      const discounted = v.discountBps > 0;
      const badges = [
        v.featured ? '<span class="chip featured">Featured</span>' : '',
        discounted
          ? `<span class="chip discount">-${v.discountBps / 100}%</span>`
          : '',
        off ? '<span class="chip off">Off</span>' : '',
      ].join('');
      return `
      <button class="menu-card ${off ? 'muted' : ''} ${v.variantId === selectedVariantId && drawerOpen ? 'active' : ''}"
              data-variant-id="${escapeHtml(v.variantId)}">
        <span class="menu-card-cat">${escapeHtml(product?.category ?? '')}</span>
        <span class="menu-card-name">${escapeHtml(v.name)}</span>
        <span class="menu-card-flavor">${escapeHtml(v.flavor)}</span>
        <span class="menu-card-foot">
          <span class="menu-card-price">${money(effectivePriceCents(v))}</span>
          <span class="menu-card-badges">${badges}</span>
        </span>
      </button>
    `;
    })
    .join('');
}

function renderDrawer(): void {
  if (!drawerOpen) return;
  const variant = variantById(selectedVariantId);
  if (!variant) {
    closeDrawer();
    return;
  }
  const product = productById(variant.productId);
  setText('drawerTitle', variant.name);
  setText('drawerSub', `${product?.name ?? ''} · ${product?.category ?? ''}`);

  input('priceInput').value = (variant.priceCents / 100).toFixed(2);
  input('discountInput').value = String(variant.discountBps / 100);

  const cost = supplyCost(
    product ?? { category: '' },
    variant,
    currentEcon().machineLevel
  );
  const costCents =
    cost.compute * SUPPLY_PRICE.compute +
    cost.context * SUPPLY_PRICE.context +
    cost.memory * SUPPLY_PRICE.memory;
  const margin = effectivePriceCents(variant) - costCents;
  const units = (n: number) => `${n} unit${n === 1 ? '' : 's'}`;

  $('variantDetails').innerHTML = `
    <div class="recipe-grid">
      <span>Context window</span><strong>${variant.contextTokens.toLocaleString()} tokens</strong>
      <span>Reasoning</span><strong>${variant.reasoning}/10</strong>
      <span>Latency</span><strong>${variant.latency}/10</strong>
      <span>Price</span><strong>${money(effectivePriceCents(variant))}${variant.discountBps > 0 ? ` (was ${money(variant.priceCents)})` : ''}</strong>
    </div>
    <div class="recipe-head">Uses per cup</div>
    <div class="recipe-grid">
      <span>Compute</span><strong>${units(cost.compute)}</strong>
      <span>Context</span><strong>${units(cost.context)}</strong>
      <span>Memory</span><strong>${units(cost.memory)}</strong>
      <span>Cost to make</span><strong>${money(costCents)}</strong>
      <span>Margin / cup</span><strong class="margin ${margin >= 0 ? 'pos' : 'neg'}">${margin < 0 ? '−' : '+'}${money(Math.abs(margin))}</strong>
    </div>
  `;

  setText('featureVariant', variant.featured ? 'Featured ✓' : 'Feature');
  $('featureVariant').classList.toggle('is-on', variant.featured);
  setText('toggleVariant', variant.active ? 'Disable recipe' : 'Enable recipe');
  setText(
    'toggleProduct',
    product?.active
      ? `Disable ${product?.name ?? 'product'}`
      : `Enable ${product?.name ?? 'product'}`
  );
}

function openDrawer(variantId: string): void {
  selectedVariantId = variantId;
  drawerOpen = true;
  $('drawer').classList.add('open');
  $('drawerScrim').classList.add('open');
  renderDrawer();
  renderMenu();
}

function closeDrawer(): void {
  drawerOpen = false;
  $('drawer').classList.remove('open');
  $('drawerScrim').classList.remove('open');
  renderMenu();
}

function cap(value: string): string {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

type QueueRow = {
  queueId: bigint;
  botId: string;
  profile: string;
  scenarioId: string;
  productId: string;
  variantId: string;
  wants: string;
  thrifty: boolean;
  arrivedTick: bigint;
};

function botInner(row: QueueRow, drink: string): string {
  const wants = `wants ${escapeHtml(row.wants)}${row.thrifty ? ' · price-sensitive' : ''}`;
  return `
    <div class="bot-tip"><b>${escapeHtml(cap(row.profile))} bot</b><br>${wants}<br>eyeing ${escapeHtml(drink)}</div>
    <div class="bot-emoji">🤖</div>
    <div class="bot-profile">${escapeHtml(row.profile)}</div>
    <div class="bot-wants">${escapeHtml(row.wants)}${row.thrifty ? ' 💸' : ''}</div>`;
}

// Render the waiting line, diffing against the live DOM so only changes animate.
function renderCounter(): void {
  const bots = document.getElementById('counterBots');
  if (!bots) return;
  const queue = [...rows(queueTable())].sort((a, b) =>
    a.queueId < b.queueId ? -1 : a.queueId > b.queueId ? 1 : 0
  );

  if (queue.length === 0) {
    if (!bots.querySelector('.empty'))
      bots.innerHTML = '<div class="empty">Press Run to open the queue.</div>';
  } else {
    const placeholder = bots.querySelector('.empty');
    if (placeholder) bots.innerHTML = '';

    const variants = new Map(
      rows(variantsTable()).map(row => [row.variantId, row])
    );
    const desired = queue.slice(0, 12); // front (lowest queueId) is served next, shown leftmost
    const desiredIds = new Set(desired.map(row => row.queueId.toString()));

    // Bots that were served leave the line.
    for (const el of Array.from(bots.children) as HTMLElement[]) {
      const id = el.getAttribute('data-queue-id');
      if (id && !desiredIds.has(id) && !el.classList.contains('leaving')) {
        el.classList.add('leaving');
        window.setTimeout(() => el.remove(), 480);
      }
    }

    const existing = new Set<string>();
    for (const el of Array.from(bots.children) as HTMLElement[]) {
      const id = el.getAttribute('data-queue-id');
      if (id) existing.add(id);
    }

    let added = 0;
    for (const row of desired) {
      const id = row.queueId.toString();
      if (existing.has(id)) continue;
      const drink = variants.get(row.variantId)?.name ?? 'a drink';
      const node = document.createElement('div');
      node.className = `bot waiting${row.thrifty ? ' thrifty' : ''} entering`;
      node.setAttribute('data-queue-id', id);
      node.style.animationDelay = `${added * 60}ms`;
      node.innerHTML = botInner(row, drink);
      bots.appendChild(node);
      const delay = added * 60;
      window.setTimeout(() => {
        node.classList.remove('entering');
        node.style.animationDelay = '';
      }, 480 + delay);
      added++;
    }
  }

  detectOutcomes();
}

// Pop each served bot: green "+$" on a sale, red "no sale" otherwise. Seed the
// first batch silently to prevent replaying past outcomes after a reload.
function detectOutcomes(): void {
  const sessions = rows(sessionsTable());
  if (sessions.length === 0) {
    seenSessionIds.clear();
    counterReady = false;
    return;
  }
  if (!counterReady) {
    for (const row of sessions) seenSessionIds.add(row.sessionId.toString());
    counterReady = true;
    return;
  }
  // Oldest-first so a burst pops in the order it happened.
  const fresh = sessions
    .filter(row => !seenSessionIds.has(row.sessionId.toString()))
    .sort((a, b) =>
      a.sessionId < b.sessionId ? -1 : a.sessionId > b.sessionId ? 1 : 0
    );
  // Fan a batch out so simultaneous serves don't stack on the same spot.
  fresh.forEach((row, i) => {
    seenSessionIds.add(row.sessionId.toString());
    if (row.stage === 'purchased')
      spawnPop(`+${money(row.revenueCents)}`, 'sale', i);
    else spawnPop(missReason(row.reason), 'miss', i);
  });
}

// Friendly counter caption for why a bot left without buying.
function missReason(reason: string): string {
  switch (reason) {
    case 'price':
      return 'too pricey';
    case 'slow':
      return 'too slow';
    case 'want_vision':
      return 'wanted image support';
    case 'want_memory':
      return 'wanted memory';
    case 'want_smart':
      return 'wanted more reasoning';
    case 'want_premium':
      return 'wanted top quality';
    case 'meh':
      return 'changed its mind';
    case 'waited':
      return 'gave up waiting';
    case 'short_context':
      return 'out of context';
    case 'short_compute':
      return 'out of compute';
    case 'short_memory':
      return 'out of memory';
    case 'unavailable':
      return 'off the menu';
    default:
      return 'no sale';
  }
}

function spawnPop(text: string, variant: 'sale' | 'miss', index = 0): void {
  const pops = document.getElementById('counterPops');
  const bots = document.getElementById('counterBots');
  if (!pops) return;
  // Anchor near the counter, then fan a batch rightward + stagger so they don't overlap.
  const anchor = bots?.querySelector('.bot') as HTMLElement | null;
  const baseLeft = anchor ? anchor.offsetLeft + anchor.offsetWidth / 2 : 30;
  const baseTop = anchor ? anchor.offsetTop + 4 : 14;
  const pop = document.createElement('div');
  pop.className = `pop ${variant}`;
  pop.textContent = text;
  pop.style.left = `${baseLeft + (index % 5) * 62}px`;
  pop.style.top = `${baseTop - (index % 2) * 12}px`;
  pop.style.animationDelay = `${index * 90}ms`;
  pops.appendChild(pop);
  pop.addEventListener('animationend', () => pop.remove());
}

function renderAll(): void {
  if (!conn) return;
  renderKpis();
  renderEcon();
  renderMenu();
  renderDrawer();
  renderCounter();
  syncRunButton();
}

function syncRunButton(): void {
  const btn = document.getElementById('runToggle');
  if (!btn) return;
  btn.classList.toggle('running', running);
  const label = btn.querySelector('.run-label');
  if (label) label.textContent = running ? 'Pause' : 'Run';
}

// Simulation controls

function restartTimer(): void {
  if (simTimer) clearInterval(simTimer);
  simTimer = null;
  if (!running) return;
  simTimer = setInterval(() => {
    void tickSimulation().catch(err => {
      stopSimulation();
      showToast(err instanceof Error ? err.message : String(err), 'error');
    });
  }, speedMs);
}

function stopSimulation(): void {
  running = false;
  if (simTimer) clearInterval(simTimer);
  simTimer = null;
  syncRunButton();
}

async function tickSimulation(): Promise<void> {
  requireConn().reducers.simulateTick({
    ticks: 1,
    seed: `${Date.now()}:${Math.random()}`,
  });
}

// UI wiring

function wireUi(): void {
  $('menuGrid').addEventListener('click', event => {
    const card = (event.target as HTMLElement).closest(
      '[data-variant-id]'
    ) as HTMLElement | null;
    if (card?.dataset.variantId) openDrawer(card.dataset.variantId);
  });
  $('drawerClose').addEventListener('click', () => closeDrawer());
  $('drawerScrim').addEventListener('click', () => closeDrawer());
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && drawerOpen) closeDrawer();
  });
}

function guard(fn: () => Promise<void>): () => Promise<void> {
  return async () => {
    try {
      await fn();
    } catch (err) {
      showToast(err instanceof Error ? err.message : String(err), 'error');
    }
  };
}

function wireActions(): void {
  $('runToggle').addEventListener('click', () => {
    running = !running;
    restartTimer();
    renderAll();
  });
  $('tickOnce').addEventListener(
    'click',
    guard(() => tickSimulation())
  );
  select('speedSelect').addEventListener('change', () => {
    speedMs = Number(select('speedSelect').value);
    restartTimer();
  });
  $('resetSim').addEventListener(
    'click',
    guard(async () => {
      stopSimulation();
      requireConn().reducers.resetSimulation({
        scenarioId: activeScenarioId(),
      });
      showToast('Simulation reset.');
    })
  );
  $('savePrice').addEventListener(
    'click',
    guard(async () => {
      const cents = Math.max(
        0,
        Math.round(Number(input('priceInput').value) * 100)
      );
      requireConn().reducers.setVariantPrice({
        variantId: selectedVariantId,
        priceCents: cents,
      });
    })
  );
  $('saveDiscount').addEventListener(
    'click',
    guard(async () => {
      const bps = Math.max(
        0,
        Math.min(9000, Math.round(Number(input('discountInput').value) * 100))
      );
      requireConn().reducers.setVariantDiscount({
        variantId: selectedVariantId,
        discountBps: bps,
      });
    })
  );
  $('featureVariant').addEventListener(
    'click',
    guard(async () => {
      requireConn().reducers.setFeaturedVariant({
        variantId: selectedVariantId,
      });
    })
  );
  $('toggleVariant').addEventListener(
    'click',
    guard(async () => {
      const row = variantById(selectedVariantId);
      if (!row) throw new Error('No recipe selected.');
      requireConn().reducers.setVariantActive({
        variantId: selectedVariantId,
        active: !row.active,
      });
    })
  );
  $('toggleProduct').addEventListener(
    'click',
    guard(async () => {
      const variant = variantById(selectedVariantId);
      const product = variant ? productById(variant.productId) : undefined;
      if (!product) throw new Error('No product selected.');
      requireConn().reducers.setProductActive({
        productId: product.productId,
        active: !product.active,
      });
    })
  );
  $('bankGrid').addEventListener('click', event => {
    const btn = (event.target as HTMLElement).closest(
      '[data-supply]'
    ) as HTMLElement | null;
    const kind = btn?.dataset.supply;
    if (!kind) return;
    void guard(async () => {
      requireConn().reducers.buySupply({ kind, units: BUY_UNITS });
    })();
  });
  $('upgradeGrid').addEventListener('click', event => {
    const btn = (event.target as HTMLElement).closest(
      '[data-upgrade]'
    ) as HTMLElement | null;
    const kind = btn?.dataset.upgrade;
    if (!kind) return;
    void guard(async () => {
      requireConn().reducers.buyUpgrade({ kind });
    })();
  });
}

function wireTableEvents(): void {
  const render = () => renderAll();
  const sources = [
    productsTable(),
    variantsTable(),
    scenariosTable(),
    configTable(),
    metricsTable(),
    sessionsTable(),
    queueTable(),
    econTable(),
    analyticsSummaryTable(),
  ];
  for (const source of sources) {
    source.onInsert(render);
    source.onUpdate(render);
    source.onDelete(render);
  }
}

async function run(): Promise<void> {
  // Wire the chrome first so the menu and drawer are interactive immediately.
  wireUi();
  wireActions();

  const config = await loadServerConfig();

  // The link ships with a working default href in the HTML; upgrade it to the
  // configured PostHog host when the server provides one.
  const link = document.getElementById(
    'posthogLink'
  ) as HTMLAnchorElement | null;
  if (link && config.posthogAppUrl) {
    link.href = config.posthogAppUrl;
  }

  conn = await connect(config);

  // Seed this browser's café (idempotent); rows stream in via the subscriptions.
  try {
    requireConn().reducers.initSession({});
  } catch (err) {
    console.error('init_session failed', err);
  }

  conn
    .subscriptionBuilder()
    .onApplied(() => {
      renderAll();
      showToast('Context Cafe ready.');
    })
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([
      'SELECT * FROM cafe_products',
      'SELECT * FROM cafe_variants',
      'SELECT * FROM cafe_scenarios',
      'SELECT * FROM cafe_config',
      'SELECT * FROM cafe_metrics',
      'SELECT * FROM cafe_econ',
      'SELECT * FROM cafe_queue',
      'SELECT * FROM cafe_recent_sessions',
      'SELECT * FROM cafe_analytics_summary',
    ]);

  wireTableEvents();

  renderAll();
}

run().catch(err => {
  showToast(err instanceof Error ? err.message : String(err), 'error');
});
