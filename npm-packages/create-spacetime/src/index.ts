#!/usr/bin/env node

import { Command } from "commander";
import inquirer from "inquirer";
import chalk from "chalk";
import fs from "fs-extra";
import path from "path";
import { sync } from "cross-spawn";

import { createProject } from "./lib/create-project.js";
import {
  getTemplateChoices,
  isValidTemplate,
  TEMPLATES,
  DEFAULT_TEMPLATE,
} from "./templates/index.js";
import { detectPackageManager, getPackageVersion } from "./utils/package.js";
import { getValidPackageName, checkDependencies } from "./utils/validation.js";
import { formatTargetDir, isEmpty, emptyDir } from "./utils/directory.js";
import { checkSpacetimeLogin } from "./utils/spacetime.js";
import { DEFAULT_TARGET_DIR } from "./constants.js";

const cwd = process.cwd();

async function handleCreateProject(projectNameArg: string, options: any) {
  console.log(chalk.blue("Welcome to create-spacetime!"));
  console.log();

  const packageManager = detectPackageManager();
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
    isLoggedIn = await checkSpacetimeLogin();

    console.log(
      isLoggedIn
        ? chalk.green("Already logged in to SpacetimeDB")
        : chalk.yellow("Not logged in to SpacetimeDB"),
    );

    if (!isLoggedIn && !options.yes) {
      const { shouldLogin } = await inquirer.prompt([
        {
          type: "confirm",
          name: "shouldLogin",
          message: "Login to SpacetimeDB? (required for Maincloud)",
          default: true,
        },
      ]);

      if (shouldLogin) {
        console.log("Opening SpacetimeDB login...");
        try {
          const result = sync("spacetime", ["login"], {
            stdio: "inherit",
            encoding: "utf8",
            timeout: 30000,
          });
          isLoggedIn = result.status === 0;
          console.log(
            isLoggedIn
              ? chalk.green("Successfully logged in to SpacetimeDB")
              : chalk.yellow("Login failed - default to local deployment"),
          );
        } catch {
          console.log(chalk.yellow("Login failed - default to local deployment"));
        }
      } else {
        console.log(chalk.gray("Skipping login - default to local deployment"));
      }
    } else if (!isLoggedIn && options.yes) {
      console.log(chalk.gray("Skipping login (--yes mode) - default to local deployment"));
    }
    console.log();
  }

  const argTargetDir = formatTargetDir(projectNameArg);
  let targetDir = argTargetDir || DEFAULT_TARGET_DIR;

  const getProjectName = () => (targetDir === "." ? path.basename(path.resolve()) : targetDir);

  if (options.dryRun) {
    let validatedName: string;
    try {
      validatedName = getValidPackageName(getProjectName());
    } catch (error) {
      console.error(chalk.red(`Invalid project name: ${getProjectName()}`));
      console.error(
        chalk.yellow(
          error instanceof Error ? error.message : "Please provide a valid project name",
        ),
      );
      process.exit(1);
    }

    const selectedTemplate = options.template || DEFAULT_TEMPLATE;
    const useLocal = options.local || !isLoggedIn;

    console.log(chalk.green("Dry run - showing what would be created:"));
    console.log();
    console.log(`Project name: ${chalk.bold(validatedName)}`);
    console.log(`Template: ${chalk.bold(selectedTemplate)}`);
    console.log(`Package manager: ${chalk.bold(packageManager)}`);
    console.log(`Target: ${chalk.bold(useLocal ? "Local deployment" : "Maincloud")}`);
    const supportsAutoSetup = deps.spacetime && isValidTemplate(selectedTemplate);
    console.log(`Auto setup: ${chalk.bold(supportsAutoSetup ? "Yes" : "No")}`);
    console.log(`Path: ${chalk.bold(path.resolve(validatedName))}`);
    console.log();
    console.log(chalk.green("Nothing created (dry run mode)"));
    return;
  }

  let promptProjectName: string | undefined;
  let promptTemplate: string | undefined;
  let promptAutoSetup: boolean | undefined;

  try {
    if (options.yes) {
      promptProjectName = argTargetDir ? undefined : DEFAULT_TARGET_DIR;
      promptTemplate = options.template || DEFAULT_TEMPLATE;
      promptAutoSetup = deps.spacetime;
    } else {
      const questions = [];

      if (!argTargetDir) {
        questions.push({
          type: "input",
          name: "projectName",
          message: "Project name:",
          default: DEFAULT_TARGET_DIR,
          validate: (input: string) => {
            try {
              getValidPackageName(input);
              return true;
            } catch (error) {
              return error instanceof Error ? error.message : "Invalid project name";
            }
          },
        });
      }

      if (!options.template) {
        questions.push({
          type: "list",
          name: "template",
          message: "Choose a server language:",
          choices: getTemplateChoices(),
          default: DEFAULT_TEMPLATE,
        });

        questions.push({
          type: "input",
          name: "githubRepo",
          message: "GitHub repository (full URL or owner/repo format):",
          when: (answers: any) => answers.template === "custom",
          validate: (input: string) => {
            if (!input.trim()) {
              return "Please enter a GitHub repository";
            }

            if (input.includes(" ")) {
              return "Repository name cannot contain spaces";
            }

            if (!input.includes("/") && !input.startsWith("https://github.com/")) {
              return `Please use owner/repo format`;
            }

            return true;
          },
        });
      }

      if (!options.local && deps.spacetime) {
        if (isLoggedIn) {
          console.log(chalk.gray("Using Maincloud deployment (authenticated)"));
        } else {
          console.log(chalk.gray("Using local deployment (not authenticated for Maincloud)"));
        }
      }

      if (deps.spacetime) {
        questions.push({
          type: "confirm",
          name: "autoSetup",
          message: () => {
            const target = options.local ? "local" : isLoggedIn ? "maincloud" : "local";
            return `Build and ${target === "maincloud" ? "deploy to Maincloud" : "publish locally"} after setup?`;
          },
          default: deps.spacetime,
          // only asks autoSetup for default templates
          when: (answers: any) => {
            const selectedTemplate = options.template || answers.template;
            return !selectedTemplate || isValidTemplate(selectedTemplate);
          },
        });
      }

      if (questions.length > 0) {
        const result = await inquirer.prompt(questions);
        promptProjectName = result.projectName;
        promptTemplate = result.template === "custom" ? result.githubRepo : result.template;
        promptAutoSetup = result.autoSetup;
      }
    }
  } catch {
    console.log(chalk.red("Operation cancelled"));
    process.exit(0);
  }

  if (promptProjectName) {
    targetDir = formatTargetDir(promptProjectName) || DEFAULT_TARGET_DIR;
  }

  const projectName = getProjectName();
  let validatedName: string;
  try {
    validatedName = getValidPackageName(projectName);
  } catch (error) {
    console.error(chalk.red(`Invalid project name: ${projectName}`));
    console.error(
      chalk.yellow(error instanceof Error ? error.message : "Please provide a valid project name"),
    );
    process.exit(1);
  }

  const selectedTemplate = options.template || promptTemplate || DEFAULT_TEMPLATE;
  const useLocal = options.local || !isLoggedIn;

  const root = path.join(cwd, validatedName);

  if ((await fs.pathExists(root)) && !(await isEmpty(root))) {
    const { shouldOverwrite } = await inquirer.prompt([
      {
        type: "confirm",
        name: "shouldOverwrite",
        message: `Directory "${validatedName}" is not empty. Remove existing files?`,
        default: true,
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
}

async function init() {
  const program = new Command();

  program
    .name("create-spacetime")
    .description("Create a new SpacetimeDB project")
    .version(getPackageVersion())
    .argument("[project-name]", "Name of the project to create")
    .option(
      "-t, --template <template>",
      `Template to use: ${Object.keys(TEMPLATES).join(", ")}, or GitHub repository (owner/repo format)`,
    )
    .option("--local", "Use local SpacetimeDB server instead of cloud")
    .option("-y, --yes", "Skip interactive prompts and use defaults")
    .option("--dry-run", "Show what would be created without actually creating it")
    .action(handleCreateProject);

  program.parse();
}

init().catch((e) => {
  const errorMessage = e instanceof Error ? e.message : String(e);
  console.error(chalk.red("Error:"), errorMessage);
  process.exit(1);
});
