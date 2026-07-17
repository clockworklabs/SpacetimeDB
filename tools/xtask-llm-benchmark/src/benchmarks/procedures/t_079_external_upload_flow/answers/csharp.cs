using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [Table(Accessor = "UploadedAsset", Public = true)]
    public partial struct UploadedAsset { [PrimaryKey] public ulong Id; public string Url; public ulong Size; }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Upload(HandlerContext ctx, HttpRequest request) => new(
        201, HttpVersion.Http11, new(), HttpBody.FromString("https://files.local/object-1")
    );

    [SpacetimeDB.HttpRouter]
    public static Router Routes() => SpacetimeDB.Router.New().Post("/upload", Handlers.Upload);

    [SpacetimeDB.Procedure]
    public static string UploadAndRegister(ProcedureContext ctx, string serverUrl, byte[] data)
    {
        var request = new HttpRequest {
            Uri = $"{serverUrl.TrimEnd('/')}/v1/database/{ProcedureContextBase.Identity}/route/upload",
            Method = SpacetimeDB.HttpMethod.Post,
            Headers = new() { new HttpHeader("content-type", "application/octet-stream") },
            Body = new HttpBody(data),
        };
        return ctx.Http.Send(request).Match(response => {
            var assetUrl = response.Body.ToStringUtf8Lossy();
            ctx.WithTx(tx => { tx.Db.UploadedAsset.Insert(new UploadedAsset { Id = 1, Url = assetUrl, Size = (ulong)data.Length }); return 0; });
            return assetUrl;
        }, error => throw new Exception(error.Message));
    }
}
