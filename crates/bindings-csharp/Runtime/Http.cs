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

/// <summary>
/// Represents an HTTP method (e.g. GET, POST).
/// </summary>
/// <remarks>
/// Unknown methods are supported by providing an arbitrary string value.
/// </remarks>
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

/// <summary>
/// Represents an HTTP header name/value pair.
/// </summary>
/// <remarks>
/// Multiple headers with the same name are permitted.
/// The <c>IsSensitive</c> flag is a local-only hint and is not transmitted to the host.
/// </remarks>
public readonly record struct HttpHeader(string Name, byte[] Value, bool IsSensitive = false)
{
    public HttpHeader(string name, string value)
        : this(name, Encoding.ASCII.GetBytes(value), false) { }
}

/// <summary>
/// Represents the body of an HTTP request or response.
/// </summary>
/// <remarks>
/// Bodies are treated as raw bytes. Use <see cref="ToStringUtf8Lossy"/> when interpreting a body as UTF-8 text.
/// </remarks>
public readonly record struct HttpBody(byte[] Bytes)
{
    public static HttpBody Empty => new(Array.Empty<byte>());

    public byte[] ToBytes() => Bytes;

    public string ToStringUtf8Lossy() => Encoding.UTF8.GetString(Bytes);

    public static HttpBody FromString(string s) => new(Encoding.UTF8.GetBytes(s));
}

/// <summary>
/// Represents an HTTP request to be executed by the SpacetimeDB host from within a procedure.
/// </summary>
/// <remarks>
/// The request body is stored separately from the request metadata in the host ABI.
/// </remarks>
public sealed class HttpRequest
{
    /// <summary>Request URI.</summary>
    /// <remarks>Must not be null or empty.</remarks>
    public required string Uri { get; init; }

    /// <summary>HTTP method to use (e.g. GET, POST).</summary>
    public HttpMethod Method { get; init; } = HttpMethod.Get;

    /// <summary>HTTP headers to include with the request.</summary>
    public List<HttpHeader> Headers { get; init; } = new();

    /// <summary>Request body bytes.</summary>
    public HttpBody Body { get; init; } = HttpBody.Empty;

    /// <summary>HTTP version to report in the request metadata.</summary>
    public HttpVersion Version { get; init; } = HttpVersion.Http11;

    /// <summary>
    /// Optional timeout for the request.
    /// </summary>
    /// <remarks>
    /// The SpacetimeDB host clamps all timeouts to a maximum of 500ms.
    /// </remarks>
    public TimeSpan? Timeout { get; init; }
}

/// <summary>
/// Represents an HTTP response returned by the SpacetimeDB host.
/// </summary>
/// <remarks>
/// A non-2xx status code is still returned as a successful response; callers should inspect
/// <see cref="StatusCode"/> to handle application-level errors from the remote server.
/// </remarks>
public readonly record struct HttpResponse(
    ushort StatusCode,
    HttpVersion Version,
    List<HttpHeader> Headers,
    HttpBody Body
);

/// <summary>
/// Error returned when the SpacetimeDB host could not execute an HTTP request.
/// </summary>
/// <remarks>
/// This indicates a failure to perform the request (e.g. DNS failure, connection error, timeout),
/// not an application-level HTTP error response (which is represented by <see cref="HttpResponse.StatusCode"/>).
/// </remarks>
public sealed class HttpError(string message) : Exception(message)
{
    private readonly string message = message;

    public override string Message => message;
}

/// <summary>
/// Allows a procedure to perform outbound HTTP requests via the host.
/// </summary>
/// <remarks>
/// This API is available from <c>ProcedureContext.Http</c>.
///
/// The request metadata (method/headers/timeout/uri/version) is encoded using a stable wire format
/// and executed by the SpacetimeDB host. The request body is sent separately as raw bytes.
///
/// <para>
/// <b>Transaction limitation:</b> HTTP requests cannot be performed while a mutable transaction is open.
/// If called inside <c>WithTx</c>, the host will reject the call (<c>WOULD_BLOCK_TRANSACTION</c>).
/// </para>
///
/// <para>
/// <b>Timeouts:</b> The host clamps all HTTP timeouts to a maximum of 500ms.
/// </para>
///
/// <para>
/// The returned response may have any HTTP status code (including non-2xx). This is still considered a
/// successful HTTP exchange; <see cref="Send"/> only returns an error when the request could not be
/// initiated or completed (e.g. DNS failure, connection failure, timeout).
/// </para>
/// </remarks>
public sealed class HttpClient
{
    private static readonly TimeSpan MaxTimeout = TimeSpan.FromMilliseconds(500);

    /// <summary>
    /// Send a simple <c>GET</c> request to <paramref name="uri"/> with no headers.
    /// </summary>
    /// <param name="uri">The request URI.</param>
    /// <param name="timeout">
    /// Optional timeout for the request. The host clamps timeouts to a maximum of 500ms.
    /// </param>
    /// <returns>
    /// <c>Ok(HttpResponse)</c> when a response was received (regardless of HTTP status code),
    /// or <c>Err(HttpError)</c> if the request failed to execute.
    /// </returns>
    /// <example>
    /// <code>
    /// [SpacetimeDB.Procedure]
    /// public static string FetchSchema(ProcedureContext ctx)
    /// {
    ///     var result = ctx.Http.Get("http://localhost:3000/v1/database/schema");
    ///     if (!result.IsSuccess)
    ///     {
    ///         return $"ERR {result.Error}";
    ///     }
    ///
    ///     var response = result.Value!;
    ///     return response.Body.ToStringUtf8Lossy();
    /// }
    /// </code>
    /// </example>
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

    /// <summary>
    /// Send an HTTP request described by <paramref name="request"/> and wait for its response.
    /// </summary>
    /// <param name="request">
    /// Request metadata (method, headers, uri, version, optional timeout) plus a request body.
    /// </param>
    /// <returns>
    /// <c>Ok(HttpResponse)</c> when a response was received (including non-2xx status codes),
    /// or <c>Err(HttpError)</c> when the host could not perform the request.
    /// </returns>
    /// <remarks>
    /// This method does not throw for expected failures; errors are returned as <c>Result.Err</c>.
    /// </remarks>
    /// <example>
    /// <code>
    /// [SpacetimeDB.Procedure]
    /// public static string PostSomething(ProcedureContext ctx)
    /// {
    ///     var request = new HttpRequest
    ///     {
    ///         Uri = "https://some-remote-host.invalid/upload",
    ///         Method = new HttpMethod("POST"),
    ///         Headers = new()
    ///         {
    ///             new HttpHeader("Content-Type", "text/plain"),
    ///         },
    ///         Body = HttpBody.FromString("This is the body of the HTTP request"),
    ///         Timeout = TimeSpan.FromMilliseconds(100),
    ///     };
    ///
    ///     var result = ctx.Http.Send(request);
    ///     if (!result.IsSuccess)
    ///     {
    ///         return $"ERR {result.Error}";
    ///     }
    ///
    ///     var response = result.Value!;
    ///     return $"OK status={response.StatusCode} body={response.Body.ToStringUtf8Lossy()}";
    /// }
    /// </code>
    /// </example>
    /// <example>
    /// <code>
    /// [SpacetimeDB.Procedure]
    /// public static string FetchMay404(ProcedureContext ctx)
    /// {
    ///     var result = ctx.Http.Get("https://example.invalid/missing");
    ///     if (!result.IsSuccess)
    ///     {
    ///         // DNS failure, connection drop, timeout, etc.
    ///         return $"ERR transport: {result.Error}";
    ///     }
    ///
    ///     var response = result.Value!;
    ///     if (response.StatusCode != 200)
    ///     {
    ///         // Application-level HTTP error response.
    ///         return $"ERR http status={response.StatusCode}";
    ///     }
    ///
    ///     return $"OK {response.Body.ToStringUtf8Lossy()}";
    /// }
    /// </code>
    /// </example>
    /// <example>
    /// <code>
    /// [SpacetimeDB.Procedure]
    /// public static void DontDoThis(ProcedureContext ctx)
    /// {
    ///     ctx.WithTx(tx =>
    ///     {
    ///         // The host rejects this with WOULD_BLOCK_TRANSACTION.
    ///         var _ = ctx.Http.Get("https://example.invalid/");
    ///         return 0;
    ///     });
    /// }
    /// </code>
    /// </example>
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
