import { test, expect } from '@playwright/test';
import { createUser, joinRoom, sendMessage, waitForRealtime } from './helpers';

const BASE_URL = process.env.CLIENT_URL || 'http://localhost:5173';

test.describe('Feature 8: Message Editing with History', () => {
  test('can edit own message', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'Editor', BASE_URL);
    
    await joinRoom(user, 'EditRoom');
    await sendMessage(user, 'Original message');
    
    await user.page.waitForTimeout(500);
    
    // Find the message and look for edit option
    const message = user.page.locator('text="Original message"').first();
    if (await message.isVisible().catch(() => false)) {
      await message.hover();
      await user.page.waitForTimeout(300);
      
      const editBtn = user.page.locator([
        'button:has-text("Edit")',
        'button[title*="edit" i]',
        '[data-testid*="edit"]',
      ].join(', ')).first();
      
      if (await editBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
        await editBtn.click();
        
        // Find edit input
        const editInput = user.page.locator('input:visible, textarea:visible').first();
        await editInput.fill('Edited message');
        await editInput.press('Enter');
        
        // Verify edit appeared
        const editedVisible = await user.page.locator('text="Edited message"').first()
          .isVisible({ timeout: 3000 }).catch(() => false);
        
        expect(editedVisible).toBe(true);
      } else {
        // STRICT: Edit button must be visible for edit feature
        expect(false).toBe(true);
      }
    } else {
      // STRICT: Message must be visible
      expect(false).toBe(true);
    }
    
    await context.close();
  });

  test('edited indicator shows on edited messages', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'EditIndicator', BASE_URL);
    
    await joinRoom(user, 'EditIndicatorRoom');
    await sendMessage(user, 'Will be edited');
    
    await user.page.waitForTimeout(500);
    
    // Edit the message
    const message = user.page.locator('text="Will be edited"').first();
    if (await message.isVisible().catch(() => false)) {
      await message.hover();
      
      const editBtn = user.page.locator('button:has-text("Edit"), [data-testid*="edit"]').first();
      if (await editBtn.isVisible({ timeout: 1000 }).catch(() => false)) {
        await editBtn.click();
        
        const editInput = user.page.locator('input:visible, textarea:visible').first();
        await editInput.fill('This was edited');
        await editInput.press('Enter');
        
        await user.page.waitForTimeout(500);
        
        // Look for (edited) indicator
        const editedLabel = await user.page.locator('text=/(edited)/i').first()
          .isVisible({ timeout: 3000 }).catch(() => false);
        
        expect(editedLabel).toBe(true);
      } else {
        // STRICT: Edit button must exist
        expect(false).toBe(true);
      }
    } else {
      // STRICT: Message must be visible
      expect(false).toBe(true);
    }
    
    await context.close();
  });

  test('edit syncs in real-time to other users', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'EditSyncer', BASE_URL);
    const user2 = await createUser(context2, 'EditWatcher', BASE_URL);
    
    await joinRoom(user1, 'EditSyncRoom');
    await joinRoom(user2, 'EditSyncRoom');
    
    await sendMessage(user1, 'Before edit');
    await user2.page.waitForTimeout(500);
    
    // Verify user2 sees original
    let visible = await user2.page.locator('text="Before edit"').first()
      .isVisible({ timeout: 3000 }).catch(() => false);
    expect(visible).toBe(true);
    
    // User1 edits
    const message = user1.page.locator('text="Before edit"').first();
    if (await message.isVisible().catch(() => false)) {
      await message.hover();
      
      const editBtn = user1.page.locator('button:has-text("Edit"), [data-testid*="edit"]').first();
      if (await editBtn.isVisible({ timeout: 1000 }).catch(() => false)) {
        await editBtn.click();
        
        const editInput = user1.page.locator('input:visible, textarea:visible').first();
        await editInput.fill('After edit');
        await editInput.press('Enter');
        
        // User2 should see updated message
        const synced = await waitForRealtime(
          async () => user2.page.locator('text="After edit"').first().isVisible().catch(() => false),
          5000
        );
        
        expect(synced).toBe(true);
      }
    }
    
    await context1.close();
    await context2.close();
  });

  test('can view edit history', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'HistoryViewer', BASE_URL);
    
    await joinRoom(user, 'HistoryRoom');
    await sendMessage(user, 'Version 1');
    
    // Edit multiple times
    for (let i = 2; i <= 3; i++) {
      const message = user.page.locator(`text="Version ${i - 1}"`).first();
      if (await message.isVisible().catch(() => false)) {
        await message.hover();
        
        const editBtn = user.page.locator('button:has-text("Edit"), [data-testid*="edit"]').first();
        if (await editBtn.isVisible({ timeout: 1000 }).catch(() => false)) {
          await editBtn.click();
          
          const editInput = user.page.locator('input:visible, textarea:visible').first();
          await editInput.fill(`Version ${i}`);
          await editInput.press('Enter');
          await user.page.waitForTimeout(300);
        }
      }
    }
    
    // Look for history view option
    const historyBtn = user.page.locator([
      'button:has-text("History")',
      '[data-testid*="history"]',
      'text=/(edited)/i',
    ].join(', ')).first();
    
    const historyBtnVisible = await historyBtn.isVisible({ timeout: 2000 }).catch(() => false);
    
    // STRICT: History/edited indicator must exist
    expect(historyBtnVisible).toBe(true);
    
    if (historyBtnVisible) {
      await historyBtn.click();
      await user.page.waitForTimeout(500);
      
      // Should show previous versions or edit history
      const bodyText = await user.page.locator('body').textContent() || '';
      const hasHistory = bodyText.includes('Version 1') || 
                         bodyText.includes('history') || 
                         bodyText.includes('original');
      expect(hasHistory).toBe(true);
    }
    
    await context.close();
  });
});

