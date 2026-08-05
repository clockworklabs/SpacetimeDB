import { chromium } from "playwright";

function rnd() {
  return Math.random().toString(36).slice(2, 8);
}

async function signupAndJoin(browser, username, roomName, isCreator) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await page.goto("http://localhost:6374");
  await page.fill('[data-testid="signup-username"]', username);
  await page.fill('[data-testid="signup-password"]', "password123");
  await page.click('[data-testid="signup-submit"]');
  await page.waitForSelector('[data-testid="current-user"]');
  if (isCreator) {
    await page.click('[data-testid="room-create"]');
    await page.fill('[data-testid="room-name-input"]', roomName);
    await page.click('[data-testid="room-name-submit"]');
    await page.waitForSelector('[data-testid="message-input"]');
  } else {
    await page.waitForSelector(`[data-testid="room-item"]:has-text("${roomName}")`, { timeout: 8000 });
    await page.click(`[data-testid="room-item"]:has-text("${roomName}")`);
    await page.waitForSelector('[data-testid="message-input"]');
  }
  return page;
}

async function main() {
  const browser = await chromium.launch();
  const roomName = "invar_" + rnd();
  const names = ["alice", "bob", "carol", "dave"].map((n) => n + "_" + rnd());

  const alice = await signupAndJoin(browser, names[0], roomName, true);
  const bob = await signupAndJoin(browser, names[1], roomName, false);
  const carol = await signupAndJoin(browser, names[2], roomName, false);
  const dave = await signupAndJoin(browser, names[3], roomName, false);
  const pages = { alice, bob, carol, dave };

  await alice.fill('[data-testid="message-input"]', "react to this");
  await alice.press('[data-testid="message-input"]', "Enter");
  await alice.waitForTimeout(500);

  // --- reaction-count-is-exact: 4 users click react-button concurrently ---
  await Promise.all(Object.values(pages).map((p) => p.locator('[data-testid="react-button"]').first().click()));
  await alice.waitForTimeout(4000);

  const countText = await alice.locator('[data-testid="reaction-count"]').first().textContent({ timeout: 10000 }).catch(() => null);
  console.log("reaction-count-is-exact: alice sees count =", countText, countText === "4" ? "PASS" : "FAIL");

  const bobCount = await bob.locator('[data-testid="reaction-count"]').first().textContent({ timeout: 10000 }).catch(() => null);
  console.log("reaction-count-agrees-across-clients: alice=", countText, "bob=", bobCount, countText === bobCount ? "PASS" : "FAIL");

  // --- reacting-twice-does-not-double-count ---
  await alice.locator('[data-testid="react-button"]').first().click();
  await alice.waitForTimeout(2000);
  await alice.locator('[data-testid="react-button"]').first().click();
  await alice.waitForTimeout(2500);
  const countAfter1 = await alice.locator('[data-testid="reaction-count"]').first().textContent({ timeout: 8000 }).catch(() => null);
  const countAfter2 = await bob.locator('[data-testid="reaction-count"]').first().textContent({ timeout: 8000 }).catch(() => null);
  console.log("reacting-twice-does-not-double-count: alice=", countAfter1, "bob=", countAfter2, countAfter1 === countAfter2 ? "PASS (agree)" : "FAIL (disagree)");

  // --- pin-cap-holds-under-concurrency ---
  for (let i = 0; i < 4; i++) {
    await alice.fill('[data-testid="message-input"]', `PIN${i}`);
    await alice.press('[data-testid="message-input"]', "Enter");
    await alice.waitForTimeout(1200);
  }
  await Promise.all(Object.values(pages).map((p) => p.locator('[data-testid="pin-button"]').first().click()));
  await alice.waitForTimeout(5000);

  const alicePinCount = await alice.locator('[data-testid="pinned-item"]').count();
  const bobPinCount = await bob.locator('[data-testid="pinned-item"]').count();
  console.log("pin-cap-holds-under-concurrency: alice pins =", alicePinCount, "bob pins =", bobPinCount, (alicePinCount === 3 && bobPinCount === 3) ? "PASS" : "FAIL");

  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
