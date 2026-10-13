import type { Entry } from './rendering';

export interface KeyboardServices {
  entries(): readonly Entry[];
  focusPath(): string | null;
  selected: Set<string>;
  visibleFilePaths(): string[];
  setFocus(path: string): void;
  clearFocus(): void;
  openFolder(path: string): void;
  openFile(path: string): void;
  toggleSelect(path: string): void;
  deleteSelection(): void;
  deleteFile(path: string): void;
  deleteFolder(path: string): void;
  hasSearch(): boolean;
  clearSearch(): void;
  render(): void;
}

function focusedEntry(services: KeyboardServices): Entry | undefined {
  const focusPath = services.focusPath();
  return focusPath
    ? services.entries().find(entry => entry.path === focusPath)
    : undefined;
}

function moveFocus(services: KeyboardServices, delta: number): void {
  const entries = services.entries();
  if (entries.length === 0) return;
  const currentIndex = entries.findIndex(
    entry => entry.path === services.focusPath()
  );
  const nextIndex =
    currentIndex < 0
      ? delta > 0
        ? 0
        : entries.length - 1
      : Math.min(entries.length - 1, Math.max(0, currentIndex + delta));
  const entry = entries[nextIndex]!;
  services.setFocus(entry.path);
  document
    .querySelector(
      entry.type === 'file'
        ? `[data-file="${CSS.escape(entry.path)}"]`
        : `[data-folder="${CSS.escape(entry.path)}"]`
    )
    ?.scrollIntoView({ block: 'nearest' });
}

export function handleListKey(
  event: KeyboardEvent,
  services: KeyboardServices
): void {
  const tag = document.activeElement?.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
  if (
    document.getElementById('dialog')!.classList.contains('open') ||
    document.getElementById('lightbox')!.classList.contains('open') ||
    document.getElementById('ctx')!.classList.contains('open')
  )
    return;

  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') {
    event.preventDefault();
    for (const path of services.visibleFilePaths()) services.selected.add(path);
    services.render();
    return;
  }
  const entry = focusedEntry(services);
  if (event.key === 'ArrowDown' || event.key === 'ArrowRight') {
    event.preventDefault();
    moveFocus(services, 1);
  } else if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') {
    event.preventDefault();
    moveFocus(services, -1);
  } else if (event.key === 'Enter' && entry) {
    event.preventDefault();
    if (entry.type === 'folder') services.openFolder(entry.path);
    else services.openFile(entry.path);
  } else if (event.key === ' ' && entry?.type === 'file') {
    event.preventDefault();
    services.toggleSelect(entry.path);
  } else if (event.key === 'Delete') {
    event.preventDefault();
    if (services.selected.size > 0) services.deleteSelection();
    else if (entry?.type === 'file') services.deleteFile(entry.path);
    else if (entry) services.deleteFolder(entry.path);
  } else if (event.key === 'Escape') {
    if (services.selected.size > 0) services.selected.clear();
    else if (services.focusPath()) services.clearFocus();
    else if (services.hasSearch()) services.clearSearch();
    else return;
    services.render();
  }
}
