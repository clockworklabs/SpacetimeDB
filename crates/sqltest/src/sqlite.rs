use crate::db::DBRunner;
use crate::space::Kind;
use async_trait::async_trait;
use rusqlite::types::Value;
use spacetimedb_sats::{meta_type::MetaType, AlgebraicType};
use sqllogictest::{AsyncDB, DBOutput};
use std::path::PathBuf;
use tempfile::TempDir;

fn kind(of: &str) -> AlgebraicType {
    let of = of.to_uppercase();
    match of.as_str() {
        "INT" => AlgebraicType::I32,
        "INTEGER" => AlgebraicType::I32,
        "BIGINT" => AlgebraicType::I64,
        "BOOLEAN" => AlgebraicType::Bool,
        "VARCHAR" => AlgebraicType::String,
        "TEXT" => AlgebraicType::String,
        "REAL" => AlgebraicType::F32,
        x => {
            if of.starts_with("VARCHAR") {
                AlgebraicType::String
            } else {
                unimplemented!("sqlite kind {}", x)
            }
        }
    }
}

fn columns(stmt: &mut rusqlite::Statement) -> Vec<(String, AlgebraicType)> {
    stmt.columns()
        .iter()
        .map(|col| {
            let kind = col.decl_type().map(kind).unwrap_or_else(AlgebraicType::meta_type);

            (col.name().to_string(), kind)
        })
        .collect::<Vec<_>>()
}

pub struct Sqlite {
    pub(crate) conn: rusqlite::Connection,
    #[allow(dead_code)]
    pub(crate) tmp_dir: TempDir,
}

impl Sqlite {
    pub fn new() -> anyhow::Result<Self> {
        let tmp_dir = TempDir::with_prefix("sqlite_test")?;
        let mut file = PathBuf::from(tmp_dir.path());
        file.push("db.db");

        let conn = rusqlite::Connection::open(file)?;

        Ok(Self { conn, tmp_dir })
    }

    pub fn into_db(self) -> DBRunner {
        DBRunner::Sqlite(self)
    }
}

#[async_trait]
impl AsyncDB for Sqlite {
    type Error = rusqlite::Error;
    type ColumnType = Kind;

    async fn run(&mut self, sql: &str) -> Result<DBOutput<Self::ColumnType>, Self::Error> {
        let mut stmt = self.conn.prepare(sql)?;

        let lower_sql = sql.trim_start().to_ascii_lowercase();
        let is_query_sql = {
            lower_sql.starts_with("select")
                || (lower_sql.starts_with("insert")
                    || lower_sql.starts_with("update")
                    || lower_sql.starts_with("delete"))
        };
        if !is_query_sql {
            stmt.execute([])?;
            return Ok(DBOutput::StatementComplete(0));
        }

        let mut columns = columns(&mut stmt);
        let mut rows = stmt.query([])?;
        let mut data = Vec::new();
        let mut meta = AlgebraicType::meta_type();

        while let Some(row) = rows.next()? {
            let mut new = Vec::with_capacity(columns.len());

            for (name, dectype) in &mut columns {
                let value = row.get::<_, Value>(name.as_str())?;
                let (value, kind) = match value {
                    Value::Null => ("null".into(), AlgebraicType::never()),
                    Value::Integer(x) => (x.to_string(), AlgebraicType::I64),
                    Value::Real(x) => (format!("{:?}", x), AlgebraicType::F64),
                    Value::Text(x) => (format!("'{}'", x), AlgebraicType::String),
                    _ => unimplemented!("Sqlite from"),
                };
                if dectype == &mut meta {
                    *dectype = kind;
                }
                new.push(value);
            }

            data.push(new);
        }

        Ok(DBOutput::Rows {
            types: columns.into_iter().map(|(_, ty)| Kind(ty)).collect(),
            rows: data,
        })
    }

    fn engine_name(&self) -> &str {
        "Sqlite"
    }
}
