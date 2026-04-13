namespace SpacetimeDB;

using System.ComponentModel;

// NOTE: These types define the stable BSATN wire format for the procedure_http_request ABI.
// They must match `spacetimedb_lib::http::{Request, Response}` exactly (field order + types),
// because the host BSATN-decodes these bytes directly and may trap on mismatch.
// Do not reorder fields or extend these types; add a new versioned ABI instead.

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public partial record HttpMethodWire
    : TaggedEnum<(
        Unit Get,
        Unit Head,
        Unit Post,
        Unit Put,
        Unit Delete,
        Unit Connect,
        Unit Options,
        Unit Trace,
        Unit Patch,
        string Extension
    )>;

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public enum HttpVersionWire : byte
{
    Http09,
    Http10,
    Http11,
    Http2,
    Http3,
}

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public partial struct HttpHeaderPairWire
{
    public string Name;
    public byte[] Value;
}

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public partial struct HttpHeadersWire
{
    public HttpHeaderPairWire[] Entries;
}

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public partial struct HttpTimeoutWire
{
    public TimeDuration Timeout;
}

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public partial struct HttpRequestWire
{
    public HttpMethodWire Method;
    public HttpHeadersWire Headers;
    public HttpTimeoutWire? Timeout;
    public string Uri;
    public HttpVersionWire Version;
}

[Type]
[EditorBrowsable(EditorBrowsableState.Never)]
public partial struct HttpResponseWire
{
    public HttpHeadersWire Headers;
    public HttpVersionWire Version;
    public ushort Code;
}
