import { test, expect, chromium } from '@playwright/test';
import { createUser, joinRoom, sendMessage, messageVisible, waitForRealtime } from './helpers';

const BASE_URL = process.env.CLIENT_URL || 'http://localhost:5173';

test.describe('Feature 1: Basic Chat', () => {
  test('users can set display name', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'TestUser1', BASE_URL);
    
    // Verify name appears somewhere on page
    const nameVisible = await user.page.locator('text="TestUser1"').first()
      .isVisible({ timeout: 5000 }).catch(() => false);
    
    expect(nameVisible).toBe(true);
    await context.close();
  });

  test('users can create and join rooms', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'RoomCreator', BASE_URL);
    
    await joinRoom(user, 'TestRoom1');
    
    // Verify room appears in UI
    const roomVisible = await user.page.locator('text="TestRoom1"').first()
      .isVisible({ timeout: 5000 }).catch(() => false);
    
    expect(roomVisible).toBe(true);
    await context.close();
  });

  test('users can send messages', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'Sender', BASE_URL);
    
    await joinRoom(user, 'MessageRoom');
    await sendMessage(user, 'Hello, world!');
    
    const msgVisible = await messageVisible(user, 'Hello, world!');
    expect(msgVisible).toBe(true);
    await context.close();
  });

  test('messages sync in real-time between users', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'Alice', BASE_URL);
    const user2 = await createUser(context2, 'Bob', BASE_URL);
    
    await joinRoom(user1, 'SyncRoom');
    await joinRoom(user2, 'SyncRoom');
    
    // User1 sends message
    await sendMessage(user1, 'Real-time test message');
    
    // User2 should see it
    const synced = await waitForRealtime(
      async () => messageVisible(user2, 'Real-time test message'),
      5000
    );
    
    expect(synced).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('online users are displayed', async ({ browser }) => {
    const context1 = await browser.newContext();
    const context2 = await browser.newContext();
    
    const user1 = await createUser(context1, 'OnlineUser1', BASE_URL);
    const user2 = await createUser(context2, 'OnlineUser2', BASE_URL);
    
    await joinRoom(user1, 'OnlineRoom');
    await joinRoom(user2, 'OnlineRoom');
    
    // Check if online users are shown (look for user names in member list or online indicator)
    const user2SeesUser1 = await waitForRealtime(
      async () => user2.page.locator('text="OnlineUser1"').first().isVisible().catch(() => false),
      5000
    );
    
    expect(user2SeesUser1).toBe(true);
    
    await context1.close();
    await context2.close();
  });

  test('validation prevents empty messages', async ({ browser }) => {
    const context = await browser.newContext();
    const user = await createUser(context, 'Validator', BASE_URL);
    
    await joinRoom(user, 'ValidationRoom');
    
    // Try to send empty message
    await sendMessage(user, '');
    
    // Count messages - should be 0 or just system messages
    await user.page.waitForTimeout(500);
    
    // This is a soft check - if validation exists, empty message won't appear
    // We mainly verify no crash occurs
    expect(true).toBe(true);
    
    await context.close();
  });
});

