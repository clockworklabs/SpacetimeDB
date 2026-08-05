import { chromium } from "playwright";

function rnd() {
  return Math.random().toString(36).slice(2, 8);
}

async function main() {
  const browser = await chromium.launch();
  const alicePage = await browser.newPage();
  const bobPage = await browser.newPage();

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

  const roomName = "storm_" + rnd();
  await alicePage.click('[data-testid="room-create"]');
  await alicePage.fill('[data-testid="room-name-input"]', roomName);
  await alicePage.click('[data-testid="room-name-submit"]');
  await alicePage.waitForSelector('[data-testid="message-input"]');

  await bobPage.waitForSelector(`[data-testid="room-item"]:has-text("${roomName}")`, { timeout: 8000 });
  await bobPage.click(`[data-testid="room-item"]:has-text("${roomName}")`);
  await bobPage.waitForSelector('[data-testid="message-input"]');

  async function sendMany(page, prefix, count, delayMs) {
    for (let i = 0; i < count; i++) {
      await page.fill('[data-testid="message-input"]', `${prefix}${i}`);
      await page.press('[data-testid="message-input"]', "Enter");
      await page.waitForTimeout(delayMs);
    }
  }

  await Promise.all([sendMany(alicePage, "AA", 6, 600), sendMany(bobPage, "BB", 6, 600)]);

  await alicePage.waitForTimeout(3000);

  const aliceTexts = await alicePage.locator('[data-testid="message-item"]').allTextContents();
  const bobTexts = await bobPage.locator('[data-testid="message-item"]').allTextContents();

  function countPrefix(texts, prefix, count) {
    const found = [];
    for (let i = 0; i < count; i++) {
      const c = texts.filter((t) => t.includes(`${prefix}${i}`)).length;
      found.push(c);
    }
    return found;
  }

  console.log("alice sees AA counts:", countPrefix(aliceTexts, "AA", 6));
  console.log("alice sees BB counts:", countPrefix(aliceTexts, "BB", 6));
  console.log("bob sees AA counts:", countPrefix(bobTexts, "AA", 6));
  console.log("bob sees BB counts:", countPrefix(bobTexts, "BB", 6));
  console.log("alice total messages:", aliceTexts.length, "bob total messages:", bobTexts.length);

  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
