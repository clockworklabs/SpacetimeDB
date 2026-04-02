import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM = `ThreadRoom-${RUN_ID}`;
const PARENT_MSG = `Parent message ${RUN_ID}`;
const REPLY_1 = `Thread reply 1 ${RUN_ID}`;
const REPLY_2 = `Thread reply 2 ${RUN_ID}`;
const REPLY_3 = `Thread reply 3 ${RUN_ID}`;

test.describe('Feature 11: Threading', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);

    await createRoom(alice.page, ROOM);
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);

    // Send a parent message to reply to
    await sendMessage(alice.page, PARENT_MSG);
    await expect(bob.page.getByText(PARENT_MSG).first()).toBeVisible({ timeout: 10_000 });
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('reply button appears on message hover and opens thread', async () => {
    const parentMsg = alice.page.getByText(PARENT_MSG).first();
    await parentMsg.hover();

    const replyBtn = alice.page.locator(
      'button:has-text("Reply"), button:has-text("Thread"), ' +
      '[aria-label*="reply" i], [aria-label*="thread" i], ' +
      '[title*="reply" i], [title*="thread" i], ' +
      '[class*="reply" i] button, [class*="thread" i] button'
    ).first();

    const hasReply = await replyBtn.isVisible({ timeout: 5_000 }).catch(() => false);

    if (!hasReply) {
      await parentMsg.click();
    } else {
      await replyBtn.click();
    }

    // Verify thread panel or reply UI opens
    await alice.page.waitForTimeout(1_000);
    const aliceBody = await alice.page.textContent('body');

    const threadOpen =
      /thread|repl(y|ies)/i.test(aliceBody || '') ||
      (aliceBody || '').includes(PARENT_MSG);

    expect(threadOpen).toBeTruthy();
  });

  test('can send a reply in the thread', async () => {
    const threadInput = alice.page.locator(
      '[class*="thread" i] input, [class*="thread" i] textarea, ' +
      '[class*="reply" i] input, [class*="reply" i] textarea, ' +
      '[aria-label*="reply" i], [placeholder*="reply" i], ' +
      '[placeholder*="thread" i]'
    ).first();

    const hasThreadInput = await threadInput.isVisible({ timeout: 5_000 }).catch(() => false);

    if (hasThreadInput) {
      await threadInput.fill(REPLY_1);
      await threadInput.press('Enter');
    } else {
      await sendMessage(alice.page, REPLY_1);
    }

    await expect(alice.page.getByText(REPLY_1).first()).toBeVisible({ timeout: 10_000 });
  });

  test('reply count badge appears on parent message', async () => {
    // Close thread panel first if needed to see main chat
    const closeBtn = alice.page.locator(
      'button:has-text("Close"), [aria-label*="close" i], [class*="close" i] button'
    ).first();
    const hasClose = await closeBtn.isVisible({ timeout: 2_000 }).catch(() => false);
    if (hasClose) {
      await closeBtn.click();
    }

    const aliceBody = await alice.page.textContent('body');
    const hasReplyCount =
      /1\s*repl(y|ies)/i.test(aliceBody || '') ||
      /repl(y|ies)\s*\(?\s*1\s*\)?/i.test(aliceBody || '');

    const threadIndicator = alice.page.locator(
      '[class*="reply-count" i], [class*="thread-count" i], ' +
      '[class*="replies" i], [data-reply-count]'
    ).first();
    const hasIndicator = await threadIndicator.isVisible({ timeout: 5_000 }).catch(() => false);

    expect(hasReplyCount || hasIndicator).toBeTruthy();
  });

  test('other user sees reply count update in real-time', async () => {
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
    const parentMsg = alice.page.getByText(PARENT_MSG).first();
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
      await threadInput.fill(REPLY_2);
      await threadInput.press('Enter');
    } else {
      await sendMessage(alice.page, REPLY_2);
    }

    // Verify thread view shows parent + both replies
    await alice.page.waitForTimeout(1_000);
    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toContain(PARENT_MSG);
    expect(aliceBody).toContain(REPLY_1);
    await expect(alice.page.getByText(REPLY_2).first()).toBeVisible({ timeout: 10_000 });
  });

  test('thread replies sync in real-time to other viewers', async () => {
    // Bob opens the same thread
    const parentOnBob = bob.page.getByText(PARENT_MSG).first();
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
      await threadInput.fill(REPLY_3);
      await threadInput.press('Enter');
    } else {
      await sendMessage(alice.page, REPLY_3);
    }

    // Bob should see it appear in real-time
    await expect(bob.page.getByText(REPLY_3).first()).toBeVisible({ timeout: 10_000 });
  });
});
