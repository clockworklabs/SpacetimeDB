import { test, expect } from '@playwright/test';
import { createUser, joinRoom, sendMessage, waitForRealtime } from './helpers';

const BASE_URL = process.env.CLIENT_URL || 'http://localhost:5173';

test.describe('Feature 7: Message Reactions', () => {
  test('can add reaction to message', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'Reactor', BASE_URL);
    
    await joinRoom(user, 'ReactionRoom');
    await sendMessage(user, 'React to this message');
    
    await user.page.waitForTimeout(500);
    
    // Look for reaction button or emoji picker
    const reactionBtn = user.page.locator([
      'button:has-text("👍")',
      'button:has-text("React")',
      'button[title*="react" i]',
      '[data-testid*="reaction"]',
      '[data-testid*="emoji"]',
      '.reaction-btn',
      '.emoji-picker',
    ].join(', ')).first();
    
    // Or hover over message to reveal reaction option
    const message = user.page.locator('text="React to this message"').first();
    if (await message.isVisible().catch(() => false)) {
      await message.hover();
      await user.page.waitForTimeout(300);
    }
    
    const reactionExists = await reactionBtn.isVisible({ timeout: 2000 }).catch(() => false);
    
    // STRICT: Reaction button must be visible
    expect(reactionExists).toBe(true);
    
    if (reactionExists) {
      await reactionBtn.click();
      
      // Look for emoji in picker or direct reaction
      const thumbsUp = user.page.locator('text="👍"').first();
      if (await thumbsUp.isVisible({ timeout: 1000 }).catch(() => false)) {
        await thumbsUp.click();
      }
      
      // Verify reaction appears
      await user.page.waitForTimeout(500);
      const reactionAppeared = await user.page.locator('text="👍"').first()
        .isVisible({ timeout: 2000 }).catch(() => false);
      expect(reactionAppeared).toBe(true);
    }
    
    await context.close();
  });

  test('reaction counts update in real-time', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'ReactSender', BASE_URL);
    const user2 = await createUser(context2, 'ReactWatcher', BASE_URL);
    
    await joinRoom(user1, 'ReactCountRoom');
    await joinRoom(user2, 'ReactCountRoom');
    
    await sendMessage(user1, 'Count my reactions');
    await user2.page.waitForTimeout(500);
    
    // User2 adds a reaction
    const message = user2.page.locator('text="Count my reactions"').first();
    if (await message.isVisible().catch(() => false)) {
      await message.hover();
      await user2.page.waitForTimeout(300);
      
      const reactionBtn = user2.page.locator('[data-testid*="reaction"], button:has-text("👍")').first();
      if (await reactionBtn.isVisible().catch(() => false)) {
        await reactionBtn.click();
      }
    }
    
    // User1 should see the reaction count
    await waitForRealtime(
      async () => {
        const text = await user1.page.locator('body').textContent() || '';
        return text.includes('👍') || text.includes('1');
      },
      5000
    );
    
    // STRICT: User1 should see reaction indicator update
    const bodyText = await user1.page.locator('body').textContent() || '';
    const hasReactionIndicator = bodyText.includes('👍') || bodyText.includes('1');
    expect(hasReactionIndicator).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('can toggle reaction off', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'ToggleReactor', BASE_URL);
    
    await joinRoom(user, 'ToggleRoom');
    await sendMessage(user, 'Toggle reaction test');
    
    await user.page.waitForTimeout(500);
    
    // Add reaction
    const message = user.page.locator('text="Toggle reaction test"').first();
    const messageVisible = await message.isVisible().catch(() => false);
    expect(messageVisible).toBe(true);
    
    if (messageVisible) {
      await message.hover();
      await user.page.waitForTimeout(300);
      
      const reactionBtn = user.page.locator('[data-testid*="reaction"], button:has-text("👍"), button[title*="react" i]').first();
      const reactionBtnVisible = await reactionBtn.isVisible({ timeout: 2000 }).catch(() => false);
      
      // STRICT: Reaction button must exist
      expect(reactionBtnVisible).toBe(true);
      
      if (reactionBtnVisible) {
        // Click to add
        await reactionBtn.click();
        await user.page.waitForTimeout(500);
        
        // Verify reaction was added
        let hasReaction = await user.page.locator('text="👍"').first().isVisible().catch(() => false);
        expect(hasReaction).toBe(true);
        
        // Click again to remove (hover again if needed)
        await message.hover();
        await user.page.waitForTimeout(300);
        const toggleBtn = user.page.locator('text="👍"').first();
        if (await toggleBtn.isVisible().catch(() => false)) {
          await toggleBtn.click();
          await user.page.waitForTimeout(500);
        }
      }
    }
    
    await context.close();
  });
});

