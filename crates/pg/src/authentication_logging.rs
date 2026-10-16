use std::fmt::Display;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthenticationFailureKind {
    InvalidCredentials,
    IdentityProvider,
    Internal,
}

pub(crate) fn log_authentication_failure(database: &str, kind: AuthenticationFailureKind, err: &impl Display) {
    match kind {
        AuthenticationFailureKind::InvalidCredentials => {
            log::warn!("PG: Authentication failed on database {database}: {err}");
        }
        AuthenticationFailureKind::IdentityProvider => {
            log::error!("PG: Identity provider failed while authenticating to database {database}: {err}");
        }
        AuthenticationFailureKind::Internal => {
            log::error!("PG: Internal authentication failure on database {database}: {err}");
        }
    }
}

#[cfg(test)]
fn log_authentication_failure_with_supplied_token(
    database: &str,
    _supplied_token: &str,
    kind: AuthenticationFailureKind,
    err: &impl Display,
) {
    log_authentication_failure(database, kind, err);
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::{Level, LevelFilter, Log, Metadata, Record};
    use std::sync::{Mutex, Once};

    const SYNTHETIC_TOKEN: &str = "SYNTHETIC_PG_AUTH_TOKEN_DO_NOT_LOG_5696";

    #[derive(Clone, Debug)]
    struct CapturedRecord {
        level: Level,
        target: String,
        message: String,
    }

    struct CapturingLogger {
        records: Mutex<Vec<CapturedRecord>>,
    }

    impl Log for CapturingLogger {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &Record<'_>) {
            self.records
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(CapturedRecord {
                    level: record.level(),
                    target: record.target().to_owned(),
                    message: record.args().to_string(),
                });
        }

        fn flush(&self) {}
    }

    static LOGGER: CapturingLogger = CapturingLogger {
        records: Mutex::new(Vec::new()),
    };
    static INSTALL_LOGGER: Once = Once::new();
    static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

    fn capture_authentication_failure(kind: AuthenticationFailureKind) -> CapturedRecord {
        let _capture_guard = CAPTURE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        INSTALL_LOGGER.call_once(|| {
            log::set_logger(&LOGGER).expect("test logger should only be installed once");
            log::set_max_level(LevelFilter::Trace);
        });

        let mut records = LOGGER.records.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        records.clear();
        drop(records);

        log_authentication_failure_with_supplied_token(
            "synthetic-database",
            SYNTHETIC_TOKEN,
            kind,
            &"controlled authentication failure",
        );

        let records = LOGGER.records.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(records.len(), 1, "expected exactly one authentication log record");
        records[0].clone()
    }

    fn assert_token_absent(record: &CapturedRecord) {
        assert!(!record.message.contains(SYNTHETIC_TOKEN), "token leaked in log message");
        assert!(!record.target.contains(SYNTHETIC_TOKEN), "token leaked in log target");
    }

    #[test]
    fn invalid_credentials_are_warned_without_logging_the_token() {
        let record = capture_authentication_failure(AuthenticationFailureKind::InvalidCredentials);

        assert_eq!(record.level, Level::Warn);
        assert_token_absent(&record);
    }

    #[test]
    fn identity_provider_failures_are_errors_without_logging_the_token() {
        let record = capture_authentication_failure(AuthenticationFailureKind::IdentityProvider);

        assert_eq!(record.level, Level::Error);
        assert_token_absent(&record);
    }

    #[test]
    fn internal_authentication_failures_are_errors_without_logging_the_token() {
        let record = capture_authentication_failure(AuthenticationFailureKind::Internal);

        assert_eq!(record.level, Level::Error);
        assert_token_absent(&record);
    }
}
