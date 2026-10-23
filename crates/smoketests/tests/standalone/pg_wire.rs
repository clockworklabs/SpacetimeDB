#![allow(clippy::disallowed_macros)]
use spacetimedb_smoketests::{require_local_server, require_psql, Smoketest};

#[test]
fn test_sql_format() {
    require_psql!();
    // This requires a local server because we don't have a clean way of providing
    // the remote server's PG port.
    require_local_server!();

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5433) // Use non-standard port to avoid conflicts
        .autopublish(false)
        .build();

    test.publish().name("pgwire-sql-format").clear(true).run().unwrap();
    test.call("test", &[]).unwrap();

    test.assert_psql(
        "pgwire-sql-format",
        "SELECT * FROM t_ints",
        r#"i_8 | i_16  |  i_32  |   i_64   |     i_128     |     i_256
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853
(1 row)"#,
    );

    test.assert_psql(
        "pgwire-sql-format",
        "SELECT * FROM t_ints_tuple",
        r#"tuple
---------------------------------------------------------------------------------------------------------------
 {"i_8": -25, "i_16": -3224, "i_32": -23443, "i_64": -2344353, "i_128": -234434897853, "i_256": -234434897853}
(1 row)"#,
    );

    test.assert_psql(
        "pgwire-sql-format",
        "SELECT * FROM t_uints",
        r#"u_8 | u_16 | u_32  |   u_64   |     u_128     |     u_256
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889
(1 row)"#,
    );

    test.assert_psql(
        "pgwire-sql-format",
        "SELECT * FROM t_uints_tuple",
        r#"tuple
-------------------------------------------------------------------------------------------------------------
 {"u_8": 105, "u_16": 1050, "u_32": 83892, "u_64": 48937498, "u_128": 4378528978889, "u_256": 4378528978889}
(1 row)"#,
    );

    test.assert_psql(
        "pgwire-sql-format",
        "SELECT * FROM t_simple_enum",
        r#"id |  action
----+----------
  1 | inactive
  2 | active
(2 rows)"#,
    );

    test.assert_psql(
        "pgwire-sql-format",
        "SELECT * FROM t_enum",
        r#"id |     color
----+---------------
  1 | {"gray": 128}
(1 row)"#,
    );
}

/// Test connecting to the database using a PostgreSQL client.
#[test]
fn test_sql_conn() {
    // This requires a local server because we don't have a clean way of providing
    // the remote server's PG port.
    require_local_server!();

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5435) // Use different port from test_sql_format/test_failures
        .autopublish(false)
        .build();

    test.publish().name("pgwire-sql-conn").clear(true).run().unwrap();
    test.call("test", &[]).unwrap();

    let token = test.read_token().unwrap();
    let pg_port = test.pg_port().expect("PostgreSQL wire protocol not enabled");
    let host = test.server_host().split(':').next().unwrap_or("127.0.0.1");

    let mut cfg = tokio_postgres::Config::new();
    cfg.host(host);
    cfg.port(pg_port);
    cfg.user("postgres");
    cfg.password(token);
    cfg.dbname("pgwire-sql-conn");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let (client, connection) = cfg.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });

        let rows = client
            .simple_query("select * from t_uints where u8 = 105 and u16 = 1050")
            .await
            .unwrap();

        let row = rows
            .iter()
            .find_map(|m| match m {
                tokio_postgres::SimpleQueryMessage::Row(r) => Some(r),
                _ => None,
            })
            .expect("Expected at least one row");

        assert_eq!(row.get(0), Some("105"));
        assert_eq!(row.get(1), Some("1050"));
        assert_eq!(row.get(2), Some("83892"));
        assert_eq!(row.get(3), Some("48937498"));
        assert_eq!(row.get(4), Some("4378528978889"));
        assert_eq!(row.get(5), Some("4378528978889"));

        // Check long-lived connection.
        for _ in 0..10 {
            let rows = client.simple_query("select count(*) as t from t_uints").await.unwrap();

            let row = rows
                .iter()
                .find_map(|m| match m {
                    tokio_postgres::SimpleQueryMessage::Row(r) => Some(r),
                    _ => None,
                })
                .expect("Expected count row");

            assert_eq!(row.get(0), Some("1"));
        }
    });
}

/// Every PGWire socket must keep its own database and authenticated identity.
#[test]
fn test_pgwire_session_state_isolation() {
    // The test exercises the real PGWire listener, which is only available on a local server.
    require_local_server!();

    const DATABASE_A: &str = "pgwire-session-a";
    const DATABASE_B: &str = "pgwire-session-b";
    const PRIVATE_DATABASE: &str = "pgwire-session-private";

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5436)
        .autopublish(false)
        .build();

    test.publish().name(DATABASE_A).clear(true).run().unwrap();
    test.call("test", &[]).unwrap();
    test.sql("UPDATE t_uints SET u8 = 11 WHERE u8 = 105").unwrap();

    test.publish().name(DATABASE_B).clear(true).run().unwrap();
    test.call("test", &[]).unwrap();
    test.sql("UPDATE t_uints SET u8 = 22 WHERE u8 = 105").unwrap();

    test.use_precompiled_module("permissions-private");
    test.publish().name(PRIVATE_DATABASE).clear(true).run().unwrap();
    let owner_token = test.read_token().unwrap();
    test.new_identity().unwrap();
    let guest_token = test.read_token().unwrap();

    let host = test.server_host().split(':').next().unwrap_or("127.0.0.1").to_owned();
    let port = test.pg_port().expect("PostgreSQL wire protocol not enabled");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    runtime.block_on(async move {
        tokio::time::timeout(std::time::Duration::from_secs(30), async move {
            // A query after an incomplete startup must not borrow an authenticated socket's state.
            let owner = connect_pgwire(&host, port, &owner_token, PRIVATE_DATABASE)
                .await
                .unwrap();
            assert_eq!(
                pgwire_column(&owner, "SELECT answer FROM secret").await.unwrap(),
                ["42"]
            );
            let (rows, saw_error) = pgwire_query_without_database(&host, port, "SELECT answer FROM secret")
                .await
                .unwrap();
            assert!(
                rows.is_empty(),
                "PGWIRE_SESSION_DATA_LEAK: unauthenticated socket returned {} data rows",
                rows.len()
            );
            assert!(
                saw_error,
                "PGWIRE_UNAUTHENTICATED_QUERY_ERROR_MISSING: query after incomplete startup returned no error"
            );
            drop(owner);

            // One client may repeat queries without another client's startup changing its session.
            let client_a = connect_pgwire(&host, port, &owner_token, DATABASE_A).await.unwrap();
            assert_eq!(
                pgwire_column(&client_a, "SELECT u8 FROM t_uints").await.unwrap(),
                ["11"]
            );
            assert_eq!(
                pgwire_column(&client_a, "SELECT u8 FROM t_uints").await.unwrap(),
                ["11"]
            );

            // A failed login must not change an already-authenticated client's state.
            assert!(connect_pgwire(&host, port, "invalid_token", DATABASE_B).await.is_err());
            assert_eq!(
                pgwire_column(&client_a, "SELECT u8 FROM t_uints").await.unwrap(),
                ["11"]
            );

            // A later login on another database must not redirect the first client's query.
            let client_b = connect_pgwire(&host, port, &owner_token, DATABASE_B).await.unwrap();
            assert_eq!(
                pgwire_column(&client_b, "SELECT u8 FROM t_uints").await.unwrap(),
                ["22"]
            );
            assert_eq!(
                pgwire_column(&client_a, "SELECT u8 FROM t_uints").await.unwrap(),
                ["11"]
            );

            drop(client_a);
            drop(client_b);

            // Reversing login order must preserve the other socket's database too.
            let client_b = connect_pgwire(&host, port, &owner_token, DATABASE_B).await.unwrap();
            let client_a = connect_pgwire(&host, port, &owner_token, DATABASE_A).await.unwrap();
            assert_eq!(
                pgwire_column(&client_a, "SELECT u8 FROM t_uints").await.unwrap(),
                ["11"]
            );
            assert_eq!(
                pgwire_column(&client_b, "SELECT u8 FROM t_uints").await.unwrap(),
                ["22"]
            );

            drop(client_a);
            drop(client_b);

            // The owner and a non-owner use different credentials on the same database.
            // Private tables are hidden from non-owners, so access fails with "no such table".
            let owner = connect_pgwire(&host, port, &owner_token, PRIVATE_DATABASE)
                .await
                .unwrap();
            let guest = connect_pgwire(&host, port, &guest_token, PRIVATE_DATABASE)
                .await
                .unwrap();
            assert_eq!(
                pgwire_column(&owner, "SELECT answer FROM secret").await.unwrap(),
                ["42"]
            );
            let error = guest
                .simple_query("SELECT answer FROM secret")
                .await
                .expect_err("Guest must not be able to read a private table");
            let db_error = error.as_db_error().expect("Expected a server-side SQL error");
            assert!(
                db_error
                    .message()
                    .contains("no such table: `secret`. If the table exists, it may be marked private."),
                "Expected private-table access denial, got: {db_error}"
            );

            drop(owner);
            drop(guest);

            let guest = connect_pgwire(&host, port, &guest_token, PRIVATE_DATABASE)
                .await
                .unwrap();
            let owner = connect_pgwire(&host, port, &owner_token, PRIVATE_DATABASE)
                .await
                .unwrap();
            assert_eq!(
                pgwire_column(&owner, "SELECT answer FROM secret").await.unwrap(),
                ["42"]
            );
            let error = guest
                .simple_query("SELECT answer FROM secret")
                .await
                .expect_err("Guest must not be able to read a private table");
            let db_error = error.as_db_error().expect("Expected a server-side SQL error");
            assert!(
                db_error
                    .message()
                    .contains("no such table: `secret`. If the table exists, it may be marked private."),
                "Expected private-table access denial, got: {db_error}"
            );
        })
        .await
        .expect("PGWire session isolation exchange timed out");
    });
}

async fn connect_pgwire(
    host: &str,
    port: u16,
    token: &str,
    database: &str,
) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
    let mut config = tokio_postgres::Config::new();
    config.host(host);
    config.port(port);
    config.user("postgres");
    config.password(token);
    config.dbname(database);

    let (client, connection) = config.connect(tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}

async fn pgwire_column(client: &tokio_postgres::Client, query: &str) -> Result<Vec<String>, tokio_postgres::Error> {
    let messages = client.simple_query(query).await?;
    Ok(messages
        .iter()
        .filter_map(|message| match message {
            tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
            _ => None,
        })
        .collect())
}

async fn pgwire_query_without_database(
    host: &str,
    port: u16,
    query: &str,
) -> Result<(Vec<Vec<u8>>, bool), std::io::Error> {
    use tokio::io::AsyncWriteExt;

    let mut stream = tokio::net::TcpStream::connect((host, port)).await?;
    let startup_parameters = b"user\0postgres\0\0";
    let startup_length = 8 + startup_parameters.len();
    stream.write_all(&(startup_length as u32).to_be_bytes()).await?;
    stream.write_all(&196_608u32.to_be_bytes()).await?;
    stream.write_all(startup_parameters).await?;

    let mut saw_startup_error = false;
    loop {
        let (tag, _) = read_pgwire_message(&mut stream).await?;
        saw_startup_error |= tag == b'E';
        if tag == b'Z' {
            break;
        }
    }
    if !saw_startup_error {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "startup without a database did not return an error",
        ));
    }

    let query_length = 4 + query.len() + 1;
    stream.write_all(b"Q").await?;
    stream.write_all(&(query_length as u32).to_be_bytes()).await?;
    stream.write_all(query.as_bytes()).await?;
    stream.write_all(&[0]).await?;

    let mut rows = Vec::new();
    let mut saw_error = false;
    loop {
        let (tag, body) = read_pgwire_message(&mut stream).await?;
        saw_error |= tag == b'E';
        if tag == b'D' {
            rows.push(body);
        }
        if tag == b'Z' {
            break;
        }
    }

    Ok((rows, saw_error))
}

async fn read_pgwire_message(stream: &mut tokio::net::TcpStream) -> Result<(u8, Vec<u8>), std::io::Error> {
    use tokio::io::AsyncReadExt;

    let mut tag = [0u8; 1];
    stream.read_exact(&mut tag).await?;
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length < 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid PGWire response message length",
        ));
    }
    let mut body = vec![0; length - 4];
    stream.read_exact(&mut body).await?;
    Ok((tag[0], body))
}

/// Test failure cases
#[test]
fn test_failures() {
    require_psql!();
    // This requires a local server because we don't have a clean way of providing
    // the remote server's PG port.
    require_local_server!();

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5434) // Use different port from test_sql_format
        .autopublish(false)
        .build();

    test.publish().name("pgwire-failure").clear(true).run().unwrap();

    // Empty query returns empty result
    let output = test.psql("pgwire-failure", "").unwrap();
    assert!(
        output.is_empty(),
        "Expected empty output for empty query, got: {}",
        output
    );

    let result = test.psql_with_token("pgwire-failure", "invalid_token", "SELECT * FROM t_uints");
    assert!(result.is_err(), "Expected error for invalid token");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Invalid token"),
        "Expected 'Invalid token' in error message, got: {}",
        err
    );

    // Returns error for unsupported sql statements
    let result = test.psql(
        "pgwire-failure",
        "SELECT CASE a WHEN 1 THEN 'one' ELSE 'other' END FROM t_uints",
    );
    assert!(result.is_err(), "Expected error for unsupported SQL");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Unsupported") || err.contains("unsupported"),
        "Expected 'Unsupported' in error message, got: {}",
        err
    );

    // And prepared statements
    let result = test.psql("pgwire-failure", "SELECT * FROM t_uints where u8 = $1");
    assert!(result.is_err(), "Expected error for prepared statement");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Unsupported") || err.contains("unsupported"),
        "Expected 'Unsupported' in error message, got: {}",
        err
    );
}
