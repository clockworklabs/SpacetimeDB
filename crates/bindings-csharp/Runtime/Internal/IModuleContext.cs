#if NET10_0_OR_GREATER
namespace SpacetimeDB.Internal;

/// <summary>Identifies a context or path selector for generated instance-aware calls.</summary>
public interface IModuleContext
{
    int InstanceId { get; }

#if !SPACETIMEDB_WASM_RUNTIME
    /// <summary>The testing backend this context reads and writes, if it has one.</summary>
    internal DatabaseAccess? Access => null;
#endif
}

/// <summary>Used by generated namespace context selectors to preserve the context type.</summary>
public interface IModuleContext<TContext> : IModuleContext
{
    TContext SelectInstance(int instanceId);
}
#endif
