import { type Browser, type BrowserContext, type Page } from '@playwright/test';

/**
 * Creates an isolated browser context for a user.
 * Each user gets their own localStorage, cookies, and session —
 * solving the same-origin collision problem from Chrome MCP grading.
 */
export async function createUserContext(
  browser: Browser,
  name: string,
  baseURL: string
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ baseURL });
  const page = await context.newPage();
  await page.goto('/');

  // Wait for the app to load — look for any input or button
  await page.waitForSelector('input, button', { timeout: 15_000 });

  // Register the user by finding the name/display-name input
  // Try common patterns: placeholder with "name", "display", "username"
  const nameInput = page.locator(
    'input[placeholder*="name" i], input[placeholder*="display" i], input[placeholder*="username" i]'
  ).first();

  if (await nameInput.isVisible({ timeout: 5_000 }).catch(() => false)) {
    await nameInput.fill(name);
    // Look for submit/join/register button near the input
    const submitBtn = page.locator(
      'button:has-text("Join"), button:has-text("Register"), button:has-text("Set"), button:has-text("Submit"), button[type="submit"]'
    ).first();
    if (await submitBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await submitBtn.click();
    } else {
      await nameInput.press('Enter');
    }
    // Wait for registration to complete — name should appear somewhere
    await page.waitForFunction(
      (n) => document.body.textContent?.includes(n),
      name,
      { timeout: 10_000 }
    );
  }

  return { context, page };
}

/**
 * Triggers a React-compatible typing event on the message input.
 * Works with both input and textarea elements.
 */
export async function triggerTyping(page: Page, text: string = 'typing test...') {
  await page.evaluate((t) => {
    const input = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      'input[placeholder*="message" i], input[placeholder*="type" i], textarea'
    );
    if (!input) return;
    const proto = input instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
    setter?.call(input, t);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, text);
}

/**
 * Sends a message in the currently active room.
 */
export async function sendMessage(page: Page, text: string) {
  const input = page.locator(
    'input[placeholder*="message" i], input[placeholder*="type" i], textarea'
  ).first();
  await input.fill(text);
  await input.press('Enter');
}

/**
 * Creates or joins a room by name.
 */
export async function createRoom(page: Page, roomName: string) {
  // Look for create room button/form
  const createBtn = page.locator(
    'button:has-text("Create"), button:has-text("New Room"), button:has-text("+"), [aria-label*="create" i]'
  ).first();
  await createBtn.click();

  // Fill room name
  const roomInput = page.locator(
    'input[placeholder*="room" i], input[placeholder*="name" i]'
  ).first();
  await roomInput.fill(roomName);

  // Submit
  const submitBtn = page.locator(
    'button:has-text("Create"), button[type="submit"]'
  ).first();
  if (await submitBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
    await submitBtn.click();
  } else {
    await roomInput.press('Enter');
  }

  // Wait for room to appear
  await page.waitForFunction(
    (name) => document.body.textContent?.includes(name),
    roomName,
    { timeout: 10_000 }
  );
}

/**
 * Joins an existing room by clicking on it in the room list.
 */
export async function joinRoom(page: Page, roomName: string) {
  // Click on the room name in the sidebar/list
  const roomLink = page.locator(`text=${roomName}`).first();
  await roomLink.click();

  // If there's a "Join" button, click it
  const joinBtn = page.locator('button:has-text("Join")').first();
  if (await joinBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
    await joinBtn.click();
  }
}
