namespace SpacetimeDB.Testing;

using SpacetimeDB.Internal;

/// <summary>Hooks for observing and changing fixture state between procedure transactions.</summary>
public sealed class ProcedureTestHooks
{
    internal Action<TestContext>? AfterCommit { get; private set; }

    /// <summary>
    /// Sets a synchronous callback run after each successful transaction closes.
    /// Exceptions propagate without undoing the commit.
    /// </summary>
    public ProcedureTestHooks AfterTxCommit(Action<TestContext> callback)
    {
        TransactionCallback.Validate(callback, typeof(void));
        AfterCommit = callback;
        return this;
    }
}
