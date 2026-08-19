import { existsSync, lstatSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

export function assertNewOrEmptyDirectory(path, label) {
  const target = resolve(path);
  if (!existsSync(target)) return target;

  const stat = lstatSync(target);
  if (!stat.isDirectory() || stat.isSymbolicLink()) {
    throw new Error(`${label} must be a real directory: ${target}`);
  }
  if (readdirSync(target).length > 0) {
    throw new Error(`${label} must be new or empty; refusing to overwrite: ${target}`);
  }
  return target;
}
