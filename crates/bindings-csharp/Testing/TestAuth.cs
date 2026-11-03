namespace SpacetimeDB.Testing;

/// <summary>Immutable caller credentials for a test invocation.</summary>
public sealed class TestAuth
{
    private static readonly TestAuth internalAuth = new();

    private TestAuth() { }

    internal AuthCtx SenderAuth { get; } = AuthCtx.InternalForTesting();

    /// <summary>Calls as the fixture's database identity, without a connection or JWT.</summary>
    public static TestAuth Internal() => internalAuth;
}
