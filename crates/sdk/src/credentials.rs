//! Utilities for saving and re-using credentials.
//!
//! Users are encouraged to import this module by name and refer to its contents by qualified path, like:
//! ```ignore
//! use spacetimedb_sdk::credentials;
//! fn credential_store() -> credentials::File {
//!     credentials::File::new("my_app")
//! }
//! ```

#[cfg(not(feature = "web"))]
mod native_mod {
    use home::home_dir;
    use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize};
    use std::path::PathBuf;
    use thiserror::Error;

    const CREDENTIALS_DIR: &str = ".spacetimedb_client_credentials";

    #[derive(Error, Debug)]
    pub enum CredentialFileError {
        #[error("Failed to determine user home directory as root for credentials storage")]
        DetermineHomeDir,
        #[error("Error creating credential storage directory {path}")]
        CreateDir {
            path: PathBuf,
            #[source]
            source: std::io::Error,
        },
        #[error("Error serializing credentials for storage in file")]
        Serialize {
            #[source]
            source: bsatn::EncodeError,
        },
        #[error("Error writing BSATN-serialized credentials to file {path}")]
        Write {
            path: PathBuf,
            #[source]
            source: std::io::Error,
        },
        #[error("Error reading BSATN-serialized credentials from file {path}")]
        Read {
            path: PathBuf,
            #[source]
            source: std::io::Error,
        },
        #[error("Error deserializing credentials from bytes stored in file {path}")]
        Deserialize {
            path: PathBuf,
            #[source]
            source: bsatn::DecodeError,
        },
    }

    /// A file on disk which stores, or can store, a JWT for authenticating with SpacetimeDB.
    ///
    /// The file does not necessarily exist or store credentials.
    /// If the credentials have been stored previously, they can be accessed with [`File::load`].
    /// New credentials can be saved to disk with [`File::save`].
    pub struct File {
        filename: String,
    }

    #[derive(Serialize, Deserialize)]
    struct Credentials {
        token: String,
    }

    impl File {
        /// Get a handle on a file which stores a SpacetimeDB [`Identity`] and its private access token.
        ///
        /// This method does not create the file or check that it exists.
        ///
        /// Distinct applications running as the same user on the same machine
        /// may share [`Identity`]/token pairs by supplying the same `key`.
        /// Users who desire distinct credentials for their application
        /// should supply a unique `key` per application.
        ///
        /// No additional namespacing is provided to tie the stored token
        /// to a particular SpacetimeDB instance or cluster.
        /// Users who intend to connect to multiple instances or clusters
        /// should use a distinct `key` per cluster.
        pub fn new(key: impl Into<String>) -> Self {
            Self { filename: key.into() }
        }

        fn determine_home_dir() -> Result<PathBuf, CredentialFileError> {
            home_dir().ok_or(CredentialFileError::DetermineHomeDir)
        }

        fn ensure_credentials_dir() -> Result<(), CredentialFileError> {
            let mut path = Self::determine_home_dir()?;
            path.push(CREDENTIALS_DIR);

            std::fs::create_dir_all(&path).map_err(|source| CredentialFileError::CreateDir { path, source })
        }

        fn path(&self) -> Result<PathBuf, CredentialFileError> {
            let mut path = Self::determine_home_dir()?;
            path.push(CREDENTIALS_DIR);
            path.push(&self.filename);
            Ok(path)
        }

        /// Store the provided `token` to disk in the file referred to by `self`.
        ///
        /// Future calls to [`Self::load`] on a `File` with the same key can retrieve the token.
        ///
        /// Expected usage is to call this from a [`super::DbConnectionBuilder::on_connect`] callback.
        ///
        /// ```ignore
        /// DbConnection::builder()
        ///   .on_connect(|_ctx, _identity, token| {
        ///       credentials::File::new("my_app").save(token).unwrap();
        /// })
        /// ```
        pub fn save(self, token: impl Into<String>) -> Result<(), CredentialFileError> {
            Self::ensure_credentials_dir()?;

            let creds = bsatn::to_vec(&Credentials { token: token.into() })
                .map_err(|source| CredentialFileError::Serialize { source })?;
            let path = self.path()?;
            std::fs::write(&path, creds).map_err(|source| CredentialFileError::Write { path, source })?;
            Ok(())
        }

        /// Load a saved token from disk in the file referred to by `self`,
        /// if they have previously been stored by [`Self::save`].
        ///
        /// Returns `Err` if I/O fails,
        /// `None` if credentials have not previously been stored,
        /// or `Some` if credentials are successfully loaded from disk.
        /// After unwrapping the `Result`, the returned `Option` can be passed to
        /// [`super::DbConnectionBuilder::with_token`].
        ///
        /// ```ignore
        /// DbConnection::builder()
        ///   .with_token(credentials::File::new("my_app").load().unwrap())
        /// ```
        pub fn load(self) -> Result<Option<String>, CredentialFileError> {
            let path = self.path()?;

            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => return Ok(None),
                Err(source) => return Err(CredentialFileError::Read { path, source }),
            };

            let creds = bsatn::from_slice::<Credentials>(&bytes)
                .map_err(|source| CredentialFileError::Deserialize { path, source })?;
            Ok(Some(creds.token))
        }
    }
}

#[cfg(feature = "web")]
mod web_mod {
    pub use gloo_storage::{LocalStorage, SessionStorage, Storage};

    pub mod cookies {
        use thiserror::Error;
        use wasm_bindgen::{JsCast, JsValue};
        use web_sys::HtmlDocument;

        #[derive(Error, Debug)]
        pub enum CookieError {
            #[error("Error reading cookies: {0:?}")]
            Get(JsValue),

            #[error("Error setting cookie `{key}`: {js_value:?}")]
            Set { key: String, js_value: JsValue },
        }

        /// A builder for constructing and setting cookies.
        pub struct Cookie {
            name: String,
            value: String,
            path: Option<String>,
            domain: Option<String>,
            max_age: Option<i32>,
            secure: bool,
            same_site: Option<SameSite>,
        }

        impl Cookie {
            /// Creates a new cookie builder with the specified name and value.
            pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
                Self {
                    name: name.into(),
                    value: value.into(),
                    path: None,
                    domain: None,
                    max_age: None,
                    secure: false,
                    same_site: None,
                }
            }

            /// Gets the value of a cookie by name.
            pub fn get(name: &str) -> Result<Option<String>, CookieError> {
                let doc = get_html_document();
                let all = doc.cookie().map_err(|e| CookieError::Get(e))?;
                for cookie in all.split(';') {
                    let cookie = cookie.trim();
                    if let Some((k, v)) = cookie.split_once('=') {
                        if k == name {
                            return Ok(Some(v.to_string()));
                        }
                    }
                }

                Ok(None)
            }

            /// Sets the `Path` attribute (e.g., "/").
            pub fn path(mut self, path: impl Into<String>) -> Self {
                self.path = Some(path.into());
                self
            }

            /// Sets the `Domain` attribute (e.g., "example.com").
            pub fn domain(mut self, domain: impl Into<String>) -> Self {
                self.domain = Some(domain.into());
                self
            }

            /// Sets the `Max-Age` attribute in seconds.
            pub fn max_age(mut self, seconds: i32) -> Self {
                self.max_age = Some(seconds);
                self
            }

            /// Toggles the `Secure` flag.
            /// Defaults to `false`.
            pub fn secure(mut self, enabled: bool) -> Self {
                self.secure = enabled;
                self
            }

            /// Sets the `SameSite` attribute (`Strict`, `Lax`, or `None`).
            pub fn same_site(mut self, same_site: SameSite) -> Self {
                self.same_site = Some(same_site);
                self
            }

            pub fn set(self) -> Result<(), CookieError> {
                let doc = get_html_document();
                let mut parts = vec![format!("{}={}", self.name, self.value)];

                if let Some(path) = self.path {
                    parts.push(format!("Path={}", path));
                }
                if let Some(domain) = self.domain {
                    parts.push(format!("Domain={}", domain));
                }
                if let Some(age) = self.max_age {
                    parts.push(format!("Max-Age={}", age));
                }
                if self.secure {
                    parts.push("Secure".into());
                }
                if let Some(same) = self.same_site {
                    parts.push(format!("SameSite={}", same.to_string()));
                }

                let cookie_str = parts.join("; ");
                doc.set_cookie(&cookie_str).map_err(|e| CookieError::Set {
                    key: self.name.clone(),
                    js_value: e,
                })
            }

            /// Deletes the cookie by setting its value to empty and `Max-Age=0`.
            pub fn delete(self) -> Result<(), CookieError> {
                self.value("").max_age(0).set()
            }

            /// Helper to override value for delete
            fn value(mut self, value: impl Into<String>) -> Self {
                self.value = value.into();
                self
            }
        }

        /// Controls the `SameSite` attribute for cookies.
        pub enum SameSite {
            Strict,
            Lax,
            None,
        }

        impl ToString for SameSite {
            fn to_string(&self) -> String {
                match self {
                    SameSite::Strict => "Strict".into(),
                    SameSite::Lax => "Lax".into(),
                    SameSite::None => "None".into(),
                }
            }
        }

        fn get_html_document() -> HtmlDocument {
            gloo_utils::document().unchecked_into::<HtmlDocument>()
        }
    }
}

#[cfg(not(feature = "web"))]
pub use native_mod::*;

#[cfg(feature = "web")]
pub use web_mod::*;
