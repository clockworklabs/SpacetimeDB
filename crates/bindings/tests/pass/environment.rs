#![deny(warnings)]

use std::option::Option as Maybe;
use std::string::String as RenamedString;

type RequiredAlias = RenamedString;
type OptionalAlias = Maybe<RequiredAlias>;

#[derive(Debug, PartialEq, spacetimedb::EnvironmentValue)]
pub enum Mode {
    #[env(value = "in progress")]
    InProgress,
    #[env(value = "Ready")]
    Ready,
}
type ModeAlias = Mode;
type MaybeMode = Maybe<ModeAlias>;

const _: [(); 0] = [(); <ModeAlias as spacetimedb::rt::EnvironmentValue>::OPTIONAL as usize];
const _: [(); 1] = [(); <MaybeMode as spacetimedb::rt::EnvironmentValue>::OPTIONAL as usize];

const _: [(); 0] = [(); <RequiredAlias as spacetimedb::rt::EnvironmentValue>::OPTIONAL as usize];
const _: [(); 1] = [(); <OptionalAlias as spacetimedb::rt::EnvironmentValue>::OPTIONAL as usize];

#[spacetimedb::env]
pub struct Env {
    pub REQUIRED: RequiredAlias,
    pub MODE: ModeAlias,
    pub OPTIONAL_MODE: MaybeMode,
    #[env(values("false", "true"))]
    pub FLAG: String,
    #[env(values(""))]
    pub OPTIONAL: OptionalAlias,
    pub get: Maybe<RenamedString>,
    pub r#type: RenamedString,
}

fn reads(env: spacetimedb::Environment) {
    let _: String = env.REQUIRED();
    let _: Mode = env.MODE();
    let _: Option<Mode> = env.OPTIONAL_MODE();
    let _: String = env.FLAG();
    let _: Option<String> = env.OPTIONAL();
    let _: Option<String> = env.get("get");
    let _: String = env.r#type();
}

fn main() {
    let _ = reads as fn(spacetimedb::Environment);
}
