namespace SpacetimeDB.Testing;

using SpacetimeDB.Internal;

/// <summary>Configures a procedure context without changing other contexts on the fixture.</summary>
public sealed class ProcedureContextBuilder
{
    private readonly ModuleTestContext _fixture;
    private readonly TestAuth _auth;
    private Func<ModuleTestContext, HttpRequest, Result<HttpResponse, HttpError>>? _responder;
    private Action<ModuleTestContext>? _afterCommit;

    internal ProcedureContextBuilder(ModuleTestContext fixture, TestAuth auth)
    {
        this._fixture = fixture;
        this._auth = auth;
    }

    /// <summary>Sets a responder. Exceptions, including assertion failures, propagate unchanged.</summary>
    public ProcedureContextBuilder Http(
        Func<ModuleTestContext, HttpRequest, Result<HttpResponse, HttpError>> responder
    )
    {
        ArgumentNullException.ThrowIfNull(responder);
        this._responder = responder;
        return this;
    }

    /// <summary>
    /// Sets a synchronous callback run after each successful transaction closes, replacing any
    /// earlier one. Exceptions propagate without undoing the commit.
    /// </summary>
    public ProcedureContextBuilder AfterTxCommit(Action<ModuleTestContext> callback)
    {
        TransactionCallback.Validate(callback, typeof(void));
        _afterCommit = callback;
        return this;
    }

    public ProcedureContext Build() => _fixture.BuildProcedureContext(_auth, _responder, _afterCommit);
}
