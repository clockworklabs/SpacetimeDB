namespace SpacetimeDB.Module;

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using SpacetimeDB.SATS;

[SpacetimeDB.Type]
public partial struct IndexDef
{
    string IndexName;
    bool IsUnique;
    Runtime.IndexType Type;
    uint[] ColumnIds;

    public IndexDef(string name, Runtime.IndexType type, bool isUnique, uint[] columnIds)
    {
        IndexName = name;
        IsUnique = isUnique;
        Type = type;
        ColumnIds = columnIds;
    }
}


[SpacetimeDB.Type]
public partial struct ColumnDef
{
    string ColName;
    AlgebraicType ColType;

    public ColumnDef(string name, AlgebraicType type)
    {
        ColName = name;
        ColType = type;
    }
}

[SpacetimeDB.Type]
public partial struct ConstraintDef
{
    string ConstraintName;
    // bitflags should be serialized as bytes rather than sum types
    byte Kind;
    uint[] ColumnIds;

    public ConstraintDef(string name, byte kind, uint[] columnIds)
    {
        //Trace.Assert(columnIds.Length >0, "Constraints requiere to have  at least one column");
        ConstraintName = name;
        Kind = kind;
        ColumnIds = columnIds;
    }
}

[SpacetimeDB.Type]
public partial struct SequenceDef
{

    string SequenceName;
    UInt32 ColPos;
    Int128 increment;
    Int128? start;
    Int128? min_value;
    Int128? max_value;
    Int128 allocated;

    public SequenceDef(string sequenceName, uint colPos, Int128? increment = null, Int128? start = null, Int128? min_value = null, Int128? max_value = null, Int128? allocated = null)
    {
        SequenceName = sequenceName;
        ColPos = colPos;
        this.increment = increment ?? 1;
        this.start = start;
        this.min_value = min_value;
        this.max_value = max_value;
        this.allocated = allocated ?? 4_096;
    }
}

public partial struct ColumnAttrs
{
    public string ColName;
    public AlgebraicType ColType;
    public ConstraintFlags Kind;

    public ColumnAttrs(string colName, AlgebraicType colType, ConstraintFlags kind)
    {
        ColName = colName;
        ColType = colType;
        Kind = kind;
    }
}


[SpacetimeDB.Type]
public partial struct TableDef
{
    string TableName;
    ColumnDef[] Columns;
    IndexDef[] Indices;
    ConstraintDef[] Constraints;
    SequenceDef[] Sequences;
    // "system" | "user"
    string TableType;

    // "public" | "private"
    string TableAccess;

    public TableDef(string tableName, ColumnAttrs[] columns, IndexDef[] indices)
    {

        TableName = tableName;
        Columns = columns.Select(x => new ColumnDef(x.ColName, x.ColType)).ToArray();
        Constraints = columns.Select((x, pos) => new ConstraintDef(x.ColName, ((byte)x.Kind), new UInt32[((uint)pos)])).ToArray();
        Sequences = columns.Where(x => x.Kind.HasFlag(ConstraintFlags.AutoInc)).Select((x, pos) => new SequenceDef(x.ColName, (uint)pos)).ToArray();
        Indices = indices;
        TableType = "user";
        TableAccess = tableName.StartsWith('_') ? "private" : "public";
    }
}

[SpacetimeDB.Type]
public partial struct TableDesc
{
    TableDef schema;
    AlgebraicTypeRef Data;

    public TableDesc(string tableName, ColumnAttrs[] columns, IndexDef[] indices, AlgebraicTypeRef data)
    {
        schema = new TableDef(tableName, columns, indices);
        Console.WriteLine(schema.ToString());
        Data = data;
    }
}

[SpacetimeDB.Type]
public partial struct ReducerDef
{
    string Name;
    ProductTypeElement[] Args;

    public ReducerDef(string name, params ProductTypeElement[] args)
    {
        Name = name;
        Args = args;
    }
}

[SpacetimeDB.Type]
partial struct TypeAlias
{
    internal string Name;
    internal AlgebraicTypeRef Type;
}

[SpacetimeDB.Type]
partial struct MiscModuleExport : SpacetimeDB.TaggedEnum<(TypeAlias TypeAlias, Unit _Reserved)> { }

[SpacetimeDB.Type]
public partial struct ModuleDef
{
    List<AlgebraicType> Types = new();
    List<TableDesc> Tables = new();
    List<ReducerDef> Reducers = new();
    List<MiscModuleExport> MiscExports = new();

    public ModuleDef() { }

    public AlgebraicTypeRef AllocTypeRef()
    {
        var index = Types.Count;
        var typeRef = new AlgebraicTypeRef(index);
        // uninhabited type, to be replaced by a real type
        Types.Add(new SumType());
        return typeRef;
    }

    // Note: this intends to generate a valid identifier, but it's not guaranteed to be unique as it's not proper mangling.
    // Fix it up to a different mangling scheme if it causes problems.
    private static string GetFriendlyName(Type type) =>
        type.IsGenericType
            ? $"{type.Name.Remove(type.Name.IndexOf('`'))}_{string.Join("_", type.GetGenericArguments().Select(GetFriendlyName))}"
            : type.Name;

    public void SetTypeRef<T>(AlgebraicTypeRef typeRef, AlgebraicType type, bool anonymous = false)
    {
        Types[typeRef.TypeRef] = type;
        if (!anonymous)
        {
            MiscExports.Add(
                new MiscModuleExport
                {
                    TypeAlias = new TypeAlias { Name = GetFriendlyName(typeof(T)), Type = typeRef }
                }
            );
        }
    }

    public void Add(TableDesc table)
    {
        Tables.Add(table);
    }

    public void Add(ReducerDef reducer)
    {
        Reducers.Add(reducer);
    }
}

[System.Flags]
public enum ConstraintFlags : byte
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
    SpacetimeDB.Module.ReducerDef MakeReducerDef();
    void Invoke(System.IO.BinaryReader reader, Runtime.DbEventArgs args);
}

public static class FFI
{
    private static List<IReducer> reducers = new();
    private static ModuleDef module = new();

    public static void RegisterReducer(IReducer reducer)
    {
        reducers.Add(reducer);
        module.Add(reducer.MakeReducerDef());
    }

    public static void RegisterTable(TableDesc table) => module.Add(table);

    public static AlgebraicTypeRef AllocTypeRef() => module.AllocTypeRef();

    public static void SetTypeRef<T>(AlgebraicTypeRef typeRef, AlgebraicType type, bool anonymous = false) =>
        module.SetTypeRef<T>(typeRef, type, anonymous);

    // Note: this is accessed by C bindings.
    private static byte[] DescribeModule()
    {
        // replace `module` with a temporary internal module that will register ModuleDef, AlgebraicType and other internal types
        // during the ModuleDef.GetSatsTypeInfo() instead of exposing them via user's module.
        var userModule = module;
        try
        {
            module = new();
            return ModuleDef.GetSatsTypeInfo().ToBytes(userModule);
        }
        finally
        {
            module = userModule;
        }
    }

    // Note: this is accessed by C bindings.
    private static string? CallReducer(
        uint id,
        byte[] sender_identity,
        byte[] sender_address,
        ulong timestamp,
        byte[] args
    )
    {
        try
        {
            using var stream = new MemoryStream(args);
            using var reader = new BinaryReader(stream);
            reducers[(int)id].Invoke(reader, new(sender_identity, sender_address, timestamp));
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            return null;
        }
        catch (Exception e)
        {
            return e.ToString();
        }
    }
}
