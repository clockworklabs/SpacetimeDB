//! File Storage, end to end: uploads, downloads, files created in reducers, deletion, and restart.

use regex::Regex;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use spacetimedb_smoketests::{require_local_server, Smoketest};

const MODULE_CODE: &str = r#"
use spacetimedb::{FileId, FileRef, ReducerContext, Table, UploadRequest};

/// Every upload `allow_upload` accepted, by the ID the file will have.
#[spacetimedb::table(accessor = upload_log, public)]
pub struct UploadLog {
    #[primary_key]
    file: FileId,
    size: u64,
}

#[spacetimedb::table(accessor = attachment, public)]
pub struct Attachment {
    #[primary_key]
    #[auto_inc]
    id: u64,
    file: FileRef,
    /// The file reference as text, so the test can read it with SQL.
    key: String,
    name: String,
}

#[spacetimedb::reducer]
pub fn allow_upload(ctx: &ReducerContext, upload: UploadRequest) -> Result<(), String> {
    upload.sender.ok_or("sign in to upload")?;
    if upload.size > 1024 * 1024 {
        return Err("too large".into());
    }
    if upload.content_type.as_deref() == Some("application/x-forbidden") {
        return Err("forbidden content type".into());
    }
    ctx.db.upload_log().insert(UploadLog { file: upload.file_id, size: upload.size });
    Ok(())
}

/// Attaches every upload that hasn't been attached yet.
#[spacetimedb::reducer]
pub fn attach_uploads(ctx: &ReducerContext) {
    for upload in ctx.db.upload_log().iter() {
        if let Some(file) = ctx.files().reference(upload.file) {
            if ctx.db.attachment().iter().all(|a| a.file != file) {
                ctx.db.attachment().insert(Attachment { id: 0, file, key: file.to_string(), name: "upload".into() });
            }
        }
    }
}

#[spacetimedb::reducer]
pub fn create_note(ctx: &ReducerContext, text: String) -> Result<(), String> {
    let file = ctx.files().create(text.as_bytes(), Some("text/plain")).map_err(|e| e.to_string())?;
    let meta = ctx.files().metadata(file).ok_or("a new file has metadata")?;
    if meta.size != text.len() as u64 {
        return Err("wrong size".into());
    }
    ctx.db.attachment().insert(Attachment { id: 0, file, key: file.to_string(), name: "note".into() });
    Ok(())
}

#[spacetimedb::reducer]
pub fn copy_note(ctx: &ReducerContext) -> Result<(), String> {
    let note = ctx.db.attachment().iter().find(|a| a.name == "note").ok_or("no note")?;
    let file = ctx.files().copy(note.file).map_err(|e| e.to_string())?;
    ctx.db.attachment().insert(Attachment { id: 0, file, key: file.to_string(), name: "copy".into() });
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_named(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let row = ctx.db.attachment().iter().find(|a| a.name == name).ok_or("no such attachment")?;
    if !ctx.files().delete(row.file).map_err(|e| e.to_string())? {
        return Err("the file was already gone".into());
    }
    ctx.db.attachment().id().delete(row.id);
    Ok(())
}
"#;

struct Files<'a> {
    test: &'a Smoketest,
    client: Client,
}

impl<'a> Files<'a> {
    fn new(test: &'a Smoketest) -> Self {
        Self {
            test,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    fn url(&self, rest: &str) -> String {
        let db = self.test.database_identity.as_ref().expect("published");
        format!(
            "{}/v1/database/{db}/files{rest}",
            self.test.server_url.trim_end_matches('/')
        )
    }

    fn upload(&self, body: &[u8], content_type: &str, authorized: bool) -> reqwest::blocking::Response {
        let mut request = self
            .client
            .post(self.url(""))
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body.to_vec());
        if authorized {
            request = request.bearer_auth(self.test.read_token().unwrap());
        }
        request.send().unwrap()
    }

    /// Downloads without credentials: the reference in the URL is the authorization.
    fn download(&self, file_ref: &str, range: Option<&str>) -> reqwest::blocking::Response {
        let mut request = self.client.get(self.url(&format!("/{file_ref}")));
        if let Some(range) = range {
            request = request.header(reqwest::header::RANGE, range);
        }
        request.send().unwrap()
    }

    fn metadata(&self, file_ref: &str) -> reqwest::blocking::Response {
        self.client
            .get(self.url(&format!("/{file_ref}/metadata")))
            .send()
            .unwrap()
    }
}

/// The file reference of the attachment called `name`, read with SQL.
fn attachment_key(test: &Smoketest, name: &str) -> String {
    let output = test
        .sql_confirmed(&format!("SELECT key FROM attachment WHERE name = '{name}'"))
        .unwrap();
    let key = Regex::new("[0-9a-f]{64}").unwrap();
    key.find(&output)
        .unwrap_or_else(|| panic!("no attachment {name}: {output}"))
        .as_str()
        .to_owned()
}

fn forge(file_ref: &str) -> String {
    // Flip the last hex digit of the read secret.
    let (rest, last) = file_ref.split_at(63);
    let flipped = if last == "0" { "1" } else { "0" };
    format!("{rest}{flipped}")
}

#[test]
fn test_file_storage() {
    require_local_server!();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).build();
    let files = Files::new(&test);

    // Uploads need an identity or a token, and allow_upload's approval.
    assert_eq!(
        files.upload(b"hello", "text/plain", false).status(),
        StatusCode::UNAUTHORIZED
    );
    let rejected = files.upload(b"nope", "application/x-forbidden", true);
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    assert!(rejected.text().unwrap().contains("forbidden content type"));

    let response = files.upload(b"hello, files", "text/plain", true);
    assert_eq!(response.status(), StatusCode::CREATED);
    let header_ref = response.headers()["spacetime-file-ref"].to_str().unwrap().to_owned();
    let json: serde_json::Value = response.json().unwrap();
    let uploaded = json["file_ref"].as_str().unwrap().to_owned();
    assert_eq!(header_ref, uploaded);
    assert_eq!(json["size"], 12);

    // allow_upload's write committed, and only for the accepted upload.
    let log = test.sql("SELECT size FROM upload_log").unwrap();
    assert!(log.contains("12") && !log.contains("4"), "{log}");

    // Anyone with the reference can download, including byte ranges.
    let download = files.download(&uploaded, None);
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(download.headers()[reqwest::header::CONTENT_TYPE], "text/plain");
    assert_eq!(download.headers()[reqwest::header::CONTENT_SECURITY_POLICY], "sandbox");
    assert_eq!(download.bytes().unwrap().as_ref(), b"hello, files");
    let partial = files.download(&uploaded, Some("bytes=7-11"));
    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.headers()[reqwest::header::CONTENT_RANGE], "bytes 7-11/12");
    assert_eq!(partial.bytes().unwrap().as_ref(), b"files");
    assert_eq!(
        files.download(&uploaded, Some("bytes=12-")).status(),
        StatusCode::RANGE_NOT_SATISFIABLE
    );

    // A reference with the wrong secret finds nothing.
    assert_eq!(files.download(&forge(&uploaded), None).status(), StatusCode::NOT_FOUND);
    assert_eq!(files.metadata(&forge(&uploaded)).status(), StatusCode::NOT_FOUND);

    let metadata: serde_json::Value = files.metadata(&uploaded).json().unwrap();
    assert_eq!(metadata["size"], 12);
    assert_eq!(metadata["content_type"], "text/plain");

    // A module can resolve an uploaded file's ID, which allow_upload recorded, to its reference.
    test.call("attach_uploads", &[]).unwrap();
    assert_eq!(attachment_key(&test, "upload"), uploaded);

    // Clients can't call allow_upload themselves.
    assert!(test.call("allow_upload", &[]).is_err());

    // A reducer creates a file. Confirmed reads see it only once its bytes are persisted,
    // so it is downloadable as soon as the confirmed query returns it.
    test.call("create_note", &["\"a note from a reducer\""]).unwrap();
    let note = attachment_key(&test, "note");
    let download = files.download(&note, None);
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(download.bytes().unwrap().as_ref(), b"a note from a reducer");

    // A copy shares the bytes but has its own reference.
    test.call("copy_note", &[]).unwrap();
    let copy = attachment_key(&test, "copy");
    assert_ne!(copy, note);
    assert_eq!(
        files.download(&copy, None).bytes().unwrap().as_ref(),
        b"a note from a reducer"
    );

    // Deleting the note ends its record for modules and metadata, and leaves the copy. Its bytes
    // stay downloadable while history retains them, since reverting to before the deletion restores it.
    test.call("delete_named", &["\"note\""]).unwrap();
    test.sql_confirmed("SELECT * FROM attachment").unwrap();
    assert_eq!(files.metadata(&note).status(), StatusCode::NOT_FOUND);
    assert_eq!(files.download(&note, None).status(), StatusCode::OK);
    assert_eq!(files.download(&copy, None).status(), StatusCode::OK);
    assert!(
        test.call("delete_named", &["\"note\""]).is_err(),
        "the note is already deleted"
    );

    // The files' records are private.
    let private = test.sql("SELECT * FROM st_file");
    assert!(private.is_ok(), "the owner can read st_file: {private:?}");
    assert!(test.sql("DELETE FROM st_file").is_err());

    // Everything survives a restart.
    test.restart_server();
    let files = Files::new(&test);
    assert_eq!(
        files.download(&uploaded, None).bytes().unwrap().as_ref(),
        b"hello, files"
    );
    assert_eq!(
        files.download(&copy, None).bytes().unwrap().as_ref(),
        b"a note from a reducer"
    );
    assert_eq!(files.metadata(&note).status(), StatusCode::NOT_FOUND);
    assert_eq!(files.download(&note, None).status(), StatusCode::OK);
    // The staged bytes of the reducer's file were cleaned up once persisted.
    let staged = test.sql("SELECT content_hash FROM st_file_staged").unwrap();
    assert!(!Regex::new("[0-9]{10,}").unwrap().is_match(&staged), "{staged}");
}

const VIEW_MODULE_CODE: &str = r#"
use spacetimedb::{AnonymousViewContext, FileId, ReducerContext, SpacetimeType, Table, UploadRequest};

#[spacetimedb::table(accessor = upload_log, public)]
pub struct UploadLog {
    #[primary_key]
    file: FileId,
    /// Always 0, so views can scan the table through an index.
    #[index(btree)]
    kind: u8,
}

#[derive(SpacetimeType)]
pub struct UploadedFile {
    size: u64,
    content_type: Option<String>,
}

#[spacetimedb::reducer]
pub fn allow_upload(ctx: &ReducerContext, upload: UploadRequest) -> Result<(), String> {
    ctx.db.upload_log().insert(UploadLog { file: upload.file_id, kind: 0 });
    Ok(())
}

/// Completed uploads, read from each file's metadata. `allow_upload` logs a file before its
/// bytes arrive, so a file appears here only once its upload has committed.
#[spacetimedb::view(accessor = uploaded_file, public)]
pub fn uploaded_file(ctx: &AnonymousViewContext) -> Vec<UploadedFile> {
    ctx.db
        .upload_log()
        .kind()
        .filter(0u8)
        .filter_map(|upload| ctx.files().reference(upload.file))
        .filter_map(|file| ctx.files().metadata(file))
        .map(|meta| UploadedFile { size: meta.size, content_type: meta.content_type })
        .collect()
}
"#;

#[test]
fn test_file_storage_views_follow_uploads() {
    require_local_server!();
    let test = Smoketest::builder().module_code(VIEW_MODULE_CODE).build();
    let files = Files::new(&test);

    let sub = test
        .subscribe(&["SELECT * FROM uploaded_file"])
        .expect_rows(1)
        .background()
        .unwrap();
    let response = files.upload(b"hello, views", "text/plain", true);
    assert_eq!(response.status(), StatusCode::CREATED);

    // The view's only change is the completed upload, which arrives with the file's record.
    let events = sub.collect().unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    let inserts = &events[0]["uploaded_file"]["inserts"];
    assert_eq!(inserts.as_array().map(Vec::len), Some(1), "{events:?}");
    assert_eq!(inserts[0]["size"], 12, "{events:?}");
}
