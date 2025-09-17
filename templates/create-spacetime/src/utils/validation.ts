import validatePackageName from "validate-npm-package-name";
import which from "which";

export function getValidPackageName(input: string): string {
  if (!input || typeof input !== "string") {
    throw new Error("Project name cannot be empty");
  }

  const trimmed = input.trim();

  const validation = validatePackageName(trimmed);
  if (validation.validForNewPackages) {
    return trimmed;
  }

  // normalize package name to valid format
  const normalized = trimmed
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/^[._]/, "")
    .replace(/[^a-z0-9\-~]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");

  if (!normalized) {
    throw new Error("Project name contains invalid characters");
  }

  const normalizedValidation = validatePackageName(normalized);
  if (normalizedValidation.validForNewPackages) {
    return normalized;
  }
  const errorMessage = validation.errors?.[0] || "Invalid package name";
  throw new Error(errorMessage);
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
