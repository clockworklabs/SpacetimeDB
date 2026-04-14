use spacetimedb::table;

#[table(accessor = user)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[table(accessor = product)]
pub struct Product {
    #[primary_key]
    pub id: i32,
    pub title: String,
    pub price: f32,
    pub in_stock: bool,
}

#[table(accessor = note)]
pub struct Note {
    #[primary_key]
    pub id: i32,
    pub body: String,
    pub rating: i64,
    pub pinned: bool,
}
