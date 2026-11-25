class Program
{
    static void Main()
    {
        var conn = new SpacetimeDB.DbConnection();
        try
        {
            if (conn.Db.myLib.LibData.RemoteTableName != "my_lib.libData")
                throw new System.Exception(conn.Db.myLib.LibData.RemoteTableName);
            if (
                (
                    (SpacetimeDB.IReducerArgs)new SpacetimeDB.myLib.Reducer.LibInsert("value")
                ).ReducerName != "my_lib.lib_insert"
            )
                throw new System.Exception("wrong reducer name");
            if (
                (
                    (SpacetimeDB.IProcedureArgs)new SpacetimeDB.myLib.Procedure.LibCountArgs()
                ).ProcedureName != "my_lib.lib_count"
            )
                throw new System.Exception("wrong procedure name");
            var sql = new SpacetimeDB.QueryBuilder().From.myLib.LibData().ToSql();
            if (!sql.Contains("\"my_lib\".\"libData\"") || sql.Contains("myLib"))
                throw new System.Exception(sql);
            if (
                !System.Linq.Enumerable.Contains(
                    SpacetimeDB.QueryBuilder.AllTablesSqlQueries(),
                    sql
                )
            )
                throw new System.Exception("missing namespace subscription");
        }
        finally
        {
            conn.Disconnect();
        }
    }
}
