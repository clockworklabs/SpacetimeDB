namespace SpacetimeDB;

[Type]
public enum IndexType : byte
{
    BTree,
    Hash,
}

/// <summary>
/// Registers a type as the row structure of a SpacetimeDB table, enabling codegen for it.
///
/// <para>
/// Multiple [Table] attributes per type are supported. This is useful to reuse row types.
/// Each attribute instance must have a unique name and will create a SpacetimeDB table.
/// </para>
/// </summary>
[AttributeUsage(AttributeTargets.Struct, AllowMultiple = true)]
public sealed class TableAttribute : Attribute
{
    /// <summary>
    /// This identifier is used to name the SpacetimeDB table on the host as well as the
    /// table handle structures generated to access the table from within a reducer call.
    ///
    /// <para>Defaults to the <c>nameof</c> of the target type.</para>
    /// </summary>
    public string? Name;

    /// <summary>
    /// Set to <c>true</c> to make the table visible to everyone.
    ///
    /// <para>Defaults to the table only being visible to its owner.</para>
    /// </summary>
    public bool Public = false;

    public string? Index;

    public IndexType? IndexType;

    public string[]? IndexColumns;
}

[AttributeUsage(AttributeTargets.Field)]
public abstract class ColumnAttribute : Attribute
{
    public string? Table;
}

public sealed class AutoIncAttribute : ColumnAttribute { }

public sealed class PrimaryKeyAttribute : ColumnAttribute { }

public sealed class UniqueAttribute : ColumnAttribute { }

public sealed class IndexedAttribute : ColumnAttribute { }

public static class ReducerKind
{
    public const string Init = "__init__";
    public const string Update = "__update__";
    public const string Connect = "__identity_connected__";
    public const string Disconnect = "__identity_disconnected__";
}

[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class ReducerAttribute : Attribute
{
    public string? Name;
}
