import fs from "fs-extra";
import path from "path";

export function formatTargetDir(targetDir: string | undefined): string | undefined {
  if (!targetDir) return undefined;

  if (targetDir.trim() === ".") return ".";

  // sanitize directory name for file system compatibility
  const formatted = targetDir
    .trim()
    .replace(/\/+$/g, "")
    .replace(/\s+/g, "-")
    .replace(/[<>:"|?*]/g, "")
    .replace(/^\.+/, "");

  return formatted || undefined;
}

export async function isEmpty(path: string): Promise<boolean> {
  if (!(await fs.pathExists(path))) {
    return true;
  }
  const files = await fs.readdir(path);
  return files.length === 0 || (files.length === 1 && files[0] === ".git");
}

export async function emptyDir(dir: string): Promise<void> {
  if (!(await fs.pathExists(dir))) {
    return;
  }
  try {
    const files = await fs.readdir(dir);
    await Promise.all(files.map((file) => fs.remove(path.join(dir, file))));
  } catch (error) {
    throw new Error(`Failed to empty directory: ${error}`);
  }
}
