use crate::routes::router;
use crate::util::{create_dir_or_err, create_file_with_contents};
use crate::StandaloneEnv;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use spacetimedb::db::db_metrics;
use spacetimedb::{startup, worker_metrics};
use std::net::TcpListener;
use std::sync::Arc;
use clap::parser::ValueSource;

#[cfg(feature = "string")]
impl From<std::string::String> for OsStr {
    fn from(name: std::string::String) -> Self {
        Self::from_string(name.into())
    }
}

pub fn cli(is_standalone: bool) -> clap::Command {
    clap::Command::new("start")
        .about("Starts a standalone SpacetimeDB instance. This command recognizes the following environment variables: \
                \n\tSPACETIMEDB_LOG_CONFIG: The path to the log configuration file. \
                \n\tSPACETIMEDB_LOGS_PATH: The path to the directory that should contain logs for SpacetimeDB. \
                \n\tSTDB_PATH: The path to the directory that should contain the database files for SpacetimeDB. \
                \n\tSPACETIMEDB_JWT_PUB_KEY: The path to the public jwt key for verifying identities. \
                \n\tSPACETIMEDB_JWT_PRIV_KEY: The path to the private jwt key for issuing identities. \
                \n\tSPACETIMEDB_TRACY: Set to 1 to enable Tracy profiling.\
                \n\nWarning: If you set a value on the command line, it will override the value set in the environment variable.")
        .arg(
            Arg::new("listen_addr")
                .long("listen-addr")
                .short('l')
                .default_value(if is_standalone { "0.0.0.0:80" } else { "127.0.0.1:3000" })
                .help("The address and port where SpacetimeDB should listen for connections."),
        )
        .arg(
            Arg::new("log_conf_path")
                .long("log-conf-path")
                .help("The path of the file that contains the log configuration for SpacetimeDB (SPACETIMEDB_LOG_CONFIG)")
                .default_value(format!("{}/.spacetime/log.conf", std::env::var("HOME").unwrap())),
        )
        .arg(
            Arg::new("log_dir_path")
                .long("log-dir-path")
                .help("The path to the directory that should contain logs for SpacetimeDB (SPACETIMEDB_LOGS_PATH)")
                .default_value(format!("{}/.spacetime", std::env::var("HOME").unwrap())),
        )
        .arg(
            Arg::new("database_path")
                .help("The path to the directory that should contain the database files for SpacetimeDB (STDB_PATH)")
                .required(false)
                .default_value(format!("{}/.spacetime/stdb", std::env::var("HOME").unwrap())),
        )
        .arg(
            Arg::new("allow_create")
                .long("allow-create")
                .action(SetTrue)
                .help("Allows for the creation of files and directories that don't exist"),
        )
        .arg(
            Arg::new("enable_tracy")
                .long("enable-tracy")
                .action(SetTrue)
                .help("Enable Tracy profiling (SPACETIMEDB_TRACY)"),
        )
        .arg(
            Arg::new("jwt_pub_key_path")
                .long("jwt-pub-key-path")
                .help("The path to the public jwt key for verifying identities (SPACETIMEDB_JWT_PUB_KEY)")
                .default_value(format!("{}/.spacetime/id_ecdsa.pub", std::env::var("HOME").unwrap())),
        )
        .arg(
            Arg::new("jwt_priv_key_path")
                .long("jwt-priv-key-path")
                .help("The path to the private jwt key for issuing identities (SPACETIMEDB_JWT_PRIV_KEY)")
                .default_value(format!("{}/.spacetime/id_ecdsa", std::env::var("HOME").unwrap())),
        )
        // We still want to keep the executable name `spacetimedb` when we're executing as a standalone, but
        // we want the executable name to be `spacetime` when we're executing this from the CLI. We have to
        // pass these strings with static lifetimes so we can't do any dynamic string manipulation here.
        .after_help(if is_standalone {
            "Run `spacetimedb help start` for more detailed information."
        } else {
            "Run `spacetime help start` for more information."
        })
}

/// This reads an argument from the command line that is also backed by an environment variable.
fn read_argument<'a>(args: &'a ArgMatches, is_standalone: bool, value_name: &str, env_name: &str) -> Option<&'a String> {
    // This checks whether the environment variable `env_name` is set:
    let env_set = std::env::var(env_name).is_ok();
    let is_default_value = match args.value_source(value_name) {
        None => false,
        Some(source) => {
            match source {
                ValueSource::DefaultValue => true,
                _ => false,
            }
        }
    };
    let value = args.get_one::<String>(value_name);

    if !is_standalone {
        value
    } else {
        // If the environment variable is set and the value is the default value, then the environment
        // variable will take precedence. Otherwise, we use the value from the command line.
        if env_set && is_default_value {
            None
        } else {
            value
        }
    }
}

pub async fn exec(args: &ArgMatches, is_standalone: bool) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let allow_create = args.get_flag("allow_create");
    let log_conf_path = read_argument(args, is_standalone, "log_conf_path", "SPACETIMEDB_LOG_CONFIG");
    let log_dir_path = read_argument(args, is_standalone, "log_dir_path", "SPACETIMEDB_LOGS_PATH");
    let stdb_path = read_argument(args, is_standalone, "database_path", "STDB_PATH");
    let jwt_pub_key_path = read_argument(args, is_standalone, "jwt_pub_key_path", "SPACETIMEDB_JWT_PUB_KEY");
    let jwt_priv_key_path = read_argument(args, is_standalone, "jwt_priv_key_path", "SPACETIMEDB_JWT_PRIV_KEY");
    let enable_tracy = args.get_flag("enable_tracy");

    if let Some(log_conf_path) = log_conf_path {
        create_file_with_contents(
            allow_create,
            &log_conf_path,
            include_str!("../../../../crates/standalone/log.conf"),
        )?;
        std::env::set_var("SPACETIMEDB_LOG_CONFIG", log_conf_path);
    }

    if let Some(log_dir_path) = log_dir_path {
        create_dir_or_err(allow_create, &log_dir_path)?;
        std::env::set_var("SPACETIMEDB_LOGS_PATH", log_dir_path);
    }

    if let Some(stdb_path) = stdb_path {
        create_dir_or_err(allow_create, &stdb_path)?;
        std::env::set_var("STDB_PATH", stdb_path);
    }

    // If this doesn't exist, we will create it later, just set the env variable for now
    if let Some(jwt_pub_key_path) = jwt_pub_key_path {
        std::env::set_var("SPACETIMEDB_JWT_PUB_KEY", jwt_pub_key_path);
    }

    // If this doesn't exist, we will create it later, just set the env variable for now
    if let Some(jwt_priv_key_path) = jwt_priv_key_path {
        std::env::set_var("SPACETIMEDB_JWT_PRIV_KEY", jwt_priv_key_path);
    }

    if enable_tracy {
        std::env::set_var("SPACETIMEDB_TRACY", "1");
    }

    startup::configure_tracing();

    // Metrics for pieces under worker_node/ related to reducer hosting, etc.
    worker_metrics::register_custom_metrics();

    // Metrics for our use of db/.
    db_metrics::register_custom_metrics();

    let ctx = spacetimedb_client_api::ArcEnv(Arc::new(StandaloneEnv::init().await?));

    let service = router().with_state(ctx).into_make_service();

    let tcp = TcpListener::bind(listen_addr).unwrap();
    log::debug!("Starting SpacetimeDB listening on {}", tcp.local_addr().unwrap());
    axum::Server::from_tcp(tcp)?.serve(service).await?;
    Ok(())
}
