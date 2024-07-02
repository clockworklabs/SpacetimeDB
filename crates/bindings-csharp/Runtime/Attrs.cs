namespace SpacetimeDB;

public static class ReducerKind
{
    public const string Init = "__init__";
    public const string Update = "__update__";
    public const string Connect = "__identity_connected__";
    public const string Disconnect = "__identity_disconnected__";
}

[AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
public sealed class ReducerAttribute(string? name = null) : Attribute
{
    public string? Name => name;
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
    PrimaryKeyIdentity = PrimaryKey | Identity,
}

[AttributeUsage(AttributeTargets.Field, Inherited = false, AllowMultiple = false)]
public sealed class ColumnAttribute(ColumnAttrs type) : Attribute
{
    public ColumnAttrs Type => type;
}
