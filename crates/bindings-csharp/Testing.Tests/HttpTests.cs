namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class HttpTests
{
    [Fact]
    public void RespondersReceiveRequestsAndRemainScopedToBuiltContexts()
    {
        using var test = TestContext.Create(typeof(Entry).Assembly);
        var response = new HttpResponse(
            503,
            HttpVersion.Http2,
            [new HttpHeader("Retry-After", "10")],
            HttpBody.FromString("unavailable")
        );
        var request = new HttpRequest
        {
            Uri = "https://example.invalid/status",
            Method = HttpMethod.Post,
            Headers = [new HttpHeader("Content-Type", "text/plain")],
            Body = HttpBody.FromString("payload"),
            Version = HttpVersion.Http2,
            Timeout = TimeSpan.FromSeconds(300),
        };
        var builder = test.ProcedureContextBuilder(TestAuth.Internal())
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
                    fixture.Db.Entries.Insert(
                        new Entry { Text = "responder can access committed data" }
                    );
                    return Result<HttpResponse, HttpError>.Ok(response);
                }
            );
        var first = builder.Build();
        var transportError = new HttpError("simulated timeout");
        var second = builder
            .Http((_, _) => Result<HttpResponse, HttpError>.Err(transportError))
            .Build();
        Assert.Equal(response, first.Http.Send(request).UnwrapOrThrow());
        Assert.Equal(TimeSpan.FromSeconds(300), request.Timeout);
        Assert.Same(transportError, Error(second.Http.Get(request.Uri)));
        Assert.Equal(
            "no test HTTP responder configured",
            Error(test.ProcedureContext(TestAuth.Internal()).Http.Get(request.Uri)).Message
        );
        var failure = new InvalidOperationException("assertion failure");
        var throwing = builder.Http((_, _) => throw failure).Build();
        Assert.Same(
            failure,
            Assert.Throws<InvalidOperationException>(() => throwing.Http.Get(request.Uri))
        );
        Assert.Equal(1UL, test.Db.Entries.Count);
    }

    [Fact]
    public void HttpRejectsInvalidRequestsActiveTransactionsAndDisposedFixtures()
    {
        using var test = TestContext.Create(typeof(Entry).Assembly);
        var calls = 0;
        var builder = test.ProcedureContextBuilder(TestAuth.Internal())
            .Http(
                (_, _) =>
                {
                    calls++;
                    return Result<HttpResponse, HttpError>.Ok(
                        new(200, HttpVersion.Http11, [], HttpBody.Empty)
                    );
                }
            );
        var procedure = builder.Build();
        var selected = ((Internal.IModuleContext<ProcedureContext>)procedure).SelectInstance(1);
        Assert.Equal(1, ((Internal.IModuleContext)selected).InstanceId);
        Assert.Same(procedure.Http, selected.Http);
        Error(procedure.Http.Get(""));
        Error(procedure.Http.Get("https://example.invalid", TimeSpan.FromSeconds(-1)));
        procedure.WithTx(_ =>
        {
            Assert.Contains(
                "WOULD_BLOCK_TRANSACTION",
                Error(selected.Http.Get("https://example.invalid")).Message
            );
            return 0;
        });
        Assert.Throws<InvalidOperationException>(() =>
            test.WithReducerTx(
                TestAuth.Internal(),
                _ =>
                {
                    Assert.Contains(
                        "WOULD_BLOCK_TRANSACTION",
                        Error(procedure.Http.Get("https://example.invalid")).Message
                    );
                    throw new InvalidOperationException("rollback");
                }
            )
        );
        Assert.Equal(0, calls);
        Assert.Equal(
            (ushort)200,
            procedure.Http.Get("https://example.invalid").UnwrapOrThrow().StatusCode
        );
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => procedure.Http.Get("https://example.invalid"));
        Assert.Throws<ObjectDisposedException>(() => selected.Http.Get("https://example.invalid"));
        Assert.Throws<ObjectDisposedException>(() => builder.Build());
        Assert.Equal(1, calls);
    }

    private static HttpError Error(Result<HttpResponse, HttpError> result) =>
        Assert.IsType<Result<HttpResponse, HttpError>.ErrR>(result).Error;
}
