#![deny(warnings)]

#[spacetimedb::env]
pub struct Env {
    pub REQUIRED: String,
    #[env(values("false", "true"))]
    pub FLAG: String,
    #[env(values(""))]
    pub OPTIONAL: Option<String>,
    pub get: Option<String>,
    pub r#type: String,
}

fn reads(env: spacetimedb::Environment) {
    let _: String = env.REQUIRED();
    let _: String = env.FLAG();
    let _: Option<String> = env.OPTIONAL();
    let _: Option<String> = env.get("get");
    let _: String = env.r#type();
}

fn main() {
    let _ = reads as fn(spacetimedb::Environment);
}
