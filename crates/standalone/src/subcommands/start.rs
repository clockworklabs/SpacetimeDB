use spacetimedb_client_api::routes::identity::IdentityRoutes;
use spacetimedb_pg::pg_server;
use std::io::{self, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, TcpListener as StdTcpListener};
use std::sync::Arc;

use crate::{StandaloneEnv, StandaloneOptions};
use anyhow::Context;
use axum::extract::DefaultBodyLimit;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use spacetimedb::config::{parse_config, CertificateAuthority};
use spacetimedb::db::{self, Storage};
use spacetimedb::startup::{self, TracingOptions};
use spacetimedb::util::jobs::JobCores;
use spacetimedb::worker_metrics;
use spacetimedb_client_api::routes::database::DatabaseRoutes;
use spacetimedb_client_api::routes::router;
use spacetimedb_client_api::routes::subscribe::WebSocketOptions;
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};
use spacetimedb_paths::server::{ConfigToml, ServerDataDir};
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
        .arg(
            Arg::new("pg_port")
                .long("pg-port")
                .help("If specified, enables the built-in PostgreSQL wire protocol server on the given port.")
                .value_parser(clap::value_parser!(u16).range(1024..65535)),
        )
        .arg(
            Arg::new("non_interactive")
                .long("non-interactive")
                .action(SetTrue)
                .help("Run in non-interactive mode (fail immediately if port is in use)"),
        )
    // .after_help("Run `spacetime help start` for more detailed information.")
}

#[derive(Default, serde::Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    common: spacetimedb::config::ConfigFile,
    #[serde(default)]
    websocket: WebSocketOptions,
}

impl ConfigFile {
    fn read(path: &ConfigToml) -> anyhow::Result<Option<Self>> {
        parse_config(path.as_ref())
    }
}

pub async fn exec(args: &ArgMatches, db_cores: JobCores) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let pg_port = args.get_one::<u16>("pg_port");
    let non_interactive = args.get_flag("non_interactive");
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
        .get_one::<String>("page_pool_max_size")
        .map(|size| parse_size::Config::new().with_binary().parse_size(size))
        .transpose()
        .context("unrecognized format in `page_pool_max_size`")?
        .map(|size| size as usize);
    let db_config = db::Config {
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

    startup::configure_tracing(TracingOptions {
        config: config.common.logs,
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
    });

    let certs = certs
        .or(config.common.certificate_authority)
        .or_else(|| cert_dir.map(CertificateAuthority::in_cli_config_dir))
        .context("cannot omit --jwt-{pub,priv}-key-path when those options are not specified in config.toml")?;

    let data_dir = Arc::new(data_dir.clone());
    let ctx = StandaloneEnv::init(
        StandaloneOptions {
            db_config,
            websocket: config.websocket,
        },
        &certs,
        data_dir,
        db_cores,
    )
    .await?;
    worker_metrics::spawn_jemalloc_stats(listen_addr.clone());
    worker_metrics::spawn_tokio_stats(listen_addr.clone());
    worker_metrics::spawn_page_pool_stats(listen_addr.clone(), ctx.page_pool().clone());
    worker_metrics::spawn_bsatn_rlb_pool_stats(listen_addr.clone(), ctx.bsatn_rlb_pool().clone());
    let mut db_routes = DatabaseRoutes::default();
    db_routes.root_post = db_routes.root_post.layer(DefaultBodyLimit::disable());
    db_routes.db_put = db_routes.db_put.layer(DefaultBodyLimit::disable());
    db_routes.pre_publish = db_routes.pre_publish.layer(DefaultBodyLimit::disable());
    let extra = axum::Router::new().nest("/health", spacetimedb_client_api::routes::health::router());
    let service = router(&ctx, db_routes, IdentityRoutes::default(), extra).with_state(ctx.clone());

    // Check if the requested port is available on both IPv4 and IPv6.
    // If not, offer to find an available port by incrementing (unless non-interactive).
    let listen_addr = if let Some((host, port_str)) = listen_addr.rsplit_once(':') {
        if let Ok(requested_port) = port_str.parse::<u16>() {
            if !is_port_available(host, requested_port) {
                if non_interactive {
                    anyhow::bail!(
                        "Port {} is already in use. Please free up the port or specify a different port with --listen-addr.",
                        requested_port
                    );
                }
                // Port is in use, try to find an alternative
                match find_available_port(host, requested_port.saturating_add(1), 100) {
                    Some(available_port) => {
                        let question = format!(
                            "Port {} is already in use. Would you like to use port {} instead?",
                            requested_port, available_port
                        );
                        if prompt_yes_no(&question) {
                            format!("{}:{}", host, available_port)
                        } else {
                            anyhow::bail!(
                                "Port {} is already in use. Please free up the port or specify a different port with --listen-addr.",
                                requested_port
                            );
                        }
                    }
                    None => {
                        anyhow::bail!(
                            "Port {} is already in use and could not find an available port nearby. \
                             Please free up the port or specify a different port with --listen-addr.",
                            requested_port
                        );
                    }
                }
            } else {
                listen_addr.to_string()
            }
        } else {
            listen_addr.to_string()
        }
    } else {
        listen_addr.to_string()
    };

    let tcp = TcpListener::bind(&listen_addr).await.context(format!(
        "failed to bind the SpacetimeDB server to '{listen_addr}', please check that the address is valid and not already in use"
    ))?;
    socket2::SockRef::from(&tcp).set_nodelay(true)?;
    log::info!("Starting SpacetimeDB listening on {}", tcp.local_addr()?);

    if let Some(pg_port) = pg_port {
        let server_addr = listen_addr.split(':').next().unwrap();
        let tcp_pg = TcpListener::bind(format!("{server_addr}:{pg_port}")).await.context(format!(
            "failed to bind the SpacetimeDB PostgreSQL wire protocol server to {server_addr}:{pg_port}, please check that the port is valid and not already in use"
        ))?;

        let notify = Arc::new(tokio::sync::Notify::new());
        let shutdown_notify = notify.clone();
        tokio::select! {
            _ = pg_server::start_pg(notify.clone(), ctx, tcp_pg) => {},
            _ = axum::serve(tcp, service).with_graceful_shutdown(async move  {
                shutdown_notify.notified().await;
            }) => {},
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down servers...");
                notify.notify_waiters(); // Notify all tasks
            }
        }
    } else {
        log::warn!("PostgreSQL wire protocol server disabled");
        axum::serve(tcp, service)
            .with_graceful_shutdown(async {
                tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
                log::info!("Shutting down server...");
            })
            .await?;
    }

    Ok(())
}

/// Check if a port is available on the requested host for both IPv4 and IPv6.
///
/// On macOS (and some other systems), `localhost` can resolve to both IPv4 (127.0.0.1)
/// and IPv6 (::1). If SpacetimeDB binds only to IPv4 but another service is using the
/// same port on IPv6, browsers may connect to the wrong service depending on which
/// address they try first.
///
/// This function checks both the requested IPv4 address and its IPv6 equivalent:
/// - 127.0.0.1 -> also checks ::1
/// - 0.0.0.0 -> also checks ::
/// - 10.1.1.1 -> also checks ::ffff:10.1.1.1 (IPv4-mapped IPv6)
///
/// Note: There is a small race condition between this check and the actual bind -
/// another process could grab the port in between. This is unlikely in practice
/// and the actual bind will fail with a clear error if it happens.
fn is_port_available(host: &str, port: u16) -> bool {
    // Parse the host and determine which addresses to check
    let ipv4 = host
        .parse::<Ipv4Addr>()
        .unwrap_or_else(|e| panic!("Invalid IPv4 address '{}': {}", host, e));

    let ipv6 = if ipv4.is_loopback() {
        Ipv6Addr::LOCALHOST
    } else if ipv4.is_unspecified() {
        Ipv6Addr::UNSPECIFIED
    } else {
        // For specific IPs, use the IPv4-mapped IPv6 address
        ipv4.to_ipv6_mapped()
    };

    let ipv4_addr = SocketAddr::from((ipv4, port));
    let ipv6_addr = SocketAddr::V6(SocketAddrV6::new(ipv6, port, 0, 0));

    let ipv4_available = StdTcpListener::bind(ipv4_addr).is_ok();
    let ipv6_available = StdTcpListener::bind(ipv6_addr).is_ok();

    ipv4_available && ipv6_available
}

/// Find an available port starting from the requested port.
/// Returns the first port that is available on both IPv4 and IPv6.
fn find_available_port(host: &str, requested_port: u16, max_attempts: u16) -> Option<u16> {
    for offset in 0..max_attempts {
        let port = requested_port.saturating_add(offset);
        if port == 0 || port == u16::MAX {
            break;
        }
        if is_port_available(host, port) {
            return Some(port);
        }
    }
    None
}

/// Prompt the user with a yes/no question. Returns true if they answer yes.
fn prompt_yes_no(question: &str) -> bool {
    print!("{} [y/N] ", question);
    io::stdout().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn options_from_partial_toml() {
        let toml = r#"
            [logs]
            directives = [
                "banana_shake=strawberry",
            ]

            [websocket]
            idle-timeout = "1min"
            close-handshake-timeout = "500ms"
"#;

        let config: ConfigFile = toml::from_str(toml).unwrap();

        // `spacetimedb::config::ConfigFile` doesn't implement `PartialEq`,
        // so check `common` in a pedestrian way.
        assert_eq!(&config.common.logs.directives, &["banana_shake=strawberry"]);
        assert!(config.common.certificate_authority.is_none());

        assert_eq!(
            config.websocket,
            WebSocketOptions {
                idle_timeout: Duration::from_secs(60),
                close_handshake_timeout: Duration::from_millis(500),
                ..<_>::default()
            }
        );
    }
}
