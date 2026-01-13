import { Page, BrowserContext, expect } from '@playwright/test';

/**
 * Test helpers for chat app E2E tests
 * 
 * STRICT MODE: Tests should fail when features aren't working.
 * Use FeatureDetection to track what was found vs not found.
 */

export interface TestUser {
  page: Page;
  context: BrowserContext;
  name: string;
}

export interface FeatureDetection {
  found: boolean;
  selector?: string;
  reason?: string;
}

// Common selector patterns for different UI elements
export const SELECTORS = {
  nameInput: [
    'input[placeholder*="name" i]',
    'input[name="name"]',
    'input[name="displayName"]',
    'input[name="username"]',
    '#name',
    '#displayName',
    '#username',
    '[data-testid="name-input"]',
  ],
  submitButton: [
    'button[type="submit"]',
    'button:has-text("Set")',
    'button:has-text("Join")',
    'button:has-text("Enter")',
    'button:has-text("Save")',
    'button:has-text("Continue")',
    'button:has-text("Start")',
  ],
  roomInput: [
    'input[placeholder*="room" i]',
    'input[name="room"]',
    'input[name="roomName"]',
    '#roomName',
    '[data-testid="room-input"]',
  ],
  createRoomButton: [
    'button:has-text("Create")',
    'button:has-text("New Room")',
    'button:has-text("Add Room")',
    'button:has-text("+")',
    '[data-testid="create-room"]',
  ],
  messageInput: [
    'input[placeholder*="message" i]',
    'input[placeholder*="type" i]',
    'textarea[placeholder*="message" i]',
    'input[name="message"]',
    'textarea[name="message"]',
    '#message',
    '#messageInput',
    '[data-testid="message-input"]',
  ],
  sendButton: [
    'button[type="submit"]',
    'button:has-text("Send")',
    '[data-testid="send-button"]',
    'button[aria-label*="send" i]',
  ],
  editButton: [
    'button:has-text("Edit")',
    'button[title*="edit" i]',
    '[data-testid*="edit"]',
    'button[aria-label*="edit" i]',
  ],
  reactionButton: [
    'button:has-text("👍")',
    'button:has-text("React")',
    'button[title*="react" i]',
    '[data-testid*="reaction"]',
    '[data-testid*="emoji"]',
    '.reaction-btn',
    '.emoji-picker-trigger',
  ],
  scheduleButton: [
    'button:has-text("Schedule")',
    'button[title*="schedule" i]',
    '[data-testid*="schedule"]',
    'button:has-text("Later")',
    'button:has-text("⏰")',
  ],
  typingIndicator: [
    '[data-testid*="typing"]',
    '.typing-indicator',
    '[class*="typing"]',
  ],
};

/**
 * Find first visible element from a list of selectors
 */
export async function findElement(
  page: Page,
  selectors: string[],
  timeout: number = 3000
): Promise<FeatureDetection> {
  for (const selector of selectors) {
    try {
      const element = page.locator(selector).first();
      if (await element.isVisible({ timeout: Math.min(timeout, 1000) })) {
        return { found: true, selector };
      }
    } catch {
      // Continue to next selector
    }
  }
  return { found: false, reason: `None of the selectors matched: ${selectors.slice(0, 3).join(', ')}...` };
}

/**
 * Create a new user session with a unique name
 */
export async function createUser(
  context: BrowserContext,
  name: string,
  baseURL: string
): Promise<TestUser> {
  const page = await context.newPage();
  await page.goto(baseURL);
  
  // Wait for app to load - look for common elements
  await page.waitForLoadState('networkidle');
  
  // Try to set display name
  const nameResult = await findElement(page, SELECTORS.nameInput);
  
  if (nameResult.found && nameResult.selector) {
    const nameInput = page.locator(nameResult.selector).first();
    await nameInput.fill(name);
    
    // Look for submit button
    const submitResult = await findElement(page, SELECTORS.submitButton);
    
    if (submitResult.found && submitResult.selector) {
      await page.locator(submitResult.selector).first().click();
    } else {
      await nameInput.press('Enter');
    }
    
    await page.waitForTimeout(500);
  }
  
  return { page, context, name };
}

/**
 * Create or join a room
 */
export async function joinRoom(user: TestUser, roomName: string): Promise<boolean> {
  const { page } = user;
  
  // Check if room already exists in list
  const existingRoom = page.locator(`text="${roomName}"`).first();
  if (await existingRoom.isVisible({ timeout: 1000 }).catch(() => false)) {
    await existingRoom.click();
    await page.waitForTimeout(300);
    return true;
  }
  
  // Try room input
  const roomInputResult = await findElement(page, SELECTORS.roomInput);
  const createBtnResult = await findElement(page, SELECTORS.createRoomButton);
  
  if (roomInputResult.found && roomInputResult.selector) {
    const roomInput = page.locator(roomInputResult.selector).first();
    await roomInput.fill(roomName);
    
    if (createBtnResult.found && createBtnResult.selector) {
      await page.locator(createBtnResult.selector).first().click();
    } else {
      await roomInput.press('Enter');
    }
  } else if (createBtnResult.found && createBtnResult.selector) {
    await page.locator(createBtnResult.selector).first().click();
    await page.waitForTimeout(300);
    
    // Modal might appear
    const modalInput = page.locator('input:visible').first();
    if (await modalInput.isVisible({ timeout: 1000 }).catch(() => false)) {
      await modalInput.fill(roomName);
      await modalInput.press('Enter');
    }
  }
  
  await page.waitForTimeout(500);
  
  // Click on the room to join it
  const roomToJoin = page.locator(`text="${roomName}"`).first();
  if (await roomToJoin.isVisible({ timeout: 2000 }).catch(() => false)) {
    await roomToJoin.click();
    return true;
  }
  
  return false;
}

/**
 * Send a message in the current room
 * Returns true if message was sent successfully
 */
export async function sendMessage(user: TestUser, message: string): Promise<boolean> {
  const { page } = user;
  
  const msgInputResult = await findElement(page, SELECTORS.messageInput);
  
  if (!msgInputResult.found || !msgInputResult.selector) {
    return false;
  }
  
  const msgInput = page.locator(msgInputResult.selector).first();
  await msgInput.fill(message);
  
  const sendBtnResult = await findElement(page, SELECTORS.sendButton, 1000);
  
  if (sendBtnResult.found && sendBtnResult.selector) {
    await page.locator(sendBtnResult.selector).first().click();
  } else {
    await msgInput.press('Enter');
  }
  
  await page.waitForTimeout(300);
  return true;
}

/**
 * Check if a message is visible on the page
 */
export async function messageVisible(user: TestUser, message: string, timeout: number = 5000): Promise<boolean> {
  const { page } = user;
  return page.locator(`text="${message}"`).first().isVisible({ timeout }).catch(() => false);
}

/**
 * Wait for real-time update with retries
 */
export async function waitForRealtime(
  check: () => Promise<boolean>,
  timeout: number = 5000,
  interval: number = 200
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    if (await check()) return true;
    await new Promise(r => setTimeout(r, interval));
  }
  return false;
}

/**
 * Get visible text content matching a pattern
 */
export async function findText(page: Page, pattern: RegExp | string): Promise<string | null> {
  const elements = await page.locator('body').allTextContents();
  const fullText = elements.join(' ');
  
  if (typeof pattern === 'string') {
    return fullText.includes(pattern) ? pattern : null;
  }
  
  const match = fullText.match(pattern);
  return match ? match[0] : null;
}

/**
 * Check if a feature's UI elements are present
 * Use this for strict feature detection
 */
export async function detectFeature(
  page: Page,
  featureName: string,
  requiredSelectors: string[]
): Promise<FeatureDetection> {
  const result = await findElement(page, requiredSelectors, 3000);
  if (!result.found) {
    result.reason = `Feature "${featureName}" not detected: UI elements not found`;
  }
  return result;
}

/**
 * Hover over a message to reveal action buttons
 */
export async function hoverMessage(page: Page, messageText: string): Promise<boolean> {
  const message = page.locator(`text="${messageText}"`).first();
  if (await message.isVisible().catch(() => false)) {
    await message.hover();
    await page.waitForTimeout(300);
    return true;
  }
  return false;
}

/**
 * Click action button on a message (edit, react, etc.)
 */
export async function clickMessageAction(
  page: Page,
  messageText: string,
  actionSelectors: string[]
): Promise<boolean> {
  const hovered = await hoverMessage(page, messageText);
  if (!hovered) return false;
  
  const actionResult = await findElement(page, actionSelectors, 2000);
  if (actionResult.found && actionResult.selector) {
    await page.locator(actionResult.selector).first().click();
    return true;
  }
  return false;
}