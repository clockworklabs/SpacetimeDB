use crate::db::DBRunner;
use async_trait::async_trait;
use spacetimedb::db::relational_db::{open_db, RelationalDB};
use spacetimedb::error::DBError;
use spacetimedb::sql::execute::{compile_sql, execute_sql};
use spacetimedb_sats::relation::MemTable;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, BuiltinType, BuiltinValue};
use sqllogictest::{AsyncDB, ColumnType, DBOutput};
use std::fs;
use std::io::Write;
use tempdir::TempDir;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Kind(pub(crate) AlgebraicType);

impl ColumnType for Kind {
    fn from_char(value: char) -> Option<Self> {
        match value {
            'B' => Some(Kind(AlgebraicType::Bool)),
            'T' => Some(Kind(AlgebraicType::String)),
            'I' => Some(Kind(AlgebraicType::I64)),
            'R' => Some(Kind(AlgebraicType::F32)),
            _ => Some(Kind(AlgebraicType::make_meta_type())),
        }
    }

    fn to_char(&self) -> char {
        match &self.0 {
            AlgebraicType::Builtin(x) => match x {
                BuiltinType::I8
                | BuiltinType::U8
                | BuiltinType::U16
                | BuiltinType::I16
                | BuiltinType::I32
                | BuiltinType::U32
                | BuiltinType::I64
                | BuiltinType::U64
                | BuiltinType::I128
                | BuiltinType::U128 => 'I',
                BuiltinType::F32 | BuiltinType::F64 => 'R',
                BuiltinType::String => 'T',
                BuiltinType::Bool => 'B',
                BuiltinType::Array(_) | BuiltinType::Map(_) => '?',
            },
            _ => '!',
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
}

impl SpaceDb {
    pub fn new() -> anyhow::Result<Self> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let conn = open_db(&tmp_dir)?;
        Ok(Self { conn, tmp_dir })
    }

    pub(crate) fn run_sql(&self, sql: &str) -> anyhow::Result<Vec<MemTable>> {
        let ast = compile_sql(&self.conn, sql)?;
        let result = execute_sql(&self.conn, ast)?;
        //remove comments to see which SQL worked. Can't collect it outside from lack of a hook in the external `sqllogictest` crate... :(
        //append_file(&std::path::PathBuf::from(".ok.sql"), sql)?;
        Ok(result)
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
        let mut output: Vec<_> = vec![];

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

        for row in r.data {
            let mut row_vec = vec![];

            for value in row.elements {
                let value = match value {
                    AlgebraicValue::Builtin(x) => match x {
                        BuiltinValue::Bool(x) => {
                            //for compat with sqlite...
                            if x { "1" } else { "0" }.to_string()
                        }
                        BuiltinValue::I8(x) => x.to_string(),
                        BuiltinValue::U8(x) => x.to_string(),
                        BuiltinValue::I16(x) => x.to_string(),
                        BuiltinValue::U16(x) => x.to_string(),
                        BuiltinValue::I32(x) => x.to_string(),
                        BuiltinValue::U32(x) => x.to_string(),
                        BuiltinValue::I64(x) => x.to_string(),
                        BuiltinValue::U64(x) => x.to_string(),
                        BuiltinValue::I128(x) => x.to_string(),
                        BuiltinValue::U128(x) => x.to_string(),
                        BuiltinValue::F32(x) => format!("{:?}", x.as_ref()),
                        BuiltinValue::F64(x) => format!("{:?}", x.as_ref()),
                        BuiltinValue::String(x) => format!("'{}'", x),
                        x => x.to_satn(),
                    },
                    x => x.to_satn(),
                };
                row_vec.push(value);
            }

            output.push(row_vec);
        }

        Ok(DBOutput::Rows {
            types: header,
            rows: output,
        })
    }

    fn engine_name(&self) -> &str {
        "SpaceTimeDb"
    }
}
