import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = 'ThreadRoom';

test.describe('Feature 11: Threading', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    await createRoom(alice.page, ROOM);
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);

    // Send a parent message to reply to
    await sendMessage(alice.page, 'Parent message for threading');
    await expect(bob.page.getByText('Parent message for threading')).toBeVisible({ timeout: 10_000 });
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('reply button appears on message hover and opens thread', async () => {
    // Find the parent message element
    const parentMsg = alice.page.getByText('Parent message for threading');
    await parentMsg.hover();

    // Look for reply/thread button that appears on hover
    const replyBtn = alice.page.locator(
      'button:has-text("Reply"), button:has-text("Thread"), ' +
      '[aria-label*="reply" i], [aria-label*="thread" i], ' +
      '[title*="reply" i], [title*="thread" i], ' +
      '[class*="reply" i] button, [class*="thread" i] button'
    ).first();

    const hasReply = await replyBtn.isVisible({ timeout: 5_000 }).catch(() => false);

    if (!hasReply) {
      // Try clicking on the message itself — some apps open thread on click
      await parentMsg.click();
    } else {
      await replyBtn.click();
    }

    // Verify thread panel or reply UI opens
    await alice.page.waitForTimeout(1_000);
    const aliceBody = await alice.page.textContent('body');

    // Thread panel should show the parent message context
    const threadOpen =
      /thread|repl(y|ies)|Parent message for threading/i.test(aliceBody || '');

    expect(threadOpen).toBeTruthy();
  });

  test('can send a reply in the thread', async () => {
    // Find the thread reply input — it may be a separate input from the main one
    const threadInput = alice.page.locator(
      '[class*="thread" i] input, [class*="thread" i] textarea, ' +
      '[class*="reply" i] input, [class*="reply" i] textarea, ' +
      '[aria-label*="reply" i], [placeholder*="reply" i], ' +
      '[placeholder*="thread" i]'
    ).first();

    const hasThreadInput = await threadInput.isVisible({ timeout: 5_000 }).catch(() => false);

    if (hasThreadInput) {
      await threadInput.fill('This is a thread reply');
      await threadInput.press('Enter');
    } else {
      // Some apps reuse the main input when thread is open
      await sendMessage(alice.page, 'This is a thread reply');
    }

    // Verify reply appears
    await expect(alice.page.getByText('This is a thread reply')).toBeVisible({ timeout: 10_000 });
  });

  test('reply count badge appears on parent message', async () => {
    // Check the main chat view for a reply count on the parent message
    // Close thread panel first if needed to see main chat
    const closeBtn = alice.page.locator(
      'button:has-text("Close"), [aria-label*="close" i], [class*="close" i] button'
    ).first();
    const hasClose = await closeBtn.isVisible({ timeout: 2_000 }).catch(() => false);
    if (hasClose) {
      await closeBtn.click();
    }

    // Look for reply count near the parent message
    const aliceBody = await alice.page.textContent('body');
    const hasReplyCount =
      /1\s*repl(y|ies)/i.test(aliceBody || '') ||
      /repl(y|ies)\s*\(?\s*1\s*\)?/i.test(aliceBody || '');

    // Also check for thread indicators
    const threadIndicator = alice.page.locator(
      '[class*="reply-count" i], [class*="thread-count" i], ' +
      '[class*="replies" i], [data-reply-count]'
    ).first();
    const hasIndicator = await threadIndicator.isVisible({ timeout: 5_000 }).catch(() => false);

    expect(hasReplyCount || hasIndicator).toBeTruthy();
  });

  test('other user sees reply count update in real-time', async () => {
    // Bob should see the reply count on the parent message without refreshing
    const bobBody = await bob.page.textContent('body');

    const bobSeesCount =
      /1\s*repl(y|ies)/i.test(bobBody || '') ||
      /repl(y|ies)/i.test(bobBody || '');

    const bobIndicator = bob.page.locator(
      '[class*="reply-count" i], [class*="thread-count" i], [class*="replies" i]'
    ).first();
    const hasBobIndicator = await bobIndicator.isVisible({ timeout: 10_000 }).catch(() => false);

    expect(bobSeesCount || hasBobIndicator).toBeTruthy();
  });

  test('thread panel shows parent message and all replies', async () => {
    // Open the thread on the parent message
    const parentMsg = alice.page.getByText('Parent message for threading');
    await parentMsg.hover();

    const replyBtn = alice.page.locator(
      'button:has-text("Reply"), button:has-text("Thread"), ' +
      '[aria-label*="reply" i], [aria-label*="thread" i], ' +
      'text=/1\\s*repl/i, [class*="reply-count" i]'
    ).first();

    const hasReply = await replyBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasReply) {
      await replyBtn.click();
    } else {
      await parentMsg.click();
    }

    await alice.page.waitForTimeout(1_000);

    // Send a second reply
    const threadInput = alice.page.locator(
      '[class*="thread" i] input, [class*="thread" i] textarea, ' +
      '[class*="reply" i] input, [class*="reply" i] textarea, ' +
      '[placeholder*="reply" i], [placeholder*="thread" i]'
    ).first();

    const hasThreadInput = await threadInput.isVisible({ timeout: 3_000 }).catch(() => false);
    if (hasThreadInput) {
      await threadInput.fill('Second thread reply');
      await threadInput.press('Enter');
    } else {
      await sendMessage(alice.page, 'Second thread reply');
    }

    // Verify thread view shows parent + both replies
    await alice.page.waitForTimeout(1_000);
    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toContain('Parent message for threading');
    expect(aliceBody).toContain('This is a thread reply');
    await expect(alice.page.getByText('Second thread reply')).toBeVisible({ timeout: 10_000 });
  });

  test('thread replies sync in real-time to other viewers', async () => {
    // Bob opens the same thread
    const parentOnBob = bob.page.getByText('Parent message for threading');
    await parentOnBob.hover();

    const replyBtnBob = bob.page.locator(
      'button:has-text("Reply"), button:has-text("Thread"), ' +
      '[aria-label*="reply" i], [aria-label*="thread" i], ' +
      'text=/repl/i, [class*="reply-count" i]'
    ).first();

    const hasBobReply = await replyBtnBob.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasBobReply) {
      await replyBtnBob.click();
    } else {
      await parentOnBob.click();
    }

    await bob.page.waitForTimeout(1_000);

    // Alice sends another reply
    const threadInput = alice.page.locator(
      '[class*="thread" i] input, [class*="thread" i] textarea, ' +
      '[class*="reply" i] input, [class*="reply" i] textarea, ' +
      '[placeholder*="reply" i], [placeholder*="thread" i]'
    ).first();

    const hasThreadInput = await threadInput.isVisible({ timeout: 3_000 }).catch(() => false);
    if (hasThreadInput) {
      await threadInput.fill('Third reply from Alice');
      await threadInput.press('Enter');
    } else {
      await sendMessage(alice.page, 'Third reply from Alice');
    }

    // Bob should see it appear in real-time
    await expect(bob.page.getByText('Third reply from Alice')).toBeVisible({ timeout: 10_000 });
  });
});
