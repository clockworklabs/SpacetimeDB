import { githubIcon, googleIcon, spacetimeMark } from './icons';

export type AuthMode = 'login' | 'signup' | 'forgot' | 'reset';

export type AuthProviderAvailability = {
  google: boolean;
  github: boolean;
};

export type AuthPanelActions = {
  login(args: { email: string; password: string }): Promise<void>;
  signup(args: {
    email: string;
    password: string;
    name?: string;
  }): Promise<void>;
  forgotPassword(email: string): Promise<void>;
  resetPassword(token: string, newPassword: string): Promise<void>;
  oauthStart(provider: 'google' | 'github'): void;
};

export type AuthPanelOptions = {
  productName: string;
  actions: AuthPanelActions;
  logoSrc?: string;
  initialMode?: AuthMode;
  resetToken?: string;
};

export type AuthPanelController = {
  destroy(): void;
  focus(): void;
  setMode(mode: AuthMode, resetToken?: string): void;
  setProviders(providers: Partial<AuthProviderAvailability>): void;
  showMessage(kind: 'error' | 'success', message: string): void;
};

type AuthModeCopy = {
  title: string;
  subtitle: string;
  submit: string;
  showEmail: boolean;
  showName: boolean;
  showPassword: boolean;
  showForgot: boolean;
  togglePrompt: string;
  toggleText: string;
};

export function authModeCopy(
  mode: AuthMode,
  productName: string
): AuthModeCopy {
  switch (mode) {
    case 'signup':
      return {
        title: 'Create account',
        subtitle: `Create an account to continue to ${productName}.`,
        submit: 'Create account',
        showEmail: true,
        showName: true,
        showPassword: true,
        showForgot: false,
        togglePrompt: 'Already have an account?',
        toggleText: 'Sign in',
      };
    case 'forgot':
      return {
        title: 'Reset your password',
        subtitle:
          'Enter your email. A reset link will be sent if the account exists.',
        submit: 'Send reset email',
        showEmail: true,
        showName: false,
        showPassword: false,
        showForgot: false,
        togglePrompt: '',
        toggleText: 'Back to sign in',
      };
    case 'reset':
      return {
        title: 'Set a new password',
        subtitle: 'Enter a new password with at least eight characters.',
        submit: 'Reset password',
        showEmail: false,
        showName: false,
        showPassword: true,
        showForgot: false,
        togglePrompt: '',
        toggleText: 'Back to sign in',
      };
    case 'login':
      return {
        title: `Welcome to ${productName}`,
        subtitle: 'Sign in to continue.',
        submit: 'Sign in',
        showEmail: true,
        showName: false,
        showPassword: true,
        showForgot: true,
        togglePrompt: "Don't have an account?",
        toggleText: 'Sign up',
      };
  }
}

export function authUrlState(location: Pick<Location, 'pathname' | 'search'>): {
  mode: AuthMode;
  resetToken?: string;
  oauthError?: string;
  verified: boolean;
} {
  const params = new URLSearchParams(location.search);
  const resetToken =
    location.pathname === '/auth/password/reset'
      ? (params.get('token') ?? undefined)
      : undefined;
  return {
    mode: resetToken ? 'reset' : 'login',
    resetToken,
    oauthError: params.get('error') ?? undefined,
    verified: params.get('verified') === '1',
  };
}

export function clearAuthResultParams(
  location: Pick<Location, 'pathname' | 'search'>,
  history: Pick<History, 'replaceState'>
): void {
  const params = new URLSearchParams(location.search);
  const hadResult = params.has('error') || params.has('verified');
  if (!hadResult) return;
  params.delete('error');
  params.delete('verified');
  const search = params.toString();
  history.replaceState(
    {},
    '',
    `${location.pathname}${search ? `?${search}` : ''}`
  );
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

function field(
  id: string,
  labelText: string,
  type: HTMLInputElement['type'],
  autocomplete: HTMLInputElement['autocomplete'],
  placeholder: string
): { wrapper: HTMLDivElement; input: HTMLInputElement } {
  const wrapper = element('div', 'auth-field');
  const label = element('label');
  label.htmlFor = id;
  label.textContent = labelText;
  const input = element('input');
  input.id = id;
  input.type = type;
  input.autocomplete = autocomplete;
  input.placeholder = placeholder;
  wrapper.append(label, input);
  return { wrapper, input };
}

function oauthButton(
  provider: 'google' | 'github',
  label: string
): HTMLButtonElement {
  const button = element('button', 'btn oauth block');
  button.type = 'button';
  button.dataset.provider = provider;
  button.append(provider === 'google' ? googleIcon() : githubIcon(), label);
  return button;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function mountAuthPanel(
  root: HTMLElement,
  options: AuthPanelOptions
): AuthPanelController {
  const abort = new AbortController();
  let mode = options.initialMode ?? 'login';
  let resetToken = options.resetToken;
  let providers: AuthProviderAvailability = { google: false, github: false };
  root.classList.add('example-auth-panel');

  const form = element('form', 'auth-card');

  const logoLink = element('a');
  logoLink.href = 'https://spacetimedb.com';
  logoLink.target = '_blank';
  logoLink.rel = 'noopener noreferrer';
  logoLink.className = 'auth-logo-link';
  const logo = options.logoSrc
    ? Object.assign(element('img', 'auth-logo'), {
        src: options.logoSrc,
        alt: 'SpacetimeDB',
      })
    : spacetimeMark();
  logoLink.append(logo);

  const title = element('h1');
  const subtitle = element('p', 'auth-sub');
  const oauth = element('div', 'auth-oauth');
  const google = oauthButton('google', 'Continue with Google');
  const github = oauthButton('github', 'Continue with GitHub');
  oauth.append(google, github);

  const divider = element('div', 'auth-divider');
  const dividerText = element('span');
  dividerText.textContent = 'or';
  divider.append(dividerText);

  const email = field(
    'example-auth-email',
    'Email',
    'email',
    'email',
    'you@example.com'
  );
  const name = field(
    'example-auth-name',
    'Name (optional)',
    'text',
    'name',
    'Display name'
  );
  const password = field(
    'example-auth-password',
    'Password',
    'password',
    'current-password',
    ''
  );
  password.input.minLength = 8;

  const message = element('p', 'auth-message');
  message.setAttribute('role', 'status');
  message.setAttribute('aria-live', 'polite');
  message.hidden = true;

  const submit = element('button', 'btn primary block');
  submit.type = 'submit';

  const forgotFoot = element('p', 'auth-foot');
  const forgot = element('button', 'auth-link');
  forgot.type = 'button';
  forgot.textContent = 'Forgot password?';
  forgotFoot.append(forgot);

  const toggleFoot = element('p', 'auth-foot');
  const togglePrompt = element('span');
  const toggle = element('button', 'auth-link');
  toggle.type = 'button';
  toggleFoot.append(togglePrompt, document.createTextNode(' '), toggle);

  form.append(
    logoLink,
    title,
    subtitle,
    oauth,
    divider,
    email.wrapper,
    name.wrapper,
    password.wrapper,
    message,
    submit,
    forgotFoot,
    toggleFoot
  );
  root.replaceChildren(form);

  function showMessage(kind: 'error' | 'success', text: string): void {
    message.className = `auth-message ${kind}`;
    message.textContent = text;
    message.hidden = false;
  }

  function clearMessage(): void {
    message.textContent = '';
    message.hidden = true;
  }

  function applyMode(): void {
    const copy = authModeCopy(mode, options.productName);
    title.textContent = copy.title;
    subtitle.textContent = copy.subtitle;
    submit.textContent = copy.submit;
    email.wrapper.hidden = !copy.showEmail;
    email.input.required = copy.showEmail;
    name.wrapper.hidden = !copy.showName;
    password.wrapper.hidden = !copy.showPassword;
    password.input.required = copy.showPassword;
    password.input.autocomplete =
      mode === 'login' ? 'current-password' : 'new-password';
    password.input.placeholder = mode === 'login' ? '' : 'Minimum 8 characters';
    forgotFoot.hidden = !copy.showForgot;
    togglePrompt.textContent = copy.togglePrompt;
    toggle.textContent = copy.toggleText;
    clearMessage();
  }

  function applyProviders(): void {
    for (const [button, enabled, envNames] of [
      [google, providers.google, 'GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET'],
      [github, providers.github, 'GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET'],
    ] as const) {
      button.disabled = !enabled;
      button.setAttribute('aria-disabled', String(!enabled));
      button.title = enabled ? '' : `Set ${envNames} in .env`;
    }
  }

  async function run(action: () => Promise<void>): Promise<void> {
    clearMessage();
    submit.disabled = true;
    form.setAttribute('aria-busy', 'true');
    try {
      await action();
    } catch (error) {
      showMessage('error', errorMessage(error));
    } finally {
      submit.disabled = false;
      form.removeAttribute('aria-busy');
    }
  }

  form.addEventListener(
    'submit',
    event => {
      event.preventDefault();
      void run(async () => {
        const emailValue = email.input.value.trim();
        const passwordValue = password.input.value;
        if (mode === 'signup') {
          await options.actions.signup({
            email: emailValue,
            password: passwordValue,
            name: name.input.value.trim() || undefined,
          });
          return;
        }
        if (mode === 'forgot') {
          await options.actions.forgotPassword(emailValue);
          mode = 'login';
          applyMode();
          showMessage(
            'success',
            'If the account exists, a password reset link was sent.'
          );
          return;
        }
        if (mode === 'reset') {
          if (!resetToken) throw new Error('auth.missing_reset_token');
          await options.actions.resetPassword(resetToken, passwordValue);
          resetToken = undefined;
          mode = 'login';
          window.history.replaceState({}, '', '/');
          applyMode();
          showMessage(
            'success',
            'Password reset. Sign in with the new password.'
          );
          return;
        }
        await options.actions.login({
          email: emailValue,
          password: passwordValue,
        });
      });
    },
    { signal: abort.signal }
  );

  forgot.addEventListener(
    'click',
    () => {
      mode = 'forgot';
      applyMode();
      email.input.focus();
    },
    { signal: abort.signal }
  );

  toggle.addEventListener(
    'click',
    () => {
      mode =
        mode === 'forgot' || mode === 'reset'
          ? 'login'
          : mode === 'login'
            ? 'signup'
            : 'login';
      applyMode();
      email.input.focus();
    },
    { signal: abort.signal }
  );

  for (const button of [google, github]) {
    button.addEventListener(
      'click',
      () => {
        const provider = button.dataset.provider as 'google' | 'github';
        if (!providers[provider]) {
          showMessage('error', `${provider} OAuth is not configured.`);
          return;
        }
        options.actions.oauthStart(provider);
      },
      { signal: abort.signal }
    );
  }

  applyMode();
  applyProviders();

  return {
    destroy() {
      abort.abort();
      root.replaceChildren();
      root.classList.remove('example-auth-panel');
    },
    focus() {
      (mode === 'reset' ? password.input : email.input).focus();
    },
    setMode(nextMode, nextResetToken) {
      mode = nextMode;
      resetToken = nextResetToken;
      applyMode();
    },
    setProviders(nextProviders) {
      providers = { ...providers, ...nextProviders };
      applyProviders();
    },
    showMessage,
  };
}
