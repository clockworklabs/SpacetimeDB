import { sync } from "cross-spawn";
import { TIMEOUTS } from "../constants.js";

export async function checkSpacetimeLogin(): Promise<boolean> {
  try {
    const result = sync("spacetime", ["login", "show"], {
      stdio: "pipe",
      encoding: "utf8",
      timeout: TIMEOUTS.COMMAND_TIMEOUT,
      windowsHide: true,
    });

    if (result.status === 0 && result.stdout) {
      const output = String(result.stdout);

      if (output.includes("You are not logged in")) {
        return false;
      }

      if (output.includes("You are logged in as")) {
        return true;
      }
    }

    return false;
  } catch (error) {
    console.error("SpacetimeDB login check failed:", error);
    return false;
  }
}
