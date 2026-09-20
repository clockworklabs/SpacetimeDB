import type { BrowserContext, Request, Response } from 'playwright';
import { ActionInconclusive } from './action-contract.js';

// Use a real browser and its cookie policy. Never copy the victim's headers,
// storage or token into the attacker page. Stored-effect checks own the verdict.
export async function crossOriginPost(context: Pick<BrowserContext, 'newPage' | 'grantPermissions'>,
  request: { readonly url: string; readonly method?: string; readonly body?: string | null },
  site: 'same-site' | 'cross-site', signal: AbortSignal) {
  const target = new URL(request.url);
  if (!['http:', 'https:'].includes(target.protocol) || target.username || target.password
    || (request.method ?? 'POST') !== 'POST') {
    throw new Error('Cross-origin probe requires an HTTP POST without URL credentials');
  }
  const origin = new URL(target.origin);
  origin.port = target.port === '18081' ? '18082' : '18081';
  if (site === 'cross-site') origin.hostname = target.hostname === '127.0.0.2' ? '127.0.0.3' : '127.0.0.2';
  origin.pathname = '/__stack_bench_origin__';
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('console', message => { if (message.type() === 'error' && errors.length < 3) errors.push(message.text().slice(0,500)); });
  let closeOnAbort: Promise<void> | undefined;
  const abort = () => { closeOnAbort ??= page.close(); void closeOnAbort.catch(() => {}); };
  signal.addEventListener('abort', abort, { once: true });
  try {
    signal.throwIfAborted();
    // The appliance uses private/loopback addresses. Grant only this fixture's
    // network permission; keep CORS, SameSite and application defenses enabled.
    await context.grantPermissions(['local-network-access'], { origin: origin.origin });
    await page.route(origin.href, route => route.fulfill({
      status: 200, contentType: 'text/html', body: '<!doctype html><title>Origin probe</title>',
    }));
    await page.goto(origin.href, { waitUntil: 'domcontentloaded', timeout: 10_000 });
    const actualOrigin = await page.evaluate(() => location.origin);
    if (actualOrigin !== origin.origin || actualOrigin === target.origin) {
      throw new Error('Cross-origin observer did not establish the attacker origin');
    }
    let sent: Request | undefined, response: Response | undefined, requestFailure: string | undefined;
    page.on('request', value => {
      if (value.url() === target.href && value.method() === 'POST') sent = value;
    });
    page.on('response', value => {
      if (value.request() === sent) response = value;
    });
    page.on('requestfailed', value => {
      if (value === sent) requestFailure = value.failure()?.errorText;
    });
    const browserResult = await page.evaluate(async ({ url, body }) => {
      try {
        const reply = await fetch(url, { method: 'POST', body, mode: 'no-cors',
          credentials: 'include', headers: { 'Content-Type': 'text/plain' },
          signal: AbortSignal.timeout(15_000) });
        return { type: reply.type, status: reply.status };
      } catch (error) { return { error: String(error) }; }
    }, { url: target.href, body: request.body });
    signal.throwIfAborted();
    // Opaque responses do not expose their body. Chromium can discard JSON
    // after fetch resolves, leaving Playwright's finished() pending. The browser
    // reply and observed headers prove delivery; stored effects own the verdict.
    if (!('type' in browserResult) || browserResult.type !== 'opaque' || !sent || !response
      || sent.redirectedTo()) {
      throw new ActionInconclusive(`Cross-origin POST did not retain a complete browser request and response: ${JSON.stringify({
        browserResult, requestObserved: Boolean(sent), responseObserved: Boolean(response),
        requestFailure, errors,
      })}`);
    }
    const headers = await sent.allHeaders();
    if (headers.origin !== origin.origin || headers.authorization) {
      throw new Error('Cross-origin request has an invalid origin or an injected authorization header');
    }
    return { site, sourceOrigin: origin.origin, targetOrigin: target.origin, localNetworkAccess: 'granted',
      method: 'POST', cookieSent: Boolean(headers.cookie), responseStatus: response.status(),
      browserResponseType: browserResult.type };
  } finally {
    signal.removeEventListener('abort', abort);
    await (closeOnAbort ?? page.close());
  }
}
