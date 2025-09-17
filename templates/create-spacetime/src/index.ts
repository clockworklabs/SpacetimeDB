#!/usr/bin/env node

import { Command } from "commander";
import inquirer from "inquirer";
import chalk from "chalk";
import fs from "fs-extra";
import path from "path";
import spawn from "cross-spawn";
import { readFileSync } from "fs";
import { createProject } from "./create-project.js";
import { getTemplateChoices, getValidTemplateKeys, isValidTemplate } from "./templates/index.js";
import { detectPackageManager } from "./utils/packageManager.js";
import { getValidPackageName, checkDependencies } from "./utils/validation.js";

const defaultTargetDir = "my-spacetime-app";
const cwd = process.cwd();

function getPackageVersion(): string {
  try {
    const packageJson = JSON.parse(
      readFileSync(new URL("../package.json", import.meta.url), "utf8"),
    );
    return packageJson.version || "0.0.3";
  } catch {
    return "0.0.3";
  }
}

init().catch((e) => {
  const errorMessage = e instanceof Error ? e.message : String(e);
  console.error(chalk.red("Error:"), errorMessage);
  process.exit(1);
});

async function checkSpacetimeLogin(): Promise<{ loggedIn: boolean }> {
  try {
    const result = spawn.sync("spacetime", ["login", "show"], {
      stdio: "pipe",
      encoding: "utf8",
      timeout: 10000,
      windowsHide: true,
    });

    if (result.status === 0 && result.stdout) {
      const output = String(result.stdout);

      if (output.includes("You are not logged in")) {
        return { loggedIn: false };
      }

      if (output.includes("You are logged in as")) {
        return { loggedIn: true };
      }
    }

    return { loggedIn: false };
  } catch (error) {
    console.error("SpacetimeDB login check failed:", error);
    return { loggedIn: false };
  }
}

function validateProjectName(input: string): { valid: boolean; name?: string; error?: string } {
  try {
    const name = getValidPackageName(input || "");
    return { valid: true, name };
  } catch (error) {
    return { valid: false, error: error instanceof Error ? error.message : "Invalid project name" };
  }
}

async function init() {
  const program = new Command();

  program
    .name("create-spacetime")
    .description("Create a new SpacetimeDB project")
    .version(getPackageVersion())
    .argument("[project-name]", "Name of the project to create")
    .option("-t, --template <template>", `Template to use (${getValidTemplateKeys().join(", ")})`)
    .option("--local", "Use local SpacetimeDB server instead of cloud")
    .option("-y, --yes", "Skip interactive prompts and use defaults")
    .option("--dry-run", "Show what would be created without actually creating it")
    .action(async (projectNameArg, options) => {
      console.log(chalk.blue("Welcome to create-spacetime!"));
      console.log();

      const packageManager = await detectPackageManager();
      console.log(`Using package manager: ${chalk.green(packageManager)}`);
      console.log();

      const deps = await checkDependencies();

      if (!deps.node || !deps.npm) {
        console.error(chalk.red("Error: Node.js and npm are required but not found."));
        console.error(`Please install Node.js from ${chalk.bold("https://nodejs.org/")}`);
        process.exit(1);
      }

      if (!deps.spacetime) {
        console.warn(chalk.yellow("Warning: SpacetimeDB CLI not found."));
        console.warn(`Install from: ${chalk.bold("https://spacetimedb.com/install")}`);
        console.log();
      }

      let isLoggedIn = false;

      if (deps.spacetime && !options.local) {
        const loginInfo = await checkSpacetimeLogin();
        isLoggedIn = loginInfo.loggedIn;

        if (isLoggedIn) {
          console.log(chalk.green("Already logged in to SpacetimeDB"));
        } else {
          console.log(chalk.yellow("Not logged in to SpacetimeDB"));

          if (!options.yes) {
            const { shouldLogin } = await inquirer.prompt([
              {
                type: "confirm",
                name: "shouldLogin",
                message: "Do you want to login to SpacetimeDB? (required to deploy to Maincloud)",
                default: true,
              },
            ]);

            if (shouldLogin) {
              console.log("Opening SpacetimeDB login...");
              try {
                const loginResult = spawn.sync("spacetime", ["login"], {
                  stdio: "inherit",
                  encoding: "utf8",
                  timeout: 30000,
                });

                if (loginResult.status === 0) {
                  console.log(chalk.green("Successfully logged in to SpacetimeDB"));
                  isLoggedIn = true;
                } else {
                  console.log(chalk.yellow("Login cancelled or failed"));
                  console.log(chalk.gray("Will default to local deployment"));
                }
              } catch (error) {
                console.log(chalk.yellow("Login failed"));
                console.log(chalk.gray("Will default to local deployment"));
              }
            } else {
              console.log(chalk.gray("Skipping login - will default to local deployment"));
            }
          } else {
            console.log(
              chalk.gray("Skipping login (--yes mode) - will default to local deployment"),
            );
          }
        }
        console.log();
      }

      if (options.template && !isValidTemplate(options.template)) {
        console.error(chalk.red(`Invalid template: ${options.template}`));
        console.error(chalk.yellow(`Valid templates: ${getValidTemplateKeys().join(", ")}`));
        process.exit(1);
      }

      const argTargetDir = formatTargetDir(projectNameArg);
      let targetDir = argTargetDir || defaultTargetDir;

      const getProjectName = () => (targetDir === "." ? path.basename(path.resolve()) : targetDir);

      if (options.dryRun) {
        const nameValidation = validateProjectName(getProjectName());
        if (!nameValidation.valid) {
          console.error(chalk.red(`Invalid project name: ${getProjectName()}`));
          console.error(
            chalk.yellow(nameValidation.error || "Please provide a valid project name"),
          );
          process.exit(1);
        }

        const selectedTemplate = options.template || "rust";
        const useLocal = options.local || !isLoggedIn;

        console.log(chalk.green("Dry run - showing what would be created:"));
        console.log();
        console.log(`Project name: ${chalk.bold(nameValidation.name)}`);
        console.log(`Template: ${chalk.bold(selectedTemplate)}`);
        console.log(`Package manager: ${chalk.bold(packageManager)}`);
        console.log(`Target: ${chalk.bold(useLocal ? "Local deployment" : "Maincloud")}`);
        console.log(`Auto setup: ${chalk.bold(deps.spacetime ? "Yes" : "No")}`);
        console.log(`Path: ${chalk.bold(path.resolve(nameValidation.name!))}`);
        console.log();
        console.log(chalk.green("Nothing created (dry run mode)"));
        return;
      }

      let promptProjectName: string | undefined;
      let promptTemplate: string | undefined;
      let promptDeploymentTarget: string | undefined;
      let promptAutoSetup: boolean | undefined;

      try {
        if (options.yes) {
          // use defaults with -y option
          promptProjectName = argTargetDir ? undefined : defaultTargetDir;
          promptTemplate = options.template || "rust";
          promptDeploymentTarget = options.local ? "local" : isLoggedIn ? "cloud" : "local";
          promptAutoSetup = deps.spacetime;
        } else {
          // interactive prompts
          const questions = [];

          if (!argTargetDir) {
            questions.push({
              type: "input",
              name: "projectName",
              message: "Project name:",
              default: defaultTargetDir,
              validate: (input: string) => {
                const validation = validateProjectName(input);
                return validation.valid || validation.error || "Invalid project name";
              },
            });
          }

          if (!options.template) {
            questions.push({
              type: "list",
              name: "template",
              message: "Choose a server language:",
              choices: getTemplateChoices(),
              default: "rust",
            });
          }

          // only asks deployment target if user is logged in or --local flag wasn't specified
          if (!options.local && deps.spacetime && isLoggedIn) {
            questions.push({
              type: "list",
              name: "deploymentTarget",
              message: "Choose deployment target:",
              choices: [
                {
                  name: "Maincloud [authenticated]",
                  value: "cloud",
                },
                {
                  name: "Local",
                  value: "local",
                },
              ],
              default: "cloud",
            });
          }

          if (deps.spacetime) {
            questions.push({
              type: "confirm",
              name: "autoSetup",
              message: (answers: any) => {
                const target =
                  answers.deploymentTarget ||
                  (options.local ? "local" : isLoggedIn ? "cloud" : "local");
                return `Build and ${target === "cloud" ? "deploy to Maincloud" : "publish locally"}?`;
              },
              default: deps.spacetime,
            });
          }

          if (questions.length > 0) {
            const result = await inquirer.prompt(questions);
            promptProjectName = result.projectName;
            promptTemplate = result.template;
            promptDeploymentTarget = result.deploymentTarget;
            promptAutoSetup = result.autoSetup;
          }
        }
      } catch {
        console.log(chalk.red("Operation cancelled"));
        process.exit(0);
      }

      if (promptProjectName) {
        targetDir = formatTargetDir(promptProjectName) || defaultTargetDir;
      }

      const projectName = getProjectName();
      const nameValidation = validateProjectName(projectName);
      if (!nameValidation.valid) {
        console.error(chalk.red(`Invalid project name: ${projectName}`));
        console.error(chalk.yellow(nameValidation.error || "Please provide a valid project name"));
        process.exit(1);
      }

      const selectedTemplate = options.template || promptTemplate || "rust";
      const useLocal = options.local || promptDeploymentTarget === "local" || !isLoggedIn;

      const validatedName = nameValidation.name!;
      const root = path.join(cwd, validatedName);

      if ((await fs.pathExists(root)) && !(await isEmpty(root))) {
        const { shouldOverwrite } = await inquirer.prompt([
          {
            type: "confirm",
            name: "shouldOverwrite",
            message: `Directory "${validatedName}" is not empty. Remove existing files?`,
            default: false,
          },
        ]);

        if (!shouldOverwrite) {
          console.log(chalk.red("Operation cancelled"));
          process.exit(0);
        }

        await emptyDir(root);
      } else if (!(await fs.pathExists(root))) {
        await fs.ensureDir(root);
      }

      console.log(`\n${chalk.cyan("Setting up SpacetimeDB project...")}`);
      if (useLocal) {
        console.log(chalk.gray("Target: Local deployment"));
      } else {
        console.log(chalk.gray("Target: Maincloud"));
      }
      console.log();

      const success = await createProject({
        name: validatedName,
        root,
        template: selectedTemplate,
        packageManager,
        useLocal,
        autoSetup: promptAutoSetup && deps.spacetime,
      });

      if (!success) {
        console.error(chalk.red("Project creation failed"));
        process.exit(1);
      }
    });

  program.parse();
}

function formatTargetDir(targetDir: string | undefined): string | undefined {
  if (!targetDir) return undefined;

  // sanitize directory name for file system compatibility
  const formatted = targetDir
    .trim()
    .replace(/\/+$/g, "")
    .replace(/\s+/g, "-")
    .replace(/[<>:"|?*]/g, "")
    .replace(/^\.+/, "");

  return formatted || undefined;
}

async function isEmpty(path: string): Promise<boolean> {
  if (!(await fs.pathExists(path))) {
    return true;
  }
  const files = await fs.readdir(path);
  return files.length === 0 || (files.length === 1 && files[0] === ".git");
}

async function emptyDir(dir: string): Promise<void> {
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
