import { beforeEach, describe, expect, test, vi } from 'vitest';
import { ConnectionId, Identity } from '../src';
import {
  getGlobalLogLevel,
  setGlobalLogLevel,
  stringify,
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

  test('stringify renders short Uint8Array as full hex', () => {
    const payload = new Uint8Array([0, 1, 15, 16, 255]);
    expect(stringify({ payload })).toBe('{"payload":"0x00010f10ff"}');
  });

  test('stringify renders long Uint8Array as summary', () => {
    const payload = Uint8Array.from(Array.from({ length: 30 }, (_, i) => i));
    expect(stringify({ payload })).toBe(
      '{"payload":"Uint8Array(len=30, head=0x00010203040506070809)"}'
    );
  });

  test('stringify renders identity wrappers as hex string', () => {
    const identity = Identity.fromString(
      'c2005a97608f921d92a0f68cb32ecbf10829b5221993a6fba2f62058cc9b3233'
    );
    expect(stringify({ identity })).toBe(
      '{"identity":"c2005a97608f921d92a0f68cb32ecbf10829b5221993a6fba2f62058cc9b3233"}'
    );
  });

  test('stringify renders connection id wrappers as hex string', () => {
    const connectionId = ConnectionId.fromString(
      'e4df11f8f7ad5f05f4e90e401a3890ca'
    );
    expect(stringify({ connectionId })).toBe(
      '{"connectionId":"e4df11f8f7ad5f05f4e90e401a3890ca"}'
    );
  });

  test('stringify redacts token-like keys', () => {
    expect(
      stringify({
        token: 'secret-token',
        authToken: 'auth-secret',
        authorization: 'Bearer abc',
        accessToken: 'access-secret',
        refreshToken: 'refresh-secret',
      })
    ).toBe(
      '{"accessToken":"[REDACTED]","authToken":"[REDACTED]","authorization":"[REDACTED]","refreshToken":"[REDACTED]","token":"[REDACTED]"}'
    );
  });

  test('stringify redacts nested token fields', () => {
    expect(
      stringify({
        tag: 'InitialConnection',
        value: { token: 'jwt', nested: { authToken: 'inner-secret' } },
      })
    ).toBe(
      '{"tag":"InitialConnection","value":{"nested":{"authToken":"[REDACTED]"},"token":"[REDACTED]"}}'
    );
  });

  test('stringify truncates long normal arrays', () => {
    const arr = Array.from({ length: 30 }, (_, i) => i);
    expect(stringify({ arr })).toBe(
      '{"arr":"Array(len=30, head=[0,1,2,3,4,5,6,7,8,9])"}'
    );
  });

  test('stringify preserves short normal arrays', () => {
    const arr = [1, 2, 3];
    expect(stringify({ arr })).toBe('{"arr":[1,2,3]}');
  });
});
