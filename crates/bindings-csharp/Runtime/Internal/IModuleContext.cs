#if NET10_0_OR_GREATER
namespace SpacetimeDB.Internal;

/// <summary>Used by generated namespace context selectors.</summary>
public interface IModuleContext<TContext>
{
    int InstanceId { get; }
    TContext SelectInstance(int instanceId);
}
#endif
