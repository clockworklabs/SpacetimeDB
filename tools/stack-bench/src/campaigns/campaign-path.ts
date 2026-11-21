import { existsSync, lstatSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';

export const CAMPAIGN_FILE = Object.freeze({
  plan: 'plan.json',
  state: 'state.json',
  reportJson: 'report.json',
  reportHtml: 'report.html',
});

export function campaignChildPath(root: string, path: string, label: string): string {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is not a child of the campaign directory`);
  }
  let cursor = absoluteRoot;
  for (const segment of rel.split(sep)) {
    cursor = join(cursor, segment);
    if (!existsSync(cursor)) break;
    if (lstatSync(cursor).isSymbolicLink()) {
      throw new Error(`${label} contains a symbolic link: ${cursor}`);
    }
  }
  return absolute;
}
