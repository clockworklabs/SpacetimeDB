#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    age: u8,
}
