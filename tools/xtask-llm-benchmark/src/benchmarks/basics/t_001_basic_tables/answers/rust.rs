use spacetimedb::table;

#[table(name = user)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[table(name = product)]
pub struct Product {
    #[primary_key]
    pub id: i32,
    pub title: String,
    pub price: f32,
    pub in_stock: bool,
}

#[table(name = note)]
pub struct Note {
    #[primary_key]
    pub id: i32,
    pub body: String,
    pub rating: i64,
    pub pinned: bool,
}
