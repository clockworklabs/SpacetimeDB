import { FILE_BYTES_MAX } from '@spacetimedb/files/constants';
import type { FileSummary } from './codegen/app/types';
import type { DialogOptions } from './dialog';
import {
  errorCode,
  escapeHtml,
  fmtSize,
  humanError,
  joinPath,
  normalizePath,
  type Visibility,
} from './utils';

export interface DroppedEntries {
  files: Array<{ file: File; rel: string }>;
  dirs: string[];
}

function walkEntry(
  entry: FileSystemEntry,
  prefix: string,
  output: DroppedEntries
): Promise<void> {
  return new Promise(resolve => {
    if (entry.isFile) {
      (entry as FileSystemFileEntry).file(
        file => {
          output.files.push({ file, rel: prefix + entry.name });
          resolve();
        },
        () => resolve()
      );
      return;
    }
    if (!entry.isDirectory) {
      resolve();
      return;
    }

    const relativePath = prefix + entry.name;
    output.dirs.push(relativePath);
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    const children: FileSystemEntry[] = [];
    const readBatch = () =>
      reader.readEntries(
        entries => {
          void (async () => {
            if (entries.length > 0) {
              children.push(...entries);
              readBatch();
              return;
            }
            for (const child of children)
              await walkEntry(child, `${relativePath}/`, output);
            resolve();
          })();
        },
        () => resolve()
      );
    readBatch();
  });
}

export async function collectDropped(
  dataTransfer: DataTransfer
): Promise<DroppedEntries> {
  const output: DroppedEntries = { files: [], dirs: [] };
  const items = [...(dataTransfer.items ?? [])];
  const entries = items
    .map(item => item.webkitGetAsEntry?.())
    .filter((entry): entry is FileSystemEntry => Boolean(entry));
  if (entries.length > 0) {
    for (const entry of entries) await walkEntry(entry, '', output);
  } else {
    for (const file of [...(dataTransfer.files ?? [])]) {
      output.files.push({ file, rel: file.name });
    }
  }
  return output;
}

export interface UploadServices {
  ready(): boolean;
  files(): readonly FileSummary[];
  createFolder(path: string): Promise<unknown>;
  uploadFile(args: {
    path: string;
    mimeType: string;
    bytes: Uint8Array;
    visibility: Visibility;
  }): Promise<unknown>;
  freeName(path: string): string;
  openDialog(
    title: string,
    bodyHtml: string,
    onSave: (() => void | Promise<void>) | null,
    options?: DialogOptions
  ): void;
  setProgress(uploading: boolean, label?: string): void;
  toast(kind: 'ok' | 'err', message: string): void;
}

export class UploadController {
  constructor(private readonly services: UploadServices) {}

  async upload(entries: DroppedEntries, targetFolder: string): Promise<void> {
    if (!this.services.ready()) return;
    const { files, dirs } = entries;
    if (files.length === 0 && dirs.length === 0) return;
    const conflicts = files.filter(entry =>
      this.services
        .files()
        .some(file => file.path === joinPath(targetFolder, entry.rel))
    );
    if (conflicts.length > 0) {
      const listHtml =
        conflicts
          .slice(0, 6)
          .map(
            conflict => `<div class="meta">${escapeHtml(conflict.rel)}</div>`
          )
          .join('') +
        (conflicts.length > 6
          ? `<div class="meta">...and ${conflicts.length - 6} more</div>`
          : '');
      this.services.openDialog(
        `${conflicts.length} file${conflicts.length === 1 ? '' : 's'} already exist${conflicts.length === 1 ? 's' : ''}`,
        `<p>Replace the existing file${conflicts.length === 1 ? '' : 's'}, or keep both by renaming the new one${conflicts.length === 1 ? '' : 's'}?</p>${listHtml}`,
        () => this.perform(entries, targetFolder, 'replace'),
        {
          okLabel: 'Replace',
          altLabel: 'Keep both',
          onAlt: () => this.perform(entries, targetFolder, 'keep-both'),
        }
      );
      return;
    }
    await this.perform(entries, targetFolder, 'replace');
  }

  private async perform(
    entries: DroppedEntries,
    targetFolder: string,
    conflictMode: 'replace' | 'keep-both'
  ): Promise<void> {
    const directories = new Set(entries.dirs);
    for (const entry of entries.files) {
      const parts = entry.rel.split('/').slice(0, -1);
      let path = '';
      for (const part of parts) {
        path = path ? `${path}/${part}` : part;
        directories.add(path);
      }
    }
    for (const relativePath of [...directories].sort(
      (a, b) => a.split('/').length - b.split('/').length
    )) {
      try {
        await this.services.createFolder(joinPath(targetFolder, relativePath));
      } catch (error) {
        if (errorCode(error) !== 'vault.folder_exists') {
          this.services.toast('err', humanError(error));
          return;
        }
      }
    }

    const failures: string[] = [];
    const accepted = entries.files.filter(entry => {
      if (entry.file.size <= FILE_BYTES_MAX) return true;
      failures.push(
        `${entry.rel}: ${fmtSize(entry.file.size)} exceeds the ${fmtSize(FILE_BYTES_MAX)} cap`
      );
      return false;
    });
    let completed = 0;
    this.services.setProgress(
      true,
      accepted.length ? `Uploading 0/${accepted.length}...` : undefined
    );
    for (const entry of accepted) {
      try {
        let path = normalizePath(joinPath(targetFolder, entry.rel), 'file');
        const existingFiles = this.services.files();
        if (
          conflictMode === 'keep-both' &&
          existingFiles.some(file => file.path === path)
        ) {
          path = this.services.freeName(path);
        }
        const existing = existingFiles.find(file => file.path === path);
        await this.services.uploadFile({
          path,
          mimeType: entry.file.type || 'application/octet-stream',
          bytes: new Uint8Array(await entry.file.arrayBuffer()),
          visibility:
            (existing?.visibility as Visibility | undefined) ?? 'owner',
        });
        completed++;
        this.services.setProgress(
          true,
          `Uploading ${completed}/${accepted.length}...`
        );
      } catch (error) {
        failures.push(humanError(error, { name: entry.rel }));
      }
    }
    this.services.setProgress(false);
    if (completed > 0) {
      this.services.toast(
        'ok',
        `${completed} file${completed === 1 ? '' : 's'} uploaded`
      );
    }
    for (const failure of failures) this.services.toast('err', failure);
  }
}
