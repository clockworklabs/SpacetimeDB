import { errors } from 'playwright';
import { ActionInconclusive } from './action-contract.js';

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
    throw error;
  }
}
