use crate::pg::Pg;
use crate::space::{Kind, SpaceDb};
use crate::sqlite::Sqlite;
use async_trait::async_trait;
use spacetimedb::error::DBError;
use sqllogictest::{AsyncDB, DBOutput};

pub enum DBRunner {
    Sqlite(Sqlite),
    Space(SpaceDb),
    Pg(Pg),
}

#[async_trait]
impl AsyncDB for DBRunner {
    type Error = DBError;
    type ColumnType = Kind;

    async fn run(&mut self, sql: &str) -> Result<DBOutput<Self::ColumnType>, Self::Error> {
        let mut last = None;
        for x in sql.split('\n') {
            last = Some(match self {
                DBRunner::Space(db) => db.run(x).await?,
                DBRunner::Sqlite(db) => db.run(x).await.map_err(|err| DBError::Other(err.into()))?,
                DBRunner::Pg(db) => db.run(x).await.map_err(|err| DBError::Other(err.into()))?,
            })
        }

        Ok(last.unwrap())
    }
}
