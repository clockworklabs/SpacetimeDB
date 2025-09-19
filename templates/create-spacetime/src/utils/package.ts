import { readFileSync } from "fs";

export type PackageManager = "npm" | "yarn" | "pnpm" | "bun";

export function getPackageVersion(): string {
  const packageJson = JSON.parse(
    readFileSync(new URL("../../package.json", import.meta.url), "utf8"),
  );
  return packageJson.version;
}

export function detectPackageManager(): PackageManager {
  const userAgent = process.env.npm_config_user_agent || "";
  if (userAgent.includes("pnpm")) return "pnpm";
  if (userAgent.includes("yarn")) return "yarn";
  if (userAgent.includes("bun")) return "bun";

  return process.versions.bun ? "bun" : "npm";
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

export function getRunCommand(pm: PackageManager, script: string): string {
  return pm === "yarn" || pm === "pnpm" ? `${pm} ${script}` : `${pm} run ${script}`;
}
