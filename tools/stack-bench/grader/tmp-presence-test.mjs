import { chromium } from "playwright";

function rnd() {
  return Math.random().toString(36).slice(2, 8);
}

async function main() {
  const browser = await chromium.launch();
  const alicePage = await browser.newPage();
  const bobPage = await browser.newPage();

  alicePage.on("console", (msg) => console.log("[alice console]", msg.type(), msg.text()));
  bobPage.on("console", (msg) => console.log("[bob console]", msg.type(), msg.text()));
  alicePage.on("pageerror", (err) => console.log("[alice pageerror]", err.message));
  bobPage.on("pageerror", (err) => console.log("[bob pageerror]", err.message));

  const uA = "alice_" + rnd();
  const uB = "bob_" + rnd();

  await alicePage.goto("http://localhost:6374");
  await alicePage.fill('[data-testid="signup-username"]', uA);
  await alicePage.fill('[data-testid="signup-password"]', "password123");
  await alicePage.click('[data-testid="signup-submit"]');
  await alicePage.waitForSelector('[data-testid="current-user"]');
  console.log("alice signed up");

  await bobPage.goto("http://localhost:6374");
  await bobPage.fill('[data-testid="signup-username"]', uB);
  await bobPage.fill('[data-testid="signup-password"]', "password123");
  await bobPage.click('[data-testid="signup-submit"]');
  await bobPage.waitForSelector('[data-testid="current-user"]');
  console.log("bob signed up");

  await alicePage.waitForSelector('[data-testid="online-users"]:has-text("' + uB + '")', { timeout: 8000 });
  console.log("alice sees bob online");

  await bobPage.click('[data-testid="signout"]');
  console.log("bob clicked signout at", Date.now());

  await bobPage.waitForTimeout(500);
  const bobShowsAuth = await bobPage.locator('[data-testid="signup-username"]').count();
  console.log("bob page shows auth screen after signout:", bobShowsAuth > 0);

  try {
    await alicePage.waitForSelector('[data-testid="online-users"]:has-text("' + uB + '")', { state: "detached", timeout: 12000 });
    console.log("bob disappeared from alice's online list at", Date.now(), " -- PASS");
  } catch (e) {
    console.log("bob STILL VISIBLE after 12000ms -- FAIL");
    const html = await alicePage.locator('[data-testid="online-users"]').innerHTML();
    console.log("online-users html:", html);
  }

  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
