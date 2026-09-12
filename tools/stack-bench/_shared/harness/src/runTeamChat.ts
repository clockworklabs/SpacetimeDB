// Entry point for the team-chat verifier.
//
// Harbor contract: write the scalar reward to /logs/verifier/reward.txt. We
// additionally write reward.json (multiple named metrics — Harbor supports
// float/int metric maps) and result.json (full per-check findings: the
// machine-readable payload a multi-step agent loop can feed back to the agent).
//
// Env:
//   ADAPTER_PATH  path to the per-backend adapter module (or argv[2])
//   RESTART_CMD   shell command that restarts the backend process (durability
//                 phase). Typically "/opt/stack-bench/backendctl restart".
//   REWARD_DIR    override /logs/verifier for local runs.

import { execSync } from "child_process";
import { promises as fs } from "fs";
import path from "path";
import type { AppClientFactory } from "./appClient.js";
import { runTeamChatScenario, scoreChecks, GROUP_WEIGHTS } from "./teamChat/scenario.js";

const REWARD_DIR = process.env.REWARD_DIR ?? "/logs/verifier";

async function writeOutputs(reward: number, rewardMetrics: Record<string, number>, result: object) {
  await fs.mkdir(REWARD_DIR, { recursive: true });
  await fs.writeFile(path.join(REWARD_DIR, "reward.txt"), reward.toFixed(4) + "\n");
  await fs.writeFile(path.join(REWARD_DIR, "reward.json"), JSON.stringify(rewardMetrics, null, 2));
  await fs.writeFile(path.join(REWARD_DIR, "result.json"), JSON.stringify(result, null, 2));
}

async function main() {
  const adapterPath = process.env.ADAPTER_PATH ?? process.argv[2];
  if (!adapterPath) throw new Error("ADAPTER_PATH (env) or argv path to the adapter is required");

  const mod = await import(path.resolve(adapterPath));
  const makeClient: AppClientFactory = mod.default ?? mod.makeClient;
  if (typeof makeClient !== "function") {
    throw new Error(`adapter ${adapterPath} must default-export an AppClientFactory`);
  }

  const restartCmd = process.env.RESTART_CMD;
  const restartBackend = restartCmd
    ? async () => {
        console.log(`\n== restarting backend: ${restartCmd}`);
        execSync(restartCmd, { stdio: "inherit", timeout: 180_000 });
      }
    : undefined;

  console.log("== team-chat scenario starting");
  const { checks, metrics } = await runTeamChatScenario({ makeClient, restartBackend });
  const { reward, groups } = scoreChecks(checks);

  const rewardMetrics: Record<string, number> = { reward: Number(reward.toFixed(4)) };
  for (const [g, s] of Object.entries(groups)) rewardMetrics[g] = Number(s.score.toFixed(4));
  for (const [k, v] of Object.entries(metrics)) if (typeof v === "number" && isFinite(v)) rewardMetrics[k] = v;

  await writeOutputs(reward, rewardMetrics, { reward, weights: GROUP_WEIGHTS, groups, metrics, checks });

  console.log(`\n== team-chat reward=${reward.toFixed(4)}`);
  for (const [g, s] of Object.entries(groups)) {
    console.log(`   ${g.padEnd(12)} ${s.passed}/${s.total} (weight ${GROUP_WEIGHTS[g as keyof typeof GROUP_WEIGHTS]})`);
  }
}

main().catch(async (err) => {
  // Any harness/adapter crash is a 0 reward, not a Harbor infra failure.
  console.error("team-chat harness error:", err);
  await writeOutputs(0, { reward: 0 }, { error: String(err) }).catch(() => {});
  process.exit(0);
});
