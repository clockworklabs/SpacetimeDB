namespace SpacetimeDB.Module;

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using SpacetimeDB.BSATN;

[SpacetimeDB.Type]
public partial struct IndexDef(
    string name,
    Runtime.IndexType type,
    bool isUnique,
    RawBindings.ColId[] columnIds
)
{
    string IndexName = name;
    bool IsUnique = isUnique;
    Runtime.IndexType Type = type;
    uint[] ColumnIds = columnIds.Select(id => (uint)id).ToArray();
}

[SpacetimeDB.Type]
public partial struct ColumnDef(string name, AlgebraicType type)
{
    internal string ColName = name;
    AlgebraicType ColType = type;
}

[SpacetimeDB.Type]
public partial struct ConstraintDef(string name, ColumnAttrs kind, uint[] columnIds)
{
    string ConstraintName = name;

    // bitflags should be serialized as bytes rather than sum types
    byte Kind = (byte)kind;
    uint[] ColumnIds = columnIds;
}

[SpacetimeDB.Type]
public partial struct SequenceDef(
    string sequenceName,
    uint colPos,
    Int128? increment = null,
    Int128? start = null,
    Int128? min_value = null,
    Int128? max_value = null,
    Int128? allocated = null
)
{
    string SequenceName = sequenceName;
    uint ColPos = colPos;
    Int128 increment = increment ?? 1;
    Int128? start = start;
    Int128? min_value = min_value;
    Int128? max_value = max_value;
    Int128 allocated = allocated ?? 4_096;
}

// Not part of the database schema, just used by the codegen to group column definitions with their attributes.
public struct ColumnDefWithAttrs(ColumnDef columnDef, ColumnAttrs attrs)
{
    public ColumnDef ColumnDef = columnDef;
    public ColumnAttrs Attrs = attrs;
}

[SpacetimeDB.Type]
public partial struct TableDef(string tableName, ColumnDefWithAttrs[] columns, bool isPublic)
{
    string TableName = tableName;
    ColumnDef[] Columns = columns.Select(col => col.ColumnDef).ToArray();
    IndexDef[] Indices = [];
    ConstraintDef[] Constraints = columns
        // Important: the position must be stored here, before filtering.
        .Select((col, pos) => (col, pos))
        .Where(pair => pair.col.Attrs != ColumnAttrs.UnSet)
        .Select(pair => new ConstraintDef(
            $"ct_{tableName}_{pair.col.ColumnDef.ColName}_{pair.col.Attrs}",
            pair.col.Attrs,
            [(uint)pair.pos]
        ))
        .ToArray();
    SequenceDef[] Sequences = [];

    // "system" | "user"
    string TableType = "user";

    // "public" | "private"
    string TableAccess = isPublic ? "public" : "private";
}

[SpacetimeDB.Type]
public partial struct TableDesc(TableDef schema, AlgebraicType.Ref typeRef)
{
    TableDef Schema = schema;
    int TypeRef = typeRef.Ref_;
}

[SpacetimeDB.Type]
public partial struct ReducerDef(string name, params AggregateElement[] args)
{
    string Name = name;
    AggregateElement[] Args = args;
}

[SpacetimeDB.Type]
partial struct TypeAlias(string name, AlgebraicType.Ref typeRef)
{
    string Name = name;
    int TypeRef = typeRef.Ref_;
}

[SpacetimeDB.Type]
partial record MiscModuleExport : SpacetimeDB.TaggedEnum<(TypeAlias TypeAlias, Unit _Reserved)>;

[SpacetimeDB.Type]
public partial struct ModuleDef()
{
    internal List<AlgebraicType> Types = [];
    public List<TableDesc> Tables = [];
    public List<ReducerDef> Reducers = [];
    internal List<MiscModuleExport> MiscExports = [];
}

[System.Flags]
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

public static class ReducerKind
{
    public const string Init = "__init__";
    public const string Update = "__update__";
    public const string Connect = "__identity_connected__";
    public const string Disconnect = "__identity_disconnected__";
}

public interface IReducer
{
    SpacetimeDB.Module.ReducerDef MakeReducerDef(ITypeRegistrar registrar);
    void Invoke(System.IO.BinaryReader reader, Runtime.ReducerContext args);
}

public struct TypeRegistrar() : ITypeRegistrar
{
    public ModuleDef Module = new();
    private Dictionary<Type, AlgebraicType.Ref> RegisteredTypes = [];

    // Note: this intends to generate a valid identifier, but it's not guaranteed to be unique as it's not proper mangling.
    // Fix it up to a different mangling scheme if it causes problems.
    private static string GetFriendlyName(Type type) =>
        type.IsGenericType
            ? $"{type.Name.Remove(type.Name.IndexOf('`'))}_{string.Join("_", type.GetGenericArguments().Select(GetFriendlyName))}"
            : type.Name;

    // Registers type in the module definition.
    //
    // To avoid issues with self-recursion during registration as well as unnecessary construction
    // of algebraic types for types that have already been registered, we accept a factory
    // returning an AlgebraicType instead of the AlgebraicType itself.
    //
    // The factory callback will be called with the allocated type reference that can be used for
    // e.g. self-recursion even before the algebraic type itself is constructed.
    public AlgebraicType.Ref RegisterType<T>(Func<AlgebraicType.Ref, AlgebraicType> makeType)
    {
        if (RegisteredTypes.TryGetValue(typeof(T), out var existingTypeRef))
        {
            return existingTypeRef;
        }
        var typeRef = new AlgebraicType.Ref(Module.Types.Count);
        RegisteredTypes.Add(typeof(T), typeRef);
        // Put a dummy value just so that we get stable index even if `makeType` recursively adds more types.
        Module.Types.Add(typeRef);
        // Now we can safely call `makeType`.
        var realType = makeType(typeRef);
        // And, finally, replace the dummy value with the real one.
        Module.Types[typeRef.Ref_] = realType;
        // If it's a table, it doesn't need an alias and will be registered automatically.
        if (!typeof(T).IsDefined(typeof(SpacetimeDB.TableAttribute), false))
        {
            Module.MiscExports.Add(
                new MiscModuleExport.TypeAlias(new TypeAlias(GetFriendlyName(typeof(T)), typeRef))
            );
        }
        return typeRef;
    }
}

public static class FFI
{
    private static readonly List<IReducer> reducers = [];
    public static readonly TypeRegistrar TypeRegistrar = new();

    public static void RegisterReducer(IReducer reducer)
    {
        reducers.Add(reducer);
        TypeRegistrar.Module.Reducers.Add(reducer.MakeReducerDef(TypeRegistrar));
    }

    public static void RegisterTable(TableDesc tableDesc)
    {
        TypeRegistrar.Module.Tables.Add(tableDesc);
    }

    public static RawBindings.Buffer __describe_module__()
    {
        // replace `module` with a temporary internal module that will register ModuleDef, AlgebraicType and other internal types
        // during the ModuleDef.GetSatsTypeInfo() instead of exposing them via user's module.
        try
        {
            var moduleBytes = IStructuralReadWrite.ToBytes(TypeRegistrar.Module);
            var res = RawBindings._buffer_alloc(moduleBytes, (uint)moduleBytes.Length);
            return res;
        }
        catch (Exception e)
        {
            Runtime.Log($"Error while describing the module: {e}", Runtime.LogLevel.Error);
            return RawBindings.Buffer.INVALID;
        }
    }

    public static RawBindings.Buffer __call_reducer__(
        uint id,
        RawBindings.Buffer caller_identity,
        RawBindings.Buffer caller_address,
        ulong timestamp,
        RawBindings.Buffer args
    )
    {
        try
        {
            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            reducers[(int)id].Invoke(
                reader,
                new(caller_identity.Consume(), caller_address.Consume(), timestamp)
            );
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            return /* no exception */
            RawBindings.Buffer.INVALID;
        }
        catch (Exception e)
        {
            var error_str = e.ToString();
            var error_bytes = System.Text.Encoding.UTF8.GetBytes(error_str);
            return RawBindings._buffer_alloc(error_bytes, (uint)error_bytes.Length);
        }
    }
}
