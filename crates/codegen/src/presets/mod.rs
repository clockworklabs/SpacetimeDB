use spacetimedb_schema::def::ModuleDef;

pub mod react;

pub use self::react::React;

pub trait LangPreset {
  fn generate(&self, module: &ModuleDef) -> Vec<(String, String)>;
}

