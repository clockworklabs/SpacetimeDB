import '@spacetimedb/example-ui/styles.css';
import { DbConnection, tables, type ErrorContext } from './module_bindings/app';

interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

const TOKEN_KEY = 'dossier-token';
const RECONNECT_MILLIS = 2_000;
const STEP_LOST = 'workflow.step_lost';

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
  el.className = 'toast err show';
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

function runOf(dossier: { runId: bigint }) {
  return [...conn!.db.myRuns.iter()].find(row => row.id === dossier.runId);
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
      const status = runOf(dossier)?.status.tag;
      if (status === 'Running' || status === 'Waiting') {
        const dot = element('span', 'busy-dot');
        dot.title =
          runOf(dossier)?.waitingFor === 'plan-approved'
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

function topicList(topics: string[]): HTMLElement {
  const list = element('ol');
  list.append(...topics.map(topic => element('li', undefined, topic)));
  return list;
}

// The plan can be edited until it is approved. Drafts survive re-renders.
const drafts = new Map<bigint, string[]>();
const MAX_TOPICS = 4;

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
    row.append(
      input,
      button('btn icon', '×', () => {
        draft.splice(index, 1);
        render();
      })
    );
    row.lastElementChild!.setAttribute('aria-label', 'Remove topic');
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

function renderMessages(): void {
  const wrap = $('msg-list');
  const dossier = [...conn!.db.myDossiers.iter()].find(
    row => row.id === activeId
  );
  $('chat-title').textContent = dossier?.question ?? 'New dossier';
  $('msg-list').hidden = !dossier;
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
  const running = status === 'Running' || (status === 'Waiting' && !waiting);
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

  const nodes: HTMLElement[] = [
    message('user', 'You', document.createTextNode(dossier.question)),
  ];

  if (dossier.topics.length > 0) {
    nodes.push(
      message(
        'assistant',
        'Plan',
        ...(waiting ? planEditor(dossier) : [topicList(dossier.topics)])
      )
    );
  }

  // Steps that finished before the first lost attempt, and what they cost.
  const lost = steps.find(row => row.error === STEP_LOST);
  const lostAt = lost?.ranAt.microsSinceUnixEpoch;
  const notice = () => {
    const kept = steps.filter(
      row => row.id < lost!.id && row.status.tag === 'Ok'
    ).length;
    const tokens = notes
      .filter(note => note.savedAt.microsSinceUnixEpoch < lostAt!)
      .reduce((sum, note) => sum + note.tokens, dossier.planTokens);
    return message(
      'tool',
      'Restart',
      document.createTextNode(
        `Resumed after a restart  ·  ${kept} finished steps kept  ·  ${tokens.toLocaleString()} tokens not spent again`
      )
    );
  };

  if (lost) nodes.push(notice());
  for (const note of notes) {
    const body: Node[] = [
      element('p', undefined, note.summary ?? 'No sources found.'),
    ];
    if (note.sources.length > 0) {
      const sources = element('p', 'sources');
      note.sources.forEach((source, index) => {
        if (index > 0) sources.append(' · ');
        const link = element('a', undefined, source.title);
        link.href = source.url;
        link.target = '_blank';
        link.rel = 'noopener noreferrer';
        sources.append(link);
      });
      body.push(sources);
    }
    nodes.push(message('assistant', note.topic, ...body));
  }

  if (dossier.report) {
    const report = element('p', 'report', dossier.report);
    nodes.push(message('assistant', 'Report', report));
  }
  if (run?.error) {
    nodes.push(message('error', 'Error', document.createTextNode(run.error)));
  }

  if (status === 'Cancelled') {
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
