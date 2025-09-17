import fs from "fs-extra";
import path from "path";

export type PackageManager = "npm" | "yarn" | "pnpm" | "bun";

const packageManagers: PackageManager[] = ["pnpm", "yarn", "bun", "npm"];

export async function detectPackageManager(): Promise<PackageManager> {
  const cwd = process.cwd();

  const [hasBun, hasPnpm, hasYarn] = await Promise.all([
    fs.pathExists(path.join(cwd, "bun.lockb")),
    fs.pathExists(path.join(cwd, "pnpm-lock.yaml")),
    fs.pathExists(path.join(cwd, "yarn.lock")),
  ]);

  if (hasBun) return "bun";
  if (hasPnpm) return "pnpm";
  if (hasYarn) return "yarn";

  const userAgent = process.env.npm_config_user_agent;
  if (userAgent) {
    for (const manager of packageManagers) {
      if (userAgent.includes(manager)) {
        return manager;
      }
    }
  }

  if (process.versions.bun) {
    return "bun";
  }

  return "npm";
}

const INSTALL_COMMANDS: Record<PackageManager, string[]> = {
  npm: ["install", "--no-fund", "--no-audit", "--loglevel=error"],
  yarn: ["install", "--silent"],
  pnpm: ["install"],
  bun: ["install"],
};

export function getInstallCommand(packageManager: PackageManager): string[] {
  return INSTALL_COMMANDS[packageManager];
}

const RUN_COMMANDS: Record<PackageManager, string> = {
  npm: "npm run",
  yarn: "yarn",
  pnpm: "pnpm",
  bun: "bun run",
};

export function getRunCommand(packageManager: PackageManager, script: string): string {
  return `${RUN_COMMANDS[packageManager]} ${script}`;
}
