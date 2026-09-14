import { errors } from 'playwright';
import { ActionApplicationFailure, ActionInconclusive } from './action-contract.js';

// Navigation did not reach a testable page. A timeout alone does not prove
// an application defect; external resources can delay DOMContentLoaded.
export async function runApplicationNavigation(operation: () => Promise<unknown>): Promise<void> {
  try {
    await operation();
  } catch (error) {
    if (error instanceof errors.TimeoutError) {
      throw new ActionInconclusive('application navigation timed out before the page was ready', {
        expected: 'DOMContentLoaded before the navigation deadline',
      });
    }
    if (error instanceof Error && /net::ERR_CONNECTION_REFUSED\b/.test(error.message)) {
      throw new ActionApplicationFailure('application refused the navigation connection', {
        expected: 'a reachable application page',
      });
    }
    if (error instanceof Error && /net::ERR_[A-Z_]+\b/.test(error.message)) {
      throw new ActionInconclusive('application navigation failed before the page was ready', {
        expected: 'a reachable application page',
      });
    }
    throw error;
  }
}
