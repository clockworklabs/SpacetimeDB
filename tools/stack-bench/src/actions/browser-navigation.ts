import { errors } from 'playwright';
import type { Page, Request } from 'playwright';
import { ActionApplicationFailure, ActionInconclusive } from './action-contract.js';

// Navigation did not reach a testable page. A timeout alone does not prove
// an application defect; external resources can delay DOMContentLoaded.
export async function runApplicationNavigation(operation: () => Promise<unknown>,
  page?: Partial<Pick<Page, 'on' | 'off'>>): Promise<void> {
  const pending = new Map<Request, string>();
  const requested = (request: Request) => {
    if (pending.size >= 20) return;
    try {
      const url = new URL(request.url());
      const origin = ['http:', 'https:'].includes(url.protocol) ? url.origin : url.protocol;
      pending.set(request, `${request.resourceType()} ${origin.slice(0, 200)}`);
    } catch { /* Do not record full URLs or credentials. */ }
  };
  const completed = (request: Request) => { pending.delete(request); };
  page?.on?.('request', requested);
  page?.on?.('requestfinished', completed);
  page?.on?.('requestfailed', completed);
  try {
    await operation();
  } catch (error) {
    if (error instanceof errors.TimeoutError) {
      throw new ActionInconclusive('application navigation timed out before the page was ready', {
        retryable: true,
        expected: 'DOMContentLoaded before the navigation deadline',
        observation: { pendingResources: [...pending.values()] },
      });
    }
    if (error instanceof Error && /net::ERR_CONNECTION_REFUSED\b/.test(error.message)) {
      throw new ActionApplicationFailure('application refused the navigation connection', {
        expected: 'a reachable application page',
      });
    }
    if (error instanceof Error && /net::ERR_[A-Z_]+\b/.test(error.message)) {
      throw new ActionInconclusive('application navigation failed before the page was ready', {
        retryable: true,
        expected: 'a reachable application page',
      });
    }
    throw error;
  } finally {
    page?.off?.('request', requested);
    page?.off?.('requestfinished', completed);
    page?.off?.('requestfailed', completed);
  }
}
