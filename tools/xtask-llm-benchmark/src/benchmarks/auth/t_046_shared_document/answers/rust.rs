use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = document, public)]
pub struct Document {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub title: String,
}

#[table(accessor = document_share, public)]
pub struct DocumentShare {
    #[index(btree)]
    pub document_id: u64,
    #[index(btree)]
    pub shared_with: Identity,
}

#[reducer]
pub fn create_document(ctx: &ReducerContext, title: String) {
    ctx.db.document().insert(Document {
        id: 0,
        owner: ctx.sender(),
        title,
    });
}

#[reducer]
pub fn share_document(ctx: &ReducerContext, document_id: u64, target: Identity) {
    let doc = ctx.db.document().id().find(document_id).expect("not found");
    if doc.owner != ctx.sender() {
        panic!("not owner");
    }
    ctx.db.document_share().insert(DocumentShare {
        document_id,
        shared_with: target,
    });
}

#[reducer]
pub fn edit_document(ctx: &ReducerContext, document_id: u64, new_title: String) {
    let mut doc = ctx.db.document().id().find(document_id).expect("not found");
    let is_owner = doc.owner == ctx.sender();
    let is_shared = ctx
        .db
        .document_share()
        .document_id()
        .filter(document_id)
        .any(|s| s.shared_with == ctx.sender());
    if !is_owner && !is_shared {
        panic!("unauthorized");
    }
    doc.title = new_title;
    ctx.db.document().id().update(doc);
}
