use std::path::Path;

pub fn configure_logging() {
    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let env = std::env::var_os("SPACETIMEDB_LOG_CONFIG");
    let log4rs_file = env.as_deref().unwrap_or_else(|| {
        let local = Path::new("log4rs.yaml");
        if local.exists() {
            local.as_ref()
        } else {
            "/etc/spacetimedb/log4rs.yaml".as_ref()
        }
    });
    log4rs::init_file(log4rs_file, Default::default()).unwrap();
}
