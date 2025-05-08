use std::sync::Arc;

use crate::{pg_server, StandaloneEnv};
use anyhow::Context;
use axum::extract::DefaultBodyLimit;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use spacetimedb::config::{CertificateAuthority, ConfigFile};
use spacetimedb::db::{Config, Storage};
use spacetimedb::startup::{self, TracingOptions};
use spacetimedb::worker_metrics;
use spacetimedb_client_api::routes::database::DatabaseRoutes;
use spacetimedb_client_api::routes::router;
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};
use spacetimedb_paths::server::ServerDataDir;
use tokio::net::TcpListener;

pub fn cli() -> clap::Command {
    clap::Command::new("start")
        .about("Starts a standalone SpacetimeDB instance")
        .args_override_self(true)
        .override_usage("spacetime start [OPTIONS]")
        .arg(
            Arg::new("listen_addr")
                .long("listen-addr")
                .short('l')
                .default_value("0.0.0.0:3000")
                .help(
                    "The address and port where SpacetimeDB should listen for connections. \
                     This defaults to to listen on all IP addresses on port 80.",
                ),
        )
        .arg(
            Arg::new("data_dir")
                .long("data-dir")
                .help("The path to the data directory for the database")
                .required(true)
                .value_parser(clap::value_parser!(ServerDataDir)),
        )
        .arg(
            Arg::new("enable_tracy")
                .long("enable-tracy")
                .action(SetTrue)
                .help("Enable Tracy profiling"),
        )
        .arg(
            Arg::new("jwt_key_dir")
                .hide(true)
                .long("jwt-key-dir")
                .help("The directory with id_ecdsa and id_ecdsa.pub")
                .value_parser(clap::value_parser!(spacetimedb_paths::cli::ConfigDir)),
        )
        .arg(
            Arg::new("jwt_pub_key_path")
                .long("jwt-pub-key-path")
                .requires("jwt_priv_key_path")
                .help("The path to the public jwt key for verifying identities")
                .value_parser(clap::value_parser!(PubKeyPath)),
        )
        .arg(
            Arg::new("jwt_priv_key_path")
                .long("jwt-priv-key-path")
                .requires("jwt_pub_key_path")
                .help("The path to the private jwt key for issuing identities")
                .value_parser(clap::value_parser!(PrivKeyPath)),
        )
        .arg(Arg::new("in_memory").long("in-memory").action(SetTrue).help(
            "If specified the database will run entirely in memory. After the process exits all data will be lost.",
        ))
        .arg(
            Arg::new("page_pool_max_size").long("page_pool_max_size").help(
                "The maximum size of the page pool in bytes. Should be a multiple of 64KiB. The default is 8GiB.",
            ),
        )
    // .after_help("Run `spacetime help start` for more detailed information.")
}

pub async fn exec(args: &ArgMatches) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let cert_dir = args.get_one::<spacetimedb_paths::cli::ConfigDir>("jwt_key_dir");
    let certs = Option::zip(
        args.get_one::<PubKeyPath>("jwt_pub_key_path").cloned(),
        args.get_one::<PrivKeyPath>("jwt_priv_key_path").cloned(),
    )
    .map(|(jwt_pub_key_path, jwt_priv_key_path)| CertificateAuthority {
        jwt_pub_key_path,
        jwt_priv_key_path,
    });
    let data_dir = args.get_one::<ServerDataDir>("data_dir").unwrap();
    let enable_tracy = args.get_flag("enable_tracy") || std::env::var_os("SPACETIMEDB_TRACY").is_some();
    let storage = if args.get_flag("in_memory") {
        Storage::Memory
    } else {
        Storage::Disk
    };
    let page_pool_max_size = args
        .get_one::<&str>("page_pool_max_size")
        .map(|size| parse_size::Config::new().with_binary().parse_size(size))
        .transpose()
        .context("unrecognized format in `page_pool_max_size`")?
        .map(|size| size as usize);
    let db_config = Config {
        storage,
        page_pool_max_size,
    };

    banner();
    let exe_name = std::env::current_exe()?;
    let exe_name = exe_name.file_name().unwrap().to_str().unwrap();
    println!("{} version: {}", exe_name, env!("CARGO_PKG_VERSION"));
    println!("{} path: {}", exe_name, std::env::current_exe()?.display());
    println!("database running in data directory {}", data_dir.display());

    let config_path = data_dir.config_toml();
    let config = match ConfigFile::read(&data_dir.config_toml())? {
        Some(config) => config,
        None => {
            let default_config = include_str!("../../config.toml");
            data_dir.create()?;
            config_path.write(default_config)?;
            toml::from_str(default_config).unwrap()
        }
    };

    startup::StartupOptions {
        tracing: Some(TracingOptions {
            config: config.logs,
            reload_config: cfg!(debug_assertions).then_some(config_path),
            disk_logging: std::env::var_os("SPACETIMEDB_DISABLE_DISK_LOGGING")
                .is_none()
                .then(|| data_dir.logs()),
            edition: "standalone".to_owned(),
            tracy: enable_tracy || std::env::var_os("SPACETIMEDB_TRACY").is_some(),
            flamegraph: std::env::var_os("SPACETIMEDB_FLAMEGRAPH").map(|_| {
                std::env::var_os("SPACETIMEDB_FLAMEGRAPH_PATH")
                    .unwrap_or("/var/log/flamegraph.folded".into())
                    .into()
            }),
        }),
        ..Default::default()
    }
    .configure();

    let certs = certs
        .or(config.certificate_authority)
        .or_else(|| cert_dir.map(CertificateAuthority::in_cli_config_dir))
        .context("cannot omit --jwt-{pub,priv}-key-path when those options are not specified in config.toml")?;

    let data_dir = Arc::new(data_dir.clone());
    let ctx = StandaloneEnv::init(db_config, &certs, data_dir).await?;
    worker_metrics::spawn_jemalloc_stats(listen_addr.clone());
    worker_metrics::spawn_tokio_stats(listen_addr.clone());
    worker_metrics::spawn_page_pool_stats(listen_addr.clone(), ctx.page_pool().clone());

    let mut db_routes = DatabaseRoutes::default();
    db_routes.root_post = db_routes.root_post.layer(DefaultBodyLimit::disable());
    db_routes.db_put = db_routes.db_put.layer(DefaultBodyLimit::disable());
    let extra = axum::Router::new().nest("/health", spacetimedb_client_api::routes::health::router());
    let service = router(&ctx, db_routes, extra).with_state(ctx.clone());

    let tcp = TcpListener::bind(listen_addr).await?;
    socket2::SockRef::from(&tcp).set_nodelay(true)?;
    log::debug!("Starting SpacetimeDB listening on {}", tcp.local_addr().unwrap());
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(());

    tokio::select! {
        _ = pg_server::start_pg(shutdown_rx.clone(), ctx) => {},
        _ = axum::serve(tcp, service).with_graceful_shutdown(async move {
            shutdown_rx.changed().await.ok();
        }) => {},
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down servers...");
            let _ = shutdown_tx.send(()); // Notify all tasks
        }
    }

    Ok(())
}

fn banner() {
    println!(
        r#"
┌───────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                       │
│                                                                                                       │
│                                                                              ⢀⠔⠁                      │
│                                                                            ⣠⡞⠁                        │
│                                              ⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣀⣀⣀⣀⣀⣀⣀⣤⣤⡴⠒    ⢀⣠⡾⠋                          │
│                                         ⢀⣤⣶⣾88888888888888888888⠿⠋    ⢀⣴8⡟⠁                           │
│                                      ⢀⣤⣾88888⡿⠿⠛⠛⠛⠛⠛⠛⠛⠛⠻⠿88888⠟⠁    ⣠⣾88⡟                             │
│                                    ⢀⣴88888⠟⠋⠁ ⣀⣤⠤⠶⠶⠶⠶⠶⠤⣤⣀ ⠉⠉⠉    ⢀⣴⣾888⡟                              │
│                                   ⣠88888⠋  ⣠⠶⠋⠉         ⠉⠙⠶⣄   ⢀⣴888888⠃                              │
│                                  ⣰8888⡟⠁ ⣰⠟⠁               ⠈⠻⣆ ⠈⢿888888                               │
│                                 ⢠8888⡟  ⡼⠁                   ⠈⢧ ⠈⢿8888⡿                               │
│                                 ⣼8888⠁ ⢸⠇                     ⠸⡇ ⠘8888⣷                               │
│                                 88888  8                       8  88888                               │
│                                 ⢿8888⡄ ⢸⡆                     ⢰⡇ ⢀8888⡟                               │
│                                 ⣾8888⣷⡀ ⢳⡀                   ⢀⡞  ⣼8888⠃                               │
│                                 888888⣷⡀ ⠹⣦⡀               ⢀⣴⠏ ⢀⣼8888⠏                                │
│                                ⢠888888⠟⠁   ⠙⠶⣄⣀         ⣀⣠⠶⠋  ⣠88888⠋                                 │
│                                ⣼888⡿⠟⠁    ⣀⣀⣀ ⠉⠛⠒⠶⠶⠶⠶⠶⠒⠛⠉ ⢀⣠⣴88888⠟⠁                                  │
│                               ⣼88⡿⠋    ⢀⣴88888⣶⣦⣤⣤⣤⣤⣤⣤⣤⣤⣶⣾88888⡿⠛⠁                                    │
│                             ⢀⣼8⠟⠁    ⣠⣶88888888888888888888⡿⠿⠛⠁                                       │
│                            ⣠⡾⠋⠁    ⠤⠞⠛⠛⠉⠉⠉⠉⠉⠉⠉⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉                                            │
│                          ⢀⡼⠋                                                                          │
│                        ⢀⠔⠁                                                                            │
│                                                                                                       │
│                                                                                                       │
│  .d8888b.                                     888    d8b                        8888888b.  888888b.   │
│ d88P  Y88b                                    888    Y8P                        888  "Y88b 888  "88b  │
│ Y88b.                                         888                               888    888 888  .88P  │
│  "Y888b.   88888b.   8888b.   .d8888b .d88b.  888888 888 88888b.d88b.   .d88b.  888    888 8888888K.  │
│     "Y88b. 888 "88b     "88b d88P"   d8P  Y8b 888    888 888 "888 "88b d8P  Y8b 888    888 888  "Y88b │
│       "888 888  888 .d888888 888     88888888 888    888 888  888  888 88888888 888    888 888    888 │
│ Y88b  d88P 888 d88P 888  888 Y88b.   Y8b.     Y88b.  888 888  888  888 Y8b.     888  .d88P 888   d88P │
│  "Y8888P"  88888P"  "Y888888  "Y8888P "Y8888   "Y888 888 888  888  888  "Y8888  8888888P"  8888888P"  │
│            888                                                                                        │
│            888                                                                                        │
│            888                                                                                        │
│                                  "Multiplayer at the speed of light"                                  │
└───────────────────────────────────────────────────────────────────────────────────────────────────────┘
    "#
    )
}
