namespace SpacetimeDB.Testing.Tests;

using System.Text.Json;
using SpacetimeDB.Internal;
using Module = ChatModule.Module;

public class AcceptanceTests
{
    [Theory]
    [InlineData("https://auth.example.com", "my-app", null)]
    [InlineData(
        "https://auth.example.com",
        "another-app",
        "This token was issued for another application"
    )]
    [InlineData("https://evil.example.com", "my-app", "This token was issued by another provider")]
    public void ChecksConnectionClaims(string issuer, string audience, string? expectedError)
    {
        using var test = TestContext.Create(typeof(Module).Assembly);
        var auth = TestAuth.FromJwtPayload(
            JsonSerializer.Serialize(
                new
                {
                    iss = issuer,
                    sub = "alice",
                    aud = audience,
                }
            ),
            ConnectionId.FromHexString("00000000000000000000000000000001")!.Value
        );
        if (expectedError is null)
            test.WithReducerTx(auth, Module.ClientConnected);
        else
            Assert.Equal(
                expectedError,
                Assert
                    .Throws<InvalidOperationException>(() =>
                    test.WithReducerTx(auth, Module.ClientConnected)
                    )
                    .Message
            );
    }

    [Fact]
    public void ScheduledReducersRunOnlyWhenExplicitlyCalled()
    {
        using var test = TestContext.Create(typeof(Module).Assembly);
        var now = new Timestamp(2_000_000_000);
        int[] ages = [16, 15, 1];
        for (var i = 0; i < ages.Length; i++)
            test.Db.Edit.Insert(
                new Module.Edit
                {
                    Id = (ulong)i + 1,
                    EditedAt = new Timestamp(
                        now.MicrosecondsSinceUnixEpoch - ages[i] * 60L * 1_000_000
                    ),
                    Live = true,
                }
            );
        var timer = test.Db.ExpireTimer.Insert(
            new Module.ExpireTimer
            {
                ScheduledAt = new ScheduleAt.Interval(new TimeDuration(300_000_000)),
            }
        );
        test.Clock.Set(now);
        Assert.All(test.Db.Edit.Iter(), edit => Assert.True(edit.Live));
        test.WithReducerTx(TestAuth.Internal(), ctx => Module.ExpireOldEdits(ctx, timer));
        Assert.Equal(
            [false, true, true],
            test.Db.Edit.Iter().OrderBy(edit => edit.Id).Select(edit => edit.Live)
        );
        Assert.Equal(timer, Assert.Single(test.Db.ExpireTimer.Iter()));
    }

    [Fact]
    public void DocumentedProcedureRecordsHttpAndTransportFailures()
    {
        using var test = TestContext.Create(typeof(Module).Assembly);
        var now = new Timestamp(100);
        test.Clock.Set(now);
        Result<HttpResponse, HttpError>[] replies =
        [
            Result<HttpResponse, HttpError>.Ok(new(503, HttpVersion.Http11, [], HttpBody.Empty)),
            Result<HttpResponse, HttpError>.Err(new HttpError("connection reset")),
            Result<HttpResponse, HttpError>.Ok(new(200, HttpVersion.Http11, [], HttpBody.Empty)),
        ];
        var requests = 0;
        var commits = 0;
        var context = test.ProcedureContextBuilder(TestAuth.Internal())
            .Http(
                (_, request) =>
                {
                    Assert.Equal("https://example.invalid/status", request.Uri);
                    return replies[requests++];
                }
            )
            .Hooks(
                new ProcedureTestHooks().AfterTxCommit(fixture =>
                {
                    commits++;
                    fixture.Clock.Advance(new TimeDuration(10));
                })
            )
            .Build();
        foreach (var _ in replies)
            Module.CheckStatus(context);
        Assert.Equal(3, requests);
        Assert.Equal(2, commits);
        Assert.Equal(
            [now, new Timestamp(110)],
            test.Db.FetchFailure.Iter().OrderBy(row => row.Id).Select(row => row.At)
        );
    }

    [Fact]
    public void UnsupportedServicesFailWithoutHostImports()
    {
        Assert.Throws<NotSupportedException>(() => ProcedureContext.Identity);
        Assert.Throws<NotSupportedException>(() => IReducerContext.GetDatabaseIdentity());
        Assert.Throws<NotSupportedException>(() => IViewContext.GetIdentity());
        Assert.Throws<NotSupportedException>(() => new LogStopwatch("unsupported"));
        var connection = ConnectionId.FromHexString("00000000000000000000000000000001")!.Value;
        Assert.Throws<NotSupportedException>(() =>
            AuthCtx.BuildFromSystemTables(connection, default)
        );
#pragma warning disable STDB_UNSTABLE
        using var test = TestContext.Create(typeof(Module).Assembly);
        var procedure = test.ProcedureContext(TestAuth.Internal());
        var selected = ((IModuleContext<ProcedureContext>)procedure).SelectInstance(1);
        Assert.Throws<NotSupportedException>(() =>
            Module.VolatileNonatomicScheduleImmediateSendMessage(selected, "message")
        );
        Assert.Throws<NotSupportedException>(() =>
            Module.VolatileNonatomicScheduleImmediateCheckStatus(selected)
        );
        Assert.Contains(
            "VolatileNonatomicScheduleImmediate",
            Assert
                .Throws<NotSupportedException>(() =>
                    Module.VolatileNonatomicScheduleImmediateSendMessage("message")
                )
                .Message
        );
        Assert.Contains(
            "VolatileNonatomicScheduleImmediate",
            Assert
                .Throws<NotSupportedException>(Module.VolatileNonatomicScheduleImmediateCheckStatus)
                .Message
        );
#pragma warning restore STDB_UNSTABLE
    }
}
