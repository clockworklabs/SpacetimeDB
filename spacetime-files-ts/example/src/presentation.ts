import type { Timestamp } from 'spacetimedb';

export function formatFileSize(
  value: number | bigint | string | undefined
): string {
  const bytes = Number(value ?? 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

export function timestampMilliseconds(
  timestamp: Timestamp | undefined
): number {
  if (!timestamp) return 0;
  try {
    return Number(timestamp.microsSinceUnixEpoch / 1000n);
  } catch {
    return 0;
  }
}

export function formatTimestamp(timestamp: Timestamp | undefined): string {
  const milliseconds = timestampMilliseconds(timestamp);
  if (!milliseconds) return '';
  const date = new Date(milliseconds);
  if (date.toDateString() === new Date().toDateString()) {
    return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

export function escapeHtml(value: unknown): string {
  return String(value ?? '').replace(
    /[&<>"']/g,
    character =>
      ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[
        character
      ]!
  );
}

export function fileKindPresentation(mimeType: string | undefined): {
  className: string;
  iconName: string;
} {
  if (!mimeType) return { className: 'generic', iconName: 'file' };
  if (mimeType.startsWith('image/')) {
    return { className: 'image', iconName: 'file-image' };
  }
  if (mimeType.startsWith('audio/') || mimeType.startsWith('video/')) {
    return { className: 'media', iconName: 'file-media' };
  }
  if (mimeType.startsWith('text/') || mimeType === 'application/json') {
    return { className: 'text', iconName: 'file-text' };
  }
  return { className: 'generic', iconName: 'file' };
}

const ERROR_MESSAGES: Record<string, string> = {
  'vault.folder_not_empty':
    "That folder isn't empty. Delete its contents first.",
  'vault.folder_exists': 'A folder with that name already exists here.',
  'vault.file_exists':
    'A file with that name already exists at the destination.',
  'vault.parent_not_found': "That destination folder doesn't exist.",
  'vault.folder_not_found': "That folder doesn't exist.",
  'vault.file_not_found': "That file doesn't exist.",
  'vault.cannot_delete_root': "The root folder can't be deleted.",
  'vault.cannot_rename_root': "The root folder can't be renamed.",
  'vault.invalid_file_path': 'A file needs a name.',
  'vault.invalid_path': "That name isn't allowed.",
  'vault.invalid_visibility': 'That visibility value is invalid.',
  'files.invalid_path': "That name isn't allowed.",
  'files.invalid_visibility': 'That visibility value is invalid.',
  'files.not_found': "That file doesn't exist.",
  'files.invalid_mime_type': 'That file type is invalid.',
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error ?? '');
}

export function errorCode(error: unknown): string {
  return (
    (errorMessage(error).match(/\b(?:vault|files)\.[a-z_]+/) ?? [])[0] ?? ''
  );
}

export function humanError(
  error: unknown,
  context: { name?: string } = {}
): string {
  const rawMessage = errorMessage(error) || 'Something went wrong';
  const sizeMatch = rawMessage.match(/^files\.too_large:(\d+)\/(\d+)/);
  if (sizeMatch) {
    const name = context.name ? `"${context.name}"` : 'That file';
    return `${name} is ${formatFileSize(sizeMatch[1])}. Vault caps files at ${formatFileSize(sizeMatch[2])}.`;
  }
  return ERROR_MESSAGES[errorCode(error)] ?? rawMessage;
}
