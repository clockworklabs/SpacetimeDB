use std::net::TcpListener;
use std::sync::Arc;
use clap::{Arg, ArgMatches};
use spacetimedb::{startup, worker_metrics};
use spacetimedb::db::db_metrics;
use crate::routes::router;
use crate::{Config, StandaloneEnv};

pub fn cli(is_standalone: bool) -> clap::Command {
    clap::Command::new("start")
        .about("Starts a standalone SpacetimeDB instance")
        .arg(
            Arg::new("advertise_addr")
                .long("advertise-addr")
                .short('a')
                .required(false)
                .help("The control node address where this node should be advertised")
        ).arg(
            Arg::new("listen_addr")
                .long("listen-addr")
                .short('l')
                .required(false)
                .default_value(Config::DEFAULT_ADDR)
                .help("The address and port where SpacetimeDB should listen for connections"),
        )
        .after_help(if is_standalone
        {
            "Run `spacetimedb help start` for more detailed information."
        }
        else
        {
            "Run `spacetime help start` for more information."
        })
}

pub async fn exec(args: &ArgMatches) -> anyhow::Result<()> {
    let listen_addr = args.get_one::<String>("listen_addr").unwrap();
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
