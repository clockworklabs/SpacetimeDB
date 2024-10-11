use std::path::PathBuf;
use std::sync::Arc;

use crate::routes::router;
use crate::StandaloneEnv;
use anyhow::Context;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use spacetimedb::config::{CertificateAuthority, ConfigFile};
use spacetimedb::db::{Config, Storage};
use spacetimedb::startup::{self, TracingOptions};
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};
use spacetimedb_paths::server::ServerDataDir;
use spacetimedb_paths::SpacetimePaths;
use tokio::net::TcpListener;

#[cfg(feature = "string")]
impl From<std::string::String> for OsStr {
    fn from(name: std::string::String) -> Self {
        Self::from_string(name.into())
    }
}

pub enum ProgramMode {
    Standalone,
    CLI,
}

impl ProgramMode {
    /// The address mask and port to listen on
    /// based on the mode we're running the program in.
    fn listen_addr(&self) -> &'static str {
        match self {
            ProgramMode::Standalone => "0.0.0.0:3000",
            ProgramMode::CLI => "127.0.0.1:3000",
        }
    }

    /// Help string for the address mask and port option,
    /// based on the mode we're running the program in.
    fn listen_addr_help(&self) -> &'static str {
        match self {
            ProgramMode::Standalone => "The address and port where SpacetimeDB should listen for connections. This defaults to to listen on all IP addresses on port 80.",
            ProgramMode::CLI => "The address and port where SpacetimeDB should listen for connections. This defaults to local connections only on port 3000. Use an IP address or 0.0.0.0 in order to allow remote connections to SpacetimeDB.",
        }
    }

    // We still want to keep the executable name `spacetimedb` when we're executing as a standalone, but
    // we want the executable name to be `spacetime` when we're executing this from the CLI. We have to
    // pass these strings with static lifetimes so we can't do any dynamic string manipulation here.
    fn after_help(&self) -> &'static str {
        match self {
            ProgramMode::Standalone => "Run `spacetimedb help start` for more detailed information.",
            ProgramMode::CLI => "Run `spacetime help start` for more information.",
        }
    }
}

pub fn default_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap().join(if cfg!(windows) {
        "SpacetimeDB/data"
    } else {
        "spacetime/data"
    })
}

pub fn cli(mode: ProgramMode) -> clap::Command {
    let jwt_pub_key_path_arg = Arg::new("jwt_pub_key_path")
        .long("jwt-pub-key-path")
        .requires("jwt_priv_key_path")
        .help("The path to the public jwt key for verifying identities")
        .value_parser(clap::value_parser!(PubKeyPath));
    let jwt_priv_key_path_arg = Arg::new("jwt_priv_key_path")
        .long("jwt-priv-key-path")
        .requires("jwt_pub_key_path")
        .help("The path to the private jwt key for issuing identities")
        .value_parser(clap::value_parser!(PrivKeyPath));
    let mut data_dir_arg = Arg::new("data_dir")
        .long("data-dir")
        .help("The path to the data directory for the database")
        .required(true)
        .value_parser(clap::value_parser!(ServerDataDir));

    let in_memory_arg = Arg::new("in_memory")
        .long("in-memory")
        .action(SetTrue)
        .help("If specified the database will run entirely in memory. After the process exits all data will be lost.");

    match mode {
        ProgramMode::CLI => {
            data_dir_arg = data_dir_arg.required(false);
        }
        ProgramMode::Standalone => {
            data_dir_arg = data_dir_arg.required(true);
        }
    }

    clap::Command::new("start")
        .about("Starts a standalone SpacetimeDB instance")
        // .long_about("Starts a standalone SpacetimeDB instance. This command recognizes the following environment variables: \
        //         \n\tSPACETIMEDB_LOG_CONFIG: The path to the log configuration file. \
        //         \n\tSPACETIMEDB_LOGS_PATH: The path to the directory that should contain logs for SpacetimeDB. \
        //         \n\tSTDB_PATH: The path to the directory that should contain the database files for SpacetimeDB. \
        //         \n\tSPACETIMEDB_JWT_PUB_KEY: The path to the public jwt key for verifying identities. \
        //         \n\tSPACETIMEDB_JWT_PRIV_KEY: The path to the private jwt key for issuing identities. \
        //         \n\tSPACETIMEDB_TRACY: Set to 1 to enable Tracy profiling.\
        //         \n\nWarning: If you set a value on the command line, it will override the value set in the environment variable.")
        .arg(
            Arg::new("listen_addr")
                .long("listen-addr")
                .short('l')
                .default_value(mode.listen_addr())
                .help(mode.listen_addr_help()),
        )
        .arg(data_dir_arg)
        .arg(
            Arg::new("enable_tracy")
                .long("enable-tracy")
                .action(SetTrue)
                .help("Enable Tracy profiling"),
        )
        .arg(jwt_pub_key_path_arg)
        .arg(jwt_priv_key_path_arg)
        .arg(in_memory_arg)
        .after_help(mode.after_help())
}

pub async fn exec(paths: Option<&SpacetimePaths>, args: &ArgMatches) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let certs = Option::zip(
        args.get_one::<PubKeyPath>("jwt_pub_key_path").cloned(),
        args.get_one::<PrivKeyPath>("jwt_priv_key_path").cloned(),
    )
    .map(|(jwt_pub_key_path, jwt_priv_key_path)| CertificateAuthority {
        jwt_pub_key_path,
        jwt_priv_key_path,
    });
    let data_dir = args
        .get_one::<ServerDataDir>("data_dir")
        .or(paths.map(|p| &p.data_dir))
        // cli should pass Some(paths), while standalone has data-dir as a required arg
        .unwrap();
    let enable_tracy = args.get_flag("enable_tracy") || std::env::var_os("SPACETIMEDB_TRACY").is_some();
    let storage = if args.get_flag("in_memory") {
        Storage::Memory
    } else {
        Storage::Disk
    };
    let db_config = Config { storage };

    banner();
    let exe_name = std::env::current_exe()?;
    let exe_name = exe_name.file_name().unwrap().to_str().unwrap();
    println!("{} version: {}", exe_name, env!("CARGO_PKG_VERSION"));
    println!("{} path: {}", exe_name, std::env::current_exe()?.display());

    // if let Some(log_conf_path) = log_conf_path {
    //     create_file_with_contents(log_conf_path, include_str!("../../log.conf"))?;
    //     set_env_with_warning("SPACETIMEDB_LOG_CONFIG", log_conf_path);
    // }

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
        .or_else(|| paths.map(|paths| CertificateAuthority::in_cli_config_dir(&paths.cli_config_dir)))
        .context("cannot omit --jwt-{pub,priv}-key-path when those options are not specified in config.toml")?;

    let data_dir = Arc::new(data_dir.clone());
    let ctx = StandaloneEnv::init(db_config, &certs, data_dir).await?;

    let service = router(ctx);

    let tcp = TcpListener::bind(listen_addr).await?;
    socket2::SockRef::from(&tcp).set_nodelay(true)?;
    log::debug!("Starting SpacetimeDB listening on {}", tcp.local_addr().unwrap());
    axum::serve(tcp, service).await?;
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
