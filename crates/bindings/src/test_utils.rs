//! Utilities for testing SpacetimeDB modules without a running host.
//!
//! Enabled via the `test-utils` feature. In your module crate:
//!
//! ```toml
//! [dev-dependencies]
//! spacetimedb = { features = ["test-utils"] }
//! ```
//!
//! Table names are registered before the test runner starts via platform init
//! sections (`.init_array` on ELF, `__mod_init_func` on Mach-O, `.CRT$XCU`
//! on Windows). No explicit setup or initialization call is needed.

use std::sync::Mutex;

use spacetimedb_lib::RawModuleDef;

#[cfg(not(target_arch = "wasm32"))]
pub use spacetimedb_test_datastore::{TestDatastore, TestDatastoreError};

/// A deterministic clock for module unit tests.
#[cfg(not(target_arch = "wasm32"))]
pub struct TestClock {
    now: std::rc::Rc<std::cell::Cell<crate::Timestamp>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl TestClock {
    /// Create a clock initialized to `timestamp`.
    pub fn new(timestamp: crate::Timestamp) -> Self {
        Self {
            now: std::rc::Rc::new(std::cell::Cell::new(timestamp)),
        }
    }

    /// Return the current test timestamp.
    pub fn now(&self) -> crate::Timestamp {
        self.now.get()
    }

    /// Set the current test timestamp.
    pub fn set(&self, timestamp: crate::Timestamp) {
        self.now.set(timestamp);
    }

    /// Advance the current test timestamp by `duration`.
    pub fn advance(&self, duration: crate::TimeDuration) {
        self.set(
            self.now()
                .checked_add(duration)
                .expect("advancing test clock overflowed Timestamp"),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for TestClock {
    fn default() -> Self {
        Self::new(crate::Timestamp::UNIX_EPOCH)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Clone for TestClock {
    fn clone(&self) -> Self {
        Self { now: self.now.clone() }
    }
}

/// A deterministic RNG seed source for module unit tests.
#[cfg(all(feature = "rand08", not(target_arch = "wasm32")))]
pub struct TestRng {
    seed: std::rc::Rc<std::cell::Cell<Option<u64>>>,
}

#[cfg(all(feature = "rand08", not(target_arch = "wasm32")))]
impl TestRng {
    /// Create a test RNG seed source initialized to `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            seed: std::rc::Rc::new(std::cell::Cell::new(Some(seed))),
        }
    }

    /// Return the optional seed used to initialize each new reducer context RNG.
    pub fn seed(&self) -> Option<u64> {
        self.seed.get()
    }

    /// Set the seed used to initialize future reducer context RNGs.
    pub fn set_seed(&self, seed: u64) {
        self.seed.set(Some(seed));
    }

    /// Clear the test seed, causing future reducer contexts to seed RNG from timestamp.
    pub fn clear_seed(&self) {
        self.seed.set(None);
    }
}

#[cfg(all(feature = "rand08", not(target_arch = "wasm32")))]
impl Default for TestRng {
    fn default() -> Self {
        Self {
            seed: std::rc::Rc::new(std::cell::Cell::new(None)),
        }
    }
}

#[cfg(all(feature = "rand08", not(target_arch = "wasm32")))]
impl Clone for TestRng {
    fn clone(&self) -> Self {
        Self {
            seed: self.seed.clone(),
        }
    }
}

/// Authentication mode for a test reducer call.
#[cfg(not(target_arch = "wasm32"))]
pub enum TestAuth {
    /// An internal reducer call with no connection and no JWT.
    Internal,
    /// An authenticated client reducer call with a validated JWT payload.
    Authenticated {
        jwt_payload: String,
        connection_id: crate::ConnectionId,
        sender: crate::Identity,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl TestAuth {
    /// Create auth for an internal reducer call.
    pub fn internal() -> Self {
        Self::Internal
    }

    /// Create auth for an authenticated reducer call from a validated JWT payload.
    pub fn from_jwt_payload(
        jwt_payload: impl Into<String>,
        connection_id: crate::ConnectionId,
    ) -> Result<Self, TestAuthError> {
        let jwt_payload = jwt_payload.into();
        let claims: serde_json::Value = serde_json::from_str(&jwt_payload).map_err(TestAuthError::InvalidPayload)?;
        let sender = validate_test_jwt_claims(&claims).map_err(TestAuthError::InvalidClaims)?;
        Ok(Self::Authenticated {
            jwt_payload,
            connection_id,
            sender,
        })
    }

    fn into_parts(
        self,
        internal_identity: crate::Identity,
    ) -> (crate::AuthCtx, Option<crate::ConnectionId>, crate::Identity) {
        match self {
            Self::Internal => (crate::AuthCtx::internal(), None, internal_identity),
            Self::Authenticated {
                jwt_payload,
                connection_id,
                sender,
            } => (
                crate::AuthCtx::from_jwt_payload(jwt_payload),
                Some(connection_id),
                sender,
            ),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn validate_test_jwt_claims(claims: &serde_json::Value) -> anyhow::Result<crate::Identity> {
    let issuer = required_claim(claims, "iss")?;
    let subject = required_claim(claims, "sub")?;

    if issuer.len() > 128 {
        anyhow::bail!("Issuer too long: {issuer:?}");
    }
    if subject.len() > 128 {
        anyhow::bail!("Subject too long: {subject:?}");
    }
    if issuer.is_empty() {
        anyhow::bail!("Issuer empty");
    }
    if subject.is_empty() {
        anyhow::bail!("Subject empty");
    }

    let computed_identity = crate::Identity::from_claims(issuer, subject);
    if let Some(token_identity) = claims.get("hex_identity") {
        let token_identity: crate::Identity = serde_json::from_value(token_identity.clone())
            .map_err(|err| anyhow::anyhow!("invalid hex_identity claim: {err}"))?;
        if token_identity != computed_identity {
            anyhow::bail!(
                "Identity mismatch: token identity {token_identity:?} does not match computed identity {computed_identity:?}",
            );
        }
    }

    Ok(computed_identity)
}

#[cfg(not(target_arch = "wasm32"))]
fn required_claim<'a>(claims: &'a serde_json::Value, name: &str) -> anyhow::Result<&'a str> {
    claims
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Missing `{name}` claim"))?
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Claim `{name}` must be a string"))
}

/// Errors returned when constructing test reducer authentication.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub enum TestAuthError {
    InvalidPayload(serde_json::Error),
    InvalidClaims(anyhow::Error),
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for TestAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPayload(error) => write!(f, "invalid JWT payload JSON: {error}"),
            Self::InvalidClaims(error) => write!(f, "invalid JWT claims: {error}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for TestAuthError {}

/// Hooks invoked at procedure transaction boundaries in native unit tests.
#[cfg(all(feature = "unstable", not(target_arch = "wasm32")))]
#[derive(Default)]
pub struct ProcedureTestHooks {
    after_tx_commit: Vec<Box<dyn FnMut(&TestContext) -> anyhow::Result<()>>>,
    on_sleep: Vec<Box<dyn FnMut(&TestContext, crate::Timestamp) -> anyhow::Result<()>>>,
}

#[cfg(all(feature = "unstable", not(target_arch = "wasm32")))]
impl ProcedureTestHooks {
    /// Create an empty hook set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a hook that runs after each successful procedure transaction commit.
    ///
    /// Hook failures panic after the transaction has already committed.
    pub fn after_tx_commit(mut self, hook: impl FnMut(&TestContext) -> anyhow::Result<()> + 'static) -> Self {
        self.after_tx_commit.push(Box::new(hook));
        self
    }

    /// Add a hook that runs when a test procedure sleeps.
    ///
    /// Hook failures panic before the procedure resumes.
    pub fn on_sleep(
        mut self,
        hook: impl FnMut(&TestContext, crate::Timestamp) -> anyhow::Result<()> + 'static,
    ) -> Self {
        self.on_sleep.push(Box::new(hook));
        self
    }

    #[doc(hidden)]
    pub fn __run_after_tx_commit(&mut self, ctx: &TestContext) {
        for hook in &mut self.after_tx_commit {
            hook(ctx).unwrap_or_else(|err| panic!("procedure test after_tx_commit hook failed: {err}"));
        }
    }

    #[doc(hidden)]
    pub fn __run_on_sleep(&mut self, ctx: &TestContext, wake_time: crate::Timestamp) {
        for hook in &mut self.on_sleep {
            hook(ctx, wake_time).unwrap_or_else(|err| panic!("procedure test on_sleep hook failed: {err}"));
        }
    }
}

/// Builder for a native unit-test procedure context.
#[cfg(all(feature = "unstable", not(target_arch = "wasm32")))]
pub struct ProcedureContextBuilder<'a> {
    test: &'a TestContext,
    auth: TestAuth,
    hooks: ProcedureTestHooks,
}

#[cfg(all(feature = "unstable", not(target_arch = "wasm32")))]
impl<'a> ProcedureContextBuilder<'a> {
    /// Install transaction hooks used by this procedure context.
    pub fn hooks(mut self, hooks: ProcedureTestHooks) -> Self {
        self.hooks = hooks;
        self
    }

    /// Build the procedure context.
    pub fn build(self) -> crate::ProcedureContext {
        self.test.procedure_context_with_hooks(self.auth, self.hooks)
    }
}

/// A native unit-test context with an in-memory module datastore.
#[cfg(not(target_arch = "wasm32"))]
pub struct TestContext {
    pub db: crate::Local,
    pub clock: TestClock,
    #[cfg(feature = "rand08")]
    pub rng: TestRng,
    pub identity: crate::Identity,
    datastore: std::sync::Arc<TestDatastore>,
    #[cfg(feature = "unstable")]
    http_responder: std::rc::Rc<std::cell::RefCell<Option<crate::http::TestHttpResponder>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Clone for TestContext {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            clock: self.clock.clone(),
            #[cfg(feature = "rand08")]
            rng: self.rng.clone(),
            identity: self.identity,
            datastore: self.datastore.clone(),
            #[cfg(feature = "unstable")]
            http_responder: self.http_responder.clone(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TestContext {
    /// Create a test context from the module definition registered in this test binary.
    pub fn new() -> Result<Self, TestDatastoreError> {
        Self::from_module_def(module_def())
    }

    /// Create a test context from `raw`.
    pub fn from_module_def(raw: RawModuleDef) -> Result<Self, TestDatastoreError> {
        let datastore = std::sync::Arc::new(TestDatastore::from_module_def(raw)?);
        Ok(Self {
            db: crate::Local::__test(datastore.clone()),
            clock: TestClock::default(),
            #[cfg(feature = "rand08")]
            rng: TestRng::default(),
            identity: crate::Identity::ZERO,
            datastore,
            #[cfg(feature = "unstable")]
            http_responder: std::rc::Rc::new(std::cell::RefCell::new(None)),
        })
    }

    /// The underlying in-memory datastore.
    pub fn datastore(&self) -> &std::sync::Arc<TestDatastore> {
        &self.datastore
    }

    /// Set the HTTP responder used by future procedure contexts created from this test context.
    #[cfg(feature = "unstable")]
    pub fn set_http_responder(
        &self,
        responder: impl Fn(&TestContext, crate::http::Request) -> Result<crate::http::Response, crate::http::Error>
            + 'static,
    ) {
        self.http_responder.borrow_mut().replace(std::rc::Rc::new(responder));
    }

    #[cfg(feature = "unstable")]
    fn http_responder(&self) -> crate::http::TestHttpResponder {
        self.http_responder.borrow().clone().unwrap_or_else(|| {
            std::rc::Rc::new(|_, _| Err(crate::http::Error::new("no test HTTP responder configured")))
        })
    }

    /// Run `body` with a reducer context backed by a single mutable transaction.
    ///
    /// The transaction commits when `body` returns `Ok`, rolls back when `body`
    /// returns `Err`, and rolls back during unwinding if `body` panics.
    pub fn with_reducer_tx<T, E>(
        &self,
        auth: TestAuth,
        body: impl FnOnce(&crate::ReducerContext) -> Result<T, E>,
    ) -> Result<T, E> {
        with_reducer_tx(
            &self.datastore,
            self.identity,
            self.clock.now(),
            #[cfg(feature = "rand08")]
            self.rng.seed(),
            auth,
            body,
        )
    }

    /// Create a procedure context backed by this test context's datastore.
    #[cfg(feature = "unstable")]
    pub fn procedure_context(&self, auth: TestAuth) -> crate::ProcedureContext {
        self.procedure_context_with_hooks(auth, ProcedureTestHooks::new())
    }

    /// Create a builder for a procedure context backed by this test context's datastore.
    #[cfg(feature = "unstable")]
    pub fn procedure_context_builder(&self, auth: TestAuth) -> ProcedureContextBuilder<'_> {
        ProcedureContextBuilder {
            test: self,
            auth,
            hooks: ProcedureTestHooks::new(),
        }
    }

    #[cfg(feature = "unstable")]
    fn procedure_context_with_hooks(&self, auth: TestAuth, hooks: ProcedureTestHooks) -> crate::ProcedureContext {
        let (auth, connection_id, sender) = auth.into_parts(self.identity);
        crate::ProcedureContext::__test(
            self.datastore.clone(),
            sender,
            auth,
            connection_id,
            self.clock.now(),
            self.identity,
            crate::http::HttpClient::test(self.clone(), self.http_responder()),
            self.clone(),
            hooks,
            #[cfg(feature = "rand08")]
            self.rng.seed(),
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn with_reducer_tx<T, E>(
    datastore: &std::sync::Arc<TestDatastore>,
    identity: crate::Identity,
    timestamp: crate::Timestamp,
    #[cfg(feature = "rand08")] rng_seed: Option<u64>,
    auth: TestAuth,
    body: impl FnOnce(&crate::ReducerContext) -> Result<T, E>,
) -> Result<T, E> {
    use core::mem;

    let test_tx = std::rc::Rc::new(datastore.begin_mut_tx());
    let rollback_tx = test_tx.clone();

    struct DoOnDrop<F: Fn()>(F);
    impl<F: Fn()> Drop for DoOnDrop<F> {
        fn drop(&mut self) {
            (self.0)();
        }
    }

    let rollback_guard = DoOnDrop(move || {
        rollback_tx
            .rollback()
            .expect("should have a pending mutable test transaction")
    });

    let (auth, connection_id, sender) = auth.into_parts(identity);
    let ctx = crate::ReducerContext::__test(
        crate::Local::__test_tx(test_tx.clone()),
        sender,
        auth,
        connection_id,
        timestamp,
        identity,
        #[cfg(feature = "rand08")]
        rng_seed,
    );

    let res = body(&ctx);
    mem::forget(rollback_guard);
    match res {
        Ok(value) => {
            test_tx
                .commit()
                .expect("committing mutable test reducer transaction failed");
            Ok(value)
        }
        Err(error) => {
            test_tx
                .rollback()
                .expect("should have a pending mutable test transaction");
            Err(error)
        }
    }
}

static TABLE_NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Returns the name of every table defined in this binary via the `#[table]`
/// macro. Names are registered before `main()` runs, so this is safe to call
/// from any test without setup.
///
/// # Example
///
/// ```rust
/// use spacetimedb::test_utils::all_table_names;
///
/// #[test]
/// fn check_tables() {
///     let names = all_table_names();
///     assert!(names.contains(&"my_table"));
/// }
/// ```
pub fn all_table_names() -> Vec<&'static str> {
    TABLE_NAMES.lock().unwrap().clone()
}

/// Returns the raw module definition registered by this native test binary.
///
/// This is the same versioned `RawModuleDef` shape that a module serializes
/// from `__describe_module__`, but it is built directly from the native
/// registrations emitted by the module macros.
pub fn module_def() -> RawModuleDef {
    crate::rt::module_def_for_tests()
}

/// Called by init functions generated by the `#[table]` macro.
/// Not intended for direct use.
#[doc(hidden)]
pub fn __register_table_name(name: &'static str) {
    TABLE_NAMES.lock().unwrap().push(name);
}
