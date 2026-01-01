namespace SpacetimeDB;

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using Internal;
using SpacetimeDB.BSATN;

public enum HttpVersion : byte
{
    Http09,
    Http10,
    Http11,
    Http2,
    Http3,
}

public readonly record struct HttpMethod(string Value)
{
    public static readonly HttpMethod Get = new("GET");
    public static readonly HttpMethod Head = new("HEAD");
    public static readonly HttpMethod Post = new("POST");
    public static readonly HttpMethod Put = new("PUT");
    public static readonly HttpMethod Delete = new("DELETE");
    public static readonly HttpMethod Connect = new("CONNECT");
    public static readonly HttpMethod Options = new("OPTIONS");
    public static readonly HttpMethod Trace = new("TRACE");
    public static readonly HttpMethod Patch = new("PATCH");
}

// `IsSensitive` is a local-only hint. The current stable HTTP wire format does not carry
// header sensitivity metadata (Rust wire type uses only (name: string, value: bytes)),
// so this flag is not transmitted to the host.
public readonly record struct HttpHeader(string Name, byte[] Value, bool IsSensitive = false)
{
    public HttpHeader(string name, string value)
        : this(name, Encoding.ASCII.GetBytes(value), false) { }
}

public readonly record struct HttpBody(byte[] Bytes)
{
    public static HttpBody Empty => new(Array.Empty<byte>());

    public byte[] ToBytes() => Bytes;

    public string ToStringUtf8Lossy() => Encoding.UTF8.GetString(Bytes);

    public static HttpBody FromString(string s) => new(Encoding.UTF8.GetBytes(s));
}

public sealed class HttpRequest
{
    public required string Uri { get; init; }
    public HttpMethod Method { get; init; } = HttpMethod.Get;
    public List<HttpHeader> Headers { get; init; } = new();
    public HttpBody Body { get; init; } = HttpBody.Empty;
    public HttpVersion Version { get; init; } = HttpVersion.Http11;
    public TimeSpan? Timeout { get; init; }
}

public readonly record struct HttpResponse(
    ushort StatusCode,
    HttpVersion Version,
    List<HttpHeader> Headers,
    HttpBody Body
);

public sealed class HttpError(string message) : Exception(message)
{
    private readonly string message = message;

    public override string Message => message;
}

public sealed class HttpClient
{
    private static readonly TimeSpan MaxTimeout = TimeSpan.FromMilliseconds(500);

    public Result<HttpResponse, HttpError> Get(string uri, TimeSpan? timeout = null) =>
        Send(
            new HttpRequest
            {
                Uri = uri,
                Method = HttpMethod.Get,
                Body = HttpBody.Empty,
                Timeout = timeout,
            }
        );

    public Result<HttpResponse, HttpError> Send(HttpRequest request)
    {
        // The host syscall expects BSATN-encoded spacetimedb_lib::http::Request bytes.
        // A mismatch in the wire layout can cause the host to trap during BSATN decode,
        // so the C# `Http*Wire` types must remain in lockstep with the Rust definitions.
        try
        {
            if (string.IsNullOrEmpty(request.Uri))
            {
                return Result<HttpResponse, HttpError>.Err(
                    new HttpError("URI must not be null or empty")
                );
            }

            // The host clamps all HTTP timeouts to a maximum of 500ms.
            // Clamp here as well to keep C# behavior aligned with the Rust docs and to reduce surprises.
            var timeout = request.Timeout;
            if (timeout is not null)
            {
                if (timeout.Value < TimeSpan.Zero)
                {
                    return Result<HttpResponse, HttpError>.Err(
                        new HttpError("Timeout must not be negative")
                    );
                }

                if (timeout.Value > MaxTimeout)
                {
                    timeout = MaxTimeout;
                }
            }

            var requestWire = new HttpRequestWire
            {
                Method = ToWireMethod(request.Method),
                Headers = new HttpHeadersWire
                {
                    Entries = request.Headers.Select(ToWireHeader).ToArray(),
                },
                Timeout = timeout is null
                    ? null
                    : new HttpTimeoutWire { Timeout = (TimeDuration)timeout.Value },
                Uri = request.Uri,
                Version = ToWireVersion(request.Version),
            };

            var requestBytes = IStructuralReadWrite.ToBytes(
                new HttpRequestWire.BSATN(),
                requestWire
            );
            var bodyBytes = request.Body.ToBytes();

            var status = FFI.procedure_http_request(
                requestBytes,
                (uint)requestBytes.Length,
                bodyBytes,
                (uint)bodyBytes.Length,
                out var out_
            );

            switch (status)
            {
                case Errno.OK:
                {
                    var responseWireBytes = out_.A.Consume();
                    var responseWire = FromBytes(new HttpResponseWire.BSATN(), responseWireBytes);

                    var body = new HttpBody(out_.B.Consume());
                    var (statusCode, version, headers) = FromWireResponse(responseWire);

                    return Result<HttpResponse, HttpError>.Ok(
                        new HttpResponse(statusCode, version, headers, body)
                    );
                }
                case Errno.HTTP_ERROR:
                {
                    var errorWireBytes = out_.A.Consume();
                    var err = FromBytes(new SpacetimeDB.BSATN.String(), errorWireBytes);
                    return Result<HttpResponse, HttpError>.Err(new HttpError(err));
                }
                case Errno.WOULD_BLOCK_TRANSACTION:
                    return Result<HttpResponse, HttpError>.Err(
                        new HttpError(
                            "HTTP requests cannot be performed while a mutable transaction is open (WOULD_BLOCK_TRANSACTION)."
                        )
                    );
                default:
                    return Result<HttpResponse, HttpError>.Err(
                        new HttpError(FFI.ErrnoHelpers.ToException(status).ToString())
                    );
            }
        }
        // Important: avoid throwing across the procedure boundary.
        // Throwing here would trap the module (and fail the whole procedure invocation).
        // Convert all unexpected failures (including decode errors / unexpected errno) into Result.Err instead.
        catch (Exception ex)
        {
            return Result<HttpResponse, HttpError>.Err(new HttpError(ex.ToString()));
        }
    }

    private static T FromBytes<T>(IReadWrite<T> rw, byte[] bytes)
    {
        using var ms = new MemoryStream(bytes);
        using var reader = new BinaryReader(ms);
        var value = rw.Read(reader);
        if (ms.Position != ms.Length)
        {
            throw new InvalidOperationException(
                "Unrecognized extra bytes while decoding BSATN value"
            );
        }
        return value;
    }

    private static HttpMethodWire ToWireMethod(HttpMethod method)
    {
        var m = method.Value;
        return m switch
        {
            "GET" => new HttpMethodWire.Get(default),
            "HEAD" => new HttpMethodWire.Head(default),
            "POST" => new HttpMethodWire.Post(default),
            "PUT" => new HttpMethodWire.Put(default),
            "DELETE" => new HttpMethodWire.Delete(default),
            "CONNECT" => new HttpMethodWire.Connect(default),
            "OPTIONS" => new HttpMethodWire.Options(default),
            "TRACE" => new HttpMethodWire.Trace(default),
            "PATCH" => new HttpMethodWire.Patch(default),
            _ => new HttpMethodWire.Extension(m),
        };
    }

    private static HttpVersionWire ToWireVersion(HttpVersion version) =>
        version switch
        {
            HttpVersion.Http09 => HttpVersionWire.Http09,
            HttpVersion.Http10 => HttpVersionWire.Http10,
            HttpVersion.Http11 => HttpVersionWire.Http11,
            HttpVersion.Http2 => HttpVersionWire.Http2,
            HttpVersion.Http3 => HttpVersionWire.Http3,
            _ => throw new ArgumentOutOfRangeException(nameof(version)),
        };

    private static HttpHeaderPairWire ToWireHeader(HttpHeader header) =>
        new() { Name = header.Name, Value = header.Value };

    private static (
        ushort statusCode,
        HttpVersion version,
        List<HttpHeader> headers
    ) FromWireResponse(HttpResponseWire responseWire)
    {
        var version = responseWire.Version switch
        {
            HttpVersionWire.Http09 => HttpVersion.Http09,
            HttpVersionWire.Http10 => HttpVersion.Http10,
            HttpVersionWire.Http11 => HttpVersion.Http11,
            HttpVersionWire.Http2 => HttpVersion.Http2,
            HttpVersionWire.Http3 => HttpVersion.Http3,
            _ => throw new InvalidOperationException("Invalid HTTP version returned from host"),
        };

        var headers = responseWire
            .Headers.Entries.Select(h => new HttpHeader(h.Name, h.Value, false))
            .ToList();

        return (responseWire.Code, version, headers);
    }
}
