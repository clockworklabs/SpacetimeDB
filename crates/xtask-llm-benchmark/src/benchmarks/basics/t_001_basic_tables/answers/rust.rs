use spacetimedb::table;

#[table(name = users)]
pub struct Users {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[table(name = products)]
pub struct Products {
    #[primary_key]
    pub id: i32,
    pub title: String,
    pub price: f32,
    pub in_stock: bool,
}

#[table(name = notes)]
pub struct Notes {
    #[primary_key]
    pub id: i32,
    pub body: String,
    pub rating: i64,
    pub pinned: bool,
}
