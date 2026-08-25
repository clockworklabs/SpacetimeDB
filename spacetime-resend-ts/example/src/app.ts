import {
  DbConnection,
  type ErrorContext,
  type EventContext,
  type SubscriptionEventContext,
} from './codegen';
import type { Timestamp } from 'spacetimedb';
import { messageHtml } from '../spacetimedb/src/message';

type TableAccessor<T> = {
  iter(): Iterable<T>;
  onInsert(cb: (ctx: EventContext, row: T) => void): void;
  onUpdate(cb: (ctx: EventContext, old: T, row: T) => void): void;
  onDelete(cb: (ctx: EventContext, row: T) => void): void;
};

// Mirrors the caller-scoped my_dispatch_emails view. Options arrive as
// `T | undefined`; status is a tagged enum.
type ResendEmail = {
  resendId: string;
  fromAddress: string;
  toAddressesJson: string;
  subject?: string;
  status: { tag: string } | string;
  lastError?: string;
  bouncedAt?: Timestamp;
  failedAt?: Timestamp;
  failureReason?: string;
  complained: boolean;
  complainedAt?: Timestamp;
  opened: boolean;
  openedAt?: Timestamp;
  clicked: boolean;
  clickedAt?: Timestamp;
  deliveredAt?: Timestamp;
  sentAt?: Timestamp;
  createdAt: Timestamp;
  updatedAt: Timestamp;
};

type SendResult = {
  ok: boolean;
  resendId?: string;
  message: string;
};

type ServerConfig = {
  stdbUri: string;
  database: string;
  resendConfigured: boolean;
  defaultFrom: string;
  allowedRecipients: string[];
};

const $ = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;

let conn: DbConnection | null = null;
let currentConfig: ServerConfig | null = null;
let sending = false;

function escapeHtml(value: unknown): string {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function tagOf(value: { tag: string } | string | undefined): string {
  if (value && typeof value === 'object' && 'tag' in value) return value.tag;
  return String(value ?? '');
}

function timestampMs(ts: Timestamp | undefined): number {
  if (!ts) return 0;
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function timeLabel(ts: Timestamp | undefined): string {
  const ms = timestampMs(ts);
  if (!ms) return '';
  return new Date(ms).toLocaleTimeString([], {
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
  });
}

const AVATAR_COLORS = [
  '#4cf490',
  '#02befa',
  '#a880ff',
  '#fbdc8e',
  '#ff9e9e',
  '#00ccb4',
  '#ff80fb',
];

// The Resend test addresses always get the same, recognizable colour.
const KNOWN_AVATAR_COLORS: Record<string, string> = {
  'delivered@resend.dev': '#4cf490', // green
  'bounced@resend.dev': '#ff9e9e', // orange
  'complained@resend.dev': '#a880ff', // purple
};

function avatarColor(seed: string): string {
  const known = KNOWN_AVATAR_COLORS[seed.toLowerCase()];
  if (known) return known;
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}

function recipient(row: ResendEmail): string {
  try {
    const parsed = JSON.parse(row.toAddressesJson);
    if (Array.isArray(parsed) && parsed.length > 0) return String(parsed[0]);
  } catch {
    /* fall through */
  }
  return row.toAddressesJson;
}

function initials(email: string): string {
  const local = email.split('@')[0] ?? email;
  return (local.slice(0, 2) || '?').toUpperCase();
}

function showError(message: string) {
  $('form-error').textContent = message;
}

function clearError() {
  $('form-error').textContent = '';
}

let flashTimer: ReturnType<typeof setTimeout> | undefined;

function flashSent() {
  const btn = $('send-btn') as HTMLButtonElement;
  btn.classList.add('sent');
  btn.textContent = 'Sent';
  if (flashTimer) clearTimeout(flashTimer);
  flashTimer = setTimeout(() => {
    btn.classList.remove('sent');
    if (!sending) btn.textContent = 'Send';
  }, 1800);
}

async function loadServerConfig(): Promise<ServerConfig> {
  const res = await fetch('/api/config');
  if (!res.ok) throw new Error(`/api/config returned ${res.status}`);
  return (await res.json()) as ServerConfig;
}

async function connect(config: ServerConfig): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(config.stdbUri)
      .withDatabaseName(config.database)
      .onConnect(c => resolve(c))
      .onDisconnect((_ctx, err) => {
        showError(`Disconnected: ${err?.message ?? 'connection lost'}`);
      })
      .onConnectError((_ctx: ErrorContext, err) => reject(err))
      .build();
  });
}

function table<T>(name: string): TableAccessor<T> | undefined {
  const db = (conn?.db ?? {}) as Record<string, TableAccessor<T> | undefined>;
  return db[name];
}

function emails(): ResendEmail[] {
  return [...(table<ResendEmail>('myDispatchEmails')?.iter() ?? [])].sort(
    (a, b) => {
      return timestampMs(b.createdAt) - timestampMs(a.createdAt);
    }
  );
}

type Node = {
  key: string;
  label: string;
  at?: Timestamp;
  state: 'done' | 'live' | 'pending' | 'bad' | 'warn';
};

// Turn a stored email into an ordered set of lifecycle nodes. The EmailStatus enum
// tops out at Delivered; Opened/Clicked are booleans; negatives are terminal.
function timeline(row: ResendEmail): Node[] {
  const status = tagOf(row.status);
  const nodes: Node[] = [
    { key: 'queued', label: 'Queued', at: row.createdAt, state: 'done' },
  ];

  const sentReached =
    Boolean(row.sentAt) ||
    Boolean(row.deliveredAt) ||
    row.opened ||
    row.clicked ||
    status === 'Sent' ||
    status === 'Delivered';
  nodes.push({
    key: 'sent',
    label: 'Sent',
    at: row.sentAt,
    state: sentReached ? 'done' : 'pending',
  });

  if (status === 'Bounced') {
    nodes.push({
      key: 'bounced',
      label: 'Bounced',
      at: row.bouncedAt,
      state: 'bad',
    });
    return nodes;
  }
  if (status === 'Failed') {
    nodes.push({
      key: 'failed',
      label: 'Failed',
      at: row.failedAt,
      state: 'bad',
    });
    return nodes;
  }
  if (status === 'Cancelled') {
    nodes.push({ key: 'cancelled', label: 'Cancelled', state: 'warn' });
    return nodes;
  }

  // A complaint (marked as spam) implies the mail was delivered, and it is terminal.
  const deliveredReached =
    Boolean(row.deliveredAt) ||
    row.opened ||
    row.clicked ||
    status === 'Delivered' ||
    row.complained;
  const delayed = status === 'DeliveryDelayed' && !deliveredReached;
  nodes.push({
    key: 'delivered',
    label: delayed ? 'Delayed' : 'Delivered',
    at: row.deliveredAt,
    state: deliveredReached ? 'done' : delayed ? 'warn' : 'pending',
  });

  if (row.complained) {
    // Terminal: show completed positive steps followed by Complaint.
    // No trailing "pending" steps and no live pulse - the lifecycle is over.
    if (row.opened)
      nodes.push({
        key: 'opened',
        label: 'Opened',
        at: row.openedAt,
        state: 'done',
      });
    if (row.clicked)
      nodes.push({
        key: 'clicked',
        label: 'Clicked',
        at: row.clickedAt,
        state: 'done',
      });
    nodes.push({
      key: 'complaint',
      label: 'Complaint',
      at: row.complainedAt,
      state: 'bad',
    });
    return nodes;
  }

  nodes.push({
    key: 'opened',
    label: 'Opened',
    at: row.openedAt,
    state: row.opened ? 'done' : 'pending',
  });
  nodes.push({
    key: 'clicked',
    label: 'Clicked',
    at: row.clickedAt,
    state: row.clicked ? 'done' : 'pending',
  });

  // The furthest reached positive node is the live frontier. A completed later
  // node makes the final completed positive node the subtle accent.
  const lastDone = nodes.map(n => n.state === 'done').lastIndexOf(true);
  if (
    lastDone > 0 &&
    nodes[lastDone].state === 'done' &&
    nodes.some(n => n.state === 'pending')
  ) {
    nodes[lastDone].state = 'live';
  }
  return nodes;
}

function headPill(row: ResendEmail): { cls: string; text: string } {
  const status = tagOf(row.status);
  if (status === 'Bounced') return { cls: 'red', text: 'Bounced' };
  if (status === 'Failed') return { cls: 'red', text: 'Failed' };
  if (status === 'Cancelled') return { cls: 'muted', text: 'Cancelled' };
  if (row.complained) return { cls: 'red', text: 'Complaint' };
  if (row.clicked) return { cls: 'green', text: 'Clicked' };
  if (row.opened) return { cls: 'green', text: 'Opened' };
  if (row.deliveredAt || status === 'Delivered')
    return { cls: 'green', text: 'Delivered' };
  if (status === 'DeliveryDelayed') return { cls: 'yellow', text: 'Delayed' };
  if (row.sentAt || status === 'Sent') return { cls: 'green', text: 'Sent' };
  return { cls: 'muted', text: 'Queued' };
}

// Per-email rendered-node keys ensure each changed transition animates once.
const lastReached = new Map<string, Set<string>>();
const knownEmails = new Set<string>();
const lastCounts: Record<string, number> = {
  sent: -1,
  delivered: -1,
  opened: -1,
  bounced: -1,
};

function prefersReducedMotion(): boolean {
  return (
    typeof matchMedia === 'function' &&
    matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

// Odometer roll: stack the current and incoming values in a clip window. An
// increase rolls up from below; a decrease rolls down.
function rollNumber(el: HTMLElement, from: number, to: number) {
  const up = to > from;
  const top = up ? from : to;
  const bottom = up ? to : from;
  el.innerHTML =
    `<span class="odo-clip"><span class="odo ${up ? 'roll-up' : 'roll-down'}">` +
    `<span class="odo-line">${top}</span><span class="odo-line">${bottom}</span>` +
    `</span></span>`;
  const odo = el.querySelector('.odo');
  if (!odo) {
    el.textContent = String(to);
    return;
  }
  // Collapse back to a plain number once the roll finishes (or is superseded).
  const settle = () => {
    if (el.contains(odo)) el.textContent = String(to);
  };
  odo.addEventListener('animationend', settle, { once: true });
  setTimeout(settle, 600);
}

function bumpMetric(id: string, key: string, value: number) {
  const el = $(id);
  const prev = lastCounts[key];
  lastCounts[key] = value;
  if (prev < 0 || prev === value || prefersReducedMotion()) {
    el.textContent = String(value);
    return;
  }
  rollNumber(el, prev, value);
}

function renderMetrics(list: ResendEmail[]) {
  const delivered = list.filter(
    r =>
      r.deliveredAt || r.opened || r.clicked || tagOf(r.status) === 'Delivered'
  ).length;
  const opened = list.filter(r => r.opened).length;
  const bounced = list.filter(r => {
    const s = tagOf(r.status);
    return s === 'Bounced' || s === 'Failed' || r.complained;
  }).length;
  bumpMetric('count-sent', 'sent', list.length);
  bumpMetric('count-delivered', 'delivered', delivered);
  bumpMetric('count-opened', 'opened', opened);
  bumpMetric('count-bounced', 'bounced', bounced);
}

function nodeHtml(node: Node, lit: boolean): string {
  const time = node.at ? timeLabel(node.at) : '';
  return `
    <div class="node ${node.state}${lit ? ' just-lit' : ''}">
      <span class="bead"></span>
      <span class="node-label">${escapeHtml(node.label)}</span>
      <span class="node-time">${escapeHtml(time)}</span>
    </div>`;
}

function mailHtml(
  row: ResendEmail,
  nodes: Node[],
  newlyLit: Set<string>,
  isNew: boolean
): string {
  const to = recipient(row);
  const pill = headPill(row);
  const bad = pill.cls === 'red';
  const subject =
    row.subject && row.subject.length ? row.subject : '(no subject)';
  const track = nodes.map(n => nodeHtml(n, newlyLit.has(n.key))).join('');
  const error =
    bad && row.failureReason
      ? `<div class="mail-err">${escapeHtml(row.failureReason)}</div>`
      : bad && row.lastError
        ? `<div class="mail-err">${escapeHtml(row.lastError)}</div>`
        : '';
  return `
    <article class="mail ${bad ? 'bad' : ''}${isNew ? ' card-enter' : ''}">
      <div class="mail-head">
        <span class="avatar" style="--av: ${avatarColor(to)}">${escapeHtml(initials(to))}</span>
        <span class="mail-who">
          <span class="mail-to">${escapeHtml(to)}</span>
          <span class="mail-sub">${escapeHtml(subject)}</span>
        </span>
        <span class="mail-when">${escapeHtml(timeLabel(row.createdAt))}</span>
        <button class="mail-del" type="button" data-del="${escapeHtml(row.resendId)}" title="Delete" aria-label="Delete">&times;</button>
      </div>
      <div class="track">${track}</div>
      ${error}
    </article>`;
}

function reachedKeys(nodes: Node[]): Set<string> {
  return new Set(nodes.filter(n => n.state !== 'pending').map(n => n.key));
}

function render() {
  const list = emails();
  renderMetrics(list);
  $('feed-count').textContent = `${list.length} sent`;
  ($('clear-btn') as HTMLButtonElement).disabled = list.length === 0;

  if (list.length === 0) {
    $('feed').innerHTML =
      `<div class="empty">No dispatches yet.<br>Compose a message and hit Send to watch it move.</div>`;
    lastReached.clear();
    knownEmails.clear();
    return;
  }

  const entries = list.map(row => ({ row, nodes: timeline(row) }));

  $('feed').innerHTML = entries
    .map(({ row, nodes }) => {
      const reached = reachedKeys(nodes);
      const prev = lastReached.get(row.resendId);
      const isNew = !knownEmails.has(row.resendId);
      const newlyLit = new Set<string>();
      if (prev) for (const k of reached) if (!prev.has(k)) newlyLit.add(k);
      return mailHtml(row, nodes, newlyLit, isNew);
    })
    .join('');

  // Record post-render state so the next render can diff against it.
  const currentIds = new Set<string>();
  for (const { row, nodes } of entries) {
    lastReached.set(row.resendId, reachedKeys(nodes));
    knownEmails.add(row.resendId);
    currentIds.add(row.resendId);
  }
  for (const id of [...knownEmails]) {
    if (!currentIds.has(id)) {
      knownEmails.delete(id);
      lastReached.delete(id);
    }
  }
}

// Coalesce the burst of table events from a single webhook (email row update +
// delivery-event insert land together) into one render, so the change diff sees the
// whole transition at once and animates the node that advanced.
let renderScheduled = false;
function scheduleRender() {
  if (renderScheduled) return;
  renderScheduled = true;
  setTimeout(() => {
    renderScheduled = false;
    render();
  }, 0);
}

function wireTable(name: string) {
  const accessor = table<unknown>(name);
  if (!accessor) throw new Error(`missing table accessor: ${name}`);
  accessor.onInsert(() => scheduleRender());
  accessor.onUpdate(() => scheduleRender());
  accessor.onDelete(() => scheduleRender());
}

function wireDataHandlers() {
  for (const name of ['myDispatchEmails', 'myDispatchDeliveryEvents']) {
    wireTable(name);
  }

  conn!
    .subscriptionBuilder()
    .onApplied((_ctx: SubscriptionEventContext) => {
      render();
      if (!currentConfig?.resendConfigured) {
        showError(
          'RESEND_API_KEY is missing in this example server environment.'
        );
      }
    })
    .onError((ctx: ErrorContext) => {
      console.error('subscription error:', ctx.event);
      showError('Subscription failed. Check the server console.');
    })
    .subscribe([
      'SELECT * FROM my_dispatch_emails',
      'SELECT * FROM my_dispatch_delivery_events',
    ]);
}

async function sendDispatch(
  to: string,
  subject: string,
  message: string
): Promise<SendResult> {
  if (!conn) throw new Error('not connected');
  return conn.procedures.sendDispatch({ to, subject, message });
}

async function deleteDispatch(resendId: string): Promise<void> {
  if (!conn) throw new Error('not connected');
  await conn.procedures.deleteDispatch({ resendId });
}

async function clearDispatches(): Promise<void> {
  if (!conn) throw new Error('not connected');
  await conn.procedures.clearDispatches({});
}

function setSending(next: boolean) {
  sending = next;
  const btn = $('send-btn') as HTMLButtonElement;
  btn.disabled = next;
  if (next) btn.textContent = 'Sending...';
  else if (!btn.classList.contains('sent')) btn.textContent = 'Send';
}

function renderPreview() {
  const subject =
    ($('subject-input') as HTMLInputElement).value.trim() || '(no subject)';
  const message = ($('message-input') as HTMLTextAreaElement).value.trim();
  const from = currentConfig?.defaultFrom || 'onboarding@resend.dev';
  const body = message
    ? messageHtml(message)
    : '<span class="mp-empty">Nothing to preview yet.</span>';
  $('message-preview').innerHTML = `
    <div class="mp-head">
      <div class="mp-subject">${escapeHtml(subject)}</div>
      <div class="mp-from">From ${escapeHtml(from)}</div>
    </div>
    <div class="mp-body">${body}</div>`;
}

function setMsgTab(tab: 'write' | 'preview') {
  const isPreview = tab === 'preview';
  if (isPreview) renderPreview();
  ($('message-input') as HTMLTextAreaElement).hidden = isPreview;
  $('message-preview').hidden = !isPreview;
  document.querySelectorAll<HTMLElement>('.msg-tab').forEach(b => {
    b.classList.toggle('active', b.dataset.tab === tab);
  });
}

async function submitCompose() {
  if (sending) return;
  const to = ($('to-input') as HTMLInputElement).value.trim();
  const subject = ($('subject-input') as HTMLInputElement).value.trim();
  const message = ($('message-input') as HTMLTextAreaElement).value.trim();
  if (!to) {
    showError('Add a recipient first.');
    return;
  }
  if (!message) {
    showError('Write a message before sending.');
    return;
  }
  clearError();
  setSending(true);
  try {
    const result = await sendDispatch(to, subject, message);
    setSending(false);
    if (result.ok) {
      ($('subject-input') as HTMLInputElement).value = '';
      ($('message-input') as HTMLTextAreaElement).value = '';
      setMsgTab('write');
      flashSent();
    } else {
      showError(result.message);
    }
  } catch (err) {
    setSending(false);
    showError(err instanceof Error ? err.message : String(err));
  }
}

function wireActions() {
  $('compose-form').addEventListener('submit', event => {
    event.preventDefault();
    submitCompose().catch(err => {
      console.error(err);
      showError(err instanceof Error ? err.message : String(err));
    });
  });

  document.querySelectorAll<HTMLElement>('.msg-tab').forEach(button => {
    button.addEventListener('click', () => {
      setMsgTab(button.dataset.tab === 'preview' ? 'preview' : 'write');
    });
  });

  document
    .querySelectorAll<HTMLButtonElement>('[data-fill]')
    .forEach(button => {
      button.addEventListener('click', () => {
        const email = button.dataset.fill ?? 'delivered@resend.dev';
        ($('to-input') as HTMLInputElement).value = email;
        const subject = $('subject-input') as HTMLInputElement;
        const message = $('message-input') as HTMLTextAreaElement;
        if (!subject.value.trim()) subject.value = 'Hello from Dispatch';
        if (!message.value.trim())
          message.value = 'Watching this one travel through the pipeline.';
        clearError();
        ($('to-input') as HTMLInputElement).focus();
      });
    });

  $('feed').addEventListener('click', event => {
    const btn = (event.target as HTMLElement).closest<HTMLElement>(
      '[data-del]'
    );
    if (!btn) return;
    const resendId = btn.dataset.del;
    if (!resendId) return;
    btn.setAttribute('disabled', 'true');
    deleteDispatch(resendId).catch(err => {
      console.error(err);
      showError(err instanceof Error ? err.message : String(err));
    });
  });

  $('clear-btn').addEventListener('click', () => {
    clearDispatches().catch(err => {
      console.error(err);
      showError(err instanceof Error ? err.message : String(err));
    });
  });
}

async function main() {
  wireActions();
  currentConfig = await loadServerConfig();
  const recipientInput = $('to-input') as HTMLInputElement;
  recipientInput.value = currentConfig.allowedRecipients[0] ?? '';
  conn = await connect(currentConfig);
  wireDataHandlers();
}

main().catch(err => {
  console.error(err);
  showError(err instanceof Error ? err.message : String(err));
});
