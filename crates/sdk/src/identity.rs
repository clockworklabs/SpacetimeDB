use crate::callbacks::CallbackId;
use crate::global_connection::with_credential_store;
use anyhow::{anyhow, Context, Result};
use spacetimedb_lib::de::Deserialize;
use spacetimedb_lib::ser::Serialize;
use spacetimedb_sats::bsatn;
// TODO: impl ser/de for `Identity`, `Token`, `Credentials` so that clients can stash them
//       to disk and use them to re-connect.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// A unique public identifier for a client connected to a database.
pub struct Identity {
    __identity_bytes: Vec<u8>,
}

impl Identity {
    /// Get a reference to the bytes of this identity.
    ///
    /// This may be useful for saving the bytes to disk in order to reconnect
    /// with the same identity, though client authors are encouraged
    /// to use the BSATN `Serialize` and `Deserialize` traits
    /// rather than saving bytes directly.
    ///
    /// Due to a current limitation in Spacetime's handling of tables which store identities,
    /// filter methods for fields defined by the module to have type `Identity`
    /// accept bytes, rather than an `Identity` structure.
    /// As such, it is necessary to do e.g.
    /// `MyTable::filter_by_identity(some_identity.bytes().to_owned())`.
    pub fn bytes(&self) -> &[u8] {
        &self.__identity_bytes
    }

    /// Construct an `Identity` containing the `bytes`.
    ///
    /// This method does not verify that `bytes` represents a valid identity.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { __identity_bytes: bytes }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// A private access token for a client connected to a database.
pub struct Token {
    pub(crate) string: String,
}

impl Token {
    /// Get a reference to the string representation of this token.
    ///
    /// This may be useful for saving the string to disk in order to reconnect
    /// with the same token, though client authors are encouraged
    /// to use the BSATN `Serialize` and `Deserialize` traits
    /// rather than saving the token string directly.
    pub fn string(&self) -> &str {
        &self.string
    }

    /// Construct a token from its string representation.
    ///
    /// This method does not verify that `string` represents a valid token.
    pub fn from_string(string: String) -> Self {
        Token { string }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Credentials, including a private access token, sufficient to authenticate a client
/// connected to a database.
pub struct Credentials {
    pub identity: Identity,
    pub token: Token,
}

#[derive(Copy, Clone)]
pub struct ConnectCallbackId {
    id: CallbackId<Credentials>,
}

/// Register a callback to be invoked upon authentication with the database.
///
/// The callback will be invoked with the `Credentials` provided by the database to
/// identify this connection. If `Credentials` were supplied to `connect`, those passed to
/// the callback will be equivalent to the ones used to connect. If the initial connection
/// was anonymous, a new set of `Credentials` will be generated by the database to
/// identify this user.
///
// TODO: What happens if you pass an inconsistent or unknown `Credentials` to `connect`?
//       Two cases:
//       - The `Token` used to connect is known, but does not match the `Identity` passed
//         to `connect`. This is undoubtedly the client's fault, as each `Token` uniquely
//         identifies a single `Identity`. The server cannot detect it, because we send
//         only the `Token`.
//       - The `Token` used to connect is unknown by the server. Presumably the server
//         detects this and refuses the connection. Find out and document - pgoldman
//         2023-06-12.
//
/// The `Credentials` passed to the callback can be saved and used to authenticate the
/// same user in future connections.
// TODO: Implement ser/de for `Credentials` to allow this.
///
/// The returned `ConnectCallbackId` can be passed to `remove_on_connect` to unregister
/// the callback.
pub fn on_connect(callback: impl FnMut(&Credentials) + Send + 'static) -> ConnectCallbackId {
    let id = with_credential_store(|cred_store| cred_store.register_on_connect(callback));
    ConnectCallbackId { id }
}

/// Register a callback to be invoked once upon authentication with the database.
///
/// The callback will be invoked with the `Credentials` provided by the database to
/// identify this connection. If `Credentials` were supplied to `connect`, those passed to
/// the callback will be equivalent to the ones used to connect. If the initial connection
/// was anonymous, a new set of `Credentials` will be generated by the database to
/// identify this user.
///
// I (pgoldman 2023-06-14) believe that a connection will fire on-connect callbacks at
// most once anyways. Should `on_connect` just be this function,
// given it has a less restrictive type?
/// The `Credentials` passed to the callback can be saved and used to authenticate the
/// same user in future connections.
///
/// The callback will be unregistered after running.
///
/// The returned `ConnectCallbackId` can be passed to `remove_on_connect` to unregister
/// the callback.
pub fn once_on_connect(callback: impl FnOnce(&Credentials) + Send + 'static) -> ConnectCallbackId {
    let id = with_credential_store(|cred_store| cred_store.register_on_connect_oneshot(callback));
    ConnectCallbackId { id }
}

/// Unregister a previously-registered `on_connect` callback.
///
/// If `id` does not refer to a currently-registered callback, this operation does
/// nothing.
pub fn remove_on_connect(id: ConnectCallbackId) {
    with_credential_store(|cred_store| cred_store.unregister_on_connect(id.id));
}

/// Read the current connection's public `Identity`.
///
/// Returns an error if:
/// - `connect` has not yet been called.
/// - We connected anonymously, and we have not yet received our credentials.
pub fn identity() -> Result<Identity> {
    with_credential_store(|cred_store| cred_store.identity().ok_or(anyhow!("Identity not yet received")))
}

/// Read the current connection's private `Token`.
///
/// Returns an error if:
/// - `connect` has not yet been called.
/// - We connected anonymously, and we have not yet received our credentials.
pub fn token() -> Result<Token> {
    with_credential_store(|cred_store| cred_store.token().ok_or(anyhow!("Token not yet received")))
}

/// Read the current connection's `Credentials`,
/// including a public `Identity` and a private `Token`.
///
/// Returns an error if:
/// - `connect` has not yet been called.
/// - We connected anonymously, and we have not yet received our credentials.
pub fn credentials() -> Result<Credentials> {
    with_credential_store(|cred_store| cred_store.credentials().ok_or(anyhow!("Credentials not yet received")))
}

const CREDS_FILE: &str = "credentials";

/// Load a saved `Credentials` from a file within `~/dirname`, if one exists.
///
/// `dirname` is treated as a directory in the user's home directory.
/// If it contains a file named `credentials`,
/// that file is treated as a BSATN-encoded `Credentials`, deserialized and returned.
///
/// Returns `Ok(None)` if the directory or the credentials file does not exist.
/// Returns `Err` when IO or deserialization fails.
pub fn load_credentials(dirname: &str) -> Result<Option<Credentials>> {
    let mut path = home::home_dir().with_context(|| "Determining user home directory to compute credentials path")?;
    path.push(dirname);
    path.push(CREDS_FILE);

    match std::fs::read(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| "Reading BSATN-encoded credentials from file")?,
        Ok(file_contents) => bsatn::from_slice::<Credentials>(&file_contents)
            .with_context(|| "Deserializing credentials")
            .map(Some),
    }
}

/// Stores a `Credentials` to a file within `~/dirname`, to be later loaded with [`load_credentials`].
///
/// `dirname` is treated as a directory in the user's home directory.
/// The directory is created if it does not already exists.
/// A file within it named `credentials` is created or replaced,
/// containing `creds` encoded as BSATN.
///
/// Returns `Err` when IO or serialization fails.
pub fn save_credentials(dirname: &str, creds: &Credentials) -> Result<()> {
    let creds_bytes = bsatn::to_vec(creds).with_context(|| "Serializing credentials")?;

    let mut path = home::home_dir().with_context(|| "Determining user home directory to compute credentials path")?;
    path.push(dirname);

    std::fs::create_dir_all(&path).with_context(|| "Creating credentials directory")?;

    path.push(CREDS_FILE);
    std::fs::write(&path, creds_bytes).with_context(|| "Writing credentials to file")
}
