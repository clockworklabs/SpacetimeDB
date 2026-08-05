import { chromium } from "playwright";

function rnd() {
  return Math.random().toString(36).slice(2, 8);
}

async function main() {
  const browser = await chromium.launch();
  const aliceCtx = await browser.newContext();
  const bobCtx = await browser.newContext();
  const alicePage = await aliceCtx.newPage();
  const bobPage = await bobCtx.newPage();

  const uA = "alice_" + rnd();
  const uB = "bob_" + rnd();

  await alicePage.goto("http://localhost:6374");
  await alicePage.fill('[data-testid="signup-username"]', uA);
  await alicePage.fill('[data-testid="signup-password"]', "password123");
  await alicePage.click('[data-testid="signup-submit"]');
  await alicePage.waitForSelector('[data-testid="current-user"]');

  await bobPage.goto("http://localhost:6374");
  await bobPage.fill('[data-testid="signup-username"]', uB);
  await bobPage.fill('[data-testid="signup-password"]', "password123");
  await bobPage.click('[data-testid="signup-submit"]');
  await bobPage.waitForSelector('[data-testid="current-user"]');

  const roomName = "resync_" + rnd();
  await alicePage.click('[data-testid="room-create"]');
  await alicePage.fill('[data-testid="room-name-input"]', roomName);
  await alicePage.click('[data-testid="room-name-submit"]');
  await alicePage.waitForSelector('[data-testid="message-input"]');

  await bobPage.waitForSelector(`[data-testid="room-item"]:has-text("${roomName}")`, { timeout: 8000 });
  await bobPage.click(`[data-testid="room-item"]:has-text("${roomName}")`);
  await bobPage.waitForSelector('[data-testid="message-input"]');

  console.log("alice going offline");
  await aliceCtx.setOffline(true);
  await alicePage.waitForTimeout(1500);

  async function sendMany(page, prefix, count, delayMs) {
    for (let i = 0; i < count; i++) {
      await page.fill('[data-testid="message-input"]', `${prefix}${i}`);
      await page.press('[data-testid="message-input"]', "Enter");
      await page.waitForTimeout(delayMs);
    }
  }

  console.log("bob sending OFFLINE messages while alice is offline");
  await sendMany(bobPage, "OFFLINE", 3, 600);

  console.log("alice going back online");
  await aliceCtx.setOffline(false);
  await alicePage.waitForTimeout(3000);

  let aliceTexts = await alicePage.locator('[data-testid="message-item"]').allTextContents();
  console.log("alice sees after resync:", aliceTexts.filter((t) => t.includes("OFFLINE")).length, "of 3 OFFLINE messages");

  console.log("bob sending AFTER messages");
  await sendMany(bobPage, "AFTER", 2, 600);
  await alicePage.waitForTimeout(2000);

  aliceTexts = await alicePage.locator('[data-testid="message-item"]').allTextContents();
  const offlineCount = aliceTexts.filter((t) => t.includes("OFFLINE0")).length;
  const afterCounts = [0, 1].map((i) => aliceTexts.filter((t) => t.includes(`AFTER${i}`)).length);
  console.log("OFFLINE0 duplication count (should be 1):", offlineCount);
  console.log("AFTER message counts (should be [1,1]):", afterCounts);
  console.log("total messages on alice's screen:", aliceTexts.length, "(expect 5)");

  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
