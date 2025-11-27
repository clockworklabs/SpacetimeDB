use spacetimedb::Identity;

#[spacetimedb::table(name = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    #[index(btree)]
    name: Option<String>,
    online: bool,
}
