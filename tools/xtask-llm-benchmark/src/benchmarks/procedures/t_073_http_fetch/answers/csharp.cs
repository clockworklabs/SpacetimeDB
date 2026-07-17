using SpacetimeDB;
using System.Text;
#pragma warning disable STDB_UNSTABLE

[SpacetimeDB.Type]
public partial struct FetchSummary { public ushort Status; public bool HtmlContentType; public bool HasExampleDomain; }

public static partial class Module
{
    [SpacetimeDB.Procedure]
    public static FetchSummary FetchPageSummary(ProcedureContext ctx, string url)
    {
        var result = ctx.Http.Get(url);
        return result.Match(response => {
            var contentType = response.Headers.FirstOrDefault(h => h.Name.Equals("content-type", StringComparison.OrdinalIgnoreCase));
            var contentTypeValue = contentType.Value is null ? "" : Encoding.UTF8.GetString(contentType.Value);
            var body = response.Body.ToStringUtf8Lossy();
            return new FetchSummary {
                Status = response.StatusCode,
                HtmlContentType = contentTypeValue.Contains("text/html"),
                HasExampleDomain = body.Contains("Example Domain"),
            };
        }, error => throw new Exception(error.Message));
    }
}
