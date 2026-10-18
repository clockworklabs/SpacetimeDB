//! File Storage, following the patterns in the File Storage proposal.
//!
//! A `FileRef` grants read access, like the file's bytes, so only rows whose readers may download
//! the file hold one. A `FileId` names a file without granting access, so rows that more people see
//! hold those, and a view resolves them to references for the readers allowed to download.

use spacetimedb::{
    log, FileId, FileRef, FileUrl, Identity, ReducerContext, SpacetimeType, Table, UploadRequest, ViewContext,
};

/// The largest upload this module accepts.
const UPLOAD_SIZE_MAX: u64 = 5 * 1024 * 1024;

/// Who owns each file. Only a file's owner may make it their avatar.
#[spacetimedb::table(accessor = file_owner)]
pub struct FileOwner {
    #[primary_key]
    file: FileId,
    owner: Identity,
}

/// Each player's avatar, which anyone may download.
#[spacetimedb::table(accessor = avatar, public)]
pub struct Avatar {
    #[primary_key]
    owner: Identity,
    file: FileRef,
}

/// An image one player gave another. Everyone sees the gift and which file it is,
/// but only its recipient may download the image, through `my_gifts`.
#[spacetimedb::table(accessor = gift, public)]
pub struct Gift {
    #[primary_key]
    #[auto_inc]
    id: u64,
    giver: Identity,
    #[index(btree)]
    recipient: Identity,
    image: FileId,
}

#[derive(SpacetimeType)]
pub struct ReceivedGift {
    gift_id: u64,
    giver: Identity,
    image: FileRef,
}

/// The caller's gifts, with references that let them download the images.
#[spacetimedb::view(accessor = my_gifts, public)]
fn my_gifts(ctx: &ViewContext) -> Vec<ReceivedGift> {
    ctx.db
        .gift()
        .recipient()
        .filter(ctx.sender())
        .filter_map(|gift| {
            Some(ReceivedGift {
                gift_id: gift.id,
                giver: gift.giver,
                image: ctx.files().reference(gift.image)?,
            })
        })
        .collect()
}

/// Accepts uploads of up to `UPLOAD_SIZE_MAX`.
///
/// It runs before any bytes arrive, and the host has already chosen the file's ID,
/// so it records its sender, the uploader, as the file's owner.
#[spacetimedb::reducer(allow_upload)]
fn allow_upload(ctx: &ReducerContext, upload: UploadRequest) -> Result<(), String> {
    if upload.size > UPLOAD_SIZE_MAX {
        return Err(format!("uploads are limited to {UPLOAD_SIZE_MAX} bytes"));
    }
    ctx.db.file_owner().insert(FileOwner {
        file: upload.file_id,
        owner: ctx.sender(),
    });
    Ok(())
}

/// Sets the caller's avatar to an image they uploaded.
#[spacetimedb::reducer]
fn set_avatar(ctx: &ReducerContext, avatar: FileRef) -> Result<(), String> {
    // The client built `avatar`, so its secret must be checked before its ID is trusted.
    let id = ctx.files().id(avatar).ok_or("avatar not found")?;
    let owner = ctx.db.file_owner().file().find(id).ok_or("not an upload")?.owner;
    if owner != ctx.sender() {
        return Err("avatar must be your own upload".into());
    }
    let metadata = ctx.files().metadata(id).ok_or("avatar not found")?;
    if !metadata.content_type().is_some_and(|ty| ty.starts_with("image/")) {
        return Err("avatar must be an image".into());
    }
    replace_avatar(ctx, avatar)?;
    log::info!("{} set their avatar to {}", ctx.sender(), metadata.url());
    Ok(())
}

/// Sets the caller's avatar to a circle in a color drawn from their identity.
#[spacetimedb::reducer]
fn draw_avatar(ctx: &ReducerContext) -> Result<(), String> {
    let hue = u32::from(ctx.sender().to_byte_array()[0]) * 360 / 256;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2 2"><circle cx="1" cy="1" r="1" fill="hsl({hue} 70% 50%)"/></svg>"#
    );
    // The bytes travel through the commitlog with this transaction, so a reducer can create
    // files of up to 16 MiB. Larger files are uploaded.
    let id = ctx
        .files()
        .create(svg, Some("image/svg+xml".into()))
        .map_err(|e| e.to_string())?;
    ctx.db.file_owner().insert(FileOwner {
        file: id,
        owner: ctx.sender(),
    });
    // `create` returns an ID, so disclosing the new file takes a deliberate `reference`.
    let avatar = ctx.files().reference(id).ok_or("avatar not found")?;
    replace_avatar(ctx, avatar)
}

/// Only its owner could have made a file their avatar, so deleting the avatar it replaces is safe.
fn replace_avatar(ctx: &ReducerContext, avatar: FileRef) -> Result<(), String> {
    let old = ctx.db.avatar().owner().find(ctx.sender()).map(|row| row.file);
    ctx.db.avatar().owner().insert_or_update(Avatar {
        owner: ctx.sender(),
        file: avatar,
    });
    if let Some(old) = old.filter(|&old| old != avatar) {
        // The old reference came from this module's own table, so its ID needs no check.
        let old = old.id_unchecked();
        ctx.files().delete(old).map_err(|e| e.to_string())?;
        ctx.db.file_owner().file().delete(old);
    }
    Ok(())
}

/// Gives another player a copy of an image the caller can read, such as their upload or an avatar.
///
/// The gift owns its copy, so the image stays even if the original is deleted.
#[spacetimedb::reducer]
fn give(ctx: &ReducerContext, recipient: Identity, image: FileRef) -> Result<(), String> {
    // With `image.id_unchecked()`, a client that knows only a file's ID, such as a gift's image,
    // could send a forged secret and give away a copy of a file it can't read.
    let source = ctx.files().id(image).ok_or("image not found")?;
    let image = ctx.files().copy(source).map_err(|e| e.to_string())?;
    let size = ctx.files().metadata(image).map_or(0, |metadata| metadata.size());
    let gift = ctx.db.gift().insert(Gift {
        id: 0,
        giver: ctx.sender(),
        recipient,
        image,
    });
    log::info!("{} gave {recipient} {size} bytes as gift {}", ctx.sender(), gift.id);
    Ok(())
}

/// Discards one of the caller's gifts, with its image.
#[spacetimedb::reducer]
fn discard_gift(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let gift = ctx.db.gift().id().find(id).ok_or("gift not found")?;
    if gift.recipient != ctx.sender() {
        return Err("only its recipient may discard a gift".into());
    }
    ctx.db.gift().id().delete(id);
    ctx.files().delete(gift.image).map_err(|e| e.to_string())?;
    Ok(())
}
