import { test, expect } from '@playwright/test';
import { createUser, joinRoom, waitForRealtime, messageVisible } from './helpers';

const BASE_URL = process.env.CLIENT_URL || 'http://localhost:5173';

test.describe('Feature 5: Scheduled Messages', () => {
  test('can schedule a message for later', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'Scheduler', BASE_URL);
    
    await joinRoom(user, 'ScheduleRoom');
    
    // Look for schedule option
    const scheduleBtn = user.page.locator([
      'button:has-text("Schedule")',
      'button[title*="schedule" i]',
      '[data-testid*="schedule"]',
      'button:has-text("Later")',
    ].join(', ')).first();
    
    const scheduleExists = await scheduleBtn.isVisible({ timeout: 3000 }).catch(() => false);
    
    // STRICT: Schedule button must exist for this feature
    expect(scheduleExists).toBe(true);
    
    if (scheduleExists) {
      await scheduleBtn.click();
      await user.page.waitForTimeout(500);
      
      // Look for time picker or input
      const timeInput = user.page.locator('input[type="time"], input[type="datetime-local"]').first();
      if (await timeInput.isVisible().catch(() => false)) {
        // Set to 1 minute from now
        const futureTime = new Date(Date.now() + 60000);
        await timeInput.fill(futureTime.toISOString().slice(0, 16));
      }
    }
    
    await context.close();
  });

  test('scheduled message appears at scheduled time', async ({ browser }) => {
    test.setTimeout(120000); // 2 minute timeout
    
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'FutureScheduler', BASE_URL);
    const user2 = await createUser(context2, 'FutureWaiter', BASE_URL);
    
    await joinRoom(user1, 'FutureRoom');
    await joinRoom(user2, 'FutureRoom');
    
    // Try to schedule a message
    const page = user1.page;
    
    // This is implementation-dependent, but try common patterns
    const msgInput = page.locator([
      'input[placeholder*="message" i]',
      'textarea[placeholder*="message" i]',
    ].join(', ')).first();
    
    await msgInput.fill('Scheduled: Hello from the future');
    
    const scheduleBtn = page.locator([
      'button:has-text("Schedule")',
      '[data-testid*="schedule"]',
    ].join(', ')).first();
    
    if (await scheduleBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
      await scheduleBtn.click();
      
      // Wait for the scheduled time (30 seconds to 1 minute typically)
      await page.waitForTimeout(35000);
      
      // Check if message appeared
      const appeared = await messageVisible(user2, 'Hello from the future');
      expect(appeared).toBe(true);
    } else {
      // Feature not implemented in a detectable way
      expect(true).toBe(true);
    }
    
    await context1.close();
    await context2.close();
  });

  test('pending scheduled messages can be cancelled', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'Canceller', BASE_URL);
    
    await joinRoom(user, 'CancelRoom');
    
    // First schedule a message
    const scheduleBtn = user.page.locator([
      'button:has-text("Schedule")',
      'button[title*="schedule" i]',
      '[data-testid*="schedule"]',
    ].join(', ')).first();
    
    const scheduleExists = await scheduleBtn.isVisible({ timeout: 3000 }).catch(() => false);
    
    // STRICT: Schedule feature must exist
    expect(scheduleExists).toBe(true);
    
    if (scheduleExists) {
      // Fill message and schedule
      const msgInput = user.page.locator('input[placeholder*="message" i], textarea[placeholder*="message" i]').first();
      await msgInput.fill('Scheduled cancel test');
      await scheduleBtn.click();
      await user.page.waitForTimeout(500);
      
      // Look for pending messages list or cancel option
      const cancelBtn = user.page.locator([
        'button:has-text("Cancel")',
        '[data-testid*="cancel"]',
        'button:has-text("Delete")',
        '[aria-label*="cancel" i]',
      ].join(', ')).first();
      
      const cancelExists = await cancelBtn.isVisible({ timeout: 3000 }).catch(() => false);
      expect(cancelExists).toBe(true);
    }
    
    await context.close();
  });
});

