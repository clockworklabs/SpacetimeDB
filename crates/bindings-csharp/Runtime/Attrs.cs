namespace SpacetimeDB;

/// <summary>
/// This enum provides constants for special reducer kinds.
/// Do not rely on the type or values of these constants - they are only meant to be passed to the [SpacetimeDB.Reducer] attribute.
/// </summary>
public enum ReducerKind
{
    Init = Internal.Lifecycle.Init,
    Connect = Internal.Lifecycle.OnConnect,
    Disconnect = Internal.Lifecycle.OnDisconnect,
}

[AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
public sealed class ReducerAttribute : Attribute
{
    public ReducerAttribute() { }

    public ReducerAttribute(ReducerKind kind)
    {
        _ = kind;
    }
}

[AttributeUsage(
    AttributeTargets.Struct | AttributeTargets.Class,
    Inherited = false,
    AllowMultiple = false
)]
public sealed class TableAttribute : Attribute
{
    public bool Public { get; init; }
    public string? Scheduled { get; init; }
}

[Flags]
public enum ColumnAttrs : byte
{
    UnSet = 0b0000,
    Indexed = 0b0001,
    AutoInc = 0b0010,
    Unique = Indexed | 0b0100,
    Identity = Unique | AutoInc,
    PrimaryKey = Unique | 0b1000,
    PrimaryKeyAuto = PrimaryKey | AutoInc,

    /// <summary>
    /// A legacy alias, originally defined as `PrimaryKey | Identity` which is numerically same as above.
    /// </summary>.
    PrimaryKeyIdentity = PrimaryKeyAuto,
}

[AttributeUsage(AttributeTargets.Field, Inherited = false, AllowMultiple = false)]
public sealed class ColumnAttribute(ColumnAttrs type) : Attribute
{
    public ColumnAttrs Type => type;
}
