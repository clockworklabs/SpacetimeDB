import '@spacetimedb/example-ui/styles.css';
import { DbConnection, tables, type ErrorContext } from './module_bindings/app';

interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

const TOKEN_KEY = 'dossier-token';
const RECONNECT_MILLIS = 2_000;
const STEP_LOST = 'workflow.step_lost';
const UPDATE_EVERY_SECONDS = 24 * 60 * 60;

const $ = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;

let conn: DbConnection | null = null;
let activeId: bigint | null = null;
// Phones show one screen at a time: the list, or a dossier.
let view: 'list' | 'chat' = 'chat';
let selectNext = false;
// Rendering replaces the messages, so remember which step lists are open.
const openSteps = new Set<bigint>();

function element<T extends keyof HTMLElementTagNameMap>(
  tag: T,
  className?: string,
  text?: string
): HTMLElementTagNameMap[T] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
function showError(error: unknown): void {
  const el = $('toast');
  el.className = 'toast show';
  el.textContent = error instanceof Error ? error.message : String(error);
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 2400);
}

function message(kind: string, who: string, ...body: Node[]): HTMLElement {
  const node = element('div', `msg ${kind}`);
  const content = element('div', 'body');
  content.append(...body);
  node.append(element('div', 'who', who), content);
  return node;
}

function button(className: string, label: string, onClick: () => void) {
  const node = element('button', className, label);
  node.type = 'button';
  node.addEventListener('click', onClick);
  return node;
}

function link(text: string, url: string, className?: string) {
  const node = element('a', className, text);
  node.href = url;
  node.target = '_blank';
  node.rel = 'noopener noreferrer';
  return node;
}

function runOf(dossier: { runId: bigint }) {
  return [...conn!.db.myRuns.iter()].find(row => row.id === dossier.runId);
}

// Busy unless it waits for approval, or sleeps until its next update.
function isBusy(
  run:
    | { status: { tag: string }; waitingFor?: string; lastStep?: string }
    | undefined
): boolean {
  if (run?.status.tag === 'Waiting') return run.waitingFor !== 'plan-approved';
  return run?.status.tag === 'Running' && run.lastStep !== 'sleep';
}

function untilLabel(at: { microsSinceUnixEpoch: bigint }): string {
  const minutes = Math.max(
    1,
    Math.round((Number(at.microsSinceUnixEpoch / 1000n) - Date.now()) / 60_000)
  );
  return minutes < 60 ? `${minutes}m` : `${Math.round(minutes / 60)}h`;
}

function renderList(): void {
  const dossiers = [...conn!.db.myDossiers.iter()].sort((a, b) =>
    Number(b.id - a.id)
  );
  if (dossiers.length === 0) {
    $('dossier-list').replaceChildren(
      element('div', 'empty', 'no dossiers yet')
    );
    return;
  }
  $('dossier-list').replaceChildren(
    ...dossiers.map(dossier => {
      const item = element(
        'div',
        dossier.id === activeId ? 'thread-item active' : 'thread-item'
      );
      const run = runOf(dossier);
      if (isBusy(run) || run?.waitingFor === 'plan-approved') {
        const dot = element('span', 'busy-dot');
        dot.title =
          run?.waitingFor === 'plan-approved'
            ? 'needs your approval'
            : 'researching';
        item.append(dot);
      }
      item.append(element('span', 'title', dossier.question));
      item.addEventListener('click', () => {
        activeId = dossier.id;
        view = 'chat';
        render();
      });
      return item;
    })
  );
}

function stepLines(
  steps: { step: string; attempt: number; error?: string }[]
): string {
  return steps
    .map(row => {
      if (row.error === STEP_LOST)
        return `${row.step}  ·  interrupted by a restart`;
      if (row.error) return `${row.step}  ·  ${row.error}`;
      return row.attempt > 1
        ? `${row.step}  ·  done on attempt ${row.attempt}`
        : row.step;
    })
    .join('\n');
}

// While research runs, each topic shows its latest attempt.
function topicList(
  topics: string[],
  steps?: { step: string; status: { tag: string }; error?: string }[]
): HTMLElement {
  const list = element('ol');
  topics.forEach((topic, index) => {
    const item = element('li', undefined, topic);
    const last = steps
      ?.filter(row => row.step === `Research #${index + 1}`)
      .at(-1);
    const [state, label] = !steps
      ? []
      : !last
        ? ['active', 'researching']
        : last.status.tag === 'Ok'
          ? ['done', 'done']
          : last.status.tag === 'GaveUp'
            ? ['skipped', 'skipped']
            : last.error === STEP_LOST
              ? ['retrying', 'interrupted, retrying']
              : ['retrying', 'retrying'];
    if (state) item.append(element('span', `topic-status ${state}`, label));
    list.append(item);
  });
  return list;
}

// The plan can be edited until it is approved. Drafts survive re-renders.
const drafts = new Map<bigint, string[]>();
const MAX_TOPICS = 8;

function planEditor(dossier: { id: bigint; topics: string[] }): Node[] {
  const draft = drafts.get(dossier.id) ?? [...dossier.topics];
  drafts.set(dossier.id, draft);
  const rows = draft.map((topic, index) => {
    const row = element('div', 'plan-row');
    const input = element('input');
    input.value = topic;
    input.maxLength = 80;
    input.setAttribute('aria-label', `Topic ${index + 1}`);
    input.addEventListener('input', () => {
      draft[index] = input.value;
    });
    const remove = button('btn icon', '×', () => {
      draft.splice(index, 1);
      render();
    });
    remove.setAttribute('aria-label', 'Remove topic');
    row.append(input, remove);
    return row;
  });
  const actions = element('div', 'plan-actions');
  if (draft.length < MAX_TOPICS) {
    actions.append(
      button('btn small', '+ Add topic', () => {
        draft.push('');
        render();
        const inputs =
          document.querySelectorAll<HTMLInputElement>('.plan-row input');
        inputs[inputs.length - 1]?.focus();
      })
    );
  }
  actions.append(
    button('btn primary', 'Approve plan', () => {
      conn?.reducers
        .approvePlan({ dossierId: dossier.id, topics: draft })
        .then(() => drafts.delete(dossier.id))
        .catch(showError);
    })
  );
  return [...rows, actions];
}

// Turns [n] in the report into links to its numbered sources.
function citedText(
  text: string,
  sources: { title: string; url: string }[]
): HTMLElement {
  const report = element('p', 'report');
  for (const part of text.split(/(\[\d+\])/)) {
    const number = /^\[(\d+)\]$/.exec(part)?.[1];
    const source = number ? sources[Number(number) - 1] : undefined;
    if (!source) {
      report.append(part);
      continue;
    }
    const cite = link(part, source.url, 'cite');
    cite.title = source.title;
    report.append(cite);
  }
  return report;
}

function sourceList(sources: { title: string; url: string }[]): HTMLElement {
  const list = element('ol', 'report-sources');
  for (const source of sources) {
    const item = element('li');
    item.append(link(source.title, source.url));
    list.append(item);
  }
  return list;
}

function reportActions(
  dossier: {
    id: bigint;
    report?: string;
    reportSources: { title: string; url: string }[];
    reportedAt?: { microsSinceUnixEpoch: bigint };
    updateEverySeconds: number;
    nextUpdateAt?: { microsSinceUnixEpoch: bigint };
  },
  idle: boolean
): HTMLElement {
  const actions = element('div', 'report-actions');
  const copy = button('btn small', 'Copy', () => {
    const sources = dossier.reportSources
      .map((source, i) => `[${i + 1}] ${source.title} ${source.url}`)
      .join('\n');
    void navigator.clipboard
      .writeText(`${dossier.report}\n\nSources\n${sources}`)
      .then(() => {
        copy.textContent = 'Copied';
        setTimeout(() => (copy.textContent = 'Copy'), 1500);
      });
  });
  actions.append(copy);
  if (dossier.updateEverySeconds > 0) {
    actions.append(
      button('btn small', 'Stop updates', () => {
        conn?.reducers
          .cancelDossier({ dossierId: dossier.id })
          .catch(showError);
      })
    );
  } else if (idle) {
    actions.append(
      button('btn small', 'Update daily', () => {
        conn?.reducers
          .keepUpdated({
            dossierId: dossier.id,
            everySeconds: UPDATE_EVERY_SECONDS,
          })
          .catch(showError);
      })
    );
  }
  const notes: string[] = [];
  if (dossier.reportedAt) {
    notes.push(
      `Updated ${new Date(
        Number(dossier.reportedAt.microsSinceUnixEpoch / 1000n)
      ).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}`
    );
  }
  if (dossier.updateEverySeconds > 0 && dossier.nextUpdateAt) {
    notes.push(`next update in ${untilLabel(dossier.nextUpdateAt)}`);
  }
  if (notes.length > 0) {
    actions.append(element('span', 'report-meta', notes.join('  ·  ')));
  }
  return actions;
}

function renderMessages(): void {
  const wrap = $('msg-list');
  const dossier = [...conn!.db.myDossiers.iter()].find(
    row => row.id === activeId
  );
  $('chat-title').textContent = dossier?.question ?? 'New dossier';
  wrap.hidden = !dossier;
  $('empty-hero').hidden = Boolean(dossier);
  $('composer').hidden = Boolean(dossier);
  if (!dossier) {
    $('chat-actions').replaceChildren();
    return;
  }

  const run = runOf(dossier);
  const status = run?.status.tag;
  // The run also waits while its topics are researched in parallel.
  const waiting = run?.waitingFor === 'plan-approved';
  const running = isBusy(run);
  // Topics finish in any order. Show them in the plan's order.
  const notes = [...conn!.db.myNotes.iter()]
    .filter(row => row.dossierId === dossier.id)
    .sort(
      (a, b) =>
        dossier.topics.indexOf(a.topic) - dossier.topics.indexOf(b.topic)
    );
  const steps = [...conn!.db.mySteps.iter()]
    .filter(row => row.runId === dossier.runId)
    .sort((a, b) => Number(a.id - b.id));

  $('chat-actions').replaceChildren(
    ...(running || waiting
      ? [
          button('btn danger small', 'Cancel', () => {
            conn?.reducers
              .cancelDossier({ dossierId: dossier.id })
              .catch(showError);
          }),
        ]
      : [])
  );

  // An update round starts after its scheduling step.
  const roundStart =
    steps.findLast(row => row.step === 'Schedule the next update')?.id ?? 0n;
  const currentRound = steps.filter(row => row.id > roundStart);

  const nodes: HTMLElement[] = [
    message('user', 'You', document.createTextNode(dossier.question)),
  ];

  if (dossier.topics.length > 0) {
    nodes.push(
      message(
        'assistant',
        'Plan',
        ...(waiting
          ? planEditor(dossier)
          : [topicList(dossier.topics, running ? currentRound : undefined)])
      )
    );
  }

  const lost = steps.find(row => row.error === STEP_LOST);
  if (lost) {
    const kept = steps.filter(
      row => row.id < lost.id && row.status.tag === 'Ok'
    ).length;
    const tokens = notes
      .filter(
        note =>
          note.savedAt.microsSinceUnixEpoch < lost.ranAt.microsSinceUnixEpoch
      )
      .reduce((sum, note) => sum + note.tokens, dossier.planTokens);
    nodes.push(
      message(
        'tool',
        'Restart',
        document.createTextNode(
          `Resumed after a restart  ·  ${kept} finished steps kept  ·  ${tokens.toLocaleString()} tokens not spent again`
        )
      )
    );
  }
  for (const note of notes) {
    const body: Node[] = [
      element('p', undefined, note.summary ?? 'No sources found.'),
    ];
    if (note.sources.length > 0) {
      const sources = element('p', 'sources');
      note.sources.forEach((source, index) => {
        if (index > 0) sources.append(' · ');
        sources.append(link(source.title, source.url));
      });
      body.push(sources);
    }
    nodes.push(message('assistant', note.topic, ...body));
  }

  if (dossier.report) {
    nodes.push(
      message(
        'assistant',
        'Report',
        citedText(dossier.report, dossier.reportSources),
        sourceList(dossier.reportSources),
        reportActions(dossier, !running)
      )
    );
  }
  if (run?.error) {
    nodes.push(message('error', 'Error', document.createTextNode(run.error)));
  }

  if (status === 'Cancelled' && !dossier.report) {
    nodes.push(
      message(
        'tool',
        'Cancelled',
        document.createTextNode('This run was cancelled.')
      )
    );
  }

  if (steps.length > 0) {
    const details = element('details', 'toolcalls');
    details.open = openSteps.has(dossier.id);
    details.addEventListener('toggle', () => {
      if (details.open) openSteps.add(dossier.id);
      else openSteps.delete(dossier.id);
    });
    details.append(
      element('summary', undefined, `${steps.length} steps`),
      element('pre', undefined, stepLines(steps))
    );
    nodes.push(details);
  }

  if (running) {
    const typing = element('div', 'typing', 'researching');
    const dots = element('span', 'dots');
    dots.append(element('span'), element('span'), element('span'));
    typing.append(dots);
    nodes.push(typing);
  }

  if (
    wrap.contains(document.activeElement) &&
    document.activeElement?.tagName === 'INPUT'
  ) {
    return;
  }
  const wasNearBottom =
    wrap.scrollHeight - wrap.scrollTop - wrap.clientHeight < 100;
  const scrollTop = wrap.scrollTop;
  wrap.replaceChildren(...nodes);
  wrap.scrollTop = wasNearBottom ? wrap.scrollHeight : scrollTop;
}

function render(): void {
  if (!conn) return;
  $('shell').dataset.view = view;
  renderList();
  renderMessages();
}

function setConnected(connected: boolean): void {
  $<HTMLButtonElement>('btn-send').disabled = !connected;
  $('conn-status').hidden = connected;
}

function connect(config: ServerConfig): void {
  const retry = () => {
    setConnected(false);
    setTimeout(() => connect(config), RECONNECT_MILLIS);
  };
  // The token keeps your dossiers yours across reloads and server restarts.
  DbConnection.builder()
    .withUri(config.spacetimeUri)
    .withDatabaseName(config.databaseName)
    .withToken(localStorage.getItem(TOKEN_KEY) ?? undefined)
    .onConnect((current: DbConnection, _identity, token) => {
      localStorage.setItem(TOKEN_KEY, token);
      conn = current;
      setConnected(true);
      current.db.myDossiers.onInsert((_ctx, row) => {
        if (selectNext) {
          selectNext = false;
          activeId = row.id;
          view = 'chat';
        }
      });
      for (const table of [
        current.db.myDossiers,
        current.db.myNotes,
        current.db.myRuns,
        current.db.mySteps,
      ]) {
        table.onInsert(render);
        table.onUpdate(render);
        table.onDelete(render);
      }
      current
        .subscriptionBuilder()
        .onApplied(render)
        .onError((ctx: ErrorContext) => {
          console.error('subscription error:', ctx.event);
          showError('Subscription failed. Check the server console.');
        })
        .subscribe([
          tables.myDossiers,
          tables.myNotes,
          tables.myRuns,
          tables.mySteps,
        ]);
    })
    .onConnectError(retry)
    .onDisconnect(retry)
    .build();
}

function registerUiHandlers(): void {
  const input = $<HTMLTextAreaElement>('composer-input');
  const submit = () => {
    const question = input.value.trim();
    if (!question || !conn) return;
    selectNext = true;
    conn.reducers
      .startDossier({ question })
      .then(() => {
        input.value = '';
        input.style.height = 'auto';
      })
      .catch(error => {
        selectNext = false;
        showError(error);
      });
  };
  $('composer').addEventListener('submit', event => {
    event.preventDefault();
    submit();
  });
  input.addEventListener('input', () => {
    input.style.height = 'auto';
    input.style.height = `${Math.min(input.scrollHeight, 160)}px`;
  });
  input.addEventListener('keydown', event => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  });
  $('btn-new').addEventListener('click', () => {
    activeId = null;
    view = 'chat';
    render();
    input.focus();
  });
  $('btn-back').addEventListener('click', () => {
    view = 'list';
    render();
  });
  $('btn-toggle-sidebar').addEventListener('click', () => {
    const sidebar = $('sidebar');
    sidebar.classList.toggle('collapsed');
    $('btn-toggle-sidebar').textContent = sidebar.classList.contains(
      'collapsed'
    )
      ? '»'
      : '«';
  });
}

async function main(): Promise<void> {
  registerUiHandlers();
  const response = await fetch('/api/config');
  if (!response.ok) throw new Error(`/api/config returned ${response.status}`);
  connect((await response.json()) as ServerConfig);
}

main().catch(showError);
