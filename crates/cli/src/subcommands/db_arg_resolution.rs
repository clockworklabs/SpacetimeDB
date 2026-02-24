// Database argument resolution for CLI commands.
//
// When a spacetime.json config file is present, CLI commands resolve database arguments
// from the config. If the user provides a database name that doesn't match any configured
// target, the behavior depends on whether the database argument is unambiguous:
//
// | Command     | Resolution function                | DB arg unambiguous? | Auto-fallthrough? |
// |-------------|-------------------------------------|---------------------|-------------------|
// | logs        | resolve_database_arg               | Yes (dedicated arg) | Yes               |
// | delete      | resolve_database_arg               | Yes (dedicated arg) | Yes               |
// | sql         | resolve_optional_database_parts    | Only with 2+ args   | Yes (at call site)|
// | call        | resolve_optional_database_parts    | No (variable args)  | No                |
// | subscribe   | resolve_optional_database_parts    | No (variable args)  | No                |
// | describe    | resolve_database_with_optional_parts | No (optional args)| No                |
//
// "Auto-fallthrough" means: if the provided database name doesn't match any config target,
// treat it as an ad-hoc database outside the project (equivalent to --no-config for that arg).
//
// Commands using `resolve_database_arg` always have an unambiguous `<database>` arg, so we
// can safely fall through. For `sql`, the call site knows that exactly 1 query arg is expected,
// so 2+ positional args means the first must be a database. For `call`/`subscribe`/`describe`,
// the first positional could be a non-database argument, so we must error to avoid misinterpreting it.

use crate::spacetime_config::find_and_load_with_env;
use itertools::Itertools;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigDbTarget {
    pub database: String,
    pub server: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedDbArgs {
    pub database: String,
    pub server: Option<String>,
    pub remaining_args: Vec<String>,
}

/// Build an error for when the first positional arg doesn't match any configured database target.
fn unknown_database_error(db: &str, config_targets: &[ConfigDbTarget]) -> anyhow::Error {
    let known: Vec<&str> = config_targets.iter().map(|t| t.database.as_str()).collect();
    if known.len() > 1 {
        anyhow::anyhow!(
            "Multiple databases found in config: {}. Please specify which database to use, \
             or pass --no-config to use '{}' directly.",
            known.join(", "),
            db
        )
    } else {
        anyhow::anyhow!(
            "Database '{}' is not in the config file. \
             If you want to run against a database outside of the current project, pass --no-config.",
            db
        )
    }
}

pub(crate) fn load_config_db_targets(no_config: bool) -> anyhow::Result<Option<Vec<ConfigDbTarget>>> {
    if no_config {
        return Ok(None);
    }

    Ok(find_and_load_with_env(None)?
        .map(|loaded| {
            loaded
                .config
                .collect_all_targets_with_inheritance()
                .iter()
                .filter_map(|t| {
                    let database = t.fields.get("database").and_then(|v| v.as_str())?;
                    let server = t.fields.get("server").and_then(|v| v.as_str()).map(|s| s.to_string());
                    Some(ConfigDbTarget {
                        database: database.to_string(),
                        server,
                    })
                })
                .unique_by(|t| t.database.clone())
                .collect::<Vec<_>>()
        })
        .filter(|targets| !targets.is_empty()))
}

pub(crate) fn resolve_optional_database_parts(
    raw_parts: &[String],
    config_targets: Option<&[ConfigDbTarget]>,
    required_arg_name: &str,
    usage: &str,
) -> anyhow::Result<ResolvedDbArgs> {
    let require_arg = |name: &str| {
        anyhow::anyhow!(
            "the following required arguments were not provided:\n  <{}>\n\nUsage: {}",
            name,
            usage
        )
    };

    let Some(config_targets) = config_targets else {
        if raw_parts.len() < 2 {
            return if raw_parts.is_empty() {
                Err(require_arg("database"))
            } else {
                Err(require_arg(required_arg_name))
            };
        }
        return Ok(ResolvedDbArgs {
            database: raw_parts[0].clone(),
            server: None,
            remaining_args: raw_parts[1..].to_vec(),
        });
    };

    if config_targets.len() == 1 {
        let target = &config_targets[0];
        if raw_parts.is_empty() {
            return Err(require_arg(required_arg_name));
        }
        if raw_parts[0] == target.database {
            if raw_parts.len() < 2 {
                return Err(require_arg(required_arg_name));
            }
            return Ok(ResolvedDbArgs {
                database: target.database.clone(),
                server: target.server.clone(),
                remaining_args: raw_parts[1..].to_vec(),
            });
        }
        return Ok(ResolvedDbArgs {
            database: target.database.clone(),
            server: target.server.clone(),
            remaining_args: raw_parts.to_vec(),
        });
    }

    let db = &raw_parts[0];
    let Some(target) = config_targets.iter().find(|t| t.database == *db) else {
        return Err(unknown_database_error(db, config_targets));
    };
    if raw_parts.len() < 2 {
        return Err(require_arg(required_arg_name));
    }

    Ok(ResolvedDbArgs {
        database: db.clone(),
        server: target.server.clone(),
        remaining_args: raw_parts[1..].to_vec(),
    })
}

pub(crate) fn resolve_database_arg(
    raw_database: Option<&str>,
    config_targets: Option<&[ConfigDbTarget]>,
    usage: &str,
) -> anyhow::Result<ResolvedDbArgs> {
    let require_database = || {
        anyhow::anyhow!(
            "the following required arguments were not provided:\n  <database>\n\nUsage: {}",
            usage
        )
    };

    let Some(config_targets) = config_targets else {
        let database = raw_database.ok_or_else(require_database)?;
        return Ok(ResolvedDbArgs {
            database: database.to_string(),
            server: None,
            remaining_args: vec![],
        });
    };

    if config_targets.len() == 1 {
        let target = &config_targets[0];
        if let Some(db) = raw_database {
            if db != target.database {
                // The database arg is unambiguous, so treat it as an ad-hoc database
                // outside the project config (auto-fallthrough).
                return Ok(ResolvedDbArgs {
                    database: db.to_string(),
                    server: None,
                    remaining_args: vec![],
                });
            }
        }
        return Ok(ResolvedDbArgs {
            database: target.database.clone(),
            server: target.server.clone(),
            remaining_args: vec![],
        });
    }

    let db = raw_database.ok_or_else(require_database)?;
    let Some(target) = config_targets.iter().find(|t| t.database == db) else {
        // The database arg is unambiguous, so treat it as an ad-hoc database
        // outside the project config (auto-fallthrough).
        return Ok(ResolvedDbArgs {
            database: db.to_string(),
            server: None,
            remaining_args: vec![],
        });
    };

    Ok(ResolvedDbArgs {
        database: target.database.clone(),
        server: target.server.clone(),
        remaining_args: vec![],
    })
}

pub(crate) fn resolve_database_with_optional_parts(
    raw_parts: &[String],
    config_targets: Option<&[ConfigDbTarget]>,
    usage: &str,
) -> anyhow::Result<ResolvedDbArgs> {
    let require_database = || {
        anyhow::anyhow!(
            "the following required arguments were not provided:\n  <database>\n\nUsage: {}",
            usage
        )
    };

    let Some(config_targets) = config_targets else {
        let Some(database) = raw_parts.first() else {
            return Err(require_database());
        };
        return Ok(ResolvedDbArgs {
            database: database.clone(),
            server: None,
            remaining_args: raw_parts[1..].to_vec(),
        });
    };

    if config_targets.len() == 1 {
        let target = &config_targets[0];
        if raw_parts.first().is_some_and(|db| db == &target.database) {
            return Ok(ResolvedDbArgs {
                database: target.database.clone(),
                server: target.server.clone(),
                remaining_args: raw_parts[1..].to_vec(),
            });
        }
        return Ok(ResolvedDbArgs {
            database: target.database.clone(),
            server: target.server.clone(),
            remaining_args: raw_parts.to_vec(),
        });
    }

    let Some(db) = raw_parts.first() else {
        return Err(require_database());
    };
    let Some(target) = config_targets.iter().find(|t| t.database == *db) else {
        return Err(unknown_database_error(db, config_targets));
    };

    Ok(ResolvedDbArgs {
        database: db.clone(),
        server: target.server.clone(),
        remaining_args: raw_parts[1..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_database_arg, resolve_database_with_optional_parts, resolve_optional_database_parts, ConfigDbTarget,
    };

    #[test]
    fn single_db_infers_database() {
        let parts = vec!["reducer".to_string(), "arg1".to_string()];
        let targets = vec![ConfigDbTarget {
            database: "foo".to_string(),
            server: Some("maincloud".to_string()),
        }];
        let parsed = resolve_optional_database_parts(
            &parts,
            Some(&targets),
            "function_name",
            "spacetime call [database] <function_name> <arguments>...",
        )
        .unwrap();
        assert_eq!(parsed.database, "foo");
        assert_eq!(parsed.server.as_deref(), Some("maincloud"));
        assert_eq!(parsed.remaining_args, parts);
    }

    #[test]
    fn single_db_accepts_explicit_db_prefix() {
        let parts = vec!["foo".to_string(), "SELECT 1".to_string()];
        let targets = vec![ConfigDbTarget {
            database: "foo".to_string(),
            server: Some("local".to_string()),
        }];
        let parsed = resolve_optional_database_parts(
            &parts,
            Some(&targets),
            "query",
            "spacetime subscribe [database] <query> [query...]",
        )
        .unwrap();
        assert_eq!(parsed.database, "foo");
        assert_eq!(parsed.server.as_deref(), Some("local"));
        assert_eq!(parsed.remaining_args, vec!["SELECT 1".to_string()]);
    }

    #[test]
    fn multi_db_rejects_unknown_database() {
        let parts = vec!["baz".to_string(), "SELECT 1".to_string()];
        let targets = vec![
            ConfigDbTarget {
                database: "foo".to_string(),
                server: Some("maincloud".to_string()),
            },
            ConfigDbTarget {
                database: "bar".to_string(),
                server: Some("local".to_string()),
            },
        ];
        let err = resolve_optional_database_parts(
            &parts,
            Some(&targets),
            "query",
            "spacetime subscribe [database] <query> [query...]",
        )
        .unwrap_err();
        assert!(err.to_string().contains("Multiple databases found in config: foo, bar"));
        assert!(err.to_string().contains("--no-config"));
    }

    #[test]
    fn resolve_database_arg_single_target_uses_config_database() {
        let targets = vec![ConfigDbTarget {
            database: "foo".to_string(),
            server: Some("maincloud".to_string()),
        }];
        let resolved = resolve_database_arg(None, Some(&targets), "spacetime logs [database]").unwrap();
        assert_eq!(resolved.database, "foo");
        assert_eq!(resolved.server.as_deref(), Some("maincloud"));
    }

    #[test]
    fn resolve_database_with_optional_parts_single_target_allows_no_parts() {
        let targets = vec![ConfigDbTarget {
            database: "foo".to_string(),
            server: Some("maincloud".to_string()),
        }];
        let resolved =
            resolve_database_with_optional_parts(&[], Some(&targets), "spacetime describe [database] [entity]")
                .unwrap();
        assert_eq!(resolved.database, "foo");
        assert!(resolved.remaining_args.is_empty());
    }

    #[test]
    fn resolve_database_arg_single_target_falls_through_for_unknown_db() {
        let targets = vec![ConfigDbTarget {
            database: "foo".to_string(),
            server: Some("maincloud".to_string()),
        }];
        let resolved = resolve_database_arg(Some("other-db"), Some(&targets), "spacetime logs [database]").unwrap();
        assert_eq!(resolved.database, "other-db");
        assert_eq!(resolved.server, None);
    }

    #[test]
    fn resolve_database_arg_multi_target_falls_through_for_unknown_db() {
        let targets = vec![
            ConfigDbTarget {
                database: "foo".to_string(),
                server: Some("maincloud".to_string()),
            },
            ConfigDbTarget {
                database: "bar".to_string(),
                server: Some("local".to_string()),
            },
        ];
        let resolved = resolve_database_arg(Some("other-db"), Some(&targets), "spacetime logs [database]").unwrap();
        assert_eq!(resolved.database, "other-db");
        assert_eq!(resolved.server, None);
    }
}
