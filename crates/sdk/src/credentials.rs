use std::path::PathBuf;

use anyhow::{Context, Result};
use home::home_dir;
use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize, Identity};

const CREDENTIALS_DIR: &str = ".spacetimedb_client_credentials";

pub struct File {
    filename: String,
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    identity: Identity,
    token: String,
}

impl File {
    pub fn new(key: impl ToString) -> Self {
        Self {
            filename: key.to_string(),
        }
    }

    fn ensure_credentials_dir() -> Result<()> {
        let mut path = home_dir().context("Error determining user home directory as root for credentials storage")?;
        path.push(CREDENTIALS_DIR);

        std::fs::create_dir_all(&path).with_context(|| format!("Error creating credential storage directory {path:?}"))
    }

    fn path(&self) -> Result<PathBuf> {
        let mut path = home_dir().context("Error determining user home directory as root for credentials storage")?;
        path.push(CREDENTIALS_DIR);
        path.push(&self.filename);
        Ok(path)
    }

    pub fn save(self, identity: Identity, token: impl ToString) -> Result<()> {
        Self::ensure_credentials_dir()?;

        let creds = bsatn::to_vec(&Credentials {
            identity,
            token: token.to_string(),
        })
        .context("Error serializing credentials for storage in file")?;
        let path = self.path()?;
        std::fs::write(&path, &creds)
            .with_context(|| format!("Error writing BSATN-serialized credentials to file {path:?}"))?;
        Ok(())
    }

    pub fn load(self) -> Result<Option<(Identity, String)>> {
        let path = self.path()?;

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => return Ok(None),
            Err(e) => {
                return Err(e).with_context(|| format!("Error reading BSATN-serialized credentials from file {path:?}"))
            }
        };

        let creds = bsatn::from_slice::<Credentials>(&bytes).context(format!(
            "Error deserializing credentials from bytes stored in file {path:?}",
        ))?;
        Ok(Some((creds.identity, creds.token)))
    }
}
