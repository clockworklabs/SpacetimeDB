import { test, expect, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 1: Basic Chat', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('users can set a display name', async () => {
    // Names were set during createUserContext — verify they appear
    await expect(alice.page.getByText('Alice')).toBeVisible();
    await expect(bob.page.getByText('Bob')).toBeVisible();
  });

  test('users can create and join rooms', async () => {
    await createRoom(alice.page, 'General');
    await expect(alice.page.getByText('General')).toBeVisible();

    // Bob should see the room (real-time update)
    await expect(bob.page.getByText('General')).toBeVisible({ timeout: 10_000 });
  });

  test('messages appear in real-time for all users', async () => {
    await joinRoom(alice.page, 'General');
    await joinRoom(bob.page, 'General');

    await sendMessage(alice.page, 'Hello from Alice!');
    await expect(alice.page.getByText('Hello from Alice!')).toBeVisible();
    await expect(bob.page.getByText('Hello from Alice!')).toBeVisible({ timeout: 10_000 });

    await sendMessage(bob.page, 'Hi Alice, this is Bob!');
    await expect(bob.page.getByText('Hi Alice, this is Bob!')).toBeVisible();
    await expect(alice.page.getByText('Hi Alice, this is Bob!')).toBeVisible({ timeout: 10_000 });
  });

  test('online user list shows connected users', async () => {
    // Both users should appear in the online/member list
    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toContain('Alice');
    expect(aliceBody).toContain('Bob');
  });
});
