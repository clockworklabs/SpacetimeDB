export type ParsedArgs = {
  positionals: string[];
  flags: Map<string, string>;
};

export function parseArgs(argv: string[]): ParsedArgs {
  const positionals: string[] = [];
  const flags = new Map<string, string>();

  for (let i = 0; i < argv.length; ) {
    const arg = argv[i]!;
    if (arg === '--') {
      i++;
      continue;
    }
    if (!arg.startsWith('--')) {
      positionals.push(arg);
      i++;
      continue;
    }

    const key = arg.slice(2);
    const next = argv[i + 1];
    if (!next || next.startsWith('--')) {
      flags.set(key, '1');
      i++;
      continue;
    }

    flags.set(key, next);
    i += 2;
  }

  return { positionals, flags };
}

export function getStringFlag(
  flags: Map<string, string>,
  key: string,
  fallback?: string,
): string {
  const value = flags.get(key);
  if (value != null && value.length > 0) return value;
  if (fallback != null) return fallback;
  throw new Error(`Missing required --${key}`);
}

export function getOptionalStringFlag(
  flags: Map<string, string>,
  key: string,
): string | undefined {
  const value = flags.get(key);
  return value != null && value.length > 0 ? value : undefined;
}

export function getNumberFlag(
  flags: Map<string, string>,
  key: string,
  fallback?: number,
): number {
  const raw = flags.get(key);
  if (raw == null) {
    if (fallback != null) return fallback;
    throw new Error(`Missing required --${key}`);
  }

  const value = Number(raw);
  if (!Number.isFinite(value)) {
    throw new Error(`Invalid numeric value for --${key}: ${raw}`);
  }
  return value;
}

export function getBoolFlag(
  flags: Map<string, string>,
  key: string,
  fallback = false,
): boolean {
  const raw = flags.get(key);
  if (raw == null) return fallback;

  switch (raw) {
    case '1':
    case 'true':
    case 'yes':
    case 'on':
      return true;
    case '0':
    case 'false':
    case 'no':
    case 'off':
      return false;
    default:
      throw new Error(`Invalid boolean value for --${key}: ${raw}`);
  }
}

export function getStringListFlag(
  flags: Map<string, string>,
  key: string,
): string[] | undefined {
  const raw = flags.get(key);
  if (raw == null) return undefined;
  return raw
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean);
}
