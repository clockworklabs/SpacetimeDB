// SpacetimeDB connection and file-manager UI composition.
import { DbConnection, tables, type ErrorContext } from './codegen/app';
import type { FileSummary, Folder } from './codegen/app/types';
import {
  loadToken,
  saveToken,
  clearToken,
  parentPath,
  baseName,
  joinPath,
  childPrefix,
  fileUrl,
  fmtSize,
  tsMs,
  escapeHtml,
  humanError,
  type Visibility,
  type ServerConfig,
} from './utils';
import {
  downloadArchive,
  downloadFile as saveDownloadedFile,
  getFileBlob as loadFileBlob,
} from './downloads';
import { zipStamp } from './zip';
import { FileViewer } from './viewer';
import { DialogController } from './dialog';
import { collectDropped, UploadController } from './uploads';
import { ContextMenu } from './context-menu';
import { bindListActions as bindListInteractions } from './list-actions';
import { handleListKey } from './keyboard';
import {
  uploadDropped,
  wireFolderDropTarget as wireDropTarget,
} from './drop-target';
import {
  createVaultRendering,
  fileDetailsHtml,
  folderDetailsHtml,
  icon,
  selectionDetailsHtml,
  type SortKey,
} from './rendering';
import { VaultSelection } from './selection';

let conn: DbConnection | null = null;
let authToken: string | undefined = loadToken();

async function loadConfig(): Promise<ServerConfig> {
  const res = await fetch('/api/config');
  if (!res.ok) throw new Error(`/api/config returned ${res.status}`);
  return (await res.json()) as ServerConfig;
}

function connect(config: ServerConfig): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(config.stdbUri)
      .withDatabaseName(config.appDatabase)
      .withToken(authToken)
      .onConnect((c, _identity, token) => {
        authToken = token;
        saveToken(token);
        resolve(c);
      })
      .onDisconnect((_ctx, err) => {
        conn = null;
        toast(
          'err',
          err?.message ? `Connection lost: ${err.message}` : 'Connection lost'
        );
      })
      .onConnectError((_ctx, err) => {
        reject(err);
      })
      .build();
  });
}

// The bridge the UI talks to; also exposed as window.vault for console tinkering.
const vault = {
  createFolder: (path: string) => conn!.reducers.createFolder({ path }),
  deleteFolder: (path: string) => conn!.reducers.deleteFolder({ path }),
  uploadFile: (args: {
    path: string;
    mimeType: string;
    bytes: Uint8Array;
    visibility: Visibility;
  }) => conn!.reducers.uploadFile(args),
  deleteFile: (path: string) => conn!.reducers.deleteFile({ path }),
  renameFile: (oldPath: string, newPath: string) =>
    conn!.reducers.renameFile({ oldPath, newPath }),
  renameFolder: (path: string, newName: string) =>
    conn!.reducers.renameFolder({ path, newName }),
  moveFile: (oldPath: string, targetFolderPath: string) =>
    conn!.reducers.moveFile({ oldPath, targetFolderPath }),
  setFileVisibility: (path: string, visibility: Visibility) =>
    conn!.reducers.setFileVisibility({ path, visibility }),
  /** Reads file bytes over the authenticated connection (works for private files). */
  readFileBytes: async (
    path: string
  ): Promise<{ bytes: Uint8Array; mimeType: string }> => {
    const result = await conn!.procedures.readFileBytes({ path });
    // Bytes may arrive as a plain number[] over the wire; normalize.
    return { bytes: new Uint8Array(result.bytes), mimeType: result.mimeType };
  },
  getToken: () => authToken,
};

declare global {
  interface Window {
    vault?: typeof vault;
  }
}

function requireVault(): typeof vault | null {
  if (!conn) {
    toast('err', 'Vault is not connected yet.');
    return null;
  }
  return vault;
}

// UI state

const $ = <T extends HTMLElement = HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

// Persisted view preferences
const PREFS_KEY = 'vault:prefs';
interface Prefs {
  viewMode?: 'list' | 'grid';
  tileSize?: number | 's' | 'm' | 'l';
  sortKey?: SortKey;
  sortDir?: 1 | -1;
  detailsOpen?: boolean;
}
function loadPrefs(): Prefs {
  try {
    return (
      (JSON.parse(localStorage.getItem(PREFS_KEY) ?? 'null') as Prefs) ?? {}
    );
  } catch {
    return {};
  }
}
function savePrefs(): void {
  try {
    localStorage.setItem(
      PREFS_KEY,
      JSON.stringify({ viewMode, tileSize, sortKey, sortDir, detailsOpen })
    );
  } catch {
    /* ignore */
  }
}
const prefs = loadPrefs();

let folders: Folder[] = [];
let files: FileSummary[] = [];
let currentPath = '/';
let uploading = false;
let dragDepth = 0;
let sortKey: SortKey = prefs.sortKey ?? 'name';
let sortDir: 1 | -1 = prefs.sortDir ?? 1;
let viewMode: 'list' | 'grid' = prefs.viewMode ?? 'list';
// Normalize stored tile-size aliases to pixels.
let tileSize: number =
  typeof prefs.tileSize === 'number'
    ? prefs.tileSize
    : ({ s: 110, m: 150, l: 205 }[prefs.tileSize ?? 'm'] ?? 150);
let detailsOpen: boolean = prefs.detailsOpen ?? false;
let searchQuery = '';
const selection = new VaultSelection();
const selected = selection.selected;

const {
  allFolderPaths,
  fileRowHtml,
  fileTileHtml,
  folderRowHtml,
  folderTileHtml,
  immediateFolders,
  subtreeStats,
  visibleEntries,
} = createVaultRendering(() => ({
  folders,
  files,
  currentPath,
  searchQuery,
  sortKey,
  sortDir,
  selected,
  focusPath: selection.focusPath,
}));

// Returns the candidate path when available, otherwise adds a numeric suffix.
function freeName(candidate: string): string {
  const taken = (p: string) =>
    files.some(f => f.path === p) || folders.some(f => f.path === p);
  if (!taken(candidate)) return candidate;
  const dir = parentPath(candidate);
  const base = baseName(candidate);
  const dot = base.lastIndexOf('.');
  const stem = dot > 0 ? base.slice(0, dot) : base;
  const ext = dot > 0 ? base.slice(dot) : '';
  for (let k = 1; k < 1000; k++) {
    const next = joinPath(dir, `${stem} (${k})${ext}`);
    if (!taken(next)) return next;
  }
  return candidate;
}

function toast(kind: 'ok' | 'err', message: string): void {
  const el = $('toast');
  const node = document.createElement('div');
  node.className = `toast ${kind}`;
  node.textContent = message;
  el.appendChild(node);
  setTimeout(() => node.remove(), 3600);
}

const dialogs = new DialogController(error => toast('err', humanError(error)));
const openDialog = dialogs.open;
const closeDialog = dialogs.close;
const commitDialog = dialogs.commit;
const confirmDialog = dialogs.confirm.bind(dialogs);

const uploads = new UploadController({
  ready: () => requireVault() != null,
  files: () => files,
  createFolder: path => vault.createFolder(path),
  uploadFile: args => vault.uploadFile(args),
  freeName,
  openDialog,
  setProgress: (next, label) => {
    uploading = next;
    $<HTMLButtonElement>('upload').disabled = next;
    $('upload-label').textContent = next && label ? label : 'Upload';
  },
  toast,
});

const contextMenu = new ContextMenu({
  file: path => files.find(file => file.path === path),
  preview: path => void openViewer(path),
  viewDetails: path => {
    setFocus(path);
    if (!detailsOpen) {
      detailsOpen = true;
      savePrefs();
    }
    render();
  },
  downloadFile: file => void downloadFile(file),
  copyLink,
  duplicateFile: path => void duplicateFile(path),
  renameFile: openRename,
  moveFile: path => openMove([path]),
  toggleVisibility: path => void toggleVisibility(path),
  deleteFile: confirmDeleteFile,
  openFolder: path => {
    currentPath = path;
    selection.clearFocus();
    clearSearch();
    render();
  },
  downloadFolder: path => void downloadFolderZip(path),
  renameFolder: openRenameFolder,
  deleteFolder: confirmDeleteFolder,
  newFolder: openNewFolder,
  chooseFiles: () => $('file-input').click(),
  chooseFolder: () => $('folder-input').click(),
});
const openCtxMenu = contextMenu.open;
const closeCtxMenu = contextMenu.close;

const downloadServices = {
  readFileBytes: vault.readFileBytes,
  toast,
};

function getFileBlob(row: FileSummary): Promise<Blob> {
  return loadFileBlob(row, downloadServices);
}

// Thumbnails (grid view + details panel)

const thumbCache = new Map<string, string>();
let thumbGeneration = 0;
// Object-URL cache keyed by path@mtime; older revisions revoked on refresh.
async function getThumbUrl(row: FileSummary): Promise<string> {
  const key = `${row.path}@${tsMs(row.updatedAt)}`;
  const cached = thumbCache.get(key);
  if (cached) return cached;
  const url = URL.createObjectURL(await getFileBlob(row));
  for (const [k, v] of thumbCache) {
    if (k.startsWith(row.path + '@') && k !== key) {
      URL.revokeObjectURL(v);
      thumbCache.delete(k);
    }
  }
  thumbCache.set(key, url);
  return url;
}
async function loadThumbs(): Promise<void> {
  const gen = ++thumbGeneration;
  const slots = [...document.querySelectorAll<HTMLElement>('[data-thumb]')];
  for (const slot of slots) {
    if (gen !== thumbGeneration) return; // a newer render superseded us
    const row = files.find(f => f.path === slot.dataset.thumb);
    if (!row) continue;
    let url: string;
    try {
      url = await getThumbUrl(row);
    } catch {
      continue;
    }
    if (gen !== thumbGeneration) return;
    const img = document.createElement('img');
    img.src = url;
    img.alt = '';
    slot.replaceChildren(img);
  }
}

// Rendering

function renderCrumbs(): void {
  if (searchQuery) {
    $('crumbs').innerHTML =
      `<span class="search-label">${icon('search')} Search results</span>`;
    return;
  }
  const parts =
    currentPath === '/' ? [] : currentPath.split('/').filter(Boolean);
  let acc = '';
  const html = [`<button data-cd="/">Root</button>`];
  for (const part of parts) {
    acc += '/' + part;
    html.push(
      `<span class="sep">/</span><button data-cd="${escapeHtml(acc)}">${escapeHtml(part)}</button>`
    );
  }
  $('crumbs').innerHTML = html.join('');
  $('crumbs')
    .querySelectorAll<HTMLElement>('[data-cd]')
    .forEach(btn => {
      btn.addEventListener('click', () => {
        currentPath = btn.dataset.cd!;
        clearSearch();
        render();
      });
    });
}
function renderTree(): void {
  const rows: Array<{ path: string; name: string; depth: number }> = [
    { path: '/', name: 'Root', depth: 0 },
  ];
  (function walk(path: string, depth: number) {
    for (const f of immediateFolders(path).sort((a, b) =>
      a.name.localeCompare(b.name)
    )) {
      rows.push({ path: f.path, name: f.name, depth });
      walk(f.path, depth + 1);
    }
  })('/', 1);
  $('tree').innerHTML = rows
    .map(
      row => `
    <li>
      <button class="tree-btn ${row.path === currentPath && !searchQuery ? 'active' : ''}" data-path="${escapeHtml(row.path)}" style="padding-left:${10 + row.depth * 15}px">
        ${icon('folder')}<span>${escapeHtml(row.name)}</span>
      </button>
    </li>`
    )
    .join('');
  $('tree')
    .querySelectorAll<HTMLElement>('[data-path]')
    .forEach(btn => {
      btn.addEventListener('click', () => {
        currentPath = btn.dataset.path!;
        clearSearch();
        render();
      });
      btn.addEventListener('contextmenu', e =>
        openCtxMenu(e, { type: 'folder', path: btn.dataset.path! })
      );
      wireFolderDropTarget(btn, btn.dataset.path!);
    });
}
function renderHead(): void {
  $('list-head').classList.toggle('grid-mode', viewMode === 'grid');
  $('list-head')
    .querySelectorAll<HTMLElement>('[data-sort]')
    .forEach(btn => {
      const active = btn.dataset.sort === sortKey;
      btn.classList.toggle('sorted', active);
      const label =
        btn.dataset.label ?? (btn.dataset.label = btn.textContent!.trim());
      btn.textContent = active ? `${label} ${sortDir > 0 ? '^' : 'v'}` : label;
    });
  const { fs } = visibleEntries();
  const all = fs.length > 0 && fs.every(f => selected.has(f.path));
  $<HTMLInputElement>('select-all').checked = all;
  // View controls
  $('view-toggle').innerHTML = icon(viewMode === 'grid' ? 'list' : 'grid');
  $('view-toggle').title = viewMode === 'grid' ? 'List view' : 'Grid view';
  $('zoom-ctl').hidden = viewMode !== 'grid';
  $<HTMLInputElement>('tile-slider').value = String(tileSize);
}
function renderList(): void {
  const { dirs, fs } = visibleEntries();
  selection.setEntries([
    ...dirs.map(f => ({ type: 'folder' as const, path: f.path })),
    ...fs.map(f => ({ type: 'file' as const, path: f.path })),
  ]);
  const isEmpty = selection.entries.length === 0;
  $('empty').hidden = !isEmpty;
  if (isEmpty) {
    $('empty').innerHTML = searchQuery
      ? `<div class="empty">${icon('search')}<strong>No matches</strong><div>Nothing named "${escapeHtml(searchQuery)}".</div></div>`
      : `<div class="empty">${icon('upload')}<strong>This folder is empty</strong><div>Drag files anywhere on the page, or hit Upload.</div></div>`;
  }
  const list = $('list');
  list.classList.toggle('grid', viewMode === 'grid');
  list.classList.toggle('has-selection', selected.size > 0);
  list.style.setProperty('--tile', `${tileSize}px`);
  list.innerHTML =
    viewMode === 'grid'
      ? [...dirs.map(folderTileHtml), ...fs.map(fileTileHtml)].join('')
      : [...dirs.map(folderRowHtml), ...fs.map(fileRowHtml)].join('');
  bindListActions();
  if (viewMode === 'grid') void loadThumbs();
}
function renderBulkbar(): void {
  const n = selected.size;
  $('bulkbar').hidden = n === 0;
  $('toolbar-main').style.display = n === 0 ? '' : 'none';
  if (n) $('bulk-count').textContent = `${n} selected`;
}
function renderStorage(): void {
  const total = files.reduce((sum, f) => sum + Number(f.size), 0);
  $('storage').textContent = files.length
    ? `${files.length} file${files.length === 1 ? '' : 's'} | ${fmtSize(total)} stored`
    : 'No files stored yet';
}

// Details panel
function wireDetailsNav(scope: HTMLElement): void {
  scope.querySelectorAll<HTMLElement>('[data-goto]').forEach(btn =>
    btn.addEventListener('click', () => {
      currentPath = btn.dataset.goto!;
      clearSearch();
      selected.clear();
      render();
    })
  );
}
async function loadDetailsThumb(row: FileSummary): Promise<void> {
  const slot = document.querySelector<HTMLElement>(
    `[data-dthumb="${CSS.escape(row.path)}"]`
  );
  if (!slot) return;
  let url: string;
  try {
    url = await getThumbUrl(row);
  } catch {
    return;
  }
  if (!document.body.contains(slot)) return;
  const img = document.createElement('img');
  img.src = url;
  img.alt = '';
  slot.replaceChildren(img);
}
function renderFileDetails(body: HTMLElement, row: FileSummary): void {
  const isImage = (row.mimeType || '').startsWith('image/');
  body.innerHTML = fileDetailsHtml(row);
  wireDetailsNav(body);
  if (isImage) void loadDetailsThumb(row);
}
function renderFolderDetails(body: HTMLElement, folderPath: string): void {
  const isRoot = folderPath === '/';
  const row = isRoot ? null : folders.find(f => f.path === folderPath);
  body.innerHTML = folderDetailsHtml(
    folderPath,
    row ?? undefined,
    subtreeStats(folderPath)
  );
  wireDetailsNav(body);
}
function renderDetails(): void {
  document
    .querySelector('.main')!
    .classList.toggle('details-open', detailsOpen);
  $('details-toggle').classList.toggle('active', detailsOpen);
  if (!detailsOpen) return;
  const body = $('details-body');
  if (selected.size > 1) {
    const rows = files.filter(f => selected.has(f.path));
    body.innerHTML = selectionDetailsHtml(rows);
    return;
  }
  if (selected.size === 1) {
    const row = files.find(f => f.path === [...selected][0]);
    if (row) return renderFileDetails(body, row);
  }
  if (selection.focusPath) {
    const row = files.find(f => f.path === selection.focusPath);
    if (row) return renderFileDetails(body, row);
    if (folders.some(f => f.path === selection.focusPath))
      return renderFolderDetails(body, selection.focusPath);
  }
  // Nothing focused or selected: summarize the current folder (or root).
  renderFolderDetails(body, currentPath);
}
function render(): void {
  if (currentPath !== '/' && !folders.some(f => f.path === currentPath))
    currentPath = '/';
  renderCrumbs();
  renderTree();
  renderHead();
  renderList();
  renderBulkbar();
  renderStorage();
  renderDetails();
}

// Selection & focus

function setFocus(path: string): void {
  // No re-render if already focused: dblclick's second click must hit the same node.
  if (selection.focus(path)) render();
}
function toggleSelect(path: string): void {
  selection.toggle(path);
  render();
}
function rangeSelect(path: string): void {
  selection.selectRange(path);
  render();
}

function bindListActions(): void {
  bindListInteractions($('list'), {
    selected,
    files: () => files,
    setAnchor: path => selection.setAnchor(path),
    toggleSelect,
    rangeSelect,
    focus: setFocus,
    openFile: path => void openViewer(path),
    openFolder: path => {
      currentPath = path;
      selection.clearFocus();
      clearSearch();
      render();
    },
    openContext: openCtxMenu,
    render,
    toggleVisibility: path => void toggleVisibility(path),
    copyLink,
    downloadFile: file => void downloadFile(file),
    downloadFolder: path => void downloadFolderZip(path),
    renameFile: openRename,
    renameFolder: openRenameFolder,
    moveFile: path => openMove([path]),
    deleteFile: confirmDeleteFile,
    deleteFolder: confirmDeleteFolder,
    wireFolderDropTarget,
  });
}

function toggleVisibility(path: string): Promise<void> | void {
  const row = files.find(f => f.path === path);
  const v = requireVault();
  if (!row || !v) return;
  return runAction('Visibility updated', () =>
    v.setFileVisibility(path, row.visibility === 'public' ? 'owner' : 'public')
  );
}
function confirmDeleteFile(path: string): void {
  confirmDialog(
    'Delete file',
    `Delete "${baseName(path)}"? This can't be undone.`,
    async () => {
      await runAction('File deleted', () => vault.deleteFile(path));
      if (viewer.path === path) closeViewer();
    }
  );
}
function confirmDeleteFolder(path: string): void {
  confirmDialog(
    'Delete folder',
    `Delete folder "${baseName(path)}"? It must be empty.`,
    () => runAction('Folder deleted', () => vault.deleteFolder(path))
  );
}

// Internal drag-to-move + OS-file drop onto folders

function wireFolderDropTarget(el: HTMLElement, folderPath: string): void {
  wireDropTarget(el, folderPath, {
    currentPath: () => currentPath,
    endFileDrag,
    upload: (dataTransfer, path) =>
      uploadDropped(dataTransfer, path, (entries, target) =>
        uploads.upload(entries, target)
      ),
    move: moveFiles,
  });
}
async function moveFiles(paths: string[], targetFolder: string): Promise<void> {
  const v = requireVault();
  if (!paths.length || !v) return;
  const toMove = paths.filter(p => parentPath(p) !== targetFolder);
  await bulkOp(toMove, p => v.moveFile(p, targetFolder), 'moved');
  selected.clear();
  // No-op moves produce no data event, so sync the bulk bar here.
  render();
}

function openNewFolder(): void {
  openDialog(
    'New folder',
    `<label>Name<input id="folder-name" placeholder="docs" autocomplete="off" /></label>`,
    async () => {
      const name = $<HTMLInputElement>('folder-name').value.trim();
      if (!name) throw new Error('vault.invalid_path:name');
      await vault.createFolder(joinPath(currentPath, name));
      toast('ok', 'Folder created');
    }
  );
}
function openRename(path: string): void {
  openDialog(
    'Rename file',
    `<label>Name<input id="rename-name" value="${escapeHtml(baseName(path))}" autocomplete="off" /></label>`,
    async () => {
      const name = $<HTMLInputElement>('rename-name').value.trim();
      if (!name) throw new Error('vault.invalid_file_path');
      await vault.renameFile(path, joinPath(parentPath(path), name));
      toast('ok', 'File renamed');
    }
  );
}
function openRenameFolder(path: string): void {
  openDialog(
    'Rename folder',
    `<label>Name<input id="rename-name" value="${escapeHtml(baseName(path))}" autocomplete="off" /></label>`,
    async () => {
      const name = $<HTMLInputElement>('rename-name').value.trim();
      if (!name) throw new Error('vault.invalid_path:name');
      await vault.renameFolder(path, name);
      // Follow a rename within the active subtree.
      const newPath = joinPath(parentPath(path), name);
      if (currentPath === path) currentPath = newPath;
      else if (currentPath.startsWith(childPrefix(path)))
        currentPath = newPath + currentPath.slice(path.length);
      toast('ok', 'Folder renamed');
    }
  );
}
function openMove(paths: string[]): void {
  const from = paths.length === 1 ? parentPath(paths[0]!) : null;
  openDialog(
    paths.length === 1 ? 'Move file' : `Move ${paths.length} files`,
    `
    <label>Destination folder
      <select id="move-target">
        ${allFolderPaths()
          .map(
            p =>
              `<option value="${escapeHtml(p)}" ${p === from ? 'selected' : ''}>${escapeHtml(p)}</option>`
          )
          .join('')}
      </select>
    </label>`,
    () => moveFiles(paths, $<HTMLSelectElement>('move-target').value)
  );
}

// Copy link (visibility-aware)

async function writeClipboard(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}
function copyLink(path: string): void {
  const row = files.find(f => f.path === path);
  if (!row) return;
  const url = location.origin + fileUrl(row.id);
  if (row && row.visibility === 'public') {
    void writeClipboard(url)
      .then(() => toast('ok', 'Public link copied'))
      .catch(() => toast('err', 'Copy failed. Select the URL manually.'));
    return;
  }
  // A private link is dead even for the owner (HTTP has no caller identity).
  openDialog(
    'Copy link',
    `<p>This file is <strong>private</strong>. Public links require public visibility. Make it public and copy the link?</p>`,
    async () => {
      await vault.setFileVisibility(path, 'public');
      try {
        await writeClipboard(url);
        toast('ok', 'File made public, link copied');
      } catch {
        toast('err', 'File made public, but the link could not be copied.');
      }
    },
    { okLabel: 'Make public & copy' }
  );
}

// Actions

async function runAction(
  okMessage: string,
  fn: () => Promise<unknown>
): Promise<void> {
  if (!requireVault()) return;
  try {
    await fn();
    toast('ok', okMessage);
  } catch (err) {
    toast('err', humanError(err));
  }
}
async function duplicateFile(path: string): Promise<void> {
  const row = files.find(f => f.path === path);
  const v = requireVault();
  if (!row || !v) return;
  try {
    const { bytes, mimeType } = await v.readFileBytes(path);
    const base = baseName(path);
    const dot = base.lastIndexOf('.');
    const copyName =
      dot > 0
        ? `${base.slice(0, dot)} (copy)${base.slice(dot)}`
        : `${base} (copy)`;
    const target = freeName(joinPath(parentPath(path), copyName));
    await v.uploadFile({
      path: target,
      mimeType,
      bytes,
      visibility: row.visibility as Visibility,
    });
    toast('ok', `Copied to ${baseName(target)}`);
  } catch (err) {
    toast('err', humanError(err));
  }
}

// Upload (files, folders, conflicts)

async function downloadFile(row: FileSummary): Promise<void> {
  return saveDownloadedFile(row, downloadServices);
}

async function zipAndSave(
  fileRows: FileSummary[],
  dirNames: Array<{ name: string; mtimeMs: number }>,
  entryName: (f: FileSummary) => string,
  zipName: string
): Promise<void> {
  return downloadArchive(
    { fileRows, dirNames, entryName, zipName },
    downloadServices
  );
}
function downloadFolderZip(folderPath: string): Promise<void> {
  const prefix = childPrefix(folderPath);
  // Name the root archive explicitly to avoid paths that start with "//".
  const root = folderPath === '/' ? 'vault' : baseName(folderPath);
  const inFiles = files.filter(f => f.path.startsWith(prefix));
  // Directory entries preserve empty folders inside the zip.
  const inDirs = folders
    .filter(f => f.path !== folderPath && f.path.startsWith(prefix))
    .map(f => ({
      name: `${root}/${f.path.slice(prefix.length)}/`,
      mtimeMs: tsMs(f.updatedAt),
    }));
  return zipAndSave(
    inFiles,
    inDirs,
    f => `${root}/${f.path.slice(prefix.length)}`,
    `${root}-${zipStamp()}.zip`
  );
}
function downloadSelectionZip(): Promise<void> {
  const rows = files.filter(f => selected.has(f.path));
  // Keep full vault paths so structure survives a mixed selection.
  return zipAndSave(
    rows,
    [],
    f => f.path.slice(1),
    `vault-download-${zipStamp()}.zip`
  );
}

const viewer = new FileViewer({
  loadBlob: getFileBlob,
  download: downloadFile,
  iconHtml: icon,
});

function viewerOpen(): boolean {
  return viewer.isOpen();
}
function openViewer(path: string): Promise<void> {
  return viewer.open(path, visibleEntries().fs);
}
function vStep(delta: number): void {
  viewer.step(delta);
}
function closeViewer(): void {
  viewer.close();
}

// Search

function clearSearch(): void {
  if (!searchQuery) return;
  searchQuery = '';
  $<HTMLInputElement>('search').value = '';
}

// Bulk actions

// Continue-on-error loop: one summary toast, one toast per failure.
async function bulkOp(
  paths: string[],
  op: (path: string) => Promise<unknown>,
  okVerb: string
): Promise<number> {
  let done = 0;
  const failures: string[] = [];
  for (const p of paths) {
    try {
      await op(p);
      done++;
    } catch (err) {
      failures.push(`${baseName(p)}: ${humanError(err)}`);
    }
  }
  if (done) toast('ok', `${done} file${done === 1 ? '' : 's'} ${okVerb}`);
  for (const msg of failures) toast('err', msg);
  return done;
}
function bulkDelete(): void {
  const paths = [...selected];
  if (!paths.length) return;
  confirmDialog(
    `Delete ${paths.length} file${paths.length === 1 ? '' : 's'}`,
    `Delete ${paths.length} file${paths.length === 1 ? '' : 's'}? This can't be undone.`,
    async () => {
      await bulkOp(paths, p => vault.deleteFile(p), 'deleted');
      selected.clear();
      if (viewer.path && paths.includes(viewer.path)) closeViewer();
    }
  );
}
async function bulkVisibility(visibility: Visibility): Promise<void> {
  // Visibility changes preserve the current selection because files stay in place.
  await bulkOp(
    [...selected],
    p => vault.setFileVisibility(p, visibility),
    `made ${visibility === 'public' ? 'public' : 'private'}`
  );
}

function handleListKeys(e: KeyboardEvent): void {
  handleListKey(e, {
    entries: () => selection.entries,
    focusPath: () => selection.focusPath,
    selected,
    visibleFilePaths: () => visibleEntries().fs.map(file => file.path),
    setFocus,
    clearFocus: () => {
      selection.clearFocus();
    },
    openFolder: path => {
      currentPath = path;
      selection.clearFocus();
      clearSearch();
      render();
    },
    openFile: path => {
      selection.setAnchor(path);
      void openViewer(path);
    },
    toggleSelect,
    deleteSelection: bulkDelete,
    deleteFile: confirmDeleteFile,
    deleteFolder: confirmDeleteFolder,
    hasSearch: () => Boolean(searchQuery),
    clearSearch,
    render,
  });
}

// One-time wiring

function endFileDrag(): void {
  dragDepth = 0;
  document.body.classList.remove('dragging-files');
}

function wireUi(): void {
  $('new-folder').addEventListener('click', openNewFolder);
  $('upload').addEventListener('click', () => {
    if (!uploading) $('file-input').click();
  });
  $<HTMLInputElement>('file-input').addEventListener('change', e => {
    void (async () => {
      const input = e.target as HTMLInputElement;
      const picked = [...(input.files ?? [])];
      input.value = '';
      if (picked.length)
        await uploads.upload(
          { files: picked.map(f => ({ file: f, rel: f.name })), dirs: [] },
          currentPath
        );
    })();
  });
  $<HTMLInputElement>('folder-input').addEventListener('change', e => {
    void (async () => {
      const input = e.target as HTMLInputElement;
      const picked = [...(input.files ?? [])];
      input.value = '';
      if (picked.length) {
        await uploads.upload(
          {
            files: picked.map(f => ({
              file: f,
              rel: f.webkitRelativePath || f.name,
            })),
            dirs: [],
          },
          currentPath
        );
      }
    })();
  });

  // Search
  $<HTMLInputElement>('search').addEventListener('input', () => {
    searchQuery = $<HTMLInputElement>('search').value.trim();
    render();
  });
  $('search').addEventListener('keydown', e => {
    if ((e as KeyboardEvent).key === 'Escape') {
      clearSearch();
      render();
    }
  });

  // Sorting + view controls
  $('list-head')
    .querySelectorAll<HTMLElement>('[data-sort]')
    .forEach(btn => {
      btn.addEventListener('click', () => {
        const key = btn.dataset.sort as SortKey;
        if (sortKey === key) sortDir = sortDir === 1 ? -1 : 1;
        else {
          sortKey = key;
          sortDir = 1;
        }
        savePrefs();
        render();
      });
    });
  $('view-toggle').addEventListener('click', () => {
    viewMode = viewMode === 'grid' ? 'list' : 'grid';
    savePrefs();
    render();
  });
  $('details-toggle').addEventListener('click', () => {
    detailsOpen = !detailsOpen;
    savePrefs();
    render();
  });
  $('details-close').addEventListener('click', () => {
    detailsOpen = false;
    savePrefs();
    render();
  });
  // Live-resize tiles while dragging by updating the CSS variable.
  $<HTMLInputElement>('tile-slider').addEventListener('input', () => {
    tileSize = Number($<HTMLInputElement>('tile-slider').value);
    $('list').style.setProperty('--tile', `${tileSize}px`);
    savePrefs();
  });
  $<HTMLInputElement>('select-all').addEventListener('change', () => {
    const { fs } = visibleEntries();
    if ($<HTMLInputElement>('select-all').checked)
      fs.forEach(f => selected.add(f.path));
    else fs.forEach(f => selected.delete(f.path));
    render();
  });

  // Bulk actions
  $('bulk-clear').addEventListener('click', () => {
    selected.clear();
    render();
  });
  $('bulk-move').addEventListener('click', () => openMove([...selected]));
  $('bulk-download').addEventListener(
    'click',
    () => void downloadSelectionZip()
  );
  $('bulk-delete').addEventListener('click', bulkDelete);
  $('bulk-public').addEventListener(
    'click',
    () => void bulkVisibility('public')
  );
  $('bulk-private').addEventListener(
    'click',
    () => void bulkVisibility('owner')
  );

  // One-time list-background handlers (the <ul> persists across renders).
  $('list').addEventListener('contextmenu', e => {
    if ((e.target as HTMLElement).closest('[data-file], [data-folder]')) return;
    openCtxMenu(e, { type: 'background' });
  });
  $('list').addEventListener('click', e => {
    if (e.target === $('list') && selection.focusPath) {
      selection.clearFocus();
      render();
    }
  });

  // Context menu dismissal
  document.addEventListener('click', e => {
    if (!(e.target as HTMLElement).closest('#ctx')) closeCtxMenu();
  });
  document.addEventListener('scroll', closeCtxMenu, true);

  // Viewer controls
  $('lb-prev').addEventListener('click', () => vStep(-1));
  $('lb-next').addEventListener('click', () => vStep(1));
  $('lb-in').addEventListener('click', () => viewer.zoom(1.25));
  $('lb-out').addEventListener('click', () => viewer.zoom(0.8));
  $('lb-fit').addEventListener('click', () => viewer.fit());
  $('lb-full').addEventListener('click', () => viewer.fullSize());
  $('lb-download').addEventListener('click', () => {
    const row = viewer.currentFile();
    if (row) void downloadFile(row);
  });
  $('lb-close').addEventListener('click', closeViewer);
  $('lb-stage').addEventListener('click', e => {
    if (e.target === $('lb-stage')) closeViewer();
  });
  $('lightbox').addEventListener(
    'wheel',
    e => {
      if (!$('lb-stage').querySelector('img')) return;
      e.preventDefault();
      viewer.zoom(e.deltaY < 0 ? 1.1 : 0.9);
    },
    { passive: false }
  );

  // Dialog controls + keyboard
  $('dialog-ok').addEventListener('click', () => void commitDialog());
  $('dialog-cancel').addEventListener('click', closeDialog);
  $('dialog').addEventListener('click', e => {
    if (e.target === $('dialog')) closeDialog();
  });
  document.addEventListener('keydown', e => {
    if (viewerOpen()) {
      if (e.key === 'Escape') return closeViewer();
      if (e.key === 'ArrowLeft') return vStep(-1);
      if (e.key === 'ArrowRight') return vStep(1);
      return;
    }
    if ($('ctx').classList.contains('open') && e.key === 'Escape')
      return closeCtxMenu();
    if ($('dialog').classList.contains('open')) {
      if (e.key === 'Escape') closeDialog();
      else if (
        e.key === 'Enter' &&
        (e.target as HTMLElement).tagName !== 'TEXTAREA'
      ) {
        e.preventDefault();
        void commitDialog();
      }
      return;
    }
    handleListKeys(e);
  });

  // OS-file drag only ('Files' type); internal row drags carry a custom type.
  const hasFiles = (e: DragEvent) =>
    [...(e.dataTransfer?.types ?? [])].includes('Files');
  window.addEventListener('dragenter', e => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    dragDepth++;
    $('drop-path').textContent = currentPath;
    document.body.classList.add('dragging-files');
  });
  window.addEventListener('dragover', e => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'copy';
  });
  window.addEventListener('dragleave', e => {
    if (!hasFiles(e)) return;
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) document.body.classList.remove('dragging-files');
  });
  window.addEventListener('drop', e => {
    void (async () => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      endFileDrag();
      const dropped = await collectDropped(e.dataTransfer!);
      await uploads.upload(dropped, currentPath);
    })();
  });
}

// Data flow: subscriptions -> state -> render

function refreshData(): void {
  if (!conn) return;
  folders = [...conn.db.myFolders.iter()];
  files = [...conn.db.myFileSummaries.iter()];
  const filePaths = new Set(files.map(file => file.path));
  selection.prune(
    filePaths,
    new Set([...filePaths, ...folders.map(folder => folder.path)])
  );
  render();
  // Close the viewer if the file it's showing was deleted out from under it.
  if (viewer.path && viewerOpen() && !files.some(f => f.path === viewer.path))
    closeViewer();
}

// Row callbacks fire synchronously per transaction; coalesce the burst into one render.
let refreshScheduled = false;
function scheduleRefresh(): void {
  if (refreshScheduled) return;
  refreshScheduled = true;
  queueMicrotask(() => {
    refreshScheduled = false;
    refreshData();
  });
}

async function main(): Promise<void> {
  wireUi();
  render();
  let config: ServerConfig;
  try {
    config = await loadConfig();
    try {
      conn = await connect(config);
    } catch (err) {
      // Stale stored token (server wiped/rekeyed): drop it, retry anonymously.
      if (!authToken) throw err;
      authToken = undefined;
      clearToken();
      conn = await connect(config);
    }
  } catch (err) {
    toast('err', `Couldn't connect: ${humanError(err)}`);
    return;
  }

  conn
    .subscriptionBuilder()
    .onApplied(() => refreshData())
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([tables.myFolders, tables.myFileSummaries]);

  conn.db.myFolders.onInsert(scheduleRefresh);
  conn.db.myFolders.onUpdate(scheduleRefresh);
  conn.db.myFolders.onDelete(scheduleRefresh);
  conn.db.myFileSummaries.onInsert(scheduleRefresh);
  conn.db.myFileSummaries.onUpdate(scheduleRefresh);
  conn.db.myFileSummaries.onDelete(scheduleRefresh);

  window.vault = vault;
}

main().catch(err => {
  console.error(err);
  toast('err', humanError(err));
});
