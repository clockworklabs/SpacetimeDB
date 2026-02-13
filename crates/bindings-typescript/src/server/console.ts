import type { u32 } from 'spacetime:sys@2.0';
import { sys } from './runtime';
import inspect from 'object-inspect';

const fmtLog = (...data: any[]) =>
  data.map(x => (typeof x === 'string' ? x : inspect(x))).join(' ');

const console_level_error = 0;
const console_level_warn = 1;
const console_level_info = 2;
const console_level_debug = 3;
const console_level_trace = 4;
const _console_level_panic = 101;

const timerMap = new Map<string, u32>();

export const console: Console = {
  // @ts-expect-error we want a blank prototype, but typescript complains
  __proto__: {},
  [Symbol.toStringTag]: 'console',
  assert: (condition = false, ...data: any[]) => {
    if (!condition) {
      sys.console_log(console_level_error, fmtLog(...data));
    }
  },
  clear: () => {},
  debug: (...data: any[]) => {
    sys.console_log(console_level_debug, fmtLog(...data));
  },
  error: (...data: any[]) => {
    sys.console_log(console_level_error, fmtLog(...data));
  },
  info: (...data: any[]) => {
    sys.console_log(console_level_info, fmtLog(...data));
  },
  log: (...data: any[]) => {
    sys.console_log(console_level_info, fmtLog(...data));
  },
  table: (tabularData: any, _properties: any) => {
    sys.console_log(console_level_info, fmtLog(tabularData));
  },
  trace: (...data: any[]) => {
    sys.console_log(console_level_trace, fmtLog(...data));
  },
  warn: (...data: any[]) => {
    sys.console_log(console_level_warn, fmtLog(...data));
  },
  dir: (_item: any, _options: any) => {},
  dirxml: (..._data: any[]) => {},
  // Counting
  count: (_label = 'default') => {},
  countReset: (_label = 'default') => {},
  // Grouping
  group: (..._data: any[]) => {},
  groupCollapsed: (..._data: any[]) => {},
  groupEnd: () => {},
  // Timing
  time: (label = 'default') => {
    if (timerMap.has(label)) {
      sys.console_log(console_level_warn, `Timer '${label}' already exists.`);
      return;
    }
    timerMap.set(label, sys.console_timer_start(label));
  },
  timeLog: (label = 'default', ...data: any[]) => {
    sys.console_log(console_level_info, fmtLog(label, ...data));
  },
  timeEnd: (label = 'default') => {
    const spanId = timerMap.get(label);
    if (spanId === undefined) {
      sys.console_log(console_level_warn, `Timer '${label}' does not exist.`);
      return;
    }
    sys.console_timer_end(spanId);
    timerMap.delete(label);
  },
  // Additional console methods to satisfy the Console interface
  timeStamp: () => {},
  profile: () => {},
  profileEnd: () => {},
};
