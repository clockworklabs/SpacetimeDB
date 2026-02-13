use crate::db::DBRunner;
use crate::space::Kind;
use async_trait::async_trait;
use chrono::Local;
use derive_more::From;
use postgres_types::{FromSql, Type};
use rust_decimal::Decimal;
use spacetimedb_sats::AlgebraicType;
use sqllogictest::{AsyncDB, DBOutput};
use std::time::Duration;
use tokio_postgres::{Client, Column, NoTls};

fn columns(cols: &[Column]) -> Vec<(String, AlgebraicType)> {
    cols.iter()
        .map(|col| {
            let kind = kind(col.type_());

            (col.name().to_string(), kind)
        })
        .collect::<Vec<_>>()
}

fn kind(ty: &Type) -> AlgebraicType {
    match ty {
        &Type::VARCHAR | &Type::TEXT | &Type::BPCHAR | &Type::NAME | &Type::UNKNOWN => AlgebraicType::String,
        &Type::INT2 => AlgebraicType::I16,
        &Type::INT4 => AlgebraicType::I32,
        &Type::INT8 => AlgebraicType::I64,
        &Type::FLOAT4 => AlgebraicType::F32,
        &Type::FLOAT8 => AlgebraicType::F64,
        &Type::BOOL => AlgebraicType::Bool,
        &Type::NUMERIC => AlgebraicType::F64,
        &Type::DATE => AlgebraicType::String,
        &Type::TIME => AlgebraicType::String,
        &Type::TIMESTAMP => AlgebraicType::String,
        &Type::TIMESTAMPTZ => AlgebraicType::String,
        _ => unimplemented!("{}", ty),
    }
}

const SQL_DROP: &str = "\
DO
$function$
DECLARE
  _schema VARCHAR;
BEGIN
    FOR _schema IN
        SELECT schema_name
        FROM information_schema.schemata
        WHERE schema_name NOT LIKE 'pg_%' AND schema_name <> 'information_schema'
    LOOP
        RAISE NOTICE 'SCHEMA.. :%', _schema;
        EXECUTE 'DROP SCHEMA IF EXISTS ' || _schema || ' CASCADE';
    END LOOP;
    CREATE SCHEMA public;


    FOR _schema IN
        SELECT rolname
        FROM pg_roles
        WHERE rolname LIKE 'role_%'
    LOOP
        RAISE NOTICE 'ROLE.. :%', _schema;
        EXECUTE 'DROP ROLE IF EXISTS ' || _schema;
    END LOOP;
END
$function$;
";

pub struct Pg {
    pub(crate) client: Client,
}

impl Pg {
    pub async fn new() -> anyhow::Result<Self> {
        let (client, conn) = tokio_postgres::connect("postgresql://postgres@localhost/TestSpace", NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("connection error: {e}");
            }
        });
        client.batch_execute(SQL_DROP).await?;
        Ok(Self { client })
    }

    pub fn into_db(self) -> DBRunner {
        DBRunner::Pg(self)
    }
}

#[async_trait]
impl AsyncDB for Pg {
    type Error = tokio_postgres::Error;
    type ColumnType = Kind;

    async fn run(&mut self, sql: &str) -> Result<DBOutput<Self::ColumnType>, Self::Error> {
        let stmt = self.client.prepare(sql).await?;

        let is_query_sql = {
            let lower_sql = sql.trim_start().to_ascii_lowercase();
            lower_sql.starts_with("select")
                || (lower_sql.starts_with("insert")
                    || lower_sql.starts_with("update")
                    || lower_sql.starts_with("delete"))
        };
        if !is_query_sql {
            self.client.execute(&stmt, &[]).await?;
            return Ok(DBOutput::StatementComplete(0));
        }

        let cols = columns(stmt.columns());
        let rows = self.client.query(&stmt, &[]).await?;
        let mut data = Vec::new();

        for row in rows {
            let mut new = Vec::with_capacity(row.columns().len());

            for col in row.columns() {
                let value = row.get::<&str, Scalar>(col.name());

                new.push(value.0);
            }

            data.push(new);
        }

        Ok(DBOutput::Rows {
            types: cols.into_iter().map(|(_, ty)| Kind(ty)).collect(),
            rows: data,
        })
    }

    fn engine_name(&self) -> &str {
        "Postgres"
    }

    async fn sleep(dur: Duration) {
        tokio::time::sleep(dur).await
    }
}

#[derive(From)]
pub struct Scalar(String);

impl FromSql<'_> for Scalar {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let value = match ty {
            &Type::VARCHAR | &Type::TEXT | &Type::BPCHAR | &Type::NAME | &Type::UNKNOWN => {
                let x: Option<String> = FromSql::from_sql(ty, raw)?;
                if let Some(x) = x {
                    format!("'{x}'").into()
                } else {
                    "null".to_string().into()
                }
            }

            &Type::INT2 | &Type::INT4 => {
                let x: i32 = FromSql::from_sql(ty, raw)?;
                x.to_string().into()
            }
            &Type::INT8 => {
                let x: i64 = FromSql::from_sql(ty, raw)?;
                x.to_string().into()
            }
            &Type::FLOAT4 => {
                let x: f32 = FromSql::from_sql(ty, raw)?;
                format!("{x:?}").into()
            }
            &Type::FLOAT8 => {
                let x: f32 = FromSql::from_sql(ty, raw)?;
                format!("{x:?}").into()
            }
            &Type::NUMERIC => {
                let x: Decimal = FromSql::from_sql(ty, raw)?;
                let txt = x.to_string();
                if txt.contains('.') {
                    txt.into()
                } else {
                    format!("{txt}.0").into()
                }
            }
            &Type::BOOL => {
                let x: bool = FromSql::from_sql(ty, raw)?;
                //for compat with sqlite...
                let x = if x { "1" } else { "0" };
                x.to_string().into()
            }
            &Type::DATE => {
                let x: chrono::NaiveDate = FromSql::from_sql(ty, raw)?;
                format!("{x:?}").into()
            }
            &Type::TIME => {
                let x: chrono::NaiveTime = FromSql::from_sql(ty, raw)?;
                format!("{x:?}").into()
            }
            &Type::TIMESTAMP => {
                let x: chrono::DateTime<Local> = FromSql::from_sql(ty, raw)?;
                format!("{x:?}").into()
            }
            &Type::TIMESTAMPTZ => {
                let x: chrono::DateTime<Local> = FromSql::from_sql(ty, raw)?;
                format!("{x:?}").into()
            }
            _ => unimplemented!("{}", ty),
        };
        Ok(value)
    }

    fn from_sql_null(_ty: &Type) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok("null".to_string().into())
    }

    fn accepts(ty: &Type) -> bool {
        match ty {
            &Type::VARCHAR
            | &Type::TEXT
            | &Type::BPCHAR
            | &Type::NAME
            | &Type::UNKNOWN
            | &Type::BOOL
            | &Type::INT2
            | &Type::INT4
            | &Type::INT8
            | &Type::FLOAT4
            | &Type::FLOAT8
            | &Type::NUMERIC
            | &Type::DATE
            | &Type::TIME
            | &Type::TIMESTAMP
            | &Type::TIMESTAMPTZ => true,
            ty if ty.name() == "citext" => true,
            _ => false,
        }
    }
}
