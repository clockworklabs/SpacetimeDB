import type { FileSummary, Folder } from './module_bindings/app/types';
import { baseName, childPrefix, parentPath } from './paths';
import {
  escapeHtml,
  formatFileSize,
  formatTimestamp,
  fileKindPresentation,
  timestampMilliseconds,
} from './presentation';

export type SortKey = 'name' | 'size' | 'updated' | 'visibility';
export type Entry = { type: 'file' | 'folder'; path: string };

export interface VaultRenderState {
  folders: Folder[];
  files: FileSummary[];
  currentPath: string;
  searchQuery: string;
  sortKey: SortKey;
  sortDir: 1 | -1;
  selected: ReadonlySet<string>;
  focusPath: string | null;
}

export const icon = (name: string): string =>
  `<svg class="ico"><use href="#i-${name}" /></svg>`;

export function createVaultRendering(getState: () => VaultRenderState) {
  const immediateFolders = (path: string): Folder[] =>
    getState().folders.filter(folder => folder.parentPath === path);

  const immediateFiles = (path: string): FileSummary[] =>
    getState().files.filter(file => parentPath(file.path) === path);

  const allFolderPaths = (): string[] => [
    '/',
    ...getState()
      .folders.map(folder => folder.path)
      .sort(),
  ];

  const fileCmp = (a: FileSummary, b: FileSummary): number => {
    const { sortKey, sortDir } = getState();
    let result = 0;
    if (sortKey === 'size') result = Number(a.size) - Number(b.size);
    else if (sortKey === 'updated')
      result =
        timestampMilliseconds(a.updatedAt) - timestampMilliseconds(b.updatedAt);
    else if (sortKey === 'visibility')
      result = a.visibility.localeCompare(b.visibility);
    if (result === 0) result = baseName(a.path).localeCompare(baseName(b.path));
    return result * sortDir;
  };

  const folderCmp = (a: Folder, b: Folder): number => {
    const { sortKey, sortDir } = getState();
    let result =
      sortKey === 'updated'
        ? timestampMilliseconds(a.updatedAt) -
          timestampMilliseconds(b.updatedAt)
        : 0;
    if (result === 0) result = a.name.localeCompare(b.name);
    return result * sortDir;
  };

  const visibleEntries = (): { dirs: Folder[]; fs: FileSummary[] } => {
    const { folders, files, currentPath, searchQuery } = getState();
    if (searchQuery) {
      const query = searchQuery.toLowerCase();
      return {
        dirs: folders
          .filter(folder => folder.name.toLowerCase().includes(query))
          .sort(folderCmp),
        fs: files
          .filter(file => baseName(file.path).toLowerCase().includes(query))
          .sort(fileCmp),
      };
    }
    return {
      dirs: immediateFolders(currentPath).sort(folderCmp),
      fs: immediateFiles(currentPath).sort(fileCmp),
    };
  };

  const stateClasses = (path: string, selectable: boolean): string => {
    const { selected, focusPath } = getState();
    return `${selectable && selected.has(path) ? 'selected' : ''} ${path === focusPath ? 'focused' : ''}`;
  };

  const fileLiOpen = (file: FileSummary, kind: 'row' | 'tile'): string =>
    `<li class="${kind} ${stateClasses(file.path, true)}" draggable="true" data-file="${escapeHtml(file.path)}" data-drag="${escapeHtml(file.path)}">`;

  const folderLiOpen = (folder: Folder, kind: 'row' | 'tile'): string =>
    `<li class="${kind} ${stateClasses(folder.path, false)}" data-folder="${escapeHtml(folder.path)}" data-drop-folder="${escapeHtml(folder.path)}">`;

  const selectionCheckbox = (file: FileSummary): string =>
    `<span class="sel"><input type="checkbox" data-select="${escapeHtml(file.path)}" ${getState().selected.has(file.path) ? 'checked' : ''} aria-label="Select file" /></span>`;

  const fileRowHtml = (file: FileSummary): string => {
    const kind = fileKindPresentation(file.mimeType);
    const isPublic = file.visibility === 'public';
    return `
      ${fileLiOpen(file, 'row')}
        ${selectionCheckbox(file)}
        <span class="row-name"><span class="kind ${kind.className}">${icon(kind.iconName)}</span><span class="label">${escapeHtml(baseName(file.path))}</span></span>
        <span class="meta">${getState().searchQuery ? escapeHtml(parentPath(file.path)) : formatFileSize(file.size)}</span>
        <span class="meta">${formatTimestamp(file.updatedAt)}</span>
        <span class="badge-cell">
          <button class="badge ${isPublic ? 'public' : 'private'}" data-visibility="${escapeHtml(file.path)}" title="Toggle visibility">
            ${icon(isPublic ? 'globe' : 'lock')}${isPublic ? 'Public' : 'Private'}
          </button>
        </span>
        <span class="row-actions">
          <button class="icon secondary" data-link="${escapeHtml(file.path)}" title="Copy link" aria-label="Copy link">${icon('link')}</button>
          <button class="icon secondary" data-rename="${escapeHtml(file.path)}" title="Rename" aria-label="Rename">${icon('pencil')}</button>
          <button class="icon secondary" data-move="${escapeHtml(file.path)}" title="Move" aria-label="Move">${icon('move')}</button>
          <button class="icon" data-download="${escapeHtml(file.path)}" title="Download" aria-label="Download">${icon('download')}</button>
          <button class="icon danger" data-delete-file="${escapeHtml(file.path)}" title="Delete" aria-label="Delete">${icon('trash')}</button>
        </span>
      </li>`;
  };

  const folderRowHtml = (folder: Folder): string => `
    ${folderLiOpen(folder, 'row')}
      <span class="sel"></span>
      <span class="row-name"><span class="kind folder">${icon('folder')}</span><span class="label">${escapeHtml(folder.name)}</span></span>
      <span class="meta">${getState().searchQuery ? escapeHtml(parentPath(folder.path)) : 'Folder'}</span>
      <span class="meta">${formatTimestamp(folder.updatedAt)}</span>
      <span class="badge-cell"></span>
      <span class="row-actions">
        <button class="icon secondary" data-rename-folder="${escapeHtml(folder.path)}" title="Rename folder" aria-label="Rename folder">${icon('pencil')}</button>
        <button class="icon" data-download-folder="${escapeHtml(folder.path)}" title="Download as zip" aria-label="Download folder as zip">${icon('download')}</button>
        <button class="icon danger" data-delete-folder="${escapeHtml(folder.path)}" title="Delete folder" aria-label="Delete folder">${icon('trash')}</button>
      </span>
    </li>`;

  const fileTileHtml = (file: FileSummary): string => {
    const kind = fileKindPresentation(file.mimeType);
    const isPublic = file.visibility === 'public';
    const isImage = (file.mimeType || '').startsWith('image/');
    return `
      ${fileLiOpen(file, 'tile')}
        ${selectionCheckbox(file)}
        <span class="vis-dot ${isPublic ? 'public' : ''}" title="${isPublic ? 'Public' : 'Private'}">${icon(isPublic ? 'globe' : 'lock')}</span>
        <div class="thumb" ${isImage ? `data-thumb="${escapeHtml(file.path)}"` : ''}>${icon(kind.iconName)}</div>
        <div class="tile-name ${kind.className}">${icon(kind.iconName)}<span class="label" title="${escapeHtml(baseName(file.path))}">${escapeHtml(baseName(file.path))}</span></div>
      </li>`;
  };

  const folderTileHtml = (folder: Folder): string => `
    ${folderLiOpen(folder, 'tile')}
      <div class="thumb">${icon('folder')}</div>
      <div class="tile-name folder">${icon('folder')}<span class="label" title="${escapeHtml(folder.name)}">${escapeHtml(folder.name)}</span></div>
    </li>`;

  const subtreeStats = (
    folderPath: string
  ): { fileCount: number; folderCount: number; bytes: number } => {
    const { files, folders } = getState();
    const prefix = childPrefix(folderPath);
    const childFiles = files.filter(file => file.path.startsWith(prefix));
    const childFolders = folders.filter(
      folder => folder.path !== folderPath && folder.path.startsWith(prefix)
    );
    return {
      fileCount: childFiles.length,
      folderCount: childFolders.length,
      bytes: childFiles.reduce((total, file) => total + Number(file.size), 0),
    };
  };

  return {
    allFolderPaths,
    fileRowHtml,
    fileTileHtml,
    folderRowHtml,
    folderTileHtml,
    immediateFiles,
    immediateFolders,
    subtreeStats,
    visibleEntries,
  };
}

export function fileDetailsHtml(row: FileSummary): string {
  const kind = fileKindPresentation(row.mimeType);
  const isImage = (row.mimeType || '').startsWith('image/');
  const updatedAtMs = timestampMilliseconds(row.updatedAt);
  return `
    <div class="d-thumb" ${isImage ? `data-dthumb="${escapeHtml(row.path)}"` : ''}>${icon(kind.iconName)}</div>
    <div class="d-name">${icon(kind.iconName)}<span>${escapeHtml(baseName(row.path))}</span></div>
    <div class="details">
      <div><span>Type</span><b>${escapeHtml(row.mimeType || 'file')}</b></div>
      <div><span>Size</span><b>${formatFileSize(row.size)}</b></div>
      <div><span>Location</span><b><button class="linkish" data-goto="${escapeHtml(parentPath(row.path))}">${escapeHtml(parentPath(row.path))}</button></b></div>
      ${updatedAtMs ? `<div><span>Modified</span><b>${escapeHtml(new Date(updatedAtMs).toLocaleString())}</b></div>` : ''}
      <div><span>Visibility</span><b>${row.visibility === 'public' ? 'Public' : 'Private (owner only)'}</b></div>
      <div><span>SHA-256</span><b class="mono" title="${escapeHtml(row.sha256Hex ?? '')}">${escapeHtml((row.sha256Hex ?? '').slice(0, 16))}...</b></div>
    </div>`;
}

export function folderDetailsHtml(
  folderPath: string,
  folder: Folder | undefined,
  stats: { fileCount: number; folderCount: number; bytes: number }
): string {
  const isRoot = folderPath === '/';
  const updatedAtMs = folder ? timestampMilliseconds(folder.updatedAt) : 0;
  return `
    <div class="d-thumb">${icon('folder')}</div>
    <div class="d-name">${icon('folder')}<span>${escapeHtml(isRoot ? 'Root' : (folder?.name ?? ''))}</span></div>
    <div class="details">
      <div><span>Type</span><b>Folder</b></div>
      <div><span>Contents</span><b>${stats.fileCount} file${stats.fileCount === 1 ? '' : 's'}, ${stats.folderCount} folder${stats.folderCount === 1 ? '' : 's'}</b></div>
      <div><span>Size</span><b>${formatFileSize(stats.bytes)}</b></div>
      ${!isRoot ? `<div><span>Location</span><b><button class="linkish" data-goto="${escapeHtml(parentPath(folderPath))}">${escapeHtml(parentPath(folderPath))}</button></b></div>` : ''}
      ${updatedAtMs ? `<div><span>Modified</span><b>${escapeHtml(new Date(updatedAtMs).toLocaleString())}</b></div>` : ''}
    </div>`;
}

export function selectionDetailsHtml(rows: readonly FileSummary[]): string {
  const bytes = rows.reduce((total, file) => total + Number(file.size), 0);
  return `
    <div class="d-name">${icon('copy')}<span>${rows.length} files selected</span></div>
    <div class="details">
      <div><span>Total size</span><b>${formatFileSize(bytes)}</b></div>
      <div><span>Public</span><b>${rows.filter(row => row.visibility === 'public').length} of ${rows.length}</b></div>
    </div>`;
}
