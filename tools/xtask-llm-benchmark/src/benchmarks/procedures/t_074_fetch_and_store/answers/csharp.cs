using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [Table(Accessor = "FetchedRecord", Public = true)]
    public partial struct FetchedRecord { [PrimaryKey] public ulong Id; public ushort Status; public bool ValidSchema; }

    [SpacetimeDB.Procedure]
    public static void FetchAndStore(ProcedureContext ctx, string serverUrl)
    {
        var url = $"{serverUrl.TrimEnd('/')}/v1/database/{ProcedureContextBase.Identity}/schema?version=9";
        var result = ctx.Http.Get(url);
        result.Match(response => {
            var validSchema = response.Body.ToStringUtf8Lossy().Contains("\"tables\"");
            ctx.WithTx(tx => { tx.Db.FetchedRecord.Insert(new FetchedRecord { Id = 1, Status = response.StatusCode, ValidSchema = validSchema }); return 0; });
            return 0;
        }, error => throw new Exception(error.Message));
    }
}
