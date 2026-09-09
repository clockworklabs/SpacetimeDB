//! Progress metadata is separate from the protected immutable submission body.
//! Only that owner-readable body contains the complete resolved environment.
mod protected;
use super::client::{ObjectRef, UploadKind, UploadStatus};
use crate::container::{oci, PreparedContainer};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spacetimedb_lib::{
    container::OciDigest,
    deployment::{api::*, PUBLISH_PROTOCOL_VERSION},
    Identity, Uuid,
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

const MAX_RECORD_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadRecord {
    pub kind: UploadKind,
    pub object: ObjectRef,
    pub session: Option<UploadStatus>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub version: u32,
    pub server: String,
    pub artifact_endpoint: String,
    pub publisher: Identity,
    pub database: Option<Identity>,
    pub reservation: Option<ReserveDatabaseRequest>,
    pub requested_name: Option<String>,
    /// UTF-8 JSON sent verbatim on every submission attempt.
    #[serde(skip)]
    pub request_json: String,
    /// Private local integrity check only; never included in a wire request.
    pub request_digest: OciDigest,
    pub uploads: Vec<UploadRecord>,
    pub submitted: bool,
    pub status: Option<PublicationStatus>,
    pub name_confirmed: bool,
    pub naming_attempted: bool,
}
impl Record {
    pub fn request(&self) -> Result<PublishRequest> {
        ensure!(
            self.version == 2 && self.request_json.len() <= MAX_PUBLISH_REQUEST_BYTES,
            "unsupported or oversized publication resume record"
        );
        ensure!(
            spacetimedb_oci::sha256(self.request_json.as_bytes()) == self.request_digest,
            "publication request bytes changed"
        );
        let request = PublishRequest::decode(self.request_json.as_bytes())?;
        let envelope = &request.manifest.current().envelope;
        ensure!(
            envelope.version == PUBLISH_PROTOCOL_VERSION,
            "unsupported publication protocol"
        );
        ensure!(
            envelope.operation_id.get_version() == Some(spacetimedb_lib::sats::uuid::Version::V7),
            "publication operation must be UUIDv7"
        );
        ensure!(self.uploads.len() <= 259, "too many retained publication objects");
        if let Some(reservation) = &self.reservation {
            ensure!(
                reservation.operation_id == envelope.operation_id
                    && reservation.version == PUBLISH_PROTOCOL_VERSION
                    && request.creation.as_ref() == Some(&reservation.options),
                "reservation differs from the immutable publication request"
            );
        } else {
            ensure!(
                self.database.is_some() && request.creation.is_none(),
                "publication database binding is missing"
            );
        }
        for upload in &self.uploads {
            ensure!(upload.object.size > 0, "empty publication artifact");
            if let Some(status) = &upload.session {
                status.validate(upload.object, None)?;
            }
        }
        if let Some(status) = &self.status {
            self.check_status(&request, status)?;
        }
        Ok(request)
    }
    pub fn check_status(&self, request: &PublishRequest, status: &PublicationStatus) -> Result<()> {
        let envelope = &request.manifest.current().envelope;
        ensure!(
            Some(status.database_identity) == self.database
                && status.operation_id == envelope.operation_id
                && status.expected_revision == envelope.expected_revision
                && status.expected_last_operation == envelope.expected_last_operation
                && status.publication_epoch != 0
                && self
                    .status
                    .as_ref()
                    .is_none_or(|previous| previous.publication_epoch == status.publication_epoch)
                && status.proposed_revision == request.manifest.current().deployment.revision()?,
            "publication response belongs to another operation or deployment"
        );
        Ok(())
    }
}

pub struct Journal {
    directory: PathBuf,
    _parents: Vec<File>,
    _lock: File,
    pub record: Record,
}
struct IncompleteDirectory(Option<PathBuf>);
impl Drop for IncompleteDirectory {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}
impl Journal {
    pub fn create(
        base: &Path,
        record: Record,
        image: Option<PreparedContainer>,
        module: Option<&[u8]>,
    ) -> Result<Self> {
        let request = record.request()?;
        let has_image = image.is_some();
        let directory = base.join(request.manifest.current().envelope.operation_id.to_string());
        fs::create_dir_all(base)?;
        let _base_parents = protected::pin_parents(base)?;
        protected::create_directory(&directory)
            .context("cannot create protected publication directory; resume an existing operation instead")?;
        let mut incomplete = IncompleteDirectory(Some(directory.clone()));
        let parents = protected::pin_parents(&directory)?;
        let lock = Self::lock(&directory, true)?;
        let mut submission = protected::file(&directory.join("submission.json"), true, true)?;
        submission.write_all(record.request_json.as_bytes())?;
        submission.sync_all()?;
        if let Some(image) = image {
            image.persist(&directory.join("image"))?;
        }
        if let Some(module) = module {
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(directory.join("module.blob"))?;
            file.write_all(module)?;
            file.sync_all()?;
        }
        let journal = Self {
            directory,
            _parents: parents,
            _lock: lock,
            record,
        };
        journal.sync_retained_artifacts(has_image)?;
        journal.save()?;
        sync_directory_chain(&journal.directory)?;
        incomplete.0 = None;
        Ok(journal)
    }
    pub fn open(directory: &Path) -> Result<Self> {
        let parents = protected::pin_parents(directory)?;
        protected::directory(directory)?;
        let directory = directory
            .canonicalize()
            .context("publication resume directory not found")?;
        let lock = Self::lock(&directory, false)?;
        let path = directory.join("publication.json");
        let mut bytes = vec![];
        protected::file(&path, false, false)?
            .take(MAX_RECORD_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_RECORD_BYTES,
            "publication resume record is too large"
        );
        let mut record: Record =
            serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid publication progress record"))?;
        record.request_json = String::from_utf8(read_submission(&directory)?)
            .map_err(|_| anyhow::anyhow!("invalid protected publication body"))?;
        let request = record.request()?;
        ensure!(
            directory.file_name().and_then(|name| name.to_str())
                == Some(request.manifest.current().envelope.operation_id.to_string().as_str()),
            "publication directory belongs to another operation"
        );
        // Creation saves JSON only after artifact flush. Reconfirm the parent
        // link if its creator stopped before finishing that last barrier.
        sync_directory_chain(&directory)?;
        Ok(Self {
            directory,
            _parents: parents,
            _lock: lock,
            record,
        })
    }
    fn sync_retained_artifacts(&self, has_image: bool) -> Result<()> {
        let mut files = self
            .record
            .uploads
            .iter()
            .map(|upload| self.object_path(upload))
            .collect::<Vec<_>>();
        if has_image {
            files.extend(
                ["oci-layout", "index.json", "prepared.json"].map(|name| self.directory.join("image").join(name)),
            );
        }
        let mut directories = std::collections::BTreeSet::new();
        for path in files {
            ensure!(
                fs::symlink_metadata(&path)?.is_file(),
                "retained publication artifact must be a regular file"
            );
            // Write access also lets Windows flush an owned immutable artifact.
            File::options().read(true).write(true).open(&path)?.sync_all()?;
            let mut parent = path.parent();
            while let Some(path) = parent {
                directories.insert(path.to_owned());
                if path == self.directory {
                    break;
                }
                parent = path.parent();
            }
        }
        // Flush children before the directories that link them.
        for directory in directories.into_iter().rev() {
            sync_directory(&directory)?;
        }
        Ok(())
    }
    fn lock(directory: &Path, create: bool) -> Result<File> {
        protected::directory(directory)?;
        let file = protected::file(&directory.join("publication.lock"), create, true)?;
        file.try_lock()
            .context("another process is using this publication resume directory")?;
        Ok(file)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn operation(&self) -> Result<Uuid> {
        Ok(self.record.request()?.manifest.current().envelope.operation_id)
    }
    /// Check the on-disk body even on status-only recovery: missing or altered
    /// retained input must never silently become an empty environment.
    pub fn submission_bytes(&self) -> Result<Vec<u8>> {
        self.record.request()?;
        let bytes = read_submission(&self.directory)?;
        ensure!(
            bytes == self.record.request_json.as_bytes(),
            "protected publication body changed"
        );
        Ok(bytes)
    }
    pub fn save(&self) -> Result<()> {
        self.submission_bytes()?;
        let bytes = serde_json::to_vec_pretty(&self.record)?;
        ensure!(
            bytes.len() <= MAX_RECORD_BYTES,
            "publication resume record is too large"
        );
        #[cfg(not(windows))]
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        #[cfg(windows)]
        let mut file = protected::temporary(&self.directory)?;
        protected::verify_file(file.as_file())?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(self.directory.join("publication.json"))?;
        #[cfg(unix)]
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
    pub fn object_path(&self, upload: &UploadRecord) -> PathBuf {
        match upload.kind {
            UploadKind::Module => self.directory.join("module.blob"),
            _ => self.directory.join("image/blobs/sha256").join(
                upload
                    .object
                    .digest
                    .to_string()
                    .strip_prefix("sha256:")
                    .expect("SHA256"),
            ),
        }
    }
    /// Reopening state never trusts stale local object bytes. This repeats only
    /// their digest check; server preparation remains the admission authority.
    pub async fn verify_objects(&self, cancel: CancellationToken) -> Result<()> {
        let permit = crate::container::VERIFIERS
            .clone()
            .try_acquire_owned()
            .context("two OCI verifications are already running")?;
        let objects = self
            .record
            .uploads
            .iter()
            .map(|upload| (self.object_path(upload), upload.object))
            .collect::<Vec<_>>();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let deadline = Instant::now() + crate::container::VERIFY_TIMEOUT;
            for (path, object) in objects {
                oci::check(&cancel, deadline)?;
                ensure!(
                    fs::symlink_metadata(&path)?.is_file(),
                    "retained publication artifact must be a regular file"
                );
                let mut file = File::open(path)?;
                ensure!(file.metadata()?.len() == object.size, "retained artifact size changed");
                let mut hash = Sha256::new();
                let mut total = 0u64;
                let mut buffer = [0u8; 64 * 1024];
                loop {
                    oci::check(&cancel, deadline)?;
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    total = total.checked_add(read as u64).context("artifact size overflow")?;
                    ensure!(total <= object.size, "retained artifact grew");
                    hash.update(&buffer[..read]);
                }
                ensure!(
                    total == object.size && OciDigest::sha256(hash.finalize().into()) == object.digest,
                    "retained publication artifact digest changed"
                );
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("publication verification worker stopped")??;
        Ok(())
    }
}

fn sync_directory(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    // As in paths::utils::write_atomic, Windows directory handles cannot be
    // synced through std. File flushes still precede publishing the journal.
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}
fn sync_directory_chain(directory: &Path) -> Result<()> {
    let canonical = directory.canonicalize()?;
    for ancestor in canonical.ancestors() {
        sync_directory(ancestor)?;
    }
    Ok(())
}

fn read_submission(directory: &Path) -> Result<Vec<u8>> {
    protected::directory(directory)?;
    let mut bytes = Vec::new();
    protected::file(&directory.join("submission.json"), false, false)?
        .take(MAX_PUBLISH_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_PUBLISH_REQUEST_BYTES,
        "protected publication body exceeds its bound"
    );
    Ok(bytes)
}
