using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [Table(Accessor = "UploadedAsset", Public = true)]
    public partial struct UploadedAsset
    {
        [PrimaryKey] public ulong Id;
        public string Url;
        public ulong Size;
        public ushort Status;
        public bool ResponseBodyPresent;
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Upload(HandlerContext ctx, HttpRequest request) => new(
        201, HttpVersion.Http11, new(), HttpBody.FromString("https://files.local/object-1")
    );

    [SpacetimeDB.HttpRouter]
    public static Router Routes() => SpacetimeDB.Router.New().Post("/upload", Handlers.Upload);

    [SpacetimeDB.Procedure]
    public static string UploadAndRegister(ProcedureContext ctx, string uploadUrl, byte[] data)
    {
        var request = new HttpRequest {
            Uri = uploadUrl,
            Method = SpacetimeDB.HttpMethod.Post,
            Headers = new() { new HttpHeader("content-type", "application/octet-stream") },
            Body = new HttpBody(data),
        };
        return ctx.Http.Send(request).Match(response => {
            if (response.StatusCode < 200 || response.StatusCode >= 300) throw new Exception($"upload failed: {response.StatusCode}");
            var responseBodyPresent = response.Body.ToBytes().Length > 0;
            ctx.WithTx(tx => {
                tx.Db.UploadedAsset.Insert(new UploadedAsset {
                    Id = 1,
                    Url = uploadUrl,
                    Size = (ulong)data.Length,
                    Status = response.StatusCode,
                    ResponseBodyPresent = responseBodyPresent,
                });
                return 0;
            });
            return uploadUrl;
        }, error => throw new Exception(error.Message));
    }
}
