namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class HttpTests
{
    [Fact]
    public void RespondersReceiveRequestsAndRemainScopedToBuiltContexts()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var response = new HttpResponse(503, HttpVersion.Http2, [], HttpBody.FromString("busy"));
        var request = new HttpRequest
        {
            Uri = "https://example.invalid/status",
            Method = HttpMethod.Post,
            Headers = [new HttpHeader("Content-Type", "text/plain")],
            Body = HttpBody.FromString("payload"),
            Version = HttpVersion.Http2,
            Timeout = TimeSpan.FromSeconds(300),
        };
        var builder = test.CreateProcedureContextBuilder(TestAuth.Internal())
            .Http(
                (fixture, received) =>
                {
                    Assert.Same(test, fixture);
                    Assert.Equal(request.Uri, received.Uri);
                    Assert.Equal(request.Method, received.Method);
                    Assert.Equal(request.Headers, received.Headers);
                    Assert.Equal(request.Body, received.Body);
                    Assert.Equal(request.Version, received.Version);
                    Assert.Equal(TimeSpan.FromSeconds(180), received.Timeout);
                    return Result<HttpResponse, HttpError>.Ok(response);
                }
            );
        var built = builder.Build();
        var failure = new InvalidOperationException("assertion failure");
        var throwing = builder.Http((_, _) => throw failure).Build();

        // The built context keeps its responder; the caller's request is not modified.
        Assert.Equal(response, built.Http.Send(request).UnwrapOrThrow());
        Assert.Equal(TimeSpan.FromSeconds(300), request.Timeout);
        Assert.Same(
            failure,
            Assert.Throws<InvalidOperationException>(() => throwing.Http.Get(request.Uri))
        );
        Assert.Equal(
            "no test HTTP responder configured",
            Error(test.CreateProcedureContext(TestAuth.Internal()).Http.Get(request.Uri)).Message
        );
    }

    [Fact]
    public void HttpRejectsInvalidRequestsActiveTransactionsAndDisposedFixtures()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var calls = 0;
        var procedure = test.CreateProcedureContextBuilder(TestAuth.Internal())
            .Http(
                (_, _) =>
                {
                    calls++;
                    return Result<HttpResponse, HttpError>.Ok(
                        new(200, HttpVersion.Http11, [], HttpBody.Empty)
                    );
                }
            )
            .Build();
        Assert.Equal("URI must not be null or empty", Error(procedure.Http.Get("")).Message);
        Assert.Equal(
            "Timeout must not be negative",
            Error(procedure.Http.Get("https://example.invalid", TimeSpan.FromSeconds(-1))).Message
        );
        procedure.WithTx(_ =>
        {
            Assert.Contains(
                "WOULD_BLOCK_TRANSACTION",
                Error(procedure.Http.Get("https://example.invalid")).Message
            );
            // As in WASM, request validation runs before the transaction check.
            Assert.Equal("URI must not be null or empty", Error(procedure.Http.Get("")).Message);
            return 0;
        });
        Assert.Equal(0, calls);
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => procedure.Http.Get("https://example.invalid"));
    }

    private static HttpError Error(Result<HttpResponse, HttpError> result) =>
        Assert.IsType<Result<HttpResponse, HttpError>.ErrR>(result).Error;
}
