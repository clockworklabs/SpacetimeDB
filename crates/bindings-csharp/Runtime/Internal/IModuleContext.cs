#if NET10_0_OR_GREATER
namespace SpacetimeDB.Internal;

/// <summary>Identifies a context or path selector for generated instance-aware calls.</summary>
public interface IModuleContext
{
    int InstanceId { get; }
}

/// <summary>Used by generated namespace context selectors to preserve the context type.</summary>
public interface IModuleContext<TContext> : IModuleContext
{
    TContext SelectInstance(int instanceId);
}
#endif
