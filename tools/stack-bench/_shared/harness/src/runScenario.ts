import { promises as fs } from "fs";
import path from "path";
import { runChatScenario } from "./scenario.js";
import type { ChatClientFactory } from "./chatClient.js";

// Harbor's verification contract: write a reward to /logs/verifier/reward.txt.
// REWARD_DIR is overridable for local runs outside a Harbor container.
const REWARD_DIR = process.env.REWARD_DIR ?? "/logs/verifier";

async function writeReward(reward: number, extra: object = {}) {
  await fs.mkdir(REWARD_DIR, { recursive: true });
  await fs.writeFile(path.join(REWARD_DIR, "reward.txt"), reward.toFixed(4) + "\n");
  await fs.writeFile(path.join(REWARD_DIR, "result.json"), JSON.stringify({ reward, ...extra }, null, 2));
}

async function main() {
  // The per-backend adapter is selected by path: env ADAPTER_PATH or argv[2].
  const adapterPath = process.env.ADAPTER_PATH ?? process.argv[2];
  if (!adapterPath) throw new Error("ADAPTER_PATH (env) or argv path to the compiled adapter is required");

  const mod = await import(path.resolve(adapterPath));
  const makeClient: ChatClientFactory = mod.default ?? mod.makeClient;
  if (typeof makeClient !== "function") {
    throw new Error(`adapter ${adapterPath} must default-export a ChatClientFactory`);
  }

  const result = await runChatScenario(makeClient);
  const reward = result.total === 0 ? 0 : result.passed / result.total;
  await writeReward(reward, result);

  console.log(`\nstack-bench reward=${reward.toFixed(4)} (${result.passed}/${result.total} checks)`);
  for (const c of result.checks) {
    console.log(`  [${c.pass ? "PASS" : "FAIL"}] ${c.name}${c.detail ? " — " + c.detail : ""}`);
  }
}

main().catch(async (err) => {
  // Any harness/adapter crash is a 0 reward, not a Harbor infra failure.
  console.error("stack-bench harness error:", err);
  await writeReward(0, { error: String(err) }).catch(() => {});
  process.exit(0);
});
