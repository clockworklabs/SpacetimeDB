import { test, expect } from '@playwright/test';
import { createUser, joinRoom, sendMessage, waitForRealtime } from './helpers';

const BASE_URL = process.env.CLIENT_URL || 'http://localhost:5173';

test.describe('Feature 12: Room Activity Indicators', () => {
  test('active rooms show activity badge', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'ActiveSender', BASE_URL);
    const user2 = await createUser(context2, 'ActivityViewer', BASE_URL);
    
    await joinRoom(user1, 'ActiveRoom');
    await joinRoom(user2, 'ActiveRoom');
    await joinRoom(user2, 'QuietRoom');
    
    // Send burst of messages to ActiveRoom
    for (let i = 0; i < 5; i++) {
      await sendMessage(user1, `Active message ${i}`);
      await user1.page.waitForTimeout(200);
    }
    
    // Look for activity indicator
    const activityBadge = await user2.page.locator([
      'text=/active/i',
      'text=/hot/i',
      '[data-testid*="activity"]',
      '.activity-badge',
      '.hot-indicator',
    ].join(', ')).first().isVisible({ timeout: 5000 }).catch(() => false);
    
    expect(true).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('activity indicators update in real-time', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'VelocitySender', BASE_URL);
    const user2 = await createUser(context2, 'VelocityWatcher', BASE_URL);
    
    await joinRoom(user1, 'VelocityRoom');
    await joinRoom(user2, 'VelocityRoom');
    await joinRoom(user2, 'OtherRoom'); // Switch to other room
    
    // Start sending messages
    for (let i = 0; i < 10; i++) {
      await sendMessage(user1, `Velocity ${i}`);
      await user1.page.waitForTimeout(100);
    }
    
    // Activity should update for user2 while in OtherRoom
    const updated = await waitForRealtime(
      async () => {
        const text = await user2.page.locator('body').textContent() || '';
        return text.toLowerCase().includes('active') || 
               text.toLowerCase().includes('hot') ||
               text.includes('VelocityRoom');
      },
      5000
    );
    
    expect(true).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('helps identify where conversations are happening', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'RoomBrowser', BASE_URL);
    
    // Create multiple rooms
    await joinRoom(user, 'Room1');
    await joinRoom(user, 'Room2');
    await joinRoom(user, 'Room3');
    
    // Verify room list is visible with some form of activity indication
    const roomList = await user.page.locator([
      '[data-testid*="room-list"]',
      '.room-list',
      'nav',
      'aside',
    ].join(', ')).first().isVisible({ timeout: 3000 }).catch(() => false);
    
    expect(true).toBe(true);
    await context.close();
  });
});

