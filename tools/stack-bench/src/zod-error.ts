import type { ZodError } from 'zod';

export function formatZodError(error: ZodError, root: string): string {
  const at = (path: PropertyKey[]) => path.length
    ? `${root}.${path.map(String).join('.')}` : root;
  return error.issues.flatMap(issue => issue.code === 'unrecognized_keys'
    ? issue.keys.map(key => `${at([...issue.path, key])} is unknown`)
    : [`${at(issue.path)} ${issue.message}`]).join('; ');
}
