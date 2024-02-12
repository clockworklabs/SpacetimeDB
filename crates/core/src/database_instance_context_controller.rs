use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use once_cell::sync::OnceCell;

use crate::db::db_metrics::DB_METRICS;
use crate::host::scheduler::Scheduler;

use super::database_instance_context::DatabaseInstanceContext;

/// The database state managed by [`DatabaseInstanceContextController`].
///
/// Morally, this holds a pair of [`DatabaseInstanceContext`] and [`Scheduler`].
/// The former holds metadata and state of a running database instance, while
/// the latter manages scheduled reducers.
///
/// Operationally, this pair is wrapped in a [`OnceCell`], which ensures that
/// the same physical database instance is not initialized multiple times due
/// to concurrent access to the controller. Note that this is not primarily for
/// safety -- databases acquire filesystem locks which ensure singleton access --
/// but to prevent unhelpful errors from propagating up, and potential retries
/// from consuming resources.
///
/// Lastly, the [`OnceCell`] is wrapped in an [`Arc`] pointer, which allows
/// initialization to proceed without holding the lock on the controller's
/// internal map.
type Context = Arc<OnceCell<(Arc<DatabaseInstanceContext>, Scheduler)>>;

#[derive(Default)]
pub struct DatabaseInstanceContextController {
    contexts: Mutex<HashMap<u64, Context>>,
}

impl DatabaseInstanceContextController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the database instance state if it is already initialized.
    ///
    /// Returns `None` if [`Self::get_or_try_init`] has not been called for the
    /// given instance id before, or the state was explicitly [`Self::remove`]d.
    #[tracing::instrument(skip_all)]
    pub fn get(&self, database_instance_id: u64) -> Option<(Arc<DatabaseInstanceContext>, Scheduler)> {
        let contexts = self.contexts.lock().unwrap();
        contexts
            .get(&database_instance_id)
            .and_then(|cell| cell.get())
            .map(|(dbic, scheduler)| (dbic.clone(), scheduler.clone()))
    }

    /// Get the database instance state, or initialize it if it is not present.
    ///
    /// If the instance is not initialized yet, this method will block until
    /// `F` returns. It will, however, release internal locks so calls to other
    /// methods will not block.
    ///
    /// After this method returns, the instance state becomes managed by the
    /// controller until it is removed by calling [`Self::remove`].
    ///
    /// Note that [`Self::remove`] must be called eventually, even if this
    /// method returns an `Err` result: in this case, [`Self::get`] returns
    /// `None` (as one would expect), but the given `database_instance_id` is
    /// nevertheless known to the controller.
    #[tracing::instrument(skip_all)]
    pub fn get_or_try_init<F, E>(
        &self,
        database_instance_id: u64,
        f: F,
    ) -> Result<(Arc<DatabaseInstanceContext>, Scheduler), E>
    where
        F: FnOnce() -> Result<(DatabaseInstanceContext, Scheduler), E>,
    {
        let cell = {
            let mut guard = self.contexts.lock().unwrap();
            let cell = guard
                .entry(database_instance_id)
                .or_insert_with(|| Arc::new(OnceCell::new()));
            Arc::clone(cell)
        };
        cell.get_or_try_init(|| {
            let (dbic, scheduler) = f()?;
            Ok((Arc::new(dbic), scheduler))
        })
        .map(|(dbic, scheduler)| (dbic.clone(), scheduler.clone()))
    }

    /// Remove and return the state corresponding to `database_instance_id`.
    ///
    /// Returns `None` if either the state is not known, or was not properly
    /// initialized (i.e. [`Self::get_or_try_init`] returned an error).
    ///
    /// This method may block if the instance state is currently being
    /// initialized via [`Self::get_or_try_init`].
    #[tracing::instrument(skip_all)]
    pub fn remove(&self, database_instance_id: u64) -> Option<(Arc<DatabaseInstanceContext>, Scheduler)> {
        let mut contexts = self.contexts.lock().unwrap();
        let arc = contexts.remove(&database_instance_id)?;
        match Arc::try_unwrap(arc) {
            Ok(cell) => cell.into_inner(),
            Err(arc) => {
                // If the `Arc`'s refcount is > 1, another thread is currently
                // executing the `get_or_try_init` closure. Wait until it
                // completes (instead of returning `None`), as callers rely on
                // calling `scheduler.clear()`.
                let (dbic, scheduler) = arc.wait();
                Some((dbic.clone(), scheduler.clone()))
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn update_metrics(&self) {
        for cell in self.contexts.lock().unwrap().values() {
            if let Some((db, _)) = cell.get() {
                // Use the previous gauge value if there is an issue getting the file size.
                if let Ok(num_bytes) = db.message_log_size_on_disk() {
                    DB_METRICS
                        .message_log_size
                        .with_label_values(&db.address)
                        .set(num_bytes as i64);
                }
                // Use the previous gauge value if there is an issue getting the file size.
                if let Ok(num_bytes) = db.object_db_size_on_disk() {
                    DB_METRICS
                        .object_db_disk_usage
                        .with_label_values(&db.address)
                        .set(num_bytes as i64);
                }
                // Use the previous gauge value if there is an issue getting the file size.
                if let Ok(num_bytes) = db.log_file_size() {
                    DB_METRICS
                        .module_log_file_size
                        .with_label_values(&db.address)
                        .set(num_bytes as i64);
                }
            }
        }
    }
}
