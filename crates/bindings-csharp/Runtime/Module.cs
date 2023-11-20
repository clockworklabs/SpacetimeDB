namespace SpacetimeDB.Module;

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using SpacetimeDB.SATS;
using System.Runtime.InteropServices;

[SpacetimeDB.Type]
public partial struct IndexDef
{
    string Name;
    Runtime.IndexType Type;
    byte[] ColumnIds;

    public IndexDef(string name, Runtime.IndexType type, RawBindings.ColId[] columnIds)
    {
        Name = name;
        Type = type;
        ColumnIds = columnIds.Select(id => (byte)id).ToArray();
    }
}

[SpacetimeDB.Type]
public partial struct TableDef
{
    string Name;
    AlgebraicTypeRef Data;

    // bitflags should be serialized as bytes rather than sum types
    byte[] ColumnAttrs;
    IndexDef[] Indices;

    // "system" | "user"
    string TableType;

    // "public" | "private"
    string TableAccess;

    public TableDef(
        string name,
        AlgebraicTypeRef type,
        ColumnAttrs[] columnAttrs,
        IndexDef[] indices
    )
    {
        Name = name;
        Data = type;
        ColumnAttrs = columnAttrs.Cast<byte>().ToArray();
        Indices = indices;
        TableType = "user";
        TableAccess = name.StartsWith('_') ? "private" : "public";
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
    List<TableDef> Tables = new();
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

    public void Add(TableDef table)
    {
        Tables.Add(table);
    }

    public void Add(ReducerDef reducer)
    {
        Reducers.Add(reducer);
    }
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

    public static void RegisterTable(TableDef table) => module.Add(table);

    public static AlgebraicTypeRef AllocTypeRef() => module.AllocTypeRef();

    public static void SetTypeRef<T>(
        AlgebraicTypeRef typeRef,
        AlgebraicType type,
        bool anonymous = false
    ) => module.SetTypeRef<T>(typeRef, type, anonymous);

    // [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
    public static RawBindings.Buffer __describe_module__()
    {
        try
        {
            // replace `module` with a temporary internal module that will register ModuleDef, AlgebraicType and other internal types
            // during the ModuleDef.GetSatsTypeInfo() instead of exposing them via user's module.
            var userModule = module;
            try
            {
                module = new();
                var moduleBytes = ModuleDef.GetSatsTypeInfo().ToBytes(userModule);
                var res = RawBindings._buffer_alloc(moduleBytes, (uint)moduleBytes.Length);
                return res;
            }
            finally
            {
                module = userModule;
            }
        }
        catch (Exception e)
        {
            var error_str = e.ToString();
            Runtime.Log($"Error while describing the module: {error_str}", Runtime.LogLevel.Error);
            return RawBindings.Buffer.INVALID;
        }
    }

    private static byte[] Consume(this RawBindings.Buffer buffer)
    {
        var len = RawBindings._buffer_len(buffer);
        var result = new byte[len];
        RawBindings._buffer_consume(buffer, result, len);
        return result;
    }

    // [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
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
