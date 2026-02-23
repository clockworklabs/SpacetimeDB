export async function sql_single_statement(
  conn: unknown,
  from: number,
  to: number,
  amount: number,
): Promise<void> {
  if (from === to || amount <= 0) return;

  const db = conn as any;
  const isSqlite = !!db?.prepare || db?.name === 'sqlite';
  if (!db?.exec)
    throw new Error('sql_single_statement requires a SqlConnector');

  const pgSql = `
    WITH
      locks AS (
        SELECT id, balance
        FROM accounts
        WHERE id IN ($1, $2)
        ORDER BY id
      FOR UPDATE
      ),
      ok AS (
        SELECT 1
        WHERE (SELECT balance FROM locks WHERE id = $1) >= $3
      AND EXISTS (SELECT 1 FROM locks WHERE id = $2)
      )
    UPDATE accounts
    SET balance = CASE
                    WHEN id = $1 THEN balance - $3
                    WHEN id = $2 THEN balance + $3
      END
    WHERE id IN ($1, $2)
      AND EXISTS (SELECT 1 FROM ok)
      RETURNING id;
  `;

  const sqliteSql = `
    UPDATE accounts
    SET balance = CASE
                    WHEN id = ?1 THEN balance - ?3
                    WHEN id = ?2 THEN balance + ?3
      END
    WHERE id IN (?1, ?2)
      AND EXISTS (
      SELECT 1
      FROM accounts AS from_acct
             JOIN accounts AS to_acct ON to_acct.id = ?2
      WHERE from_acct.id = ?1
        AND from_acct.balance >= ?3
    )
      RETURNING id;
  `;

  const sql = isSqlite ? sqliteSql : pgSql;
  const params = [from, to, amount];

  const result = await db.exec(sql, params);

  if (result.length === 0) return;
}
