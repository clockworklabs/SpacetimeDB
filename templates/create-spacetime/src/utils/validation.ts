import validatePackageName from "validate-npm-package-name";
import which from "which";

export function getValidPackageName(input: string): string {
  if (!input?.trim()) throw new Error("Project name cannot be empty");

  // normalize package name to valid format
  const normalized = input
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/^[._]/, "")
    .replace(/[^a-z0-9\-~]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");

  const validation = validatePackageName(normalized);
  if (!validation.validForNewPackages) {
    throw new Error(validation.errors?.[0] || "Invalid package name");
  }

  return normalized;
}

export async function checkDependencies(): Promise<{
  spacetime: boolean;
  node: boolean;
  npm: boolean;
}> {
  const [spacetime, node, npm] = await Promise.all([
    which("spacetime")
      .then(() => true)
      .catch(() => false),
    which("node")
      .then(() => true)
      .catch(() => false),
    which("npm")
      .then(() => true)
      .catch(() => false),
  ]);

  return { spacetime, node, npm };
}
