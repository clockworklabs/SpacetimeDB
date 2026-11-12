mod shadowed_string {
    pub struct String;

    #[spacetimedb::env]
    pub struct ShadowedString {
        pub VALUE: String,
    }
}

mod shadowed_option {
    pub struct Option<T>(T);

    #[spacetimedb::env]
    pub struct ShadowedOption {
        pub VALUE: Option<String>,
    }
}

#[spacetimedb::env]
pub struct Unsupported {
    pub BOOL: bool,
    pub NESTED: Option<Option<String>>,
    // Even without a generated named accessor, metadata must check the type.
    pub get: u32,
}

fn main() {}
