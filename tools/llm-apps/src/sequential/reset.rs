//! `reset`: gives an app an empty database before grading, so no users, rooms or
//! messages are left over. SpacetimeDB apps get a freshly published module; Postgres
//! and MongoDB apps get their database emptied.

use super::{detect_backend, Backend};
use crate::process::{tail, Tools};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;

const POSTGRES_CONTAINER: &str = "llm-sequential-upgrade-postgres-1";
const MONGO_CONTAINER: &str = "llm-sequential-upgrade-mongodb-1";

pub fn run(app: &Path) -> Result<()> {
    let tools = Tools::new();
    let Some(backend) = detect_backend(app) else {
        bail!("cannot detect the backend of {}", app.display());
    };
    match backend {
        Backend::Spacetime => {
            println!("Resetting SpacetimeDB module...");
            let module = format!("chat-app-test-{}", chrono::Utc::now().timestamp());
            println!("  Publishing module: {module}");
            let out = Tools::output(
                tools
                    .command("spacetime")
                    .args(["publish", "-p"])
                    .arg(app.join("backend/spacetimedb"))
                    .args(["-s", "local", &module]),
            )?;
            println!("{}", tail(&out, 3));

            // Point the client at the new module.
            let config = app.join("client/src/config.ts");
            match fs::read_to_string(&config) {
                Ok(text) => {
                    let re = Regex::new(r"MODULE_NAME = '.*'").unwrap();
                    fs::write(
                        &config,
                        re.replace_all(&text, format!("MODULE_NAME = '{module}'").as_str())
                            .as_ref(),
                    )?;
                    println!("  Updated config.ts: MODULE_NAME = '{module}'");
                }
                Err(_) => println!("  WARNING: config.ts not found at {}", config.display()),
            }
            println!("  Module reset complete. Vite will hot-reload.");
        }
        Backend::Postgres => {
            println!("Resetting PostgreSQL database...");
            let db = database_name(app).unwrap_or_else(|| "spacetime".into());
            println!("  Dropping all tables in {db}...");
            let drop_all = "DO $$ DECLARE r RECORD; BEGIN \
                FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP \
                EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE'; \
                END LOOP; END $$;";
            let out = Tools::output(tools.command("docker").args([
                "exec",
                POSTGRES_CONTAINER,
                "psql",
                "-U",
                "spacetime",
                "-d",
                &db,
                "-c",
                drop_all,
            ]))?;
            println!("{}", tail(&out, 1));

            println!("  Pushing Drizzle schema...");
            let out = Tools::output(
                tools
                    .command("npx")
                    .current_dir(app.join("server"))
                    .args(["drizzle-kit", "push"]),
            )
            .context("failed to run drizzle-kit")?;
            println!("{}", tail(&out, 3));
            println!("  Database reset complete.");
        }
        Backend::Mongodb => {
            println!("Resetting MongoDB database...");
            let db = database_name(app).unwrap_or_else(|| "chat-app".into());
            // Mongoose recreates collections and indexes on next use, so dropping is enough.
            println!("  Dropping database {db}...");
            let out = Tools::output(tools.command("docker").args([
                "exec",
                MONGO_CONTAINER,
                "mongosh",
                &db,
                "--quiet",
                "--eval",
                "db.dropDatabase()",
            ]))?;
            println!("{}", tail(&out, 1));
            println!("  Database reset complete.");
        }
    }
    println!("Reset complete for {backend} backend.");
    Ok(())
}

/// The database in the app's `server/.env` `DATABASE_URL`, the last path segment.
fn database_name(app: &Path) -> Option<String> {
    let env = fs::read_to_string(app.join("server/.env")).ok()?;
    let line = env.lines().find(|l| l.contains("DATABASE_URL"))?;
    let url = line.split_once('=')?.1;
    let db = url.rsplit('/').next()?;
    Some(db.split('?').next().unwrap_or(db).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_name_comes_from_the_server_env() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path();
        assert_eq!(database_name(app), None);
        fs::create_dir_all(app.join("server")).unwrap();
        fs::write(
            app.join("server/.env"),
            "PORT=6001\nDATABASE_URL=postgresql://spacetime:spacetime@localhost:6432/spacetime_run2?sslmode=disable\n",
        )
        .unwrap();
        assert_eq!(database_name(app).as_deref(), Some("spacetime_run2"));
    }
}
