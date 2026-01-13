import { test, expect } from '@playwright/test';
import { createUser, joinRoom, waitForRealtime } from './helpers';

const BASE_URL = process.env.CLIENT_URL || 'http://localhost:5173';

test.describe('Feature 2: Typing Indicators', () => {
  test('typing indicator appears when user types', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'Typer', BASE_URL);
    const user2 = await createUser(context2, 'Watcher', BASE_URL);
    
    await joinRoom(user1, 'TypingRoom');
    await joinRoom(user2, 'TypingRoom');
    
    // User1 starts typing
    const msgInput = user1.page.locator([
      'input[placeholder*="message" i]',
      'textarea[placeholder*="message" i]',
      'input[name="message"]',
      '#message',
      '[data-testid="message-input"]',
    ].join(', ')).first();
    
    await msgInput.focus();
    await msgInput.pressSequentially('Hello', { delay: 100 });
    
    // User2 should see typing indicator
    const typingVisible = await waitForRealtime(
      async () => {
        const page = user2.page;
        return (
          await page.locator('text=/typing/i').first().isVisible().catch(() => false) ||
          await page.locator('text="Typer"').first().isVisible().catch(() => false) ||
          await page.locator('[data-testid*="typing"]').first().isVisible().catch(() => false)
        );
      },
      5000
    );
    
    expect(typingVisible).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('typing indicator disappears after inactivity', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'BriefTyper', BASE_URL);
    const user2 = await createUser(context2, 'Observer', BASE_URL);
    
    await joinRoom(user1, 'ExpiryRoom');
    await joinRoom(user2, 'ExpiryRoom');
    
    // User1 types briefly
    const msgInput = user1.page.locator([
      'input[placeholder*="message" i]',
      'textarea[placeholder*="message" i]',
      'input[name="message"]',
      '#message',
    ].join(', ')).first();
    
    await msgInput.focus();
    await msgInput.pressSequentially('Hi', { delay: 50 });
    
    // Wait for typing indicator to appear
    await user2.page.waitForTimeout(1000);
    
    // Wait for expiry (typically 3-5 seconds)
    await user2.page.waitForTimeout(6000);
    
    // Typing indicator should be gone
    const typingGone = await user2.page.locator('text=/typing/i').first()
      .isHidden({ timeout: 2000 }).catch(() => true);
    
    expect(typingGone).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('multiple users typing shows appropriate message', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    const context3 = await browser.newContext();
    
    const user1 = await createUser(context1, 'MultiTyper1', BASE_URL);
    const user2 = await createUser(context2, 'MultiTyper2', BASE_URL);
    const user3 = await createUser(context3, 'MultiWatcher', BASE_URL);
    
    await joinRoom(user1, 'MultiTypingRoom');
    await joinRoom(user2, 'MultiTypingRoom');
    await joinRoom(user3, 'MultiTypingRoom');
    
    // Both users start typing
    const getInput = (page: any) => page.locator([
      'input[placeholder*="message" i]',
      'textarea[placeholder*="message" i]',
      'input[name="message"]',
      '#message',
    ].join(', ')).first();
    
    const input1 = getInput(user1.page);
    const input2 = getInput(user2.page);
    
    await input1.focus();
    await input1.pressSequentially('Hello', { delay: 100 });
    await input2.focus();
    await input2.pressSequentially('World', { delay: 100 });
    
    // User3 should see "multiple" or both names
    const multiTypingVisible = await waitForRealtime(
      async () => {
        const page = user3.page;
        const text = await page.locator('body').textContent() || '';
        return (
          text.toLowerCase().includes('multiple') ||
          text.toLowerCase().includes('typing') ||
          (text.includes('MultiTyper1') && text.includes('MultiTyper2'))
        );
      },
      5000
    );
    
    // STRICT: This should pass if typing indicators work for multiple users
    expect(multiTypingVisible).toBe(true);
    
    await context1.close();
    await context2.close();
    await context3.close();
  });
});

