use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_tracing() {
    let timer = tracing_subscriber::fmt::time();
    let format = tracing_subscriber::fmt::format::Format::default()
        .with_timer(timer)
        .with_line_number(true)
        .with_file(true)
        .with_target(false)
        .compact();
    let fmt_layer = tracing_subscriber::fmt::Layer::default()
        .event_format(format)
        .with_writer(std::io::stderr);
    let env_filter_layer = tracing_subscriber::EnvFilter::from_default_env();

    let _ = tracing_subscriber::Registry::default()
        .with(fmt_layer)
        .with(env_filter_layer)
        .try_init();
}
