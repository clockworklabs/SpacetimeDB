import { beforeEach, describe, expect, test, vi } from 'vitest';
import {
  getGlobalLogLevel,
  setGlobalLogLevel,
  stdbLogger,
} from '../src/sdk/logger';

describe('logger', () => {
  beforeEach(() => {
    setGlobalLogLevel('debug');
    vi.restoreAllMocks();
  });

  test('setGlobalLogLevel controls emitted logs', () => {
    const spy = vi.spyOn(console, 'log').mockImplementation(() => {});

    setGlobalLogLevel('warn');
    stdbLogger('info', 'info message');
    stdbLogger('warn', 'warn message');
    stdbLogger('error', 'error message');

    expect(spy).toHaveBeenCalledTimes(2);
  });

  test('lazy log messages are only evaluated when emitted', () => {
    const spy = vi.spyOn(console, 'log').mockImplementation(() => {});
    const lazyMessage = vi.fn(() => 'computed');

    setGlobalLogLevel('error');
    stdbLogger('debug', lazyMessage);
    expect(lazyMessage).not.toHaveBeenCalled();

    stdbLogger('error', lazyMessage);
    expect(lazyMessage).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  test('trace logs are only emitted at trace level', () => {
    const spy = vi.spyOn(console, 'log').mockImplementation(() => {});

    setGlobalLogLevel('debug');
    stdbLogger('trace', 'trace message');
    expect(spy).not.toHaveBeenCalled();

    setGlobalLogLevel('trace');
    stdbLogger('trace', 'trace message');
    expect(spy).toHaveBeenCalledTimes(1);
  });

  test('getGlobalLogLevel returns the current level', () => {
    setGlobalLogLevel('info');
    expect(getGlobalLogLevel()).toBe('info');
  });
});