namespace SpacetimeDB.Testing;

/// <summary>Configures a procedure context without changing other contexts on the fixture.</summary>
public sealed class ProcedureContextBuilder
{
    private readonly TestContext fixture;
    private readonly TestAuth auth;
    private Func<TestContext, HttpRequest, Result<HttpResponse, HttpError>>? responder;
    private ProcedureTestHooks? hooks;

    internal ProcedureContextBuilder(TestContext fixture, TestAuth auth)
    {
        this.fixture = fixture;
        this.auth = auth;
    }

    /// <summary>Sets a responder. Exceptions, including assertion failures, propagate unchanged.</summary>
    public ProcedureContextBuilder Http(
        Func<TestContext, HttpRequest, Result<HttpResponse, HttpError>> responder
    )
    {
        ArgumentNullException.ThrowIfNull(responder);
        this.responder = responder;
        return this;
    }

    /// <summary>Sets hooks captured when the context is built.</summary>
    public ProcedureContextBuilder Hooks(ProcedureTestHooks hooks)
    {
        ArgumentNullException.ThrowIfNull(hooks);
        this.hooks = hooks;
        return this;
    }

    public ProcedureContext Build() =>
        fixture.CreateProcedureContext(auth, responder, hooks?.AfterCommit);
}
