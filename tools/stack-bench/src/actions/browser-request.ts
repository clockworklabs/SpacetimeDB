import { request, type BrowserContext, type Cookie } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';

const registrations = new WeakMap<object, {
  context: BrowserContext;
  userAgent: string;
  isOffline?: () => boolean;
}>();

export function registerBrowserRequest(context: BrowserContext, userAgent: string,
  isOffline?: () => boolean): void {
  registrations.set(context.request, { context, userAgent, isOffline });
}

const cookieKey = (cookie: Cookie) => JSON.stringify([cookie.name, cookie.domain, cookie.path]);
const sameCookie = (a: Cookie | undefined, b: Cookie | undefined) => a === b || (a !== undefined && b !== undefined
  && a.value === b.value && a.expires === b.expires && a.httpOnly === b.httpOnly
  && a.secure === b.secure && a.sameSite === b.sameSite && a.partitionKey === b.partitionKey);

export async function withBrowserRequest<T, R extends object>(original: R,
  callback: (request: R) => Promise<T>, updateCookies = true): Promise<T> {
  const registration = registrations.get(original);
  if (!registration) return callback(original);
  const { context, userAgent, isOffline } = registration;
  if (isOffline?.()) inconclusive('replay-unavailable', {
    actor: 'browser context', detail: 'The browser context is offline during an active network cut.',
  });
  const before = await context.cookies();
  if (before.some(cookie => cookie.partitionKey)) inconclusive('replay-unavailable', {
    actor: 'browser context', detail: 'Partitioned cookies cannot be preserved by the evaluator request context.',
  });
  // Chromium's loopback proxy is in its container. Evaluator HTTP runs here.
  const api = await request.newContext({ userAgent, storageState: { cookies: before, origins: [] } });
  try {
    if (isOffline?.()) inconclusive('replay-unavailable', {
      actor: 'browser context', detail: 'The browser context is offline during an active network cut.',
    });
    return await callback(api as unknown as R);
  } finally {
    try {
      // Routed replies apply cookies through browser fulfillment, not this jar.
      if (updateCookies) {
        const after = (await api.storageState()).cookies;
        const beforeByKey = new Map(before.map(cookie => [cookieKey(cookie), cookie]));
        const afterByKey = new Map(after.map(cookie => [cookieKey(cookie), cookie]));
        const current = new Map((await context.cookies()).map(cookie => [cookieKey(cookie), cookie]));
        const updates: Cookie[] = [];
        for (const key of new Set([...beforeByKey.keys(), ...afterByKey.keys()])) {
          const previous = beforeByKey.get(key);
          const next = afterByKey.get(key);
          if (sameCookie(previous, next) || sameCookie(current.get(key), next)) continue;
          if (!sameCookie(current.get(key), previous)) inconclusive('replay-unavailable', {
            actor: 'browser context', detail: 'A browser cookie changed concurrently with the evaluator request.',
          });
          updates.push(next ?? { ...previous!, expires: 1 });
        }
        if (updates.length) await context.addCookies(updates);
      }
    } finally { await api.dispose(); }
  }
}
