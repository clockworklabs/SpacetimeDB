use crate::routes::router;
use crate::util::{create_dir_or_err, create_file_with_contents};
use crate::StandaloneEnv;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use spacetimedb::config::{FilesGlobal, FilesLocal, SpacetimeDbFiles};
use spacetimedb::db::{Config, FsyncPolicy, Storage};
use spacetimedb::startup;
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
            ProgramMode::Standalone => "0.0.0.0:80",
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

pub fn cli(mode: ProgramMode) -> clap::Command {
    let mut log_conf_path_arg = Arg::new("log_conf_path")
        .long("log-conf-path")
        .help("The path of the file that contains the log configuration for SpacetimeDB (SPACETIMEDB_LOG_CONFIG)");
    let mut log_dir_path_arg = Arg::new("log_dir_path")
        .long("log-dir-path")
        .help("The path to the directory that should contain logs for SpacetimeDB (SPACETIMEDB_LOGS_PATH)");
    let mut database_path_arg = Arg::new("database_path")
        .help("The path to the directory that should contain the database files for SpacetimeDB (STDB_PATH)");
    let mut jwt_pub_key_path_arg = Arg::new("jwt_pub_key_path")
        .long("jwt-pub-key-path")
        .help("The path to the public jwt key for verifying identities (SPACETIMEDB_JWT_PUB_KEY)");
    let mut jwt_priv_key_path_arg = Arg::new("jwt_priv_key_path")
        .long("jwt-priv-key-path")
        .help("The path to the private jwt key for issuing identities (SPACETIMEDB_JWT_PRIV_KEY)");

    let in_memory_arg = Arg::new("in_memory")
        .long("in-memory")
        .action(SetTrue)
        .help("If specified the database will run entirely in memory. After the process exits all data will be lost.");

    let wal_fsync_arg = Arg::new("wal_fsync")
        .long("wal-fsync")
        .action(SetTrue)
        .help("If specified the database will fsync on each commit.");

    // the default root for files, this *should* be the home directory unless it cannot be determined.
    let default_root = if let Some(dir) = dirs::home_dir() {
        dir
    } else {
        println!("Warning: home directory not found, using current directory.");
        std::env::current_dir().unwrap()
    }
    .to_str()
    .unwrap()
    .to_string();

    // The CLI defaults to starting in, and getting configuration from, the user's home directory.
    // The standalone mode instead uses global directories.
    match mode {
        ProgramMode::CLI => {
            let paths = FilesLocal::hidden(default_root);

            log_conf_path_arg = log_conf_path_arg.default_value(paths.log_config().into_os_string());
            log_dir_path_arg = log_dir_path_arg.default_value(paths.logs().into_os_string());
            database_path_arg = database_path_arg.default_value(paths.db_path().into_os_string());
            jwt_pub_key_path_arg = jwt_pub_key_path_arg.default_value(paths.public_key().into_os_string());
            jwt_priv_key_path_arg = jwt_priv_key_path_arg.default_value(paths.private_key().into_os_string());
        }
        ProgramMode::Standalone => {
            let paths = FilesGlobal;

            log_conf_path_arg = log_conf_path_arg.default_value(paths.log_config().into_os_string());
            log_dir_path_arg = log_dir_path_arg.default_value(paths.logs().into_os_string());
            database_path_arg = database_path_arg.default_value(paths.db_path().into_os_string());
            jwt_pub_key_path_arg = jwt_pub_key_path_arg.default_value(paths.public_key().into_os_string());
            jwt_priv_key_path_arg = jwt_priv_key_path_arg.default_value(paths.private_key().into_os_string());
        }
    }

    clap::Command::new("start")
        .about("Starts a standalone SpacetimeDB instance")
        .long_about("Starts a standalone SpacetimeDB instance. This command recognizes the following environment variables: \
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
                .default_value(mode.listen_addr())
                .help(mode.listen_addr_help())
        )
        .arg(log_conf_path_arg)
        .arg(log_dir_path_arg)
        .arg(database_path_arg)
        .arg(
            Arg::new("enable_tracy")
                .long("enable-tracy")
                .action(SetTrue)
                .help("Enable Tracy profiling (SPACETIMEDB_TRACY)"),
        )
        .arg(jwt_pub_key_path_arg)
        .arg(jwt_priv_key_path_arg)
        .arg(in_memory_arg)
        .arg(wal_fsync_arg)
        .after_help(mode.after_help())
}

/// Sets an environment variable. Print a warning if already set.
fn set_env_with_warning(env_name: &str, env_value: &str) {
    if std::env::var(env_name).is_ok() {
        println!("Warning: {} is set in the environment, but was also passed on the command line. The value passed on the command line will be used.", env_name);
    }
    std::env::set_var(env_name, env_value);
}

/// Reads an argument from the `ArgMatches`.
///
/// If the argument is the default and the environment variable is already set,
/// then we don't want to use the default value.
/// This function will return `None` in that case.
fn read_argument<'a>(args: &'a ArgMatches, arg_name: &str, env_name: &str) -> Option<&'a String> {
    let env_is_set = std::env::var(env_name).is_ok();
    let is_default = args.value_source(arg_name) == Some(clap::parser::ValueSource::DefaultValue);

    if env_is_set && is_default {
        None
    } else {
        args.get_one::<String>(arg_name)
    }
}

pub async fn exec(args: &ArgMatches) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
    let log_conf_path = read_argument(args, "log_conf_path", "SPACETIMEDB_LOG_CONFIG");
    let log_dir_path = read_argument(args, "log_dir_path", "SPACETIMEDB_LOGS_PATH");
    let stdb_path = read_argument(args, "database_path", "STDB_PATH");
    let jwt_pub_key_path = read_argument(args, "jwt_pub_key_path", "SPACETIMEDB_JWT_PUB_KEY");
    let jwt_priv_key_path = read_argument(args, "jwt_priv_key_path", "SPACETIMEDB_JWT_PRIV_KEY");
    let enable_tracy = args.get_flag("enable_tracy");
    let storage = if args.get_flag("in_memory") {
        Storage::Memory
    } else {
        Storage::Disk
    };
    let fsync = if args.get_flag("wal_fsync") {
        FsyncPolicy::EveryTx
    } else {
        FsyncPolicy::Never
    };
    let config = Config { storage, fsync };

    banner();
    let exe_name = std::env::current_exe()?;
    let exe_name = exe_name.file_name().unwrap().to_str().unwrap();
    println!("{} version: {}", exe_name, env!("CARGO_PKG_VERSION"));
    println!("{} path: {}", exe_name, std::env::current_exe()?.display());

    if let Some(log_conf_path) = log_conf_path {
        create_file_with_contents(log_conf_path, include_str!("../../log.conf"))?;
        set_env_with_warning("SPACETIMEDB_LOG_CONFIG", log_conf_path);
    }

    if let Some(log_dir_path) = log_dir_path {
        create_dir_or_err(log_dir_path)?;
        set_env_with_warning("SPACETIMEDB_LOGS_PATH", log_dir_path);
    }

    if let Some(stdb_path) = stdb_path {
        create_dir_or_err(stdb_path)?;
        set_env_with_warning("STDB_PATH", stdb_path);
    }

    // If this doesn't exist, we will create it later, just set the env variable for now
    if let Some(jwt_pub_key_path) = jwt_pub_key_path {
        set_env_with_warning("SPACETIMEDB_JWT_PUB_KEY", jwt_pub_key_path);
    }

    // If this doesn't exist, we will create it later, just set the env variable for now
    if let Some(jwt_priv_key_path) = jwt_priv_key_path {
        set_env_with_warning("SPACETIMEDB_JWT_PRIV_KEY", jwt_priv_key_path);
    }

    if enable_tracy {
        set_env_with_warning("SPACETIMEDB_TRACY", "1");
    }

    startup::configure_tracing();

    let ctx = StandaloneEnv::init(config).await?;

    let service = router().with_state(ctx);

    let tcp = TcpListener::bind(listen_addr).await?;
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
