use crate::db::DBRunner;
use async_trait::async_trait;
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb::error::DBError;
use spacetimedb::execution_context::ExecutionContext;
use spacetimedb::sql::compiler::compile_sql;
use spacetimedb::sql::execute::execute_sql;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_sats::meta_type::MetaType;
use spacetimedb_sats::relation::MemTable;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, BuiltinType};
use sqllogictest::{AsyncDB, ColumnType, DBOutput};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Kind(pub(crate) AlgebraicType);

impl ColumnType for Kind {
    fn from_char(value: char) -> Option<Self> {
        match value {
            'B' => Some(Kind(AlgebraicType::Bool)),
            'T' => Some(Kind(AlgebraicType::String)),
            'I' => Some(Kind(AlgebraicType::I64)),
            'R' => Some(Kind(AlgebraicType::F32)),
            _ => Some(Kind(AlgebraicType::meta_type())),
        }
    }

    fn to_char(&self) -> char {
        match self.0 {
            AlgebraicType::Builtin(BuiltinType::Map(_)) | AlgebraicType::Builtin(BuiltinType::Array(_)) => '?',
            AlgebraicType::I8
            | AlgebraicType::U8
            | AlgebraicType::U16
            | AlgebraicType::I16
            | AlgebraicType::I32
            | AlgebraicType::U32
            | AlgebraicType::I64
            | AlgebraicType::U64
            | AlgebraicType::I128
            | AlgebraicType::U128 => 'I',
            AlgebraicType::F32 | AlgebraicType::F64 => 'R',
            AlgebraicType::String => 'T',
            AlgebraicType::Bool => 'B',
            AlgebraicType::Ref(_) | AlgebraicType::Sum(_) | AlgebraicType::Product(_) => '!',
        }
    }
}

#[allow(dead_code)]
fn append_file(to: &std::path::Path, content: &str) -> anyhow::Result<()> {
    let mut f = fs::OpenOptions::new().create(true).append(true).write(true).open(to)?;

    f.write_all(format!("{content}\n").as_bytes())?;

    Ok(())
}

pub struct SpaceDb {
    pub(crate) conn: RelationalDB,
    #[allow(dead_code)]
    tmp_dir: TempDir,
    auth: AuthCtx,
}

impl SpaceDb {
    pub fn new() -> anyhow::Result<Self> {
        let tmp_dir = TempDir::with_prefix("stdb_test")?;
        let in_memory = false;
        let fsync = false;
        let conn = open_db(&tmp_dir, in_memory, fsync)?;
        Ok(Self {
            conn,
            tmp_dir,
            auth: AuthCtx::for_testing(),
        })
    }

    pub(crate) fn run_sql(&self, sql: &str) -> anyhow::Result<Vec<MemTable>> {
        self.conn.with_read_only(&ExecutionContext::default(), |tx| {
            let ast = compile_sql(&self.conn, tx, sql)?;
            let result = execute_sql(&self.conn, ast, self.auth)?;
            //remove comments to see which SQL worked. Can't collect it outside from lack of a hook in the external `sqllogictest` crate... :(
            //append_file(&std::path::PathBuf::from(".ok.sql"), sql)?;
            Ok(result)
        })
    }

    pub fn into_db(self) -> DBRunner {
        DBRunner::Space(self)
    }
}

#[async_trait]
impl AsyncDB for SpaceDb {
    type Error = DBError;
    type ColumnType = Kind;

    async fn run(&mut self, sql: &str) -> Result<DBOutput<Self::ColumnType>, Self::Error> {
        let is_query_sql = {
            let lower_sql = sql.trim_start().to_ascii_lowercase();
            lower_sql.starts_with("select")
        };
        let r = self.run_sql(sql)?;
        if !is_query_sql {
            return Ok(DBOutput::StatementComplete(0));
        }
        let r = r.into_iter().next().unwrap();

        let header = r.head.fields.iter().map(|x| Kind(x.algebraic_type.clone())).collect();

        let output: Vec<Vec<_>> = r
            .data
            .into_iter()
            .map(|row| {
                row.data
                    .elements
                    .iter()
                    .map(|value| match value {
                        AlgebraicValue::Bool(x) => if *x { "1" } else { "0" }.to_string(),
                        // ^-- For compat with sqlite.
                        AlgebraicValue::I8(x) => x.to_string(),
                        AlgebraicValue::U8(x) => x.to_string(),
                        AlgebraicValue::I16(x) => x.to_string(),
                        AlgebraicValue::U16(x) => x.to_string(),
                        AlgebraicValue::I32(x) => x.to_string(),
                        AlgebraicValue::U32(x) => x.to_string(),
                        AlgebraicValue::I64(x) => x.to_string(),
                        AlgebraicValue::U64(x) => x.to_string(),
                        AlgebraicValue::I128(x) => x.to_string(),
                        AlgebraicValue::U128(x) => x.to_string(),
                        AlgebraicValue::F32(x) => format!("{:?}", x.as_ref()),
                        AlgebraicValue::F64(x) => format!("{:?}", x.as_ref()),
                        AlgebraicValue::String(x) => format!("'{}'", x),
                        x => x.to_satn(),
                    })
                    .collect()
            })
            .collect();

        Ok(DBOutput::Rows {
            types: header,
            rows: output,
        })
    }

    fn engine_name(&self) -> &str {
        "SpacetimeDB"
    }
}
