// @vitest-environment happy-dom
import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, renderHook, act, cleanup } from '@testing-library/react';
import * as React from 'react';
import { StrictMode } from 'react';
import { ConnectionId } from '../src';
import {
  SpacetimeDBMultiProvider,
  useSpacetimeDB,
  useSpacetimeDBStatus,
} from '../src/react';

/**
 * Minimal mock DbConnection + Builder — enough surface for the pool + React
 * layer to drive. Deliberately NOT using the real SDK builder (which would
 * require a WebSocket to SpacetimeDB).
 */

type ErrorContextInterface = { isActive: boolean };

class MockConnection {
  isActive = false;
  identity: any = undefined;
  token: string | undefined = undefined;
  connectionId = ConnectionId.random();
  disconnected = false;

  #onConnectCbs = new Set<(conn: MockConnection) => void>();
  #onDisconnectCbs = new Set<
    (ctx: ErrorContextInterface, err?: Error) => void
  >();
  #onConnectErrorCbs = new Set<
    (ctx: ErrorContextInterface, err: Error) => void
  >();

  disconnect(): void {
    this.disconnected = true;
    this.isActive = false;
  }
  removeOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnectCbs.delete(cb);
  }
  removeOnDisconnect(
    cb: (ctx: ErrorContextInterface, err?: Error) => void
  ): void {
    this.#onDisconnectCbs.delete(cb);
  }
  removeOnConnectError(
    cb: (ctx: ErrorContextInterface, err: Error) => void
  ): void {
    this.#onConnectErrorCbs.delete(cb);
  }
  _registerOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnectCbs.add(cb);
  }
  _registerOnDisconnect(
    cb: (ctx: ErrorContextInterface, err?: Error) => void
  ): void {
    this.#onDisconnectCbs.add(cb);
  }
  _registerOnConnectError(
    cb: (ctx: ErrorContextInterface, err: Error) => void
  ): void {
    this.#onConnectErrorCbs.add(cb);
  }

  simulateConnect(): void {
    this.isActive = true;
    for (const cb of this.#onConnectCbs) cb(this);
  }
}

class MockBuilder {
  constructor(
    private conn: MockConnection,
    private uri: string,
    private moduleName: string
  ) {}
  getUri(): string {
    return this.uri;
  }
  getModuleName(): string {
    return this.moduleName;
  }
  build(): MockConnection {
    return this.conn;
  }
  onConnect(cb: (conn: MockConnection) => void): MockBuilder {
    this.conn._registerOnConnect(cb);
    return this;
  }
  onDisconnect(
    cb: (ctx: ErrorContextInterface, err?: Error) => void
  ): MockBuilder {
    this.conn._registerOnDisconnect(cb);
    return this;
  }
  onConnectError(
    cb: (ctx: ErrorContextInterface, err: Error) => void
  ): MockBuilder {
    this.conn._registerOnConnectError(cb);
    return this;
  }
}

// Unique per-test URI so each test lives in its own ConnectionManager
// namespace — the manager is a singleton and pending releases from previous
// tests can otherwise mask fresh retains.
let uriCounter = 0;
function makeBuilder(moduleName: string): {
  conn: MockConnection;
  builder: MockBuilder;
  uri: string;
} {
  const uri = `ws://test-${++uriCounter}`;
  const conn = new MockConnection();
  const builder = new MockBuilder(conn, uri, moduleName);
  return { conn, builder, uri };
}

// Silence expected error messages during hook-throws tests.
let consoleErrorSpy: ReturnType<typeof vi.spyOn> | null = null;
beforeEach(() => {
  consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  consoleErrorSpy?.mockRestore();
  cleanup();
});

describe('SpacetimeDBMultiProvider', () => {
  test('renders children', () => {
    const { builder } = makeBuilder('mod-a');
    const { getByText } = render(
      <SpacetimeDBMultiProvider connections={{ a: builder as any }}>
        <div>hello</div>
      </SpacetimeDBMultiProvider>
    );
    expect(getByText('hello')).toBeTruthy();
  });

  test('useSpacetimeDB(key) returns the state for the matching label', () => {
    const { conn: connA, builder: builderA } = makeBuilder('mod-a');
    const { conn: connB, builder: builderB } = makeBuilder('mod-b');

    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <SpacetimeDBMultiProvider
        connections={{ a: builderA as any, b: builderB as any }}
      >
        {children}
      </SpacetimeDBMultiProvider>
    );

    const { result } = renderHook(
      () => ({ a: useSpacetimeDB('a'), b: useSpacetimeDB('b') }),
      { wrapper }
    );

    expect(result.current.a.getConnection()).toBe(connA);
    expect(result.current.b.getConnection()).toBe(connB);
  });

  test("useSpacetimeDB('missing') throws with known-keys list", () => {
    const { builder } = makeBuilder('mod-a');
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <SpacetimeDBMultiProvider connections={{ a: builder as any }}>
        {children}
      </SpacetimeDBMultiProvider>
    );

    expect(() =>
      renderHook(() => useSpacetimeDB('missing'), { wrapper })
    ).toThrow(/no connection registered.*Known keys: a/);
  });

  test('useSpacetimeDB(key) outside a MultiProvider throws', () => {
    expect(() => renderHook(() => useSpacetimeDB('a'))).toThrow(
      /SpacetimeDBMultiProvider/
    );
  });

  test('useSpacetimeDBStatus returns every labelled connection state', () => {
    const { builder: builderA } = makeBuilder('mod-a');
    const { builder: builderB } = makeBuilder('mod-b');

    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <SpacetimeDBMultiProvider
        connections={{ alpha: builderA as any, beta: builderB as any }}
      >
        {children}
      </SpacetimeDBMultiProvider>
    );

    const { result } = renderHook(() => useSpacetimeDBStatus(), { wrapper });
    expect(Array.from(result.current.keys()).sort()).toEqual(['alpha', 'beta']);
  });

  test('useSpacetimeDBStatus outside a MultiProvider throws', () => {
    expect(() => renderHook(() => useSpacetimeDBStatus())).toThrow(
      /SpacetimeDBMultiProvider/
    );
  });

  test('reacts to onConnect state changes', () => {
    const { conn, builder } = makeBuilder('mod-live');
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <SpacetimeDBMultiProvider connections={{ m: builder as any }}>
        {children}
      </SpacetimeDBMultiProvider>
    );

    const { result } = renderHook(() => useSpacetimeDB('m'), { wrapper });
    expect(result.current.isActive).toBe(false);

    act(() => {
      conn.simulateConnect();
    });

    expect(result.current.isActive).toBe(true);
  });

  test('StrictMode mount → unmount → remount keeps the connection alive', () => {
    const { conn, builder } = makeBuilder('mod-strict');

    const { unmount } = render(
      <StrictMode>
        <SpacetimeDBMultiProvider connections={{ m: builder as any }}>
          <div>child</div>
        </SpacetimeDBMultiProvider>
      </StrictMode>
    );

    // StrictMode mounts, unmounts, remounts — retain/release/retain —
    // but the pool's deferred cleanup should keep the connection alive.
    expect(conn.disconnected).toBe(false);

    unmount();
  });

  test('inline connections prop does not churn retain/release each render', () => {
    const { conn, builder } = makeBuilder('mod-inline');
    // We'll pass a fresh object literal on every render below. If the provider
    // trusted object identity alone, each re-render would retain→release→retain
    // in the same tick; the deferred cleanup would catch it but ConnectionId
    // inside the snapshot would churn. The content-based signature guard makes
    // the entries array reference stable across renders with the same shape.

    function Harness({ tick }: { tick: number }): React.JSX.Element {
      return (
        <SpacetimeDBMultiProvider connections={{ m: builder as any }}>
          <span data-testid="tick">{tick}</span>
        </SpacetimeDBMultiProvider>
      );
    }

    const { rerender } = render(<Harness tick={0} />);
    for (let i = 1; i < 5; i++) rerender(<Harness tick={i} />);

    // Connection should still be the same instance and not disconnected.
    expect(conn.disconnected).toBe(false);
  });
});
