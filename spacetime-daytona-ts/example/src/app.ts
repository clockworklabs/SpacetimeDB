import './style.css';
import { DbConnection, tables } from './module_bindings';

const create = document.querySelector<HTMLButtonElement>('#create')!;
const run = document.querySelector<HTMLButtonElement>('#run')!;
const command = document.querySelector<HTMLTextAreaElement>('#command')!;
const error = document.querySelector<HTMLElement>('#error')!;
const list = document.querySelector<HTMLElement>('#sandboxes')!;
const history = document.querySelector<HTMLElement>('#history')!;
const connection = document.querySelector<HTMLElement>('#connection')!;
let ready = false;
let busy = false;

function node<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  text: string,
  className = ''
) {
  const element = document.createElement(tag);
  element.textContent = text;
  element.className = className;
  return element;
}
function badge(state: string) {
  const element = node('span', state, 'badge');
  element.dataset.state = state;
  return element;
}
function showError(cause: unknown) {
  error.textContent = cause instanceof Error ? cause.message : String(cause);
  error.hidden = false;
}

const config: { uri: string; database: string } = await (
  await fetch('/api/config')
).json();
const tokenKey = `daytona:${config.uri}:${config.database}`;
const db = DbConnection.builder()
  .withUri(config.uri)
  .withDatabaseName(config.database)
  .withToken(localStorage.getItem(tokenKey) ?? undefined)
  .onConnect((ctx, identity, token) => {
    localStorage.setItem(tokenKey, token);
    document.querySelector('#identity')!.textContent =
      `Identity ${identity.toHexString().slice(0, 16)}`;
    document.querySelector('#grant')!.textContent =
      `spacetime call --server ${config.uri.replace(/^ws/, 'http')} ${config.database} grant_access '"${identity.toHexString()}"'`;
    ctx
      .subscriptionBuilder()
      .onApplied(() => {
        ready = true;
        connection.textContent = 'Connected';
        render();
      })
      .onError(ctx => showError(ctx.event))
      .subscribe([tables.canManage, tables.mySandboxes, tables.myExecutions]);
  })
  .onConnectError((_ctx, cause) => {
    connection.textContent = 'Connection failed';
    showError(cause);
  })
  .onDisconnect(() => {
    ready = false;
    connection.textContent = 'Disconnected';
    render();
  })
  .build();

function render() {
  const sandboxes = [...db.db.mySandboxes.iter()].sort((a, b) =>
    a.id > b.id ? -1 : 1
  );
  const active = sandboxes.find(row => row.state.tag !== 'Deleted');
  const allowed = ready && db.db.canManage.count() > 0;
  const executions = [...db.db.myExecutions.iter()].sort((a, b) =>
    a.id > b.id ? -1 : 1
  );
  document.querySelector<HTMLElement>('#access')!.hidden = !ready || allowed;
  create.disabled = !allowed || busy || Boolean(active);
  run.disabled =
    !allowed ||
    busy ||
    active?.state.tag !== 'Ready' ||
    active.deleteRequested ||
    executions.some(row => row.sandboxId === active.id && !row.finishedAt);
  list.replaceChildren();
  if (!sandboxes.length)
    list.append(
      node('p', 'No sandbox yet. Create one to run a command.', 'empty')
    );
  for (const row of sandboxes) {
    const card = node('article', '', 'sandbox');
    card.append(
      node('strong', `Sandbox ${row.id}`),
      node('p', row.remoteId ?? 'Waiting for Daytona', 'muted'),
      badge(
        row.deleteRequested && row.state.tag !== 'Deleted'
          ? 'Deleting'
          : row.state.tag
      )
    );
    if (row.expiresAt)
      card.append(
        node(
          'p',
          `Expires ${row.expiresAt.toDate().toLocaleTimeString()}`,
          'muted'
        )
      );
    if (row.error) card.append(node('p', row.error, 'muted'));
    if (row.state.tag !== 'Deleted') {
      const remove = node('button', 'Delete sandbox', 'danger');
      remove.disabled = busy || !allowed || row.deleteRequested;
      remove.onclick = () => {
        if (
          confirm('Delete this sandbox and its files? Running work will stop.')
        ) {
          void action(() => db.reducers.deleteSandbox({ sandboxId: row.id }));
        }
      };
      card.append(node('p', ''), remove);
    }
    list.append(card);
  }
  history.replaceChildren();
  document.querySelector<HTMLElement>('#empty-history')!.hidden =
    executions.length > 0;
  for (const row of executions) {
    const tr = document.createElement('tr');
    const status = node('td', '');
    status.append(badge(row.state.tag));
    if (row.error) status.append(node('p', row.error, 'muted'));
    tr.append(
      node('td', row.command),
      status,
      node(
        'td',
        row.exitCode === undefined
          ? row.finishedAt
            ? 'Unavailable'
            : 'Pending'
          : String(row.exitCode)
      )
    );
    history.append(tr);
  }
}

async function action(work: () => Promise<void>) {
  busy = true;
  error.hidden = true;
  render();
  try {
    await work();
  } catch (cause) {
    showError(cause);
  } finally {
    busy = false;
    render();
  }
}
create.onclick = () =>
  void action(() =>
    db.reducers.createSandbox({ requestKey: crypto.randomUUID() })
  );
document.querySelector('#run-form')!.addEventListener('submit', event => {
  event.preventDefault();
  if (run.disabled) return;
  const sandbox = [...db.db.mySandboxes.iter()].find(
    row => row.state.tag === 'Ready' && !row.deleteRequested
  );
  if (sandbox)
    void action(() =>
      db.reducers.runCommand({
        sandboxId: sandbox.id,
        requestKey: crypto.randomUUID(),
        command: command.value,
      })
    );
});
db.db.mySandboxes.onInsert(render);
db.db.mySandboxes.onUpdate(render);
db.db.mySandboxes.onDelete(render);
db.db.myExecutions.onInsert(render);
db.db.myExecutions.onUpdate(render);
db.db.myExecutions.onDelete(render);
db.db.canManage.onInsert(render);
db.db.canManage.onDelete(render);
