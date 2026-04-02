import { test, expect, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM_NAME = `General-${RUN_ID}`;

test.describe('Feature 1: Basic Chat', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('users can set a display name', async () => {
    await expect(alice.page.getByText(`Alice-${RUN_ID}`).first()).toBeVisible();
    await expect(bob.page.getByText(`Bob-${RUN_ID}`).first()).toBeVisible();
  });

  test('users can create and join rooms', async () => {
    await createRoom(alice.page, ROOM_NAME);
    await expect(alice.page.getByText(ROOM_NAME).first()).toBeVisible();

    // Bob should see the room (real-time update)
    await expect(bob.page.getByText(ROOM_NAME).first()).toBeVisible({ timeout: 10_000 });
  });

  test('messages appear in real-time for all users', async () => {
    await joinRoom(alice.page, ROOM_NAME);
    await joinRoom(bob.page, ROOM_NAME);

    await sendMessage(alice.page, `Hello from Alice-${RUN_ID}!`);
    await expect(alice.page.getByText(`Hello from Alice-${RUN_ID}!`).first()).toBeVisible();
    await expect(bob.page.getByText(`Hello from Alice-${RUN_ID}!`).first()).toBeVisible({ timeout: 10_000 });

    await sendMessage(bob.page, `Hi Alice, this is Bob-${RUN_ID}!`);
    await expect(bob.page.getByText(`Hi Alice, this is Bob-${RUN_ID}!`).first()).toBeVisible();
    await expect(alice.page.getByText(`Hi Alice, this is Bob-${RUN_ID}!`).first()).toBeVisible({ timeout: 10_000 });
  });

  test('online user list shows connected users', async () => {
    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toContain(`Alice-${RUN_ID}`);
    expect(aliceBody).toContain(`Bob-${RUN_ID}`);
  });
});
