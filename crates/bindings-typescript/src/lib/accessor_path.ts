// Helpers for the dotted accessor keys that submodule tables, reducers and procedures are
// registered under (`myAuth.login`). Kept out of `lib/util` so they are not re-exported
// from the package root; they are an implementation detail of the SDK and its framework hooks.

/**
 * Returns the object holding a dotted accessor key and the key's last segment,
 * creating intermediate objects as needed: `myAuth.login` -> `[root.myAuth, 'login']`.
 */
export function accessorSlot(
  root: object,
  accessorName: string
): [Record<string, unknown>, string] {
  const path = accessorName.split('.');
  const leaf = path.pop()!;
  let target = root as Record<string, unknown>;
  for (const segment of path) {
    target = (target[segment] ??= Object.create(null)) as Record<
      string,
      unknown
    >;
  }
  return [target, leaf];
}

/** Reads a dotted accessor key from nested objects: `myAuth.login` -> `root.myAuth.login`. */
export function getByAccessorPath<T = unknown>(
  root: object,
  accessorName: string
): T {
  let value: unknown = root;
  for (const segment of accessorName.split('.')) {
    value = (value as Record<string, unknown> | undefined)?.[segment];
  }
  return value as T;
}
