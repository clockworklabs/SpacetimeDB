import { ActionApplicationFailure } from './action-contract.js';
import type { BrowserCapability, BrowserPage } from './actor-action-runtime.js';

type Authentication = NonNullable<BrowserCapability['authentication']>;

export async function usesKeycloakLogin(page: BrowserPage, browser: BrowserCapability, mode: 'signup' | 'signin'): Promise<boolean> {
  if (!browser.authentication) return false;
  const username = page.locator(browser.testId(`${mode}-username`)).first();
  const signIn = page.locator(browser.testId('signin-toggle')).first();
  const signUp = page.locator(browser.testId('signup-toggle')).first();
  await (mode === 'signup' ? username.or(signUp).or(signIn) : username.or(signIn))
    .filter({ visible: true }).first().waitFor({ state: 'visible', timeout: browser.defaultWithin });
  if (await username.isVisible()) return false;
  const entry = mode === 'signup' && await signUp.isVisible() ? signUp : signIn;
  return await entry.getAttribute('data-auth-provider') === 'keycloak';
}

function isIssuerPage(currentUrl: string, issuer: string): boolean {
  const current = new URL(currentUrl);
  const expected = new URL(issuer);
  const realm = expected.pathname.replace(/\/$/, '');
  return current.origin === expected.origin
    && (current.pathname === realm || current.pathname.startsWith(`${realm}/`));
}

/** Observe the provider's real error only on its configured realm. */
export function keycloakElementSelector(
  currentUrl: string, testid: string, defaultSelector: string, authentication?: Authentication,
): string {
  if (testid !== 'auth-error' || !authentication || !isIssuerPage(currentUrl, authentication.issuer)) {
    return defaultSelector;
  }
  return '#input-error-username, #input-error-password, #input-error-password-confirm, #input-error-email, #input-error';
}

/** Drive the declared provider through its visible forms; never issue auth API calls. */
export async function authenticateWithKeycloak(options: {
  readonly page: BrowserPage;
  readonly browser: BrowserCapability;
  readonly mode: 'signup' | 'signin';
  readonly user: string;
  readonly password: string;
  readonly expectFailure?: boolean;
  readonly settleMs?: number;
  readonly signal: AbortSignal;
}): Promise<void> {
  const { page, browser, mode, user, password, signal } = options;
  const authentication = browser.authentication;
  if (!authentication) throw new Error('Keycloak browser action requires trusted authentication configuration');
  const appOrigin = new URL(page.url()).origin;
  const assertIssuer = () => {
    if (!isIssuerPage(page.url(), authentication.issuer)) {
      throw new ActionApplicationFailure('Account login did not open the configured identity provider');
    }
  };
  const fill = async (selector: string, value: string) => {
    assertIssuer();
    await page.locator(selector).first().fill(value);
  };
  const signIn = page.locator(browser.testId('signin-toggle')).first();
  const signUp = page.locator(browser.testId('signup-toggle')).first();
  const entry = mode === 'signup' ? signUp.or(signIn) : signIn;
  await entry.filter({ visible: true }).first().waitFor({ state: 'visible', timeout: browser.defaultWithin });
  await (mode === 'signup' && await signUp.isVisible() ? signUp : signIn).click({ timeout: browser.defaultWithin });
  await page.locator('#username').waitFor({ state: 'visible', timeout: browser.defaultWithin });
  assertIssuer();
  if (mode === 'signup' && !await page.locator('#password-confirm').isVisible()) {
    await page.locator('#kc-registration a').click({ timeout: browser.defaultWithin });
    await page.locator('#password-confirm').waitFor({ state: 'visible', timeout: browser.defaultWithin });
  }
  await fill('#username', user);
  if (mode === 'signup') {
    await fill('#password-confirm', password);
  }
  await fill('#password', password);
  assertIssuer();
  await page.locator(mode === 'signup' ? '#kc-register-form input[type="submit"]' : '#kc-login').click();
  if (options.expectFailure) {
    // The scenario must still observe a real auth error and no signed-in user.
    await browser.sleep(options.settleMs ?? 2000, signal);
    return;
  }
  const currentUser = page.locator(browser.testId('current-user')).first();
  await currentUser.waitFor({ state: 'visible', timeout: browser.defaultWithin * 2 });
  if (new URL(page.url()).origin !== appOrigin
    || !(await currentUser.innerText()).includes(user)) {
    throw new ActionApplicationFailure('Account login did not return the requested user to the application',
      { observation: { authenticationPath: 'keycloak' } });
  }
}
