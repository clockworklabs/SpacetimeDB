import type { FileSummary } from './codegen/app/types';
import { icon } from './rendering';
import { escapeHtml } from './utils';

export type ContextTarget =
  | { type: 'file' | 'folder'; path: string }
  | { type: 'background' };

type ContextItem = {
  label: string;
  iconName: string;
  run: () => void;
  danger?: boolean;
} | null;

export interface ContextMenuServices {
  file(path: string): FileSummary | undefined;
  preview(path: string): void;
  viewDetails(path: string): void;
  downloadFile(file: FileSummary): void;
  copyLink(path: string): void;
  duplicateFile(path: string): void;
  renameFile(path: string): void;
  moveFile(path: string): void;
  toggleVisibility(path: string): void;
  deleteFile(path: string): void;
  openFolder(path: string): void;
  downloadFolder(path: string): void;
  renameFolder(path: string): void;
  deleteFolder(path: string): void;
  newFolder(): void;
  chooseFiles(): void;
  chooseFolder(): void;
}

const item = (
  label: string,
  iconName: string,
  run: () => void,
  danger = false
): ContextItem => ({ label, iconName, run, danger });

export class ContextMenu {
  constructor(private readonly services: ContextMenuServices) {}

  open = (event: MouseEvent, target: ContextTarget): void => {
    event.preventDefault();
    event.stopPropagation();
    const items = this.itemsFor(target);
    if (!items) return;
    const menu = document.getElementById('ctx')!;
    menu.innerHTML = items
      .map(entry =>
        entry === null
          ? '<div class="sep"></div>'
          : `<button class="${entry.danger ? 'danger' : ''}">${icon(entry.iconName)}${escapeHtml(entry.label)}</button>`
      )
      .join('');
    const buttons = [...menu.querySelectorAll('button')];
    let buttonIndex = 0;
    for (const entry of items) {
      if (!entry) continue;
      buttons[buttonIndex++]!.addEventListener('click', () => {
        this.close();
        entry.run();
      });
    }
    menu.classList.add('open');
    const rect = menu.getBoundingClientRect();
    menu.style.left = `${Math.min(event.clientX, window.innerWidth - rect.width - 8)}px`;
    menu.style.top = `${Math.min(event.clientY, window.innerHeight - rect.height - 8)}px`;
  };

  close = (): void => {
    document.getElementById('ctx')!.classList.remove('open');
  };

  private itemsFor(target: ContextTarget): ContextItem[] | undefined {
    if (target.type === 'file') {
      const file = this.services.file(target.path);
      if (!file) return undefined;
      const isPublic = file.visibility === 'public';
      return [
        item('Preview', 'eye', () => this.services.preview(target.path)),
        item('View details', 'info', () =>
          this.services.viewDetails(target.path)
        ),
        item('Download', 'download', () => this.services.downloadFile(file)),
        item('Copy link', 'link', () => this.services.copyLink(target.path)),
        item('Make a copy', 'copy', () =>
          this.services.duplicateFile(target.path)
        ),
        null,
        item('Rename', 'pencil', () => this.services.renameFile(target.path)),
        item('Move...', 'move', () => this.services.moveFile(target.path)),
        item(
          isPublic ? 'Make private' : 'Make public',
          isPublic ? 'lock' : 'globe',
          () => this.services.toggleVisibility(target.path)
        ),
        null,
        item(
          'Delete',
          'trash',
          () => this.services.deleteFile(target.path),
          true
        ),
      ];
    }
    if (target.type === 'folder') {
      return [
        item('Open', 'folder', () => this.services.openFolder(target.path)),
        item('View details', 'info', () =>
          this.services.viewDetails(target.path)
        ),
        item('Download as zip', 'download', () =>
          this.services.downloadFolder(target.path)
        ),
        null,
        item('Rename', 'pencil', () => this.services.renameFolder(target.path)),
        item(
          'Delete',
          'trash',
          () => this.services.deleteFolder(target.path),
          true
        ),
      ];
    }
    return [
      item('New folder', 'plus', () => this.services.newFolder()),
      item('Upload files', 'upload', () => this.services.chooseFiles()),
      item('Upload folder', 'folder-up', () => this.services.chooseFolder()),
    ];
  }
}
