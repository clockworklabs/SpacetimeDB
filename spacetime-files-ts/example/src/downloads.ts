import type { FileSummary } from './module_bindings/app/types';
import { buildZip, type ZipEntry } from './zip';
import { baseName, fileUrl, humanError, tsMs } from './utils';

export const ARCHIVE_FILE_COUNT_MAX = 250;
export const ARCHIVE_ENTRY_COUNT_MAX = 1_000;
export const ARCHIVE_TOTAL_BYTES_MAX = 64 * 1024 * 1024;

export interface DownloadServices {
  readFileBytes(path: string): Promise<{ bytes: Uint8Array; mimeType: string }>;
  toast(kind: 'ok' | 'err', message: string): void;
}

export interface ArchiveRequest {
  fileRows: FileSummary[];
  dirNames: Array<{ name: string; mtimeMs: number }>;
  entryName(file: FileSummary): string;
  zipName: string;
}

export function archiveSelectionError(
  fileRows: readonly FileSummary[],
  directoryCount = 0
): string | undefined {
  if (fileRows.length > ARCHIVE_FILE_COUNT_MAX) {
    return `Select at most ${ARCHIVE_FILE_COUNT_MAX} files for one archive.`;
  }
  if (fileRows.length + directoryCount > ARCHIVE_ENTRY_COUNT_MAX) {
    return `Archive contents exceed the ${ARCHIVE_ENTRY_COUNT_MAX} entry limit.`;
  }
  const totalBytes = fileRows.reduce((total, file) => total + file.size, 0n);
  if (totalBytes > BigInt(ARCHIVE_TOTAL_BYTES_MAX)) {
    return `Archive contents exceed the ${ARCHIVE_TOTAL_BYTES_MAX / 1024 / 1024} MiB limit.`;
  }
  return undefined;
}

export async function getFileBlob(
  row: FileSummary,
  services: DownloadServices
): Promise<Blob> {
  if (row.visibility === 'public') {
    try {
      const response = await fetch(fileUrl(row.id));
      if (response.ok) return await response.blob();
    } catch {
      // The authenticated procedure below also serves public files.
    }
  }
  const { bytes, mimeType } = await services.readFileBytes(row.path);
  return new Blob([bytes as BlobPart], {
    type: mimeType || row.mimeType || 'application/octet-stream',
  });
}

export function saveBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 4000);
}

export async function downloadFile(
  row: FileSummary,
  services: DownloadServices
): Promise<void> {
  try {
    saveBlob(await getFileBlob(row, services), baseName(row.path));
  } catch (error) {
    services.toast('err', humanError(error));
  }
}

export async function downloadArchive(
  request: ArchiveRequest,
  services: DownloadServices
): Promise<void> {
  const { fileRows, dirNames, entryName, zipName } = request;
  if (fileRows.length === 0 && dirNames.length === 0) {
    services.toast('err', 'Nothing to download.');
    return;
  }
  const selectionError = archiveSelectionError(fileRows, dirNames.length);
  if (selectionError) {
    services.toast('err', selectionError);
    return;
  }
  if (fileRows.length > 3)
    services.toast('ok', `Zipping ${fileRows.length} files...`);

  const entries: ZipEntry[] = dirNames.map(directory => ({
    name: directory.name,
    isDir: true,
    mtimeMs: directory.mtimeMs,
  }));
  const failures: string[] = [];
  let loadedBytes = 0;
  for (const file of fileRows) {
    try {
      const blob = await getFileBlob(file, services);
      loadedBytes += blob.size;
      if (loadedBytes > ARCHIVE_TOTAL_BYTES_MAX) {
        services.toast(
          'err',
          `Downloaded contents exceed the ${ARCHIVE_TOTAL_BYTES_MAX / 1024 / 1024} MiB limit.`
        );
        return;
      }
      entries.push({
        name: entryName(file),
        bytes: new Uint8Array(await blob.arrayBuffer()),
        mtimeMs: tsMs(file.updatedAt),
      });
    } catch (error) {
      failures.push(`${baseName(file.path)}: ${humanError(error)}`);
    }
  }
  for (const message of failures) services.toast('err', message);
  if (entries.length === 0) return;
  saveBlob(buildZip(entries), zipName);
  services.toast('ok', `${zipName} ready`);
}
