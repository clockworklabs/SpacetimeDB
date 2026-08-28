using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [Table(Accessor = "FetchedRecord", Public = true)]
    public partial struct FetchedRecord { [PrimaryKey] public ulong Id; public ushort Status; public bool ValidBody; }

    [SpacetimeDB.Procedure]
    public static void FetchAndStore(ProcedureContext ctx, string url)
    {
        var result = ctx.Http.Get(url);
        result.Match(response => {
            var validBody = response.Body.ToStringUtf8Lossy().Contains("Example Domain");
            ctx.WithTx(tx => { tx.Db.FetchedRecord.Insert(new FetchedRecord { Id = 1, Status = response.StatusCode, ValidBody = validBody }); return 0; });
            return 0;
        }, error => throw new Exception(error.Message));
    }
}
