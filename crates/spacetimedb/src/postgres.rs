use bb8_postgres::{bb8::Pool, PostgresConnectionManager};
use tokio_postgres::{Config, NoTls};

static mut POOL: Option<Pool<PostgresConnectionManager<NoTls>>> = None;

pub async fn init() {
    let mut config = Config::new();
    config.user("postgres");
    config.dbname("postgres");
    config.host("postgres");
    config.port(5432);

    // Connect to Postgres
    let pg_mgr = PostgresConnectionManager::new(config, tokio_postgres::NoTls);

    let pool = match Pool::builder().build(pg_mgr).await {
        Ok(pool) => pool,
        Err(e) => panic!("bb8 error {}", e),
    };

    unsafe { POOL = Some(pool) }
}

pub async fn get_client() -> bb8_postgres::bb8::PooledConnection<'static, PostgresConnectionManager<NoTls>> {
    let pool = unsafe { POOL.as_ref().unwrap() };
    pool.get().await.unwrap()
}
