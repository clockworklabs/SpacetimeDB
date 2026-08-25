import type { FileSummary } from './module_bindings/app/types';
import type { ContextTarget } from './context-menu';

export interface ListActionServices {
  selected: Set<string>;
  files(): readonly FileSummary[];
  setAnchor(path: string): void;
  toggleSelect(path: string): void;
  rangeSelect(path: string): void;
  focus(path: string): void;
  openFile(path: string): void;
  openFolder(path: string): void;
  openContext(event: MouseEvent, target: ContextTarget): void;
  render(): void;
  toggleVisibility(path: string): void;
  copyLink(path: string): void;
  downloadFile(file: FileSummary): void;
  downloadFolder(path: string): void;
  renameFile(path: string): void;
  renameFolder(path: string): void;
  moveFile(path: string): void;
  deleteFile(path: string): void;
  deleteFolder(path: string): void;
  wireFolderDropTarget(element: HTMLElement, path: string): void;
}

export function bindListActions(
  list: HTMLElement,
  services: ListActionServices
): void {
  list
    .querySelectorAll<HTMLElement>('[data-file], [data-folder]')
    .forEach(row => {
      row.addEventListener('click', event => {
        if (
          (event.target as HTMLElement).closest(
            '.row-actions, .badge, .sel, .badge-cell, .vis-dot'
          )
        )
          return;
        const file = row.dataset.file;
        if (!file) return services.focus(row.dataset.folder!);
        if (event.ctrlKey || event.metaKey) return services.toggleSelect(file);
        if (event.shiftKey) return services.rangeSelect(file);
        services.focus(file);
      });
      row.addEventListener('dblclick', event => {
        if (
          (event.target as HTMLElement).closest(
            '.row-actions, .badge, .sel, .badge-cell, .vis-dot'
          )
        )
          return;
        if (row.dataset.file) services.openFile(row.dataset.file);
        else services.openFolder(row.dataset.folder!);
      });
      row.addEventListener('contextmenu', event => {
        const file = row.dataset.file;
        services.openContext(
          event,
          file
            ? { type: 'file', path: file }
            : { type: 'folder', path: row.dataset.folder! }
        );
      });
    });

  list.querySelectorAll<HTMLInputElement>('[data-select]').forEach(checkbox => {
    checkbox.addEventListener('change', () => {
      const path = checkbox.dataset.select!;
      if (checkbox.checked) services.selected.add(path);
      else services.selected.delete(path);
      services.setAnchor(path);
      services.render();
    });
  });
  list
    .querySelectorAll<HTMLElement>('[data-visibility]')
    .forEach(button =>
      button.addEventListener('click', () =>
        services.toggleVisibility(button.dataset.visibility!)
      )
    );
  list
    .querySelectorAll<HTMLElement>('[data-link]')
    .forEach(button =>
      button.addEventListener('click', () =>
        services.copyLink(button.dataset.link!)
      )
    );
  list.querySelectorAll<HTMLElement>('[data-download]').forEach(button => {
    button.addEventListener('click', () => {
      const file = services
        .files()
        .find(row => row.path === button.dataset.download);
      if (file) services.downloadFile(file);
    });
  });
  list.querySelectorAll<HTMLElement>('[data-download-folder]').forEach(button =>
    button.addEventListener('click', event => {
      event.stopPropagation();
      services.downloadFolder(button.dataset.downloadFolder!);
    })
  );
  list
    .querySelectorAll<HTMLElement>('[data-rename]')
    .forEach(button =>
      button.addEventListener('click', () =>
        services.renameFile(button.dataset.rename!)
      )
    );
  list.querySelectorAll<HTMLElement>('[data-rename-folder]').forEach(button =>
    button.addEventListener('click', event => {
      event.stopPropagation();
      services.renameFolder(button.dataset.renameFolder!);
    })
  );
  list
    .querySelectorAll<HTMLElement>('[data-move]')
    .forEach(button =>
      button.addEventListener('click', () =>
        services.moveFile(button.dataset.move!)
      )
    );
  list
    .querySelectorAll<HTMLElement>('[data-delete-file]')
    .forEach(button =>
      button.addEventListener('click', () =>
        services.deleteFile(button.dataset.deleteFile!)
      )
    );
  list.querySelectorAll<HTMLElement>('[data-delete-folder]').forEach(button =>
    button.addEventListener('click', event => {
      event.stopPropagation();
      services.deleteFolder(button.dataset.deleteFolder!);
    })
  );

  list.querySelectorAll<HTMLElement>('[data-drag]').forEach(row => {
    row.addEventListener('dragstart', event => {
      const path = row.dataset.drag!;
      const paths = services.selected.has(path)
        ? [...services.selected]
        : [path];
      event.dataTransfer!.setData(
        'application/x-vault-path',
        JSON.stringify(paths)
      );
      event.dataTransfer!.effectAllowed = 'move';
    });
  });
  list
    .querySelectorAll<HTMLElement>('[data-drop-folder]')
    .forEach(row =>
      services.wireFolderDropTarget(row, row.dataset.dropFolder!)
    );
}
