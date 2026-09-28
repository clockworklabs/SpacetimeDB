#[derive(spacetimedb::EnvironmentValue)]
struct NotEnum;

#[derive(spacetimedb::EnvironmentValue)]
enum Empty {}

#[derive(spacetimedb::EnvironmentValue)]
enum Generic<T> {
    Value(T),
}

#[derive(spacetimedb::EnvironmentValue)]
enum Payload {
    Value(String),
}

#[derive(spacetimedb::EnvironmentValue)]
enum Duplicate {
    #[env(value = "Same")]
    First,
    Same,
}

#[derive(spacetimedb::EnvironmentValue)]
enum DuplicateAttribute {
    #[env(value = "x", value = "y")]
    Value,
}

#[derive(spacetimedb::EnvironmentValue)]
enum WrongAttribute {
    #[env(values("x"))]
    Value,
}

#[derive(spacetimedb::EnvironmentValue)]
enum WrongLiteral {
    #[env(value = 1)]
    Value,
}

#[derive(spacetimedb::EnvironmentValue)]
pub enum Mode {
    Ready,
}

#[spacetimedb::env]
pub struct Env {
    #[env(values("other"))]
    pub MODE: Mode,
    pub NESTED: Option<Option<Mode>>,
}

fn main() {}
