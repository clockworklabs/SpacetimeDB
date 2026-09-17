import { FILE_MIME_TYPE_MAX, FILE_PATH_MAX } from './constants.ts';

const MIME_TYPE = /^[A-Za-z0-9!#$&^_.+-]+\/[A-Za-z0-9!#$&^_.+-]+$/;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code <= 0x1f || code === 0x7f) return true;
  }
  return false;
}

export class FileValidationError extends Error {}

export function ownerPathKey(owner: string, path: string): string {
  return `${owner.length}:${owner}${path}`;
}

export function validateFileOwner(owner: string): string {
  if (owner.length === 0 || owner.length > 512 || hasControlCharacter(owner)) {
    throw new FileValidationError('files.invalid_owner');
  }
  return owner;
}

export function validateFilePath(path: string): string {
  if (
    path.length < 2 ||
    path.length > FILE_PATH_MAX ||
    !path.startsWith('/') ||
    path.endsWith('/') ||
    path.includes('\\') ||
    path.includes('//') ||
    hasControlCharacter(path)
  ) {
    throw new FileValidationError('files.invalid_path');
  }
  for (const segment of path.slice(1).split('/')) {
    if (segment === '.' || segment === '..' || segment.length > 255) {
      throw new FileValidationError('files.invalid_path');
    }
  }
  return path;
}

export function validateFilePrefix(prefix: string): string {
  if (prefix === '') return prefix;
  if (
    prefix.length > FILE_PATH_MAX ||
    !prefix.startsWith('/') ||
    prefix.includes('\\') ||
    prefix.includes('//') ||
    hasControlCharacter(prefix)
  ) {
    throw new FileValidationError('files.invalid_prefix');
  }
  return prefix;
}

export function validateMimeType(mimeType: string): string {
  const value = mimeType.trim();
  if (
    value.length === 0 ||
    value.length > FILE_MIME_TYPE_MAX ||
    !MIME_TYPE.test(value)
  ) {
    throw new FileValidationError('files.invalid_mime_type');
  }
  return value.toLowerCase();
}

export function safeMimeType(mimeType: unknown): string {
  if (typeof mimeType !== 'string') return 'application/octet-stream';
  try {
    return validateMimeType(mimeType);
  } catch {
    return 'application/octet-stream';
  }
}
