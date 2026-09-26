import type { BrowserContext, Request, Response } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';
import { runBrowserInfrastructureOperation } from '../evidence/harness-errors.js';

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
  const targetRequests = new Set<string>();
  const targetResponses = new Map<string, number>();
  const errors: string[] = [];
  page.on('console', message => { if (message.type() === 'error' && errors.length < 3) errors.push(message.text().slice(0,500)); });
  let closeOnAbort: Promise<void> | undefined;
  const abort = () => { closeOnAbort ??= page.close(); void closeOnAbort.catch(() => {}); };
  signal.addEventListener('abort', abort, { once: true });
  let observer: Awaited<ReturnType<BrowserContext['newCDPSession']>> | undefined;
  try {
    signal.throwIfAborted();
    // The appliance uses private/loopback addresses. Grant only this fixture's
    // network permission; keep CORS, SameSite and application defenses enabled.
    // The harness serves this page itself, so a failure here is not app evidence.
    await runBrowserInfrastructureOperation('cross-origin fixture', async () => {
      await context.grantPermissions(['local-network-access'], { origin: origin.origin });
      await page.route(origin.href, route => route.fulfill({
        status: 200, contentType: 'text/html', body: '<!doctype html><title>Origin probe</title>',
      }));
      await page.goto(origin.href, { waitUntil: 'domcontentloaded', timeout: 10_000 });
    });
    const actualOrigin = await page.evaluate(() => location.origin);
    if (actualOrigin !== origin.origin || actualOrigin === target.origin) {
      throw new Error('Cross-origin observer did not establish the attacker origin');
    }
    observer = await page.context().newCDPSession(page);
    observer.on('Network.requestWillBeSent', event => {
      if (event.request.url === target.href && event.request.method === 'POST') targetRequests.add(event.requestId);
    });
    observer.on('Network.responseReceivedExtraInfo', event => {
      targetResponses.set(event.requestId, event.statusCode);
    });
    await observer.send('Network.enable');
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
    const networkStatus = targetRequests.size === 1
      ? targetResponses.get([...targetRequests][0]!) : undefined;
    const browserAccepted = 'type' in browserResult && browserResult.type === 'opaque';
    const policyBlocked = requestFailure === 'net::ERR_BLOCKED_BY_RESPONSE.NotSameOrigin';
    if (!sent || sent.redirectedTo() || networkStatus === undefined
      || networkStatus >= 300 && networkStatus < 400
      || !(browserAccepted && response || policyBlocked && !response)) {
      inconclusive('replay-unavailable', { actor: 'cross-origin browser', detail: `Cross-origin POST did not retain a complete browser request and response: ${JSON.stringify({
        browserResult, requestObserved: Boolean(sent), responseObserved: Boolean(response),
        networkStatus, requestFailure, errors,
      })}` });
    }
    const headers = await sent.allHeaders();
    if (headers.origin !== origin.origin || headers.authorization) {
      throw new Error('Cross-origin request has an invalid origin or an injected authorization header');
    }
    return { site, sourceOrigin: origin.origin, targetOrigin: target.origin, localNetworkAccess: 'granted',
      method: 'POST', cookieSent: Boolean(headers.cookie), responseStatus: networkStatus,
      browserResponseType: browserAccepted ? browserResult.type : 'blocked-by-policy' };
  } finally {
    signal.removeEventListener('abort', abort);
    await observer?.detach().catch(() => {});
    await (closeOnAbort ?? page.close());
  }
}
