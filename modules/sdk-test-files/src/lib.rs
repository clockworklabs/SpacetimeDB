//! A module for testing the Rust SDK's file API, with <../../../sdks/rust/tests/files-client>.

use spacetimedb::{FileRef, ReducerContext, Table, UploadRequest};

/// A named file, uploaded by a client or created by a reducer.
#[spacetimedb::table(accessor = document, public)]
pub struct Document {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[unique]
    name: String,
    file: FileRef,
    size: u64,
}

#[spacetimedb::reducer(allow_upload)]
pub fn check_upload(_ctx: &ReducerContext, upload: UploadRequest) -> Result<(), String> {
    if upload.content_type.as_deref() == Some("application/x-forbidden") {
        return Err("forbidden content type".into());
    }
    Ok(())
}

/// Names a file a client uploaded. Fails for a reference that names no file.
#[spacetimedb::reducer]
pub fn attach(ctx: &ReducerContext, name: String, file: FileRef) -> Result<(), String> {
    let metadata = ctx.files().metadata(file).ok_or("no such file")?;
    ctx.db.document().insert(Document {
        id: 0,
        name,
        file,
        size: metadata.size,
    });
    Ok(())
}

/// Creates a text file.
#[spacetimedb::reducer]
pub fn write_note(ctx: &ReducerContext, name: String, text: String) -> Result<(), String> {
    let file = ctx
        .files()
        .create(text.as_bytes(), Some("text/plain"))
        .map_err(|e| e.to_string())?;
    ctx.db.document().insert(Document {
        id: 0,
        name,
        file,
        size: text.len() as u64,
    });
    Ok(())
}

/// Deletes a named file.
#[spacetimedb::reducer]
pub fn remove(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let document = ctx.db.document().name().find(&name).ok_or("no such document")?;
    ctx.files().delete(document.file).map_err(|e| e.to_string())?;
    ctx.db.document().id().delete(document.id);
    Ok(())
}
