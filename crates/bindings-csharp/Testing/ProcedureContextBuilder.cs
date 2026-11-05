namespace SpacetimeDB.Testing;

using SpacetimeDB.Internal;

/// <summary>Configures a procedure context without changing other contexts on the fixture.</summary>
public sealed class ProcedureContextBuilder
{
    private readonly ModuleTestContext fixture;
    private readonly TestAuth auth;
    private Func<ModuleTestContext, HttpRequest, Result<HttpResponse, HttpError>>? responder;
    private Action<ModuleTestContext>? afterCommit;

    internal ProcedureContextBuilder(ModuleTestContext fixture, TestAuth auth)
    {
        this.fixture = fixture;
        this.auth = auth;
    }

    /// <summary>Sets a responder. Exceptions, including assertion failures, propagate unchanged.</summary>
    public ProcedureContextBuilder Http(
        Func<ModuleTestContext, HttpRequest, Result<HttpResponse, HttpError>> responder
    )
    {
        ArgumentNullException.ThrowIfNull(responder);
        this.responder = responder;
        return this;
    }

    /// <summary>
    /// Sets a synchronous callback run after each successful transaction closes, replacing any
    /// earlier one. Exceptions propagate without undoing the commit.
    /// </summary>
    public ProcedureContextBuilder AfterTxCommit(Action<ModuleTestContext> callback)
    {
        TransactionCallback.Validate(callback, typeof(void));
        afterCommit = callback;
        return this;
    }

    public ProcedureContext Build() => fixture.BuildProcedureContext(auth, responder, afterCommit);
}
