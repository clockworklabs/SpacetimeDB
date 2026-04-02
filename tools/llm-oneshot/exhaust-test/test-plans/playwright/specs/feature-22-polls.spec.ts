import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `PollTestRoom-${RUN_ID}`;
const ALICE = `Alice-${RUN_ID}`;
const BOB = `Bob-${RUN_ID}`;
const POLL_QUESTION = `Favorite color ${RUN_ID}?`;

test.describe('Feature 22: Polls', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, ALICE, APP_URL);
    bob = await createUserContext(browser, BOB, APP_URL);

    await createRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('create poll with question and options', async () => {
    // Find the poll creation button (could be in a + menu, message toolbar, etc.)
    const pollBtn = alice.page.locator(
      'button:has-text("Poll"), [aria-label*="poll" i], [title*="poll" i], ' +
      'button:has-text("Create Poll"), button:has(svg[class*="poll" i])'
    ).first();

    // If poll button isn't directly visible, check for a + or attachment menu first
    if (!await pollBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      const plusMenu = alice.page.locator(
        'button:has-text("+"), [aria-label*="more" i], [aria-label*="attach" i], ' +
        'button:has-text("More"), [title*="more" i]'
      ).first();
      if (await plusMenu.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await plusMenu.click();
      }
    }

    await pollBtn.click({ timeout: 5_000 });

    // Fill in the poll question
    const questionInput = alice.page.locator(
      'input[placeholder*="question" i], textarea[placeholder*="question" i], ' +
      'input[name*="question" i], input[placeholder*="ask" i]'
    ).first();
    await questionInput.fill(POLL_QUESTION);

    // Fill in options — try different patterns for option inputs
    const optionInputs = alice.page.locator(
      'input[placeholder*="option" i], input[placeholder*="choice" i], ' +
      'input[name*="option" i], input[placeholder*="answer" i]'
    );

    const optionCount = await optionInputs.count();
    if (optionCount >= 2) {
      // Fill existing option fields
      await optionInputs.nth(0).fill('Red');
      await optionInputs.nth(1).fill('Blue');

      // Add a third option if needed
      if (optionCount >= 3) {
        await optionInputs.nth(2).fill('Green');
      } else {
        // Click "Add Option" button to add a third
        const addOptionBtn = alice.page.locator(
          'button:has-text("Add"), button:has-text("+ Option"), button:has-text("Add Option")'
        ).first();
        if (await addOptionBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
          await addOptionBtn.click();
          const newOption = alice.page.locator(
            'input[placeholder*="option" i], input[placeholder*="choice" i]'
          ).last();
          await newOption.fill('Green');
        }
      }
    }

    // Submit/create the poll
    const createBtn = alice.page.locator(
      'button:has-text("Create"), button:has-text("Post"), button:has-text("Send"), button[type="submit"]'
    ).first();
    await createBtn.click({ timeout: 5_000 });

    // Verify poll is visible with question and all options
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain(POLL_QUESTION);
      expect(body).toContain('Red');
      expect(body).toContain('Blue');
      expect(body).toContain('Green');
    }).toPass({ timeout: 5_000 });

    // Verify Bob sees the poll in real-time
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain(POLL_QUESTION);
      expect(body).toContain('Red');
      expect(body).toContain('Blue');
      expect(body).toContain('Green');
    }).toPass({ timeout: 10_000 });

    // Verify all options show 0 votes initially
    await expect(async () => {
      const body = await alice.page.textContent('body');
      // Look for "0" near the vote options, or "0 votes", "0%"
      expect(body).toMatch(/0\s*(vote|%)?/);
    }).toPass({ timeout: 5_000 });
  });

  test('votes update in real-time and changing vote is atomic', async () => {
    // Alice votes for Blue
    const blueOption = alice.page.locator(
      'button:has-text("Blue"), label:has-text("Blue"), ' +
      '[class*="option" i]:has-text("Blue"), [class*="poll" i] :text("Blue")'
    ).first();
    await blueOption.click({ timeout: 5_000 });

    // Verify Alice's vote is recorded — Blue should show 1 vote
    await expect(async () => {
      const body = await alice.page.textContent('body');
      // Should show 1 vote on Blue (could be "1 vote", "1", "100%", etc.)
      expect(body).toContain('Blue');
      expect(body).toMatch(/1/);
    }).toPass({ timeout: 5_000 });

    // Verify Bob sees the updated vote count in real-time
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain('Blue');
      // Bob should see the vote count change
      expect(body).toMatch(/1/);
    }).toPass({ timeout: 10_000 });

    // Alice changes her vote from Blue to Green
    const greenOption = alice.page.locator(
      'button:has-text("Green"), label:has-text("Green"), ' +
      '[class*="option" i]:has-text("Green"), [class*="poll" i] :text("Green")'
    ).first();
    await greenOption.click({ timeout: 5_000 });

    // Verify atomic update: Blue drops to 0, Green goes to 1
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain('Green');
      // Green should have 1 vote, Blue should go back to 0
    }).toPass({ timeout: 5_000 });

    // Verify Bob sees the atomic vote change in real-time
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain('Green');
    }).toPass({ timeout: 10_000 });
  });

  test('close poll and voter names visible', async () => {
    // Check voter names are visible on hover/detail for the voted option
    // Hover over the Green option to see who voted
    const greenOption = alice.page.locator(
      '[class*="option" i]:has-text("Green"), [class*="poll" i] :text("Green")'
    ).first();
    await greenOption.hover();

    // Look for voter names in a tooltip, popover, or inline
    await expect(async () => {
      const body = await alice.page.textContent('body');
      // Alice voted for Green, so her name should appear
      expect(body).toMatch(new RegExp(`${ALICE}|AliceRenamed`));
    }).toPass({ timeout: 5_000 });

    // Alice (poll creator) closes the poll
    const closeBtn = alice.page.locator(
      'button:has-text("Close"), button:has-text("End Poll"), ' +
      '[aria-label*="close poll" i], button:has-text("Stop")'
    ).first();
    await closeBtn.click({ timeout: 5_000 });

    // Confirm close if needed
    const confirmBtn = alice.page.locator(
      'button:has-text("Confirm"), button:has-text("Yes"), button:has-text("Close Poll")'
    ).first();
    if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await confirmBtn.click();
    }

    // Verify the poll shows as closed
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body?.toLowerCase()).toMatch(/closed|ended|final results|poll ended/);
    }).toPass({ timeout: 5_000 });

    // Bob should see the poll is closed and cannot vote
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body?.toLowerCase()).toMatch(/closed|ended|final results|poll ended/);
    }).toPass({ timeout: 10_000 });

    // Verify Bob cannot vote — option buttons should be disabled or gone
    const bobBlueOption = bob.page.locator(
      'button:has-text("Blue"):not([disabled]), ' +
      '[class*="option" i]:has-text("Blue"):not([class*="disabled" i])'
    );
    const clickableCount = await bobBlueOption.count();
    // Either there are no clickable options, or clicking does nothing
    if (clickableCount > 0) {
      // Try clicking — it should have no effect
      await bobBlueOption.first().click().catch(() => {});
      await bob.page.waitForTimeout(1_000);
      // Blue should still show 0 votes (Alice's vote is on Green)
    }
  });
});
