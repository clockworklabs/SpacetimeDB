import './style.css';
import { DbConnection, tables } from './module_bindings';
import { presets, pythonCommand } from './presets';

const create = document.querySelector<HTMLButtonElement>('#create')!;
const run = document.querySelector<HTMLButtonElement>('#run')!;
const command = document.querySelector<HTMLTextAreaElement>('#command')!;
const error = document.querySelector<HTMLElement>('#error')!;
const list = document.querySelector<HTMLElement>('#sandboxes')!;
const connection = document.querySelector<HTMLElement>('#connection')!;
const output = document.querySelector<HTMLElement>('#output')!;
const outputStatus = document.querySelector<HTMLElement>('#output-status')!;
const retry = document.querySelector<HTMLButtonElement>('#retry-output')!;
let ready = false;
let busy = false;
let outputAttempt = '';
let outputRequest = 0;
let loadingOutput = false;
command.value = presets.report.code;

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
    document.querySelector('#grant')!.textContent =
      `spacetime call --server ${config.uri.replace(/^ws/, 'http')} ${config.database} grant_access '"${identity.toHexString()}"'`;
    ctx
      .subscriptionBuilder()
      .onApplied(() => {
        ready = true;
        connection.textContent = 'Connected';
        connection.hidden = true;
        render();
      })
      .onError(ctx => showError(ctx.event))
      .subscribe([tables.canManage, tables.mySandboxes, tables.myExecutions]);
  })
  .onConnectError((_ctx, cause) => {
    connection.textContent = 'Connection failed';
    connection.hidden = false;
    showError(cause);
  })
  .onDisconnect(() => {
    ready = false;
    connection.textContent = 'Disconnected';
    connection.hidden = false;
    render();
  })
  .build();

for (const [id, preset] of Object.entries(presets)) {
  document.querySelector<HTMLButtonElement>(`#load-${id}`)!.onclick = () => {
    command.value = preset.code;
    document.querySelector<HTMLDetailsElement>('#examples')!.open = false;
    render();
    command.focus();
  };
}
command.oninput = () => render();

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
  create.hidden = Boolean(active);
  run.hidden = !active;
  let runHint = '';
  if (!ready) runHint = 'Connect to SpacetimeDB to run Python.';
  else if (!allowed) runHint = 'Authorize this browser to run Python.';
  else if (!active) runHint = 'Create a sandbox to run Python.';
  else if (active.deleteRequested) runHint = 'Sandbox deletion is in progress.';
  else if (
    active.state.tag === 'Unknown' ||
    executions.some(
      row => row.sandboxId === active.id && row.state.tag === 'Unknown'
    )
  )
    runHint = 'The outcome is unknown. Check this sandbox in Daytona.';
  else if (active.state.tag !== 'Ready')
    runHint = `Sandbox is ${active.state.tag.toLowerCase()}. Wait for Ready.`;
  else if (
    executions.some(row => row.sandboxId === active.id && !row.finishedAt)
  )
    runHint = 'Wait for the current run to finish.';
  else if (!command.value.trim()) runHint = 'Enter Python code to run.';
  else if (new TextEncoder().encode(pythonCommand(command.value)).length > 4096)
    runHint = 'Code is too large. Shorten it before running.';
  run.disabled = busy || Boolean(runHint);
  document.querySelector('#run-hint')!.textContent =
    allowed && !active ? 'A 10-minute sandbox uses Daytona credits.' : runHint;
  list.replaceChildren();
  if (!sandboxes.length) list.append(node('p', 'No active sandbox.', 'empty'));
  for (const row of sandboxes.slice(0, 1)) {
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
      card.append(remove);
    }
    list.append(card);
  }
  const current = ready ? executions[0] : undefined;
  const parent = sandboxes.find(row => row.id === current?.sandboxId);
  const canLoad =
    allowed &&
    Boolean(
      current?.finishedAt &&
        current.commandId &&
        parent &&
        parent.state.tag !== 'Deleted' &&
        !parent.deleteRequested
    );
  retry.disabled = !canLoad || loadingOutput;
  retry.onclick = () => {
    if (canLoad && current) void loadOutput(current.id);
  };
  document.querySelector('#result-meta')!.textContent = current
    ? `${current.state.tag}${current.exitCode === undefined ? '' : ` · Exit ${current.exitCode}`}`
    : '';
  const attempt = `${current?.id}:${current?.state.tag}:${canLoad}`;
  if (outputAttempt !== attempt) {
    outputAttempt = attempt;
    outputRequest++;
    loadingOutput = false;
    retry.hidden = true;
    output.textContent = '';
    outputStatus.textContent = current
      ? (current.error ?? '')
      : 'Run Python to see output.';
    if (canLoad && current) void loadOutput(current.id);
    else if (current?.finishedAt) {
      if (!allowed)
        output.textContent =
          'Connect with an authorized identity to load output.';
      else if (parent?.state.tag === 'Deleted' || parent?.deleteRequested)
        output.textContent =
          'This sandbox was deleted or is being deleted. Its output is no longer available.';
      else output.textContent = 'This run has no command log to load.';
    }
  }
}

async function loadOutput(id: bigint) {
  if (loadingOutput) return;
  const request = ++outputRequest;
  loadingOutput = true;
  outputStatus.textContent = 'Loading output';
  retry.hidden = true;
  try {
    const result = await db.procedures.getCommandOutput({ executionId: id });
    if (request !== outputRequest) return;
    output.textContent = result.output || '(No output)';
    outputStatus.textContent = result.truncated
      ? 'First 16,384 characters (truncated)'
      : '';
  } catch (cause) {
    if (request !== outputRequest) return;
    outputStatus.textContent = 'Output unavailable';
    output.textContent = cause instanceof Error ? cause.message : String(cause);
    retry.hidden = false;
  } finally {
    if (request === outputRequest) {
      loadingOutput = false;
      render();
    }
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
        command: pythonCommand(command.value),
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
