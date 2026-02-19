import { stringify as ssStringify } from 'safe-stable-stringify';
import { u128ToHexString, u256ToHexString } from '../lib/util';
export type LogLevel = 'info' | 'warn' | 'error' | 'debug' | 'trace';

const LogLevelIdentifierIcon = {
  component: '📦',
  info: 'ℹ️',
  warn: '⚠️',
  error: '❌',
  debug: '🐛',
  trace: '🔍',
};

const LogStyle = {
  component:
    'color: #fff; background-color: #8D6FDD; padding: 2px 5px; border-radius: 3px;',
  info: 'color: #fff; background-color: #007bff; padding: 2px 5px; border-radius: 3px;',
  warn: 'color: #fff; background-color: #ffc107; padding: 2px 5px; border-radius: 3px;',
  error:
    'color: #fff; background-color: #dc3545; padding: 2px 5px; border-radius: 3px;',
  debug:
    'color: #fff; background-color: #28a745; padding: 2px 5px; border-radius: 3px;',
  trace:
    'color: #fff; background-color: #17a2b8; padding: 2px 5px; border-radius: 3px;',
};

const LogTextStyle = {
  component: 'color: #8D6FDD;',
  info: 'color: #007bff;',
  warn: 'color: #ffc107;',
  error: 'color: #dc3545;',
  debug: 'color: #28a745;',
  trace: 'color: #17a2b8;',
};

const LogLevelRank: Record<LogLevel, number> = {
  error: 0,
  warn: 1,
  info: 2,
  debug: 3,
  trace: 4,
};

let globalLogLevel: LogLevel = 'info';

export const setGlobalLogLevel = (level: LogLevel): void => {
  globalLogLevel = level;
};

export const getGlobalLogLevel = (): LogLevel => globalLogLevel;

const shouldLog = (level: LogLevel): boolean =>
  LogLevelRank[level] <= LogLevelRank[globalLogLevel];

// Lazy can be a function or the actual thing, so we can make verbose logs cheap when disabled.
type Lazy<T> = T | (() => T);
const resolveLazy = <T>(v: Lazy<T>): T =>
  typeof v === 'function' ? (v as () => T)() : v;

const toHex = (bytes: Uint8Array): string =>
  Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
const ARRAY_TRUNCATION_THRESHOLD = 25;
const ARRAY_PREVIEW_COUNT = 10;

const SENSITIVE_KEYS = new Set([
  'token',
  'authToken',
  'authorization',
  'accessToken',
  'refreshToken',
]);

export const stringify = (value: unknown): string | undefined =>
  ssStringify(value, (key, current) => {
    if (SENSITIVE_KEYS.has(key)) {
      return '[REDACTED]';
    }
    if (
      current &&
      typeof current === 'object' &&
      '__identity__' in current &&
      typeof (current as { __identity__: unknown }).__identity__ === 'bigint'
    ) {
      return u256ToHexString((current as { __identity__: bigint }).__identity__);
    }
    if (
      current &&
      typeof current === 'object' &&
      '__connection_id__' in current &&
      typeof (current as { __connection_id__: unknown }).__connection_id__ ===
        'bigint'
    ) {
      return u128ToHexString(
        (current as { __connection_id__: bigint }).__connection_id__
      );
    }
    if (current instanceof Uint8Array) {
      if (current.length < 25) {
        return `0x${toHex(current)}`;
      }
      const head = current.subarray(0, 10);
      return `Uint8Array(len=${current.length}, head=0x${toHex(head)})`;
    }
    if (Array.isArray(current) && current.length >= ARRAY_TRUNCATION_THRESHOLD) {
      const head = ssStringify(current.slice(0, ARRAY_PREVIEW_COUNT));
      return `Array(len=${current.length}, head=${head ?? '[]'})`;
    }
    return current;
  });

export const stdbLogger = (
  level: LogLevel,
  message: Lazy<any>,
  ...args: Lazy<any>
): void => {
  if (!shouldLog(level)) {
    return;
  }
  const resolvedMessage = resolveLazy(message);
  const resolvedArgs = args.map(resolveLazy);
  console.log(
    `%c${LogLevelIdentifierIcon[level]} ${level.toUpperCase()}%c ${resolvedMessage}`,
    LogStyle[level],
    LogTextStyle[level],
    ...resolvedArgs
  );
};
