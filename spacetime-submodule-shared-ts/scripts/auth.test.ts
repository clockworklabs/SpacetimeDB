// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  authModeCopy,
  authUrlState,
  clearAuthResultParams,
  mountAuthPanel,
} from '../src/auth-panel';

beforeEach(() => {
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

describe('authModeCopy', () => {
  it('returns login copy for the selected product', () => {
    expect(authModeCopy('login', 'Grid')).toEqual({
      title: 'Welcome to Grid',
      subtitle: 'Sign in to continue.',
      submit: 'Sign in',
      showEmail: true,
      showName: false,
      showPassword: true,
      showForgot: true,
      togglePrompt: "Don't have an account?",
      toggleText: 'Sign up',
    });
  });
});

describe('authUrlState', () => {
  it('reads a password reset token', () => {
    expect(
      authUrlState({
        pathname: '/auth/password/reset',
        search: '?token=reset-123',
      })
    ).toEqual({
      mode: 'reset',
      resetToken: 'reset-123',
      oauthError: undefined,
      verified: false,
    });
  });

  it('reads OAuth and verification results', () => {
    expect(
      authUrlState({ pathname: '/', search: '?error=denied&verified=1' })
    ).toEqual({
      mode: 'login',
      resetToken: undefined,
      oauthError: 'denied',
      verified: true,
    });
  });
});

describe('clearAuthResultParams', () => {
  it('removes callback results and retains unrelated parameters', () => {
    let nextUrl = '';
    clearAuthResultParams(
      { pathname: '/notes', search: '?error=denied&verified=1&tab=active' },
      {
        replaceState: (_data, _unused, url) => {
          nextUrl = String(url);
        },
      }
    );
    expect(nextUrl).toBe('/notes?tab=active');
  });
});

describe('mountAuthPanel', () => {
  it('submits password login through the supplied action', async () => {
    const login = vi.fn(async () => undefined);
    const root = document.createElement('div');
    document.body.append(root);
    mountAuthPanel(root, {
      productName: 'Grid',
      actions: {
        login,
        signup: vi.fn(async () => undefined),
        forgotPassword: vi.fn(async () => undefined),
        resetPassword: vi.fn(async () => undefined),
        oauthStart: vi.fn(),
      },
    });

    const email = root.querySelector<HTMLInputElement>('input[type="email"]');
    const password = root.querySelector<HTMLInputElement>(
      'input[type="password"]'
    );
    const form = root.querySelector<HTMLFormElement>('form');
    expect(email).not.toBeNull();
    expect(password).not.toBeNull();
    expect(form).not.toBeNull();
    email!.value = 'user@example.com';
    password!.value = 'password-123';
    form!.requestSubmit();

    await vi.waitFor(() => {
      expect(login).toHaveBeenCalledWith({
        email: 'user@example.com',
        password: 'password-123',
      });
    });
  });

  it('changes modes and enables configured OAuth providers', () => {
    const oauthStart = vi.fn();
    const root = document.createElement('div');
    document.body.append(root);
    const panel = mountAuthPanel(root, {
      productName: 'Chat',
      actions: {
        login: vi.fn(async () => undefined),
        signup: vi.fn(async () => undefined),
        forgotPassword: vi.fn(async () => undefined),
        resetPassword: vi.fn(async () => undefined),
        oauthStart,
      },
    });

    const google = root.querySelector<HTMLButtonElement>(
      '[data-provider="google"]'
    );
    const modeToggle =
      root.querySelectorAll<HTMLButtonElement>('.auth-link')[1];
    expect(google?.disabled).toBe(true);
    modeToggle.click();
    expect(root.querySelector('h1')?.textContent).toBe('Create account');

    panel.setProviders({ google: true });
    expect(google?.disabled).toBe(false);
    google?.click();
    expect(oauthStart).toHaveBeenCalledWith('google');
  });
});
