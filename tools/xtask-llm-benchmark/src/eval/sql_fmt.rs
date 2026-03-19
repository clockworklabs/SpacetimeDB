use crate::eval::Lang;
use heck::{ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};

#[derive(Clone, Copy)]
pub enum Casing {
    Pascal,
    Snake,
    LowerCamel,
    Upper,
    Lower,
}

pub fn casing_for_lang(lang: Lang) -> Casing {
    match lang {
        Lang::CSharp => Casing::Pascal,
        Lang::TypeScript => Casing::LowerCamel,
        _ => Casing::Snake,
    }
}

/// Convert a singular lowercase table name to the appropriate convention for each language.
/// - C#: PascalCase singular (e.g., "user" -> "User")
/// - TypeScript: camelCase singular (e.g., "user" -> "user")
/// - Rust: snake_case singular (e.g., "user" -> "user")
pub fn table_name(singular: &str, lang: Lang) -> String {
    match lang {
        Lang::CSharp => singular.to_upper_camel_case(),
        Lang::TypeScript => singular.to_lower_camel_case(),
        Lang::Rust => singular.to_snake_case(),
    }
}

pub fn ident(s: &str, casing: Casing) -> String {
    match casing {
        Casing::Snake => s.to_snake_case(),
        Casing::Pascal => s.to_upper_camel_case(),
        Casing::LowerCamel => s.to_lower_camel_case(),
        Casing::Upper => s.to_snake_case().replace('_', "").to_uppercase(),
        Casing::Lower => s.to_snake_case().replace('_', "").to_lowercase(),
    }
}

pub struct SqlBuilder {
    pub(crate) case: Casing,
}

impl SqlBuilder {
    pub fn new(case: Casing) -> Self {
        Self { case }
    }

    pub fn cols(&self, bases: &[&str]) -> Vec<String> {
        bases.iter().map(|b| ident(b, self.case)).collect()
    }

    pub fn select_by_id(&self, table: &str, cols: &[&str], id_col: &str, id_val: i64) -> String {
        let cols = self.cols(cols).join(", ");
        let idc = ident(id_col, self.case);
        format!("SELECT {cols} FROM {table} WHERE {idc}={id_val}")
    }

    pub fn count_by_id(&self, table: &str, id_col: &str, id_val: i64) -> String {
        let idc = ident(id_col, self.case);
        format!("SELECT COUNT(*) AS n FROM {table} WHERE {idc}={id_val}")
    }

    pub fn insert_values(&self, table: &str, cols: &[&str], values_sql: &[&str]) -> String {
        let cols = self.cols(cols).join(", ");
        let vals = values_sql.join(", ");
        format!("INSERT INTO {table}({cols}) VALUES ({vals})")
    }
}
