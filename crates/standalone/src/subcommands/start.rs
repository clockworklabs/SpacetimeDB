use std::net::TcpListener;
use std::sync::Arc;
use clap::{Arg, ArgMatches};
use clap::ArgAction::SetTrue;
use spacetimedb::{startup, worker_metrics};
use spacetimedb::db::db_metrics;
use crate::routes::router;
use crate::StandaloneEnv;
use crate::util::{create_dir_or_err, create_file_with_contents};

pub fn cli(is_standalone: bool) -> clap::Command {
    clap::Command::new("start")
        .about("Starts a standalone SpacetimeDB instance")
        .arg(
            Arg::new("listen_addr")
                .long("listen-addr")
                .short('l')
                .default_value(if is_standalone {
                    "0.0.0.0:80"
                } else {
                    "127.0.0.1:3000"
                })
                .help("The address and port where SpacetimeDB should listen for connections"),
        )
        .arg(
            Arg::new("log_conf_path")
                .long("log-conf-path")
                .help("The path of the file that contains the log configuration for SpacetimeDB")
        )
        .arg(
            Arg::new("log_dir_path")
                .long("log-dir-path")
                .help("The path to the directory that should contain logs for SpacetimeDB")
        )
        .arg(
            Arg::new("database_path")
                .long("database-path")
                .help("The path to the directory that should contain the database files for SpacetimeDB")
        )
        .arg(
            Arg::new("allow_create")
                .long("allow-create")
                .action(SetTrue)
                .help("Allows for the creation of files and directories that don't exist")
        )
        .arg(
            Arg::new("enable_tracy")
                .long("enable-tracy")
                .action(SetTrue)
                .help("Enable Tracy profiling"),
        )
        .arg(
            Arg::new("jwt_pub_key_path")
                .long("jwt-pub-key-path")
                .help("The path to the public jwt key for verifying identities"),
        )
        .arg(
            Arg::new("jwt_priv_key_path")
                .long("jwt-priv-key-path")
                .help("The path to the private jwt key for issuing identities"),
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

pub async fn exec(args: &ArgMatches, is_standalone: bool) -> anyhow::Result<()> {
    let home_dir = std::env::var("HOME")?;
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let allow_create = args.get_flag("allow_create");
    let log_conf_path = args.get_one::<String>("log_conf_path").cloned().or(
        if is_standalone { None } else { Some(format!("{}/.spacetime/log.conf", home_dir)) });
    let log_dir_path = args.get_one::<String>("log_dir_path").cloned().or(
    if is_standalone { None } else { Some(format!("{}/.spacetime", home_dir)) });
    let stdb_path = args.get_one::<String>("database_path").cloned().or(
        if is_standalone { None } else { Some(format!("{}/.spacetime/stdb", home_dir)) });
    let jwt_pub_key_path = args.get_one::<String>("jwt_pub_key_path").cloned().or(
        if is_standalone { None } else { Some(format!("{}/.spacetime/id_ecdsa.pub", home_dir)) });
    let jwt_priv_key_path = args.get_one::<String>("jwt_pub_key_path").cloned().or(
        if is_standalone { None } else { Some(format!("{}/.spacetime/id_ecdsa", home_dir)) });
    let enable_tracy = args.get_flag("enable_tracy");

    if let Some(log_conf_path) = log_conf_path {
        create_file_with_contents(allow_create, &log_conf_path, include_str!("../../../../crates/standalone/log.conf"))?;
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
