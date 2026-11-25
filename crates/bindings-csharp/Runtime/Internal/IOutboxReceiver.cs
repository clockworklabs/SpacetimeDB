namespace SpacetimeDB.Internal;

/// <summary>
/// Marks a reducer in generated receiver bindings. The source generator reads the
/// reducer's name, signature hash and parameters from it; it's never instantiated.
/// </summary>
public interface IOutboxReceiver { }
