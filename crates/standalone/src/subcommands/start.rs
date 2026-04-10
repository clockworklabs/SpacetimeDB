use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use spacetimedb_client_api::routes::identity::IdentityRoutes;
use spacetimedb_pg::pg_server;
use std::fmt;
use std::io::{self, Write};
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use crate::{StandaloneEnv, StandaloneOptions};
use anyhow::Context;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{self, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use spacetimedb::config::{parse_config, CertificateAuthority};
use spacetimedb::db::{self, Storage};
use spacetimedb::host::{FunctionArgs, ProcedureCallError};
use spacetimedb::identity::Identity;
use spacetimedb::startup::{self, TracingOptions};
use spacetimedb::util::jobs::JobCores;
use spacetimedb::worker_metrics;
use spacetimedb_client_api::routes::database::DatabaseRoutes;
use spacetimedb_client_api::routes::router;
use spacetimedb_client_api::routes::subscribe::WebSocketOptions;
use spacetimedb_client_api::{ControlStateReadAccess, NodeDelegate};
use spacetimedb_lib::{sats, AlgebraicValue};
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
        .arg(
            Arg::new("root_domain")
                .long("root-domain")
                .help("Root domain for web subdomain routing (for example: example.com)")
                .value_parser(clap::builder::NonEmptyStringValueParser::new()),
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

const INDEX_PROCEDURE: &str = "index";
const SUBDOMAIN_CALLER_SUBJECT: &str = "subdomain-index";

#[derive(Clone)]
struct RootDomainRouteConfig {
    root_domain: String,
    subdomain_suffix: String,
    ctx: Option<Arc<StandaloneEnv>>,
}

impl RootDomainRouteConfig {
    fn new(root_domain: String, ctx: Arc<StandaloneEnv>) -> Self {
        let subdomain_suffix = format!(".{root_domain}");
        Self {
            root_domain,
            subdomain_suffix,
            ctx: Some(ctx),
        }
    }

    #[cfg(test)]
    fn for_tests(root_domain: String) -> Self {
        let subdomain_suffix = format!(".{root_domain}");
        Self {
            root_domain,
            subdomain_suffix,
            ctx: None,
        }
    }

    fn classify_host(&self, host: &str) -> RootDomainHostMatch {
        if host == self.root_domain {
            return RootDomainHostMatch::NoMatch;
        }

        let Some(module_name) = host.strip_suffix(&self.subdomain_suffix) else {
            return RootDomainHostMatch::NoMatch;
        };

        if module_name.is_empty() || module_name.contains('.') {
            return RootDomainHostMatch::InvalidSubdomain;
        }

        RootDomainHostMatch::Module(module_name.to_owned())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RootDomainHostMatch {
    Module(String),
    InvalidSubdomain,
    NoMatch,
}

impl fmt::Display for RootDomainRouteConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.root_domain)
    }
}

fn normalize_root_domain(raw: &str) -> anyhow::Result<String> {
    let root_domain = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    anyhow::ensure!(!root_domain.is_empty(), "`--root-domain` cannot be empty");
    Ok(root_domain)
}

fn normalize_host_header_value(raw: &str) -> Option<String> {
    let host = raw.trim().trim_end_matches('.');
    if host.is_empty() {
        return None;
    }

    let host = http::uri::Authority::from_str(host)
        .map(|authority| authority.host().to_ascii_lowercase())
        .unwrap_or_else(|_| host.to_ascii_lowercase());
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

async fn root_domain_middleware(State(config): State<RootDomainRouteConfig>, req: Request, next: Next) -> Response {
    let host_match = req
        .headers()
        .get(http::header::HOST)
        .and_then(|host| host.to_str().ok())
        .and_then(normalize_host_header_value)
        .map(|host| config.classify_host(&host))
        .unwrap_or(RootDomainHostMatch::NoMatch);

    match host_match {
        RootDomainHostMatch::Module(module_name) => {
            let procedure_name = match procedure_name_from_path(req.uri().path()) {
                Ok(name) => name,
                Err(message) => return (StatusCode::NOT_FOUND, message).into_response(),
            };
            module_procedure_response(&config, &module_name, &procedure_name).await
        }
        RootDomainHostMatch::InvalidSubdomain => (
            StatusCode::NOT_FOUND,
            "Only single-label subdomains are supported for root-domain routing.",
        )
            .into_response(),
        RootDomainHostMatch::NoMatch => next.run(req).await,
    }
}

fn procedure_name_from_path(path: &str) -> Result<String, &'static str> {
    println!("path: {}", path);
    let procedure_name = path.trim_matches('/');
    if procedure_name.is_empty() {
        return Ok(INDEX_PROCEDURE.to_string());
    }
    if procedure_name.contains('/') {
        return Err("Only single-segment paths are supported for procedure routing.");
    }
    println!("procedure_name: {}", procedure_name);
    Ok(procedure_name.to_string())
}

async fn module_procedure_response(
    config: &RootDomainRouteConfig,
    module_name: &str,
    procedure_name: &str,
) -> Response {
    let Some(ctx) = config.ctx.as_ref() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Root-domain module routing context is unavailable.",
        )
            .into_response();
    };

    let database_identity = match ctx.lookup_database_identity(module_name).await {
        Ok(Some(identity)) => identity,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, format!("Module `{module_name}` not found.")).into_response();
        }
        Err(err) => {
            log::error!("Failed to resolve module `{module_name}`: {err:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to resolve module for root-domain routing.",
            )
                .into_response();
        }
    };

    let database = match ctx.get_database_by_identity(&database_identity).await {
        Ok(Some(database)) => database,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, format!("Module `{module_name}` not found.")).into_response();
        }
        Err(err) => {
            log::error!("Failed to load database for module `{module_name}`: {err:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load database for root-domain routing.",
            )
                .into_response();
        }
    };

    let leader = match ctx.leader(database.id).await {
        Ok(leader) => leader,
        Err(err) => {
            let status = match err {
                crate::GetLeaderHostError::NoSuchDatabase | crate::GetLeaderHostError::NoSuchReplica => {
                    StatusCode::NOT_FOUND
                }
                crate::GetLeaderHostError::LaunchError { .. } | crate::GetLeaderHostError::Control { .. } => {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            };
            return (status, err.to_string()).into_response();
        }
    };

    let module = match leader.module().await {
        Ok(module) => module,
        Err(err) => {
            log::error!("Failed to load module host for `{module_name}`: {err:#}");
            return (StatusCode::NOT_FOUND, format!("Module `{module_name}` not found.")).into_response();
        }
    };

    if module.info().module_def.procedure(procedure_name).is_none() {
        return (
            StatusCode::NOT_FOUND,
            format!("Procedure `{procedure_name}` not found in module `{module_name}`."),
        )
            .into_response();
    }

    let caller_identity = Identity::from_claims(spacetimedb_client_api::auth::LOCALHOST, SUBDOMAIN_CALLER_SUBJECT);
    match module
        .call_procedure(caller_identity, None, None, procedure_name, FunctionArgs::Nullary)
        .await
        .result
    {
        Ok(result) => procedure_result_response(result.return_val),
        Err(err) => procedure_error_response(module_name, procedure_name, err),
    }
}

fn procedure_result_response(return_val: AlgebraicValue) -> Response {
    match return_val {
        AlgebraicValue::String(body) => Html(body.to_string()).into_response(),
        value => (StatusCode::OK, axum::Json(sats::serde::SerdeWrapper(value))).into_response(),
    }
}

fn procedure_error_response(module_name: &str, procedure_name: &str, err: ProcedureCallError) -> Response {
    match err {
        ProcedureCallError::NoSuchProcedure => (
            StatusCode::NOT_FOUND,
            format!("Procedure `{procedure_name}` not found in module `{module_name}`."),
        )
            .into_response(),
        ProcedureCallError::NoSuchModule(_) => {
            (StatusCode::NOT_FOUND, format!("Module `{module_name}` not found.")).into_response()
        }
        ProcedureCallError::Args(err) => {
            (StatusCode::BAD_REQUEST, format!("{:#}", anyhow::anyhow!(err))).into_response()
        }
        ProcedureCallError::OutOfEnergy => (
            StatusCode::PAYMENT_REQUIRED,
            "Procedure terminated due to insufficient budget.",
        )
            .into_response(),
        ProcedureCallError::InternalError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

pub async fn exec(args: &ArgMatches, db_cores: JobCores) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let pg_port = args.get_one::<u16>("pg_port");
    let non_interactive = args.get_flag("non_interactive");
    let root_domain = args
        .get_one::<String>("root_domain")
        .map(|domain| normalize_root_domain(domain))
        .transpose()?;
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
            v8_heap_policy: config.common.v8_heap_policy,
        },
        &certs,
        data_dir,
        db_cores,
    )
    .await?;
    worker_metrics::spawn_jemalloc_stats(listen_addr.clone());
    worker_metrics::spawn_tokio_stats(
        listen_addr.clone(),
        "main".to_string(),
        tokio::runtime::Handle::current(),
    );
    worker_metrics::spawn_page_pool_stats(listen_addr.clone(), ctx.page_pool().clone());
    worker_metrics::spawn_bsatn_rlb_pool_stats(listen_addr.clone(), ctx.bsatn_rlb_pool().clone());
    let mut db_routes = DatabaseRoutes::default();
    db_routes.root_post = db_routes.root_post.layer(DefaultBodyLimit::disable());
    db_routes.db_put = db_routes.db_put.layer(DefaultBodyLimit::disable());
    db_routes.pre_publish = db_routes.pre_publish.layer(DefaultBodyLimit::disable());
    let extra = axum::Router::new().nest("/health", spacetimedb_client_api::routes::health::router());
    let mut service = router(&ctx, db_routes, IdentityRoutes::default(), extra).with_state(ctx.clone());
    if let Some(root_domain) = root_domain {
        let config = RootDomainRouteConfig::new(root_domain, ctx.clone());
        log::info!(
            "Enabled root domain routing for {} (subdomains map to module `index` procedures)",
            config,
        );
        service = service.layer(axum::middleware::from_fn_with_state(config, root_domain_middleware));
    }

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
pub fn is_port_available(host: &str, port: u16) -> bool {
    let requested = match parse_host(host) {
        Some(r) => r,
        None => return false, // invalid host string => treat as not available
    };

    let sockets = match get_sockets_info(AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6, ProtocolFlags::TCP) {
        Ok(s) => s,
        Err(_) => return false, // if we can't inspect sockets, fail closed
    };

    for si in sockets {
        let tcp = match si.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp_si) => tcp_si,
            _ => continue,
        };

        if tcp.state != TcpState::Listen {
            continue;
        }

        if tcp.local_port != port {
            continue;
        }

        if conflicts(requested, tcp.local_addr) {
            return false;
        }
    }

    true
}

#[derive(Debug, Clone, Copy)]
enum RequestedHost {
    Localhost,
    Ip(IpAddr),
}

fn parse_host(host: &str) -> Option<RequestedHost> {
    let host = host.trim();

    // Allow common bracketed IPv6 formats like "[::1]"
    let host = host.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(host);

    if host.eq_ignore_ascii_case("localhost") {
        return Some(RequestedHost::Localhost);
    }

    host.parse::<IpAddr>().ok().map(RequestedHost::Ip)
}

fn conflicts(requested: RequestedHost, listener_addr: IpAddr) -> bool {
    match requested {
        RequestedHost::Localhost => match listener_addr {
            // localhost should conflict with loopback and wildcards in each family
            IpAddr::V4(v4) => v4.is_loopback() || v4.is_unspecified(),
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        },

        RequestedHost::Ip(IpAddr::V4(req_v4)) => match listener_addr {
            IpAddr::V4(l_v4) => {
                if req_v4.is_unspecified() {
                    // 0.0.0.0 conflicts with any IPv4 listener
                    true
                } else if req_v4.is_loopback() {
                    // 127.0.0.1 conflicts with 127.0.0.1 and 0.0.0.0
                    l_v4 == req_v4 || l_v4.is_unspecified()
                } else {
                    // specific IPv4 conflicts with that IPv4 and 0.0.0.0
                    l_v4 == req_v4 || l_v4.is_unspecified()
                }
            }
            IpAddr::V6(l_v6) => {
                if req_v4.is_unspecified() {
                    // special case: 0.0.0.0 conflicts with :: (and vice versa)
                    l_v6.is_unspecified()
                } else if req_v4.is_loopback() {
                    // special case: 127.0.0.1 conflicts with ::1 (and vice versa)
                    l_v6.is_loopback()
                        // and treat IPv6 wildcard as conflicting with IPv4 loopback per your table
                        || l_v6.is_unspecified()
                        // also consider rare IPv4-mapped IPv6 listeners
                        || l_v6.to_ipv4_mapped() == Some(req_v4)
                } else {
                    // specific IPv4 should conflict with IPv6 wildcard (::) per your table
                    l_v6.is_unspecified() || l_v6.to_ipv4_mapped() == Some(req_v4)
                }
            }
        },

        RequestedHost::Ip(IpAddr::V6(req_v6)) => match listener_addr {
            IpAddr::V6(l_v6) => {
                if req_v6.is_unspecified() {
                    // :: conflicts with any IPv6 listener
                    true
                } else if req_v6.is_loopback() {
                    // ::1 conflicts with ::1 and :: (and also with 127.0.0.1 via IPv4 branch below)
                    l_v6 == req_v6 || l_v6.is_unspecified()
                } else {
                    // specific IPv6 conflicts with itself and ::
                    l_v6 == req_v6 || l_v6.is_unspecified()
                }
            }
            IpAddr::V4(l_v4) => {
                if req_v6.is_unspecified() {
                    // :: conflicts with any IPv4 listener (matches your table)
                    true
                } else if req_v6.is_loopback() {
                    // special case: ::1 conflicts with 127.0.0.1 (and vice versa)
                    l_v4.is_loopback()
                } else {
                    // Not required by your rules: specific IPv6 does NOT conflict with IPv4 listeners.
                    false
                }
            }
        },
    }
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

            [v8-heap-policy]
            heap-check-request-interval = 0
            heap-check-time-interval = "45s"
            heap-gc-trigger-fraction = 0.6
            heap-retire-fraction = 0.8
            heap-limit-mb = 128
"#;

        let config: ConfigFile = toml::from_str(toml).unwrap();

        // `spacetimedb::config::ConfigFile` doesn't implement `PartialEq`,
        // so check `common` in a pedestrian way.
        assert_eq!(&config.common.logs.directives, &["banana_shake=strawberry"]);
        assert!(config.common.certificate_authority.is_none());
        assert_eq!(config.common.v8_heap_policy.heap_check_request_interval, None);
        assert_eq!(
            config.common.v8_heap_policy.heap_check_time_interval,
            Some(Duration::from_secs(45))
        );
        assert_eq!(config.common.v8_heap_policy.heap_gc_trigger_fraction, 0.6);
        assert_eq!(config.common.v8_heap_policy.heap_retire_fraction, 0.8);
        assert_eq!(config.common.v8_heap_policy.heap_limit_bytes, Some(128 * 1024 * 1024));

        assert_eq!(
            config.websocket,
            WebSocketOptions {
                idle_timeout: Duration::from_secs(60),
                close_handshake_timeout: Duration::from_millis(500),
                ..<_>::default()
            }
        );
    }

    #[test]
    fn normalize_root_domain_trims_and_lowercases() {
        let root_domain = normalize_root_domain("  ExAmPle.Com. ").unwrap();
        assert_eq!(root_domain, "example.com");
    }

    #[test]
    fn normalize_root_domain_rejects_empty() {
        assert!(normalize_root_domain(" . ").is_err());
    }

    #[test]
    fn root_domain_matching_supports_host_ports() {
        let config = RootDomainRouteConfig::for_tests("example.com".to_string());
        let host = normalize_host_header_value("my-module.example.com:3000").unwrap();
        assert_eq!(
            config.classify_host(&host),
            RootDomainHostMatch::Module("my-module".to_string())
        );
    }

    #[test]
    fn root_domain_matching_extracts_any_single_subdomain_as_module() {
        let config = RootDomainRouteConfig::for_tests("example.com".to_string());
        let host = normalize_host_header_value("another-module.example.com").unwrap();
        assert_eq!(
            config.classify_host(&host),
            RootDomainHostMatch::Module("another-module".to_string())
        );
    }

    #[test]
    fn root_domain_matching_rejects_nested_subdomains() {
        let config = RootDomainRouteConfig::for_tests("example.com".to_string());
        let host = normalize_host_header_value("a.b.example.com").unwrap();
        assert_eq!(config.classify_host(&host), RootDomainHostMatch::InvalidSubdomain);
    }

    #[test]
    fn root_domain_matching_ignores_non_subdomain_hosts() {
        let config = RootDomainRouteConfig::for_tests("example.com".to_string());
        let host = normalize_host_header_value("localhost:3000").unwrap();
        assert_eq!(config.classify_host(&host), RootDomainHostMatch::NoMatch);
    }

    #[test]
    fn procedure_name_from_path_root_maps_to_index() {
        assert_eq!(procedure_name_from_path("/").unwrap(), INDEX_PROCEDURE);
    }

    #[test]
    fn procedure_name_from_path_single_segment_maps_to_procedure() {
        assert_eq!(procedure_name_from_path("/foo").unwrap(), "foo");
        assert_eq!(procedure_name_from_path("/foo/").unwrap(), "foo");
    }

    #[test]
    fn procedure_name_from_path_rejects_nested_paths() {
        assert!(procedure_name_from_path("/foo/bar").is_err());
    }
}
