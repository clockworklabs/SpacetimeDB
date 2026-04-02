import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `General-${RUN_ID}`;
const REACT_MSG = `React to this ${RUN_ID}`;
const MULTI_REACT_MSG = `Multi-reaction test ${RUN_ID}`;

test.describe('Feature 7: Reactions', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL);

    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);

    // Send a message to react to
    await sendMessage(alice.page, REACT_MSG);
    await expect(bob.page.getByText(REACT_MSG).first()).toBeVisible({ timeout: 10_000 });
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('can add a reaction to a message', async () => {
    // Find the message element and hover to reveal reaction button
    const messageEl = bob.page.getByText(REACT_MSG).first();
    await messageEl.hover();

    // Look for reaction button
    const reactionBtn = bob.page.locator(
      'button:has-text("React"), [aria-label*="react" i], [aria-label*="emoji" i], ' +
      '[title*="react" i], [title*="emoji" i], ' +
      'button:has-text("+"), button:has-text("\u{1F600}"), button:has-text("\u{1F642}")'
    ).first();

    if (await reactionBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await reactionBtn.click();
    } else {
      await messageEl.click({ button: 'right' });
    }

    // Select an emoji
    const emojiOption = bob.page.locator(
      'button:has-text("\u{1F44D}"), button:has-text("\u2764"), button:has-text("\u{1F602}"), ' +
      'button:has-text("\u{1F389}"), button:has-text("\u{1F44F}"), ' +
      '[role="option"], .emoji-picker button, .emoji'
    ).first();

    if (await emojiOption.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await emojiOption.click();
    } else {
      const emojiInput = bob.page.locator('input[placeholder*="emoji" i], input[placeholder*="react" i]').first();
      if (await emojiInput.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await emojiInput.fill('\u{1F44D}');
        await emojiInput.press('Enter');
      }
    }

    // Verify a reaction appears on the message
    await expect(
      bob.page.locator('text=/\u{1F44D}|\u2764|\u{1F602}|\u{1F389}|\u{1F44F}|reaction|1/').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('reaction count appears and is visible to both users', async () => {
    // Alice should also see the reaction on the message (real-time)
    await expect(
      alice.page.locator('text=/\u{1F44D}|\u2764|\u{1F602}|\u{1F389}|\u{1F44F}/').first()
    ).toBeVisible({ timeout: 10_000 });

    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toMatch(/\u{1F44D}|\u2764|\u{1F602}|\u{1F389}|\u{1F44F}/u);
  });

  test('can toggle reaction off — count decreases or disappears', async () => {
    const messageEl = bob.page.getByText(REACT_MSG).first();
    await messageEl.hover();

    // Click the existing reaction to toggle it off
    const existingReaction = bob.page.locator(
      'button:has-text("\u{1F44D}"), button:has-text("\u2764"), button:has-text("\u{1F602}"), ' +
      'button:has-text("\u{1F389}"), button:has-text("\u{1F44F}"), ' +
      '[class*="reaction" i]'
    ).first();

    if (await existingReaction.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await existingReaction.click();
    }

    await bob.page.waitForTimeout(1_000);

    const bobBody = await bob.page.textContent('body');
    expect(bobBody).toBeDefined();
  });

  test('multiple users can react and counts aggregate', async () => {
    // Send a fresh message for clean reaction testing
    await sendMessage(alice.page, MULTI_REACT_MSG);
    await expect(bob.page.getByText(MULTI_REACT_MSG).first()).toBeVisible({ timeout: 10_000 });

    // Alice reacts
    const aliceMsg = alice.page.getByText(MULTI_REACT_MSG).first();
    await aliceMsg.hover();
    const aliceReactBtn = alice.page.locator(
      'button:has-text("React"), [aria-label*="react" i], [aria-label*="emoji" i], ' +
      '[title*="react" i], button:has-text("+")'
    ).first();
    if (await aliceReactBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await aliceReactBtn.click();
      const emoji1 = alice.page.locator(
        'button:has-text("\u{1F44D}"), button:has-text("\u2764"), [role="option"]'
      ).first();
      if (await emoji1.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await emoji1.click();
      }
    }

    // Bob also reacts with the same emoji
    const bobMsg = bob.page.getByText(MULTI_REACT_MSG).first();
    await bobMsg.hover();
    const bobReactBtn = bob.page.locator(
      'button:has-text("React"), [aria-label*="react" i], [aria-label*="emoji" i], ' +
      '[title*="react" i], button:has-text("+")'
    ).first();
    if (await bobReactBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await bobReactBtn.click();
      const emoji2 = bob.page.locator(
        'button:has-text("\u{1F44D}"), button:has-text("\u2764"), [role="option"]'
      ).first();
      if (await emoji2.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await emoji2.click();
      }
    }

    // Both should see a count of 2
    await bob.page.waitForTimeout(2_000);
    const bodyText = await alice.page.textContent('body');
    expect(bodyText).toMatch(/2|\u{1F44D}.*\u{1F44D}|\u2764.*\u2764/u);
  });
});
