//! `generate`, `upgrade` and `fix`: one headless Claude Code session with OpenTelemetry
//! cost tracking, followed by telemetry parsing.

use super::{composed_prompt, detect_backend, level_prompt, telemetry, Backend, Ports, Rules, RunOptions};
use crate::paths;
use crate::process::{Claude, Tools};
use anyhow::{bail, Context, Result};
use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub enum Mode {
    Generate {
        backend: Backend,
    },
    Upgrade {
        app: PathBuf,
        composed_prompt: bool,
        resume_session: bool,
    },
    Fix {
        app: PathBuf,
    },
}

impl Mode {
    fn label(&self) -> &'static str {
        match self {
            Mode::Generate { .. } => "generate",
            Mode::Upgrade { .. } => "upgrade",
            Mode::Fix { .. } => "fix",
        }
    }
}

const POSTGRES_CONTAINER: &str = "llm-sequential-upgrade-postgres-1";
const MONGO_CONTAINER: &str = "llm-sequential-upgrade-mongodb-1";
/// The shared OpenTelemetry log is rotated once it grows past this.
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

/// A run's metadata.json. `parse-telemetry` reads it back to find the run's records.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub level: u32,
    pub backend: String,
    pub timestamp: String,
    pub started_at: String,
    pub started_at_utc: String,
    pub run_id: String,
    pub app_dir: String,
    pub prompt_file: String,
    pub phase: String,
    pub variant: String,
    pub rules: String,
    pub model: String,
    pub run_index: u32,
    pub vite_port: u32,
    pub express_port: u32,
    pub pg_database: String,
    pub mongo_database: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Database names and URLs for one run index. Run 0 uses the default databases; run N
/// gets its own so parallel runs do not share data.
struct Databases {
    pg_name: String,
    pg_url: String,
    mongo_name: String,
    mongo_url: String,
}

impl Databases {
    fn new(run_index: u32) -> Databases {
        let (pg_name, mongo_name) = if run_index == 0 {
            ("spacetime".to_owned(), "chat-app".to_owned())
        } else {
            (format!("spacetime_run{run_index}"), format!("chat-app_run{run_index}"))
        };
        Databases {
            pg_url: format!("postgresql://spacetime:spacetime@localhost:6432/{pg_name}"),
            mongo_url: format!("mongodb://localhost:6437/{mongo_name}"),
            pg_name,
            mongo_name,
        }
    }
}

pub fn run(mode: Mode, level: u32, options: &RunOptions) -> Result<()> {
    let tools = Tools::new();
    let tool_dir = paths::sequential_dir();
    let prompts_dir = paths::chat_app_prompts_dir();

    let backend = match &mode {
        Mode::Generate { backend } => *backend,
        Mode::Upgrade { app, .. } | Mode::Fix { app } => detect_backend(app).with_context(|| {
            format!(
                "cannot tell the backend of {}; it has no .benchmark-backend marker",
                app.display()
            )
        })?,
    };
    if let Mode::Fix { app } = &mode
        && !app.join("BUG_REPORT.md").is_file()
    {
        bail!(
            "no BUG_REPORT.md in {}; grade the app first to produce a bug report",
            app.display()
        );
    }
    let ports = Ports::new(backend, options.run_index);
    let dbs = Databases::new(options.run_index);

    let claude = tools.claude()?;
    println!("Using Claude CLI: {}", claude.describe());
    preflight(&tools, backend, options.run_index, &dbs, &tool_dir)?;

    let prompt_path = composed_prompt(level).with_context(|| format!("no composed prompt for level {level}"))?;
    let prompt_name = file_name(&prompt_path);
    println!("[OK] Prompt file: {prompt_name}");
    // UI contracts exist only for automated UI assertions; grading here is in the browser.
    let level_spec = strip_ui_contracts(&read(&prompt_path)?);
    println!("[OK] UI contracts stripped\n");

    let now = Local::now();
    let timestamp = now.format("%Y%m%d-%H%M%S").to_string();
    let date = now.format("%Y%m%d").to_string();
    let variant_dir = tool_dir.join(&options.variant);

    // Generate starts (or joins) today's run; upgrade and fix stay in the app's run.
    let (run_base, app, run_id) = match &mode {
        Mode::Generate { .. } => {
            let run_base = next_run_base(&variant_dir, &options.variant, &date, backend);
            let app = run_base
                .join(backend.to_string())
                .join("results")
                .join(format!("chat-app-{timestamp}"));
            fs::create_dir_all(&app)?;
            fs::write(app.join(super::BACKEND_MARKER), format!("{backend}\n"))?;
            (run_base, app, format!("{backend}-level{level}-{timestamp}"))
        }
        Mode::Upgrade { app, .. } | Mode::Fix { app } => {
            // <run>/<backend>/results/chat-app-*
            let run_base = app
                .ancestors()
                .nth(3)
                .filter(|base| base.join(backend.to_string()).is_dir())
                .map_or_else(
                    || variant_dir.join(format!("{}-{date}", options.variant)),
                    Path::to_path_buf,
                );
            let kind = if matches!(mode, Mode::Upgrade { .. }) {
                "upgrade-to-level"
            } else {
                "fix-level"
            };
            (run_base, app.clone(), format!("{backend}-{kind}{level}-{timestamp}"))
        }
    };
    let backend_dir = run_base.join(backend.to_string());
    let telemetry_dir = backend_dir.join("telemetry").join(&run_id);
    fs::create_dir_all(&telemetry_dir)?;

    let label = mode.label();
    println!("=== Sequential Upgrade: {} ===", capitalize(label));
    println!("  Variant:   {}", options.variant);
    println!("  Rules:     {}", options.rules);
    println!("  Model:     {}", options.model);
    println!("  Level:     {level}");
    println!("  Backend:   {backend}");
    println!("  Run index: {} (Vite={})", options.run_index, ports.vite);
    println!("  Run ID:    {run_id}");
    println!("  Run base:  {}", run_base.display());
    println!("  App dir:   {}", app.display());
    println!("  Telemetry: {}\n", telemetry_dir.display());

    let session_id = uuid::Uuid::new_v4().to_string();
    let metadata_path = telemetry_dir.join("metadata.json");
    let mut metadata = Metadata {
        level,
        backend: backend.to_string(),
        timestamp: timestamp.clone(),
        started_at: now.format("%Y-%m-%dT%H:%M:%S%z").to_string(),
        started_at_utc: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        run_id: run_id.clone(),
        app_dir: app.display().to_string(),
        prompt_file: prompt_name.clone(),
        phase: label.to_owned(),
        variant: options.variant.clone(),
        rules: options.rules.to_string(),
        model: options.model.clone(),
        run_index: options.run_index,
        vite_port: ports.vite,
        express_port: ports.express,
        pg_database: dbs.pg_name.clone(),
        mongo_database: dbs.mongo_name.clone(),
        session_id: session_id.clone(),
        ..Default::default()
    };
    write_json(&metadata_path, &metadata)?;
    snapshot_inputs(&tools, &backend_dir.join("inputs"), backend, &tool_dir, &prompts_dir)?;
    // Not published; kept for local tooling that wants the app from a telemetry directory.
    fs::write(telemetry_dir.join("app-dir.txt"), format!("{}\n", app.display()))?;

    let language =
        fs::read_to_string(prompts_dir.join(format!("language/typescript-{backend}.md"))).unwrap_or_default();
    let mut resume_from = None;
    let prompt = match &mode {
        Mode::Fix { .. } => {
            println!("=== Sequential Upgrade: Fix Iteration ===");
            println!("  App dir: {}", app.display());
            println!("  Bug report: {}\n", app.join("BUG_REPORT.md").display());
            fix_prompt(&app, backend, &ports)
        }
        Mode::Upgrade {
            composed_prompt: use_composed,
            resume_session,
            ..
        } => {
            let prev = level - 1;
            snapshot_level(&app, prev, "Snapshotting current app state")?;
            println!("=== Sequential Upgrade: Upgrade to Level {level} ===");
            println!("  App dir: {}", app.display());
            println!("  Backend: {backend}");
            println!("  From level: {prev} → {level}");
            println!("  Prompt: {prompt_name}\n");
            // By default only the new feature is sent; --composed-prompt sends the whole
            // cumulative spec, which is how the original level 1 to 11 runs were prompted.
            let feature = if *use_composed {
                println!("  Using composed (cumulative) feature file: {prompt_name}");
                level_spec.clone()
            } else if let Some(file) = level_prompt(&prompts_dir.join("features"), level) {
                println!("  Using incremental feature file: {}", file_name(&file));
                read(&file)?
            } else {
                println!("  WARNING: No incremental feature file for level {level}, falling back to composed prompt");
                level_spec.clone()
            };
            if *resume_session {
                resume_from = previous_session(&variant_dir, &app, &session_id);
                match &resume_from {
                    Some(id) => println!("Forking prior session: {id}"),
                    None => println!("No prior session ID found for this app — starting fresh"),
                }
            }
            upgrade_prompt(&app, backend, level, &ports, &language, &feature)
        }
        Mode::Generate { .. } => {
            let claude_md = assemble_claude_md(backend, options.rules, &tool_dir, &ports, &dbs)?;
            let claude_md = format!("<!-- run-id: {run_id} -->\n\n{claude_md}");
            // The unique first line also defeats Anthropic's prompt cache, so every run starts cold.
            fs::write(app.join("CLAUDE.md"), patch_ports(&claude_md, options.run_index, &dbs))?;
            if options.run_index > 0 {
                let db = if backend == Backend::Mongodb {
                    &dbs.mongo_name
                } else {
                    &dbs.pg_name
                };
                println!(
                    "  Patched for run-index={} (Vite={}, Express={}, DB={db})",
                    options.run_index, ports.vite, ports.express
                );
            }
            generate_prompt(&app, backend, level, &run_id, &language, &level_spec)
        }
    };

    println!("Starting Claude Code session ({label})...");
    println!("─────────────────────────────────────────────");
    let status = claude_session(
        &tools,
        &claude,
        &app,
        &tool_dir,
        &prompts_dir,
        options,
        &session_id,
        &run_id,
        resume_from.as_deref(),
        &prompt,
    )?;
    println!("\n─────────────────────────────────────────────");

    let end = Local::now();
    metadata.ended_at = Some(end.format("%Y-%m-%dT%H:%M:%S%z").to_string());
    metadata.ended_at_utc = Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    metadata.exit_code = Some(status.code().unwrap_or(-1));
    metadata.mode = Some(label.to_owned());
    write_json(&metadata_path, &metadata)?;
    save_transcript(&session_id, &telemetry_dir);

    if matches!(mode, Mode::Upgrade { .. }) && status.success() {
        snapshot_level(&app, level, "Snapshotting upgraded app state")?;
    }

    println!("\n=== {label} Complete ===");
    println!("  Started: {}", metadata.started_at);
    println!("  Ended:   {}\n", metadata.ended_at.as_deref().unwrap_or_default());

    let logs = tool_dir.join("telemetry/logs.jsonl");
    println!("Parsing telemetry...");
    match telemetry::run(&telemetry_dir, Some(&logs), None, true) {
        Ok(_) => {
            println!("\n=== Results ===");
            println!("  App:        {}", app.display());
            println!("  Cost:       {}\n", telemetry_dir.join("COST_REPORT.md").display());
            print_next_step(&mode, &app, level);
        }
        Err(err) => println!(
            "WARNING: Telemetry parsing failed ({err:#}). Raw logs at: {}",
            logs.display()
        ),
    }

    crate::process::check(status, "the Claude Code session")
}

fn preflight(tools: &Tools, backend: Backend, run_index: u32, dbs: &Databases, tool_dir: &Path) -> Result<()> {
    println!("\n=== Pre-flight Checks ===");
    match backend {
        Backend::Spacetime => {
            if !tools.succeeds("spacetime", &["server", "ping", "local"]) {
                bail!("[FAIL] SpacetimeDB is not running. Start it with: spacetime start");
            }
            println!("[OK] SpacetimeDB is running");
        }
        Backend::Postgres => {
            let psql = |sql: &str| -> Result<String> {
                Tools::output(tools.command("docker").args([
                    "exec",
                    POSTGRES_CONTAINER,
                    "psql",
                    "-U",
                    "spacetime",
                    "-d",
                    "spacetime",
                    "-c",
                    sql,
                ]))
            };
            if !tools.succeeds(
                "docker",
                &[
                    "exec",
                    POSTGRES_CONTAINER,
                    "psql",
                    "-U",
                    "spacetime",
                    "-d",
                    "spacetime",
                    "-c",
                    "SELECT 1",
                ],
            ) {
                bail!("[FAIL] PostgreSQL is not reachable. Check Docker container {POSTGRES_CONTAINER}.");
            }
            println!("[OK] PostgreSQL container is running");
            if run_index > 0 {
                let name = &dbs.pg_name;
                let exists = psql(&format!("SELECT 1 FROM pg_database WHERE datname = '{name}'"))?;
                if !exists.contains('1') {
                    psql(&format!("CREATE DATABASE {name} OWNER spacetime;"))?;
                }
                println!("[OK] PostgreSQL database: {name} (run-index {run_index})");
            } else {
                println!("[OK] PostgreSQL database: {} (default)", dbs.pg_name);
            }
        }
        Backend::Mongodb => {
            if !tools.succeeds(
                "docker",
                &[
                    "exec",
                    MONGO_CONTAINER,
                    "mongosh",
                    "--quiet",
                    "--eval",
                    "db.runCommand({ping:1})",
                ],
            ) {
                bail!("[FAIL] MongoDB is not reachable. Check Docker container {MONGO_CONTAINER}.");
            }
            println!("[OK] MongoDB container is running");
            // MongoDB creates databases on first write, so there is nothing to create.
            let which = if run_index > 0 {
                format!("run-index {run_index}")
            } else {
                "default".into()
            };
            println!("[OK] MongoDB database: {} ({which})", dbs.mongo_name);
        }
    }
    if !tools.succeeds("docker", &["info"]) {
        bail!("[FAIL] Docker is not running.");
    }

    let shared = tool_dir.join("telemetry");
    fs::create_dir_all(&shared)?;
    let logs = shared.join("logs.jsonl");
    if let Ok(meta) = fs::metadata(&logs)
        && meta.len() > MAX_LOG_BYTES
    {
        let archive = shared.join(format!("logs-{}.jsonl.bak", Local::now().format("%Y%m%d-%H%M%S")));
        fs::rename(&logs, &archive)?;
        println!(
            "[INFO] Rotated logs.jsonl ({} bytes) to {}",
            meta.len(),
            file_name(&archive)
        );
    }

    let compose = tool_dir.join("docker-compose.otel.yaml");
    let running = Tools::output(
        tools
            .command("docker")
            .args(["compose", "-f"])
            .arg(&compose)
            .args(["ps", "--status", "running"]),
    )
    .unwrap_or_default();
    if running.contains("otel-collector") {
        println!("[OK] OTel Collector is running");
    } else {
        println!("[...] Starting OTel Collector...");
        let status = tools
            .command("docker")
            .args(["compose", "-f"])
            .arg(&compose)
            .args(["up", "-d"])
            .status()?;
        crate::process::check(status, "starting the OTel Collector")?;
        println!("[OK] OTel Collector started");
    }

    // The generated apps are Node projects.
    if !tools.has("node") {
        bail!("[FAIL] Node.js not found.");
    }
    let version = Tools::output(tools.command("node").arg("--version"))?;
    println!("[OK] Node.js {}", version.trim());
    Ok(())
}

/// Today's run directory, or `-2`, `-3`... if this backend already completed a run today.
/// A backend directory without a cost summary is an abandoned attempt and is reused.
fn next_run_base(variant_dir: &Path, variant: &str, date: &str, backend: Backend) -> PathBuf {
    let completed = |base: &Path| {
        super::subdirs(&base.join(backend.to_string()).join("telemetry"))
            .iter()
            .any(|t| t.join("cost-summary.json").is_file())
    };
    let first = variant_dir.join(format!("{variant}-{date}"));
    if !completed(&first) {
        return first;
    }
    (2..)
        .map(|seq| variant_dir.join(format!("{variant}-{date}-{seq}")))
        .find(|base| !completed(base))
        .expect("an unused run directory")
}

/// Copies the prompts and specs a run uses next to its output, so the run can be
/// reproduced after the tooling changes. Upgrades and fixes reuse the first snapshot.
fn snapshot_inputs(tools: &Tools, inputs: &Path, backend: Backend, tool_dir: &Path, prompts_dir: &Path) -> Result<()> {
    if inputs.is_dir() {
        return Ok(());
    }
    let copy = |from: PathBuf, to: PathBuf| {
        if from.is_file() {
            let _ = fs::create_dir_all(to.parent().unwrap());
            let _ = fs::copy(&from, &to);
        }
    };
    let copy_md = |from: PathBuf, to: PathBuf| {
        for file in fs::read_dir(&from).into_iter().flatten().flatten() {
            if file.path().extension().is_some_and(|e| e == "md") {
                copy(file.path(), to.join(file.file_name()));
            }
        }
    };
    fs::create_dir_all(inputs)?;
    for name in [
        "CLAUDE.md",
        "DEVELOP.md",
        "docker-compose.otel.yaml",
        "otel-collector-config.yaml",
        ".gitignore",
    ] {
        copy(tool_dir.join(name), inputs.join(name));
    }
    let commit =
        Tools::output(tools.command("git").current_dir(tool_dir).args(["rev-parse", "HEAD"])).unwrap_or_default();
    let dirty = !Tools::output(
        tools
            .command("git")
            .current_dir(tool_dir)
            .args(["status", "--porcelain", "--", "."]),
    )
    .unwrap_or_default()
    .trim()
    .is_empty();
    fs::write(
        inputs.join("tooling-commit.txt"),
        format!(
            "{}{}\n",
            commit.trim(),
            if dirty { " (with uncommitted changes)" } else { "" }
        ),
    )?;

    let backends = inputs.join("backends");
    copy(
        tool_dir.join(format!("backends/{backend}.md")),
        backends.join(format!("{backend}.md")),
    );
    if backend == Backend::Spacetime {
        let skills = paths::skills_dir();
        copy(
            skills.join("typescript-server/SKILL.md"),
            backends.join("typescript-server-SKILL.md"),
        );
        copy(
            skills.join("typescript-client/SKILL.md"),
            backends.join("typescript-client-SKILL.md"),
        );
        copy(
            tool_dir.join("backends/spacetime-templates.md"),
            backends.join("spacetime-templates.md"),
        );
    }
    copy_md(tool_dir.join("test-plans"), inputs.join("test-plans"));
    copy_md(prompts_dir.join("composed"), inputs.join("prompts/composed"));
    let language = format!("typescript-{backend}.md");
    copy(
        prompts_dir.join("language").join(&language),
        inputs.join("prompts/language").join(&language),
    );
    println!("  Inputs snapshotted to {}", inputs.display());
    Ok(())
}

/// The CLAUDE.md that steers the session, by how much guidance `--rules` asks for.
fn assemble_claude_md(
    backend: Backend,
    rules: Rules,
    tool_dir: &Path,
    ports: &Ports,
    dbs: &Databases,
) -> Result<String> {
    let skill = |name: &str| -> Result<String> {
        Ok(strip_front_matter(&read(
            &paths::skills_dir().join(name).join("SKILL.md"),
        )?))
    };
    let (vite, express) = (ports.vite, ports.express);
    let text = match (rules, backend) {
        (Rules::Minimal, Backend::Spacetime) => format!(
            "Build this app using the SpacetimeDB TypeScript SDK (npm package: spacetimedb).\n\
             Server module in backend/spacetimedb/, React client in client/.\n\
             Vite dev server port: {vite}\n"
        ),
        (Rules::Minimal, Backend::Mongodb) => format!(
            "Build this app using MongoDB + Express + Socket.io + Mongoose.\n\
             Express server in server/, React client in client/.\n\
             MongoDB connection: {}\n\
             Express port: {express} | Vite port: {vite}\n",
            dbs.mongo_url
        ),
        (Rules::Minimal, Backend::Postgres) => format!(
            "Build this app using PostgreSQL + Express + Socket.io + Drizzle ORM.\n\
             Express server in server/, React client in client/.\n\
             PostgreSQL connection: {}\n\
             Express port: {express} | Vite port: {vite}\n",
            dbs.pg_url
        ),
        (Rules::Standard, Backend::Spacetime) => {
            format!(
                "{}\n---\n\n{}",
                skill("typescript-server")?,
                skill("typescript-client")?
            )
        }
        (Rules::Standard, Backend::Mongodb) => format!(
            "# MongoDB Backend\n\nMongoDB connection: `{}`\n\n\
             Use Express (port {express}) + Socket.io + Mongoose. Server in `server/`, client in `client/`.\n\
             Vite dev server port: {vite}\n",
            dbs.mongo_url
        ),
        (Rules::Standard, Backend::Postgres) => format!(
            "# PostgreSQL Backend\n\nPostgreSQL connection: `{}`\n\n\
             Use Express (port {express}) + Socket.io + Drizzle ORM. Server in `server/`, client in `client/`.\n\
             Vite dev server port: {vite}\n",
            dbs.pg_url
        ),
        (Rules::Guided, Backend::Spacetime) => format!(
            "{}\n---\n\n{}\n---\n\n{}\n---\n\n{}",
            read(&tool_dir.join("backends/spacetime.md"))?,
            skill("typescript-server")?,
            skill("typescript-client")?,
            read(&tool_dir.join("backends/spacetime-templates.md"))?
        ),
        (Rules::Guided, _) => read(&tool_dir.join(format!("backends/{backend}.md")))?,
    };
    println!("Assembled {rules} CLAUDE.md for {backend}");
    Ok(text)
}

/// Rewrites the run-0 ports and database names in a CLAUDE.md for another run index.
fn patch_ports(text: &str, run_index: u32, dbs: &Databases) -> String {
    if run_index == 0 {
        return text.to_owned();
    }
    let express = 6001 + run_index;
    [
        ("6173".to_owned(), (6173 + run_index).to_string()),
        ("6273".to_owned(), (6273 + run_index).to_string()),
        ("6373".to_owned(), (6373 + run_index).to_string()),
        // Every 6001 in the specs is the Express port, including `PORT=6001` and
        // `npx kill-port 6001`, which would otherwise stop another run's server.
        ("6001".to_owned(), express.to_string()),
        (
            "localhost:6432/spacetime".to_owned(),
            format!("localhost:6432/{}", dbs.pg_name),
        ),
        (
            "localhost:6437/chat-app".to_owned(),
            format!("localhost:6437/{}", dbs.mongo_name),
        ),
    ]
    .iter()
    .fold(text.to_owned(), |text, (from, to)| text.replace(from, to))
}

fn generate_prompt(app: &Path, backend: Backend, level: u32, run_id: &str, language: &str, spec: &str) -> String {
    let (language, spec) = (inline(language), inline(spec));
    format!(
        "Run the sequential upgrade benchmark — GENERATE AND DEPLOY ONLY.

**Configuration:**
- Level: {level}
- Backend: {backend}
- App output directory: {app} (this is also your working directory)
- Run ID: {run_id}

**Instructions:**
1. Read the CLAUDE.md in this directory — it has backend-specific setup, architecture, and SDK reference
2. Follow the phases in CLAUDE.md to generate, build, and deploy the app
3. Write all code in the current directory

If the build fails, fix and retry (up to 3 times per phase).
Write an ITERATION_LOG.md tracking any build reprompts.

Do NOT do browser testing — that happens in a separate grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: DEPLOY_COMPLETE

---

{language}

---

{spec}",
        app = app.display(),
    )
}

fn upgrade_prompt(app: &Path, backend: Backend, level: u32, ports: &Ports, language: &str, feature: &str) -> String {
    let (language, feature) = (inline(language), inline(feature));
    let prev = level - 1;
    format!(
        "Upgrade the existing chat app to add the new feature(s) from level {level}.

**App directory:** {app}
**Backend:** {backend}
**Current level:** {prev} (all features from level {prev} are already implemented and working)
**Target level:** {level}

**Instructions:**
1. Read the CLAUDE.md in this directory for backend-specific architecture and SDK reference
2. Read the existing source code to understand the current architecture
3. Add the new feature(s) to both backend and frontend, integrating with the existing code
4. Rebuild and redeploy (see CLAUDE.md for backend-specific steps)
5. Verify the build succeeds: npx tsc --noEmit && npm run build (if applicable)
6. Make sure the dev server is running on port {vite}

Features from level {prev} and below are ALREADY IMPLEMENTED — do NOT rewrite them.
Only add the NEW feature(s) that appear in the feature spec below but not in level {prev}.

Do NOT do browser testing — that happens in a separate grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: UPGRADE_COMPLETE

---

{language}

---

{feature}",
        app = app.display(),
        vite = ports.vite,
    )
}

fn fix_prompt(app: &Path, backend: Backend, ports: &Ports) -> String {
    format!(
        "Fix the bugs in the sequential upgrade app.

**App directory:** {app}
**Backend:** {backend}

**Instructions:**
1. Read the CLAUDE.md in this directory for backend-specific architecture and deploy instructions
2. Read BUG_REPORT.md in the app directory — it describes what's broken
3. Read the relevant source code files mentioned in the bug report
4. Fix each bug described in the report
5. Rebuild and redeploy ALL servers:
   - For PostgreSQL: restart the Express server (npm run dev in server/) AND the Vite client
   - For SpacetimeDB: run spacetime publish, then restart the Vite client
6. Verify the fix by testing the endpoint/behavior described in the bug report
7. Make sure ALL servers are running:
   - Client dev server on port {vite}
   - For PostgreSQL: Express API server on port {express} (test with curl)
8. Append this fix iteration to ITERATION_LOG.md in the app directory

CRITICAL: After fixing code, you MUST verify the servers are running and the bug is fixed.
Do NOT just edit files and say \"done\" — actually restart the servers and test.

Do NOT do browser testing — that happens in the grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: FIX_COMPLETE",
        app = app.display(),
        vite = ports.vite,
        express = ports.express,
    )
}

/// Runs headless Claude Code in the app directory, so it picks up the app's CLAUDE.md
/// rather than the tool's.
#[allow(clippy::too_many_arguments)]
fn claude_session(
    tools: &Tools,
    claude: &Claude,
    app: &Path,
    tool_dir: &Path,
    prompts_dir: &Path,
    options: &RunOptions,
    session_id: &str,
    run_id: &str,
    resume_from: Option<&str>,
    prompt: &str,
) -> Result<std::process::ExitStatus> {
    let mut cmd = claude.command(tools);
    cmd.current_dir(app)
        .args([
            "--print",
            "--verbose",
            "--output-format",
            "text",
            "--dangerously-skip-permissions",
        ])
        .arg("--add-dir")
        .arg(app)
        .arg("--add-dir")
        .arg(tool_dir)
        .arg("--add-dir")
        .arg(prompts_dir)
        .args(["--session-id", session_id, "--model", &options.model]);
    if let Some(previous) = resume_from {
        cmd.args(["--resume", previous, "--fork-session"]);
    }
    cmd.args(["-p", prompt])
        // A Claude Desktop agent session sets these, and they suppress telemetry.
        .env_remove("CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        // The 5-minute cache tier and a frozen CLI version keep costs comparable across runs.
        .env("FORCE_PROMPT_CACHING_5M", "1")
        .env("DISABLE_AUTOUPDATER", "1")
        .env("CLAUDE_CODE_ENABLE_TELEMETRY", "1")
        .env("OTEL_LOGS_EXPORTER", "otlp")
        .env("OTEL_METRICS_EXPORTER", "otlp")
        .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317")
        .env("OTEL_LOGS_EXPORT_INTERVAL", "1000")
        .env("OTEL_METRIC_EXPORT_INTERVAL", "5000")
        // Tags every record, so parallel runs on one collector can be told apart.
        .env(
            "OTEL_RESOURCE_ATTRIBUTES",
            format!("run.id={run_id},session.id={session_id}"),
        )
        .stdin(Stdio::null());
    cmd.status().context("failed to start Claude Code")
}

/// The session of the app's most recent earlier run, for `--resume-session`. The current
/// run's metadata is already written, so its own session is skipped.
fn previous_session(variant_dir: &Path, app: &Path, current_session: &str) -> Option<String> {
    let mut best: Option<(String, String)> = None;
    for run in super::subdirs(variant_dir) {
        for backend in super::subdirs(&run) {
            for dir in super::subdirs(&backend.join("telemetry")) {
                let Ok(text) = fs::read_to_string(dir.join("metadata.json")) else {
                    continue;
                };
                let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                let same_app = meta["appDir"]
                    .as_str()
                    .is_some_and(|d| Path::new(d) == app || fs::canonicalize(d).is_ok_and(|d| d == app));
                let session = meta["sessionId"].as_str().unwrap_or_default();
                if !same_app || session.is_empty() || session == current_session {
                    continue;
                }
                let started = meta["startedAtUtc"]
                    .as_str()
                    .or(meta["timestamp"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                if best.as_ref().is_none_or(|(s, _)| started > *s) {
                    best = Some((started, session.to_owned()));
                }
            }
        }
    }
    best.map(|(_, session)| session)
}

/// Saves the app as `level-<level>/`, without dependencies, build output or other snapshots.
fn snapshot_level(app: &Path, level: u32, what: &str) -> Result<()> {
    let dest = app.join(format!("level-{level}"));
    if dest.is_dir() {
        println!("Snapshot level-{level} already exists — skipping snapshot");
        return Ok(());
    }
    println!("{what} to level-{level}...");
    fs::create_dir_all(&dest)?;
    for entry in fs::read_dir(app)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with("level-") || name == "dev-server.log" {
            continue;
        }
        let skip = |n: &str| matches!(n, "node_modules" | "dist" | ".vite" | "drizzle");
        if skip(&name) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            crate::results_repo::copy_tree(&entry.path(), &dest.join(&name), &skip)?;
        } else {
            fs::copy(entry.path(), dest.join(&name))?;
        }
    }
    println!("  Saved to {}", dest.display());
    Ok(())
}

/// Copies Claude Code's transcript of the session next to the telemetry.
fn save_transcript(session_id: &str, telemetry_dir: &Path) {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return;
    };
    let projects = PathBuf::from(home).join(".claude/projects");
    let name = format!("{session_id}.jsonl");
    let found = super::subdirs(&projects)
        .into_iter()
        .map(|p| p.join(&name))
        .find(|p| p.is_file());
    match found {
        Some(src) if fs::copy(&src, telemetry_dir.join("session-transcript.jsonl")).is_ok() => {
            println!(
                "Saved session transcript -> {}",
                telemetry_dir.join("session-transcript.jsonl").display()
            );
        }
        _ => println!("NOTE: session transcript for {session_id} not found under ~/.claude/projects"),
    }
}

fn print_next_step(mode: &Mode, app: &Path, level: u32) {
    let app = app.display();
    match mode {
        Mode::Fix { .. } => {
            println!("=== Next Step: Re-grade the app ===");
            println!("  cargo llm-apps sequential grade {app}\n");
        }
        Mode::Upgrade { .. } => {
            println!("=== Next Step: Grade the upgraded app (level {level}) ===");
            println!("  cargo llm-apps sequential grade {app}\n");
            if composed_prompt(level + 1).is_some() {
                println!("  To continue upgrading after grading:");
                println!("    cargo llm-apps sequential upgrade {app} --level {}\n", level + 1);
            }
        }
        Mode::Generate { .. } => {
            println!("=== Next Step: Grade the app ===");
            println!("  cargo llm-apps sequential grade {app}\n");
        }
    }
}

/// Removes the UI contract blocks (each from its `**UI contract:**` line through the next
/// blank line) and the paragraph that introduces them.
pub fn strip_ui_contracts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_contract = false;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if in_contract {
            in_contract = !bare.is_empty();
            continue;
        }
        if bare.starts_with("**UI contract:**") {
            in_contract = true;
            continue;
        }
        if bare.starts_with("**Important:** Each feature below includes") {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Drops a leading `---`-delimited front matter block, as agent skills have.
pub fn strip_front_matter(text: &str) -> String {
    let mut lines = text.lines();
    let mut out = String::new();
    let mut rest: Box<dyn Iterator<Item = &str>> = Box::new(text.lines());
    if lines.next() == Some("---") {
        rest = Box::new(lines.skip_while(|l| *l != "---").skip(1));
    }
    for line in rest {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// A file's text as the prompts inline it: without trailing newlines, so the prompt
/// ends with the spec's last line.
fn inline(text: &str) -> &str {
    text.trim_end_matches(['\n', '\r'])
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)?).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_contracts_are_stripped() {
        let prompt = "# Level 1\n\
            **Important:** Each feature below includes a \"UI contract\" section.\n\
            \n\
            ### Feature 1\nDo the thing.\n\
            **UI contract:**\n- `data-testid=send`\n- `data-testid=input`\n\
            \n\
            ### Feature 2\nAnother thing.\n";
        assert_eq!(
            strip_ui_contracts(prompt),
            "# Level 1\n\n### Feature 1\nDo the thing.\n### Feature 2\nAnother thing.\n"
        );
    }

    #[test]
    fn front_matter_is_stripped_only_at_the_top() {
        assert_eq!(strip_front_matter("---\nname: x\n---\n# Body\n"), "# Body\n");
        assert_eq!(strip_front_matter("# Body\n---\nmore\n"), "# Body\n---\nmore\n");
    }

    #[test]
    fn ports_and_databases_move_with_the_run_index() {
        let dbs = Databases::new(2);
        let text = "vite 6273, express localhost:6001, PORT=6001, npx kill-port 6001, \
                    db postgresql://spacetime:spacetime@localhost:6432/spacetime";
        assert_eq!(
            patch_ports(text, 2, &dbs),
            "vite 6275, express localhost:6003, PORT=6003, npx kill-port 6003, \
             db postgresql://spacetime:spacetime@localhost:6432/spacetime_run2"
        );
        assert_eq!(patch_ports(text, 0, &Databases::new(0)), text);
    }

    #[test]
    fn a_completed_backend_starts_a_numbered_run() {
        let dir = tempfile::tempdir().unwrap();
        let variant = dir.path();
        let base = |s: &str| variant.join(s);
        assert_eq!(
            next_run_base(variant, "su", "20260101", Backend::Spacetime),
            base("su-20260101")
        );
        // An abandoned attempt (no cost summary) is reused.
        fs::create_dir_all(base("su-20260101/spacetime/telemetry/a")).unwrap();
        assert_eq!(
            next_run_base(variant, "su", "20260101", Backend::Spacetime),
            base("su-20260101")
        );
        fs::write(base("su-20260101/spacetime/telemetry/a/cost-summary.json"), "{}").unwrap();
        assert_eq!(
            next_run_base(variant, "su", "20260101", Backend::Spacetime),
            base("su-20260101-2")
        );
        // Another backend still joins the shared run.
        assert_eq!(
            next_run_base(variant, "su", "20260101", Backend::Postgres),
            base("su-20260101")
        );
    }

    #[test]
    fn level_snapshot_leaves_out_dependencies_and_other_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path();
        for f in [
            "client/src/App.tsx",
            "client/node_modules/x/index.js",
            "client/dist/a.js",
            "server/drizzle/0001.sql",
            "level-1/client/src/App.tsx",
            ".benchmark-backend",
            "dev-server.log",
            "GRADING_RESULTS.md",
        ] {
            let p = app.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        snapshot_level(app, 2, "test").unwrap();
        let snap = app.join("level-2");
        assert!(snap.join("client/src/App.tsx").is_file());
        assert!(snap.join("GRADING_RESULTS.md").is_file());
        for missing in [
            "client/node_modules",
            "client/dist",
            "server/drizzle",
            "level-1",
            ".benchmark-backend",
            "dev-server.log",
        ] {
            assert!(!snap.join(missing).exists(), "{missing} should not be in the snapshot");
        }
    }

    #[test]
    fn previous_session_is_the_newest_for_this_app() {
        let dir = tempfile::tempdir().unwrap();
        let variant = dir.path();
        let app = variant.join("su-1/spacetime/results/chat-app-1");
        fs::create_dir_all(&app).unwrap();
        let write = |id: &str, app: &Path, started: &str, session: &str| {
            let t = variant.join("su-1/spacetime/telemetry").join(id);
            fs::create_dir_all(&t).unwrap();
            let meta = serde_json::json!({"appDir": app, "startedAtUtc": started, "sessionId": session});
            fs::write(t.join("metadata.json"), meta.to_string()).unwrap();
        };
        write("spacetime-level1", &app, "2026-01-01T10:00:00Z", "first");
        write("spacetime-upgrade-to-level2", &app, "2026-01-01T12:00:00Z", "second");
        write("spacetime-fix-level1", &app, "2026-01-01T11:00:00Z", "middle");
        write("other", &variant.join("elsewhere"), "2026-01-02T00:00:00Z", "other-app");
        // The run asking is the newest of all, and must not resume itself.
        write("spacetime-upgrade-to-level3", &app, "2026-01-01T13:00:00Z", "current");
        assert_eq!(previous_session(variant, &app, "current").as_deref(), Some("second"));
    }
}
