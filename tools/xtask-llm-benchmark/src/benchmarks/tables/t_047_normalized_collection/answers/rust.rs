use spacetimedb::table;

#[table(accessor = collection_owner, public)]
pub struct CollectionOwner {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
}

#[table(
    accessor = child_item,
    public,
    index(accessor = by_owner, btree(columns = [owner_id]))
)]
pub struct ChildItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner_id: u64,
    pub value: String,
}
