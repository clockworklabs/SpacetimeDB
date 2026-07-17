using SpacetimeDB;
using System.Text;
#pragma warning disable STDB_UNSTABLE

[SpacetimeDB.Type]
public partial struct FetchSummary { public ushort Status; public bool JsonContentType; public bool HasTables; }

public static partial class Module
{
    [SpacetimeDB.Procedure]
    public static FetchSummary FetchSchemaSummary(ProcedureContext ctx, string serverUrl)
    {
        var url = $"{serverUrl.TrimEnd('/')}/v1/database/{ProcedureContextBase.Identity}/schema?version=9";
        var result = ctx.Http.Get(url);
        return result.Match(response => {
            var contentType = response.Headers.FirstOrDefault(h => h.Name.Equals("content-type", StringComparison.OrdinalIgnoreCase));
            var contentTypeValue = contentType.Value is null ? "" : Encoding.UTF8.GetString(contentType.Value);
            var body = response.Body.ToStringUtf8Lossy();
            return new FetchSummary {
                Status = response.StatusCode,
                JsonContentType = contentTypeValue.Contains("application/json"),
                HasTables = body.Contains("\"tables\""),
            };
        }, error => throw new Exception(error.Message));
    }
}
