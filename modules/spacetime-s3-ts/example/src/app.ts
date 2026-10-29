import { DbConnection, tables, type ErrorContext } from './module_bindings/app';
import type { MyFile } from './module_bindings/app/types';

interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

type StatusTone = 'neutral' | 'success' | 'error';

const TOKEN_KEY = 'spacetime-s3-example-token';
// The module uses the submodule's default file size limit.
const MAX_FILE_BYTES = 100 * 1024 * 1024;
// 24px stroke icons, one path per array entry.
const ICONS = {
  download: ['M12 3v12', 'm7 10 5 5 5-5', 'M5 21h14'],
  delete: [
    'M3 6h18',
    'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6',
    'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
  ],
};
const ERROR_MESSAGES: Record<string, string> = {
  's3.quota_exceeded': 'You have reached your file or storage limit.',
  's3.invalid_metadata': 'This file name, type, or size is not accepted.',
  's3.upload_expired': 'The upload took too long. Try again.',
  's3.file_not_found': 'That file no longer exists.',
  's3.file_not_ready': 'That file is not ready yet.',
};

let connection: DbConnection | null = null;
// Upload progress by file ID, from 0 to 1. Only this tab knows it.
const progress = new Map<string, number>();
const busy = new Set<string>();
// Upload messages by file ID, shown until the file's row can show progress.
const uploadMessages = new Map<string, string>();
// Files this tab completed; screen readers hear when each becomes ready.
const verifying = new Set<string>();

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

function byId<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`Missing required element #${id}`);
  return node as T;
}

function errorMessage(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  const code = Object.keys(ERROR_MESSAGES).find(key => text.includes(key));
  return code ? ERROR_MESSAGES[code] : text;
}

function setStatus(text: string, tone: StatusTone = 'neutral'): void {
  const status = byId('status');
  status.textContent = text;
  status.dataset.tone = tone;
}

// Read by screen readers only; the file rows show the same steps visually.
function announce(text: string): void {
  byId('announce').textContent = text;
}

function setConnected(connected: boolean): void {
  byId<HTMLButtonElement>('upload').disabled = !connected;
}

function icon(name: keyof typeof ICONS): SVGSVGElement {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('aria-hidden', 'true');
  for (const d of ICONS[name]) {
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('d', d);
    svg.append(path);
  }
  return svg;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

function button(
  name: keyof typeof ICONS,
  label: string,
  onClick: () => Promise<void>,
  id: string
) {
  const node = element('button', `icon-button ${name}`);
  node.type = 'button';
  node.title = label;
  node.setAttribute('aria-label', label);
  node.append(icon(name));
  node.disabled = busy.has(id);
  node.onclick = async () => {
    busy.add(id);
    renderFiles();
    try {
      await onClick();
    } catch (error) {
      setStatus(errorMessage(error), 'error');
    } finally {
      busy.delete(id);
      renderFiles();
    }
  };
  return node;
}

function fileCard(file: MyFile): HTMLLIElement {
  // Only the uploading tab knows a pending file is still uploading.
  const uploaded = progress.get(file.id);
  const item = element('li', 'file-card');
  item.dataset.state = file.state;
  const main = element('div', 'file-main');
  const name = element('span', 'file-name', file.filename);
  // Long names are truncated; the tooltip shows the whole name.
  name.title = file.filename;
  main.append(name, element('span', 'file-meta', formatSize(file.size)));
  if (file.state === 'pending' && uploaded !== undefined) {
    const percent = Math.round(uploaded * 100);
    const bar = element('div', 'progress');
    bar.setAttribute('role', 'progressbar');
    bar.setAttribute('aria-label', `Uploading ${file.filename}`);
    bar.setAttribute('aria-valuenow', String(percent));
    const fill = element('span');
    fill.style.width = `${percent}%`;
    bar.append(fill);
    main.append(bar);
  } else if (file.state === 'pending') {
    main.append(element('span', 'sr-only', 'Upload not finished'));
  }
  const actions = element('div', 'file-actions');
  if (file.state === 'ready') {
    actions.append(
      button('download', 'Download', () => download(file), file.id),
      button('delete', 'Delete', () => remove(file), file.id)
    );
  } else if (file.state === 'copying' || file.state === 'deleting') {
    const spinner = element('span', 'spinner');
    spinner.setAttribute('role', 'img');
    spinner.setAttribute(
      'aria-label',
      file.state === 'copying' ? 'Verifying' : 'Deleting'
    );
    actions.append(spinner);
  } else if (uploaded !== undefined) {
    // The progress bar already gives screen readers this value.
    const percent = element(
      'span',
      'percent',
      `${Math.round(uploaded * 100)}%`
    );
    percent.setAttribute('aria-hidden', 'true');
    actions.append(percent);
  } else {
    actions.append(button('delete', 'Delete', () => remove(file), file.id));
  }
  item.append(main, actions);
  return item;
}

function renderFiles(): void {
  if (!connection) return;
  const files = [...connection.db.myFiles.iter()].sort((left, right) => {
    const leftTime = left.createdAt.microsSinceUnixEpoch;
    const rightTime = right.createdAt.microsSinceUnixEpoch;
    return leftTime === rightTime ? 0 : leftTime > rightTime ? -1 : 1;
  });
  byId('files').replaceChildren(...files.map(fileCard));
  byId('file-count').textContent = String(files.length);
  byId('files').hidden = files.length === 0;
  byId('files-empty').hidden = files.length > 0;
  for (const file of files) {
    clearUploadMessage(file.id);
    if (file.state === 'ready' && verifying.delete(file.id))
      announce(`${file.filename} is ready`);
  }
}

function clearUploadMessage(id: string): void {
  // Another action's message, such as an error, stays.
  if (byId('status').textContent === uploadMessages.get(id)) setStatus('');
  uploadMessages.delete(id);
}

async function sha256Base64(file: File): Promise<string> {
  const digest = await crypto.subtle.digest(
    'SHA-256',
    await file.arrayBuffer()
  );
  return btoa(String.fromCharCode(...new Uint8Array(digest)));
}

// XMLHttpRequest reports upload progress; fetch does not.
function post(
  url: string,
  form: FormData,
  onProgress: (fraction: number) => void
): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('POST', url);
    request.upload.onprogress = event => {
      if (event.lengthComputable) onProgress(event.loaded / event.total);
    };
    request.onload = () =>
      request.status === 204
        ? resolve()
        : reject(new Error(`S3 rejected the upload (${request.status}).`));
    request.onerror = () => reject(new Error('The upload could not reach S3.'));
    request.send(form);
  });
}

async function upload(file: File): Promise<void> {
  const current = connection;
  if (!current) return;
  // Check the size before hashing reads the whole file into memory.
  if (file.size > MAX_FILE_BYTES)
    throw new Error('Files must be 100 MB or smaller.');
  setStatus(`Preparing ${file.name}...`);
  const contentType = /^[\w!#$&^.+-]+\/[\w!#$&^.+-]+$/.test(file.type)
    ? file.type
    : 'application/octet-stream';
  const { id, upload: grant } = await current.procedures.beginUpload({
    requestId: crypto.randomUUID(),
    filename: file.name,
    contentType,
    size: file.size,
    checksum: await sha256Base64(file),
  });
  if (!grant) throw new Error('This upload is no longer open.');

  // S3 requires the policy fields before the file.
  const form = new FormData();
  for (const { name, value } of grant.fields) form.append(name, value);
  form.append('file', file);
  progress.set(id, 0);
  uploadMessages.set(id, `Uploading ${file.name}...`);
  setStatus(uploadMessages.get(id)!);
  renderFiles();
  // Keep the progress entry until completion, so the file never looks stalled.
  try {
    try {
      await post(grant.url, form, fraction => {
        progress.set(id, fraction);
        renderFiles();
      });
    } catch (error) {
      // Release the reservation so it stops counting against the quota.
      try {
        await current.reducers.deleteFile({ id });
      } catch {
        throw new Error(
          `${errorMessage(error)} Delete the file to free its quota.`
        );
      }
      throw error;
    }
    verifying.add(id);
    announce(`Verifying ${file.name}`);
    // A failed copy is retried by maintenance; the file's row shows its state.
    await current.procedures.completeUpload({ id });
  } finally {
    clearUploadMessage(id);
    progress.delete(id);
    renderFiles();
  }
  setStatus('');
}

async function download(file: MyFile): Promise<void> {
  const current = connection;
  if (!current) return;
  const { url } = await current.procedures.getDownloadUrl({ id: file.id });
  // The signed URL tells the browser to save the file under its original name.
  const link = element('a');
  link.href = url;
  link.click();
}

async function remove(file: MyFile): Promise<void> {
  const current = connection;
  if (!current) return;
  // Maintenance removes the objects; the row disappears when it finishes.
  await current.reducers.deleteFile({ id: file.id });
  setStatus('');
}

function registerUiHandlers(): void {
  const input = byId<HTMLInputElement>('file-input');
  byId<HTMLButtonElement>('upload').onclick = () => input.click();
  input.onchange = async () => {
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    try {
      await upload(file);
    } catch (error) {
      setStatus(errorMessage(error), 'error');
    }
  };
}

function readToken(): string | undefined {
  try {
    return localStorage.getItem(TOKEN_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

function saveToken(token: string): void {
  try {
    localStorage.setItem(TOKEN_KEY, token);
  } catch {
    // Without storage, this tab gets a new identity on every visit.
  }
}

async function loadServerConfig(): Promise<ServerConfig> {
  const response = await fetch('/api/config');
  if (!response.ok) {
    throw new Error(`Config request failed: ${response.status}`);
  }
  return (await response.json()) as ServerConfig;
}

function connect(config: ServerConfig): void {
  DbConnection.builder()
    .withUri(config.spacetimeUri)
    .withDatabaseName(config.databaseName)
    .withToken(readToken())
    .onConnect((current: DbConnection, _identity, token) => {
      saveToken(token);
      connection = current;
      setConnected(true);
      setStatus('');
      current.db.myFiles.onInsert(renderFiles);
      current.db.myFiles.onDelete(renderFiles);
      current
        .subscriptionBuilder()
        .onApplied(renderFiles)
        .onError((ctx: ErrorContext) =>
          console.error('subscription error', ctx.event)
        )
        .subscribe([tables.myFiles]);
    })
    .onConnectError((_ctx: ErrorContext, error: Error) => {
      setConnected(false);
      setStatus(error.message, 'error');
    })
    .onDisconnect((_ctx, error) => {
      connection = null;
      setConnected(false);
      setStatus(error?.message ?? 'Disconnected', 'error');
    })
    .build();
}

async function main(): Promise<void> {
  registerUiHandlers();
  setStatus('Connecting...');
  connect(await loadServerConfig());
}

void main().catch(error => {
  setStatus(errorMessage(error), 'error');
});
