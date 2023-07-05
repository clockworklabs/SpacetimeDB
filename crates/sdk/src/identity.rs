use crate::callbacks::CallbackId;
use crate::global_connection::try_with_credential_store;
use anyhow::Result;
// TODO: impl ser/de for `Identity`, `Token`, `Credentials` so that clients can stash them
//       to disk and use them to re-connect.

#[derive(Clone, Debug, PartialEq, Eq)]
/// A unique public identifier for a client connected to a database.
pub struct Identity {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A private access token for a client connected to a database.
pub struct Token {
    pub string: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
pub fn on_connect(callback: impl FnMut(&Credentials) + Send + 'static) -> Result<ConnectCallbackId> {
    try_with_credential_store(|cred_store| cred_store.register_on_connect(callback)).map(|id| ConnectCallbackId { id })
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
pub fn once_on_connect(callback: impl FnOnce(&Credentials) + Send + 'static) -> Result<ConnectCallbackId> {
    try_with_credential_store(|cred_store| cred_store.register_on_connect_oneshot(callback))
        .map(|id| ConnectCallbackId { id })
}

/// Unregister a previously-registered `on_connect` callback.
///
/// `remove_on_connect` will return an error if called without an active database
/// connection.
///
/// If `id` does not refer to a currently-registered callback, this operation does
/// nothing.
pub fn remove_on_connect(id: ConnectCallbackId) -> Result<()> {
    try_with_credential_store(|cred_store| cred_store.unregister_on_connect(id.id))
}
