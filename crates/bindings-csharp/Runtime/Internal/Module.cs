namespace SpacetimeDB.Internal;

using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

using SpacetimeDB;
using SpacetimeDB.BSATN;

[Flags]
public enum ColumnAttrs : byte
{
    None = 0b0000,
    Indexed = 0b0001,
    AutoInc = 0b0010,
    Unique = 0b0100 | Indexed,
    PrimaryKey = 0b1000 | Unique,
}

public static partial class Module
{
    [Type]
    public enum IndexType : byte
    {
        BTree,
        Hash,
    }

    [Type]
    public partial struct IndexDef(string name, IndexType type, bool isUnique, ushort[] columnIds)
    {
        string IndexName = name;
        bool IsUnique = isUnique;
        IndexType Type = type;
        ushort[] ColumnIds = columnIds;
    }

    [Type]
    public partial struct ColumnDef(string name, AlgebraicType type)
    {
        internal string ColName = name;
        AlgebraicType ColType = type;
    }

    [Type]
    public partial struct ConstraintDef(string name, ColumnAttrs kind, ushort[] columnIds)
    {
        string ConstraintName = name;

        // bitflags should be serialized as bytes rather than sum types
        byte Kind = (byte)kind;
        ushort[] ColumnIds = columnIds;
    }

    [Type]
    public partial struct SequenceDef(
        string sequenceName,
        ushort colPos,
        Int128? increment = null,
        Int128? start = null,
        Int128? min_value = null,
        Int128? max_value = null,
        Int128? allocated = null
    )
    {
        string SequenceName = sequenceName;
        ushort ColPos = colPos;
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

    [Type]
    public partial struct TableDef(
        string tableName,
        ColumnDefWithAttrs[] columns,
        bool isPublic,
        string? scheduledReducer
    )
    {
        string TableName = tableName;
        ColumnDef[] Columns = columns.Select(col => col.ColumnDef).ToArray();
        IndexDef[] Indices = [];
        ConstraintDef[] Constraints = columns
            // Important: the position must be stored here, before filtering.
            .Select((col, pos) => (col, pos))
            .Where(pair => pair.col.Attrs != ColumnAttrs.None)
            .Select(pair => new ConstraintDef(
                $"ct_{tableName}_{pair.col.ColumnDef.ColName}_{pair.col.Attrs}",
                pair.col.Attrs,
                [(ushort)pair.pos]
            ))
            .ToArray();
        SequenceDef[] Sequences = [];

        // "system" | "user"
        string TableType = "user";

        // "public" | "private"
        string TableAccess = isPublic ? "public" : "private";

        string? ScheduledReducer = scheduledReducer;
    }

    [Type]
    public partial struct TableDesc(TableDef schema, AlgebraicType.Ref typeRef)
    {
        TableDef Schema = schema;
        int TypeRef = typeRef.Ref_;
    }

    [Type]
    public partial struct ReducerDef(string name, AggregateElement[] args)
    {
        string Name = name;
        AggregateElement[] Args = args;
    }

    [Type]
    public partial struct TypeAlias(string name, AlgebraicType.Ref typeRef)
    {
        string Name = name;
        int TypeRef = typeRef.Ref_;
    }

    [Type]
    public partial record MiscModuleExport
        : TaggedEnum<(TypeAlias TypeAlias, Unit _Reserved)>;

    [Type]
    public partial struct RawModuleDefV8()
    {
        public List<AlgebraicType> Types = [];
        public TableDesc[] Tables;
        public ReducerDef[] Reducers;
        public List<MiscModuleExport> MiscExports = [];

        // Note: this intends to generate a valid identifier, but it's not guaranteed to be unique as it's not proper mangling.
        // Fix it up to a different mangling scheme if it causes problems.
        private static string GetFriendlyName(Type type) =>
            type.IsGenericType
                ? $"{type.Name.Remove(type.Name.IndexOf('`'))}_{string.Join("_", type.GetGenericArguments().Select(GetFriendlyName))}"
                : type.Name;

        private void RegisterTypeName<T>(AlgebraicType.Ref typeRef)
        {
            // If it's a table, it doesn't need an alias as name will be registered automatically.
            if (typeof(T).IsDefined(typeof(TableAttribute), false))
            {
                return;
            }
            MiscExports.Add(
                new MiscModuleExport.TypeAlias(new(GetFriendlyName(typeof(T)), typeRef))
            );
        }

        internal AlgebraicType.Ref RegisterType<T>(Func<AlgebraicType.Ref, AlgebraicType> makeType)
        {
            var typeRef = new AlgebraicType.Ref(Types.Count);
            // Put a dummy self-reference just so that we get stable index even if `makeType` recursively adds more types.
            Types.Add(typeRef);
            // Now we can safely call `makeType` and assign the result to the reserved slot.
            Types[typeRef.Ref_] = makeType(typeRef);
            RegisterTypeName<T>(typeRef);
            return typeRef;
        }
    }

    [Type]
    internal partial record RawModuleDef
        : TaggedEnum<(RawModuleDefV8 V8BackCompat, Unit _Reserved)>;

    private static RawModuleDefV8 moduleDef = new();

    struct TypeRegistrar() : ITypeRegistrar
    {
        private readonly Dictionary<Type, AlgebraicType.Ref> types = [];

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
            // Store for the closure access.
            var types = this.types;
            if (types.TryGetValue(typeof(T), out var existingTypeRef))
            {
                return existingTypeRef;
            }
            return moduleDef.RegisterType<T>(typeRef =>
            {
                // Store the type reference in the dictionary so that we can resolve it later and to avoid infinite recursion inside `makeType`.
                types.Add(typeof(T), typeRef);
                return makeType(typeRef);
            });
        }
    }

    public static readonly ITypeRegistrar typeRegistrar = new TypeRegistrar();

    public delegate void CallReducer(BinaryReader reader);

    #pragma warning disable CS8618 // Non-nullable field must contain a non-null value when exiting constructor.
    static CallReducer[] reducers;

    static TableId?[] tableIds;
#pragma warning restore CS8618

    public static void Initialize(
        TableDesc[] tableDescs,
        ReducerDef[] reducerDefs,
        CallReducer[] reducerCalls
    )
    {
        moduleDef.Tables = tableDescs;
        moduleDef.Reducers = reducerDefs;
        reducers = reducerCalls;
        tableIds = new TableId?[tableDescs.Length];
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public static void Insert<T>(TableId id, in T row)
        where T : struct, IStructuralReadWrite
    {
        var bytes = IStructuralReadWrite.ToBytes(row);
        var len = (uint)bytes.Length;
        FFI._datastore_insert_bsatn(id, bytes, ref len);
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public static bool Update<T, C, RW>(TableId id, ColId colId, in C col, in T row)
        where T : struct, IStructuralReadWrite
        where RW : struct, IReadWrite<C>
    {
        if (Delete<C, RW>(id, colId, col))
        {
            Insert(id, row);
            return true;
        }
        return false;
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public static bool Delete<C, RW>(TableId id, ColId colId, in C col)
        where RW : struct, IReadWrite<C>
    {
        var bytes = IStructuralReadWrite.ToBytes(new RW(), col);
        FFI._delete_by_col_eq(id, colId, bytes!, (uint)bytes.Length, out var result);
        return result > 0;
    }

    public static TableId GetTableId(int idx, string name)
    {
        var id = tableIds[idx];
        if (id == null)
        {
            var bytes = Encoding.UTF8.GetBytes(name);
            FFI._table_id_from_name(bytes, (uint)bytes.Length, out var newId);
            tableIds[idx] = id = newId;
        }
        return id.Value;
    }

    private static byte[] Consume(this BytesSource source)
    {
        if (source == BytesSource.INVALID)
        {
            return [];
        }
        var buffer = new byte[0x20_000];
        var written = 0U;
        while (true)
        {
            // Write into the spare capacity of the buffer.
            var spare = buffer.AsSpan((int)written);
            var buf_len = (uint)spare.Length;
            var ret = FFI._bytes_source_read(source, spare, ref buf_len);
            written += buf_len;
            switch (ret)
            {
                // Host side source exhausted, we're done.
                case Errno.EXHAUSTED:
                    Array.Resize(ref buffer, (int)written);
                    return buffer;
                // Wrote the entire spare capacity.
                // Need to reserve more space in the buffer.
                case Errno.OK when written == buffer.Length:
                    Array.Resize(ref buffer, buffer.Length + 1024);
                    break;
                // Host didn't write as much as possible.
                // Try to read some more.
                // The host will likely not trigger this branch (current host doesn't),
                // but a module should be prepared for it.
                case Errno.OK:
                    break;
                case Errno.NO_SUCH_BYTES:
                    throw new NoSuchBytesException();
                default:
                    throw new UnknownException(ret);
            }
        }
    }

    private static void Write(this BytesSink sink, byte[] bytes)
    {
        var start = 0U;
        while (start != bytes.Length)
        {
            var written = (uint)bytes.Length;
            var buffer = bytes.AsSpan((int)start);
            FFI._bytes_sink_write(sink, buffer, ref written);
            start += written;
        }
    }

#pragma warning disable IDE1006 // Naming Styles - methods below are meant for FFI.

    public static void __describe_module__(BytesSink description)
    {
        // replace `module` with a temporary internal module that will register RawModuleDefV8, AlgebraicType and other internal types
        // during the RawModuleDefV8.GetSatsTypeInfo() instead of exposing them via user's module.
        try
        {
            // We need this explicit cast here to make `ToBytes` understand the types correctly.
            RawModuleDef versioned = new RawModuleDef.V8BackCompat(moduleDef);
            var moduleBytes = IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), versioned);
            description.Write(moduleBytes);
        }
        catch (Exception e)
        {
            Runtime.Log($"Error while describing the module: {e}", Runtime.LogLevel.Error);
        }
    }

    public static Errno __call_reducer__(
        uint id,
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        ulong address_0,
        ulong address_1,
        DateTimeOffsetRepr timestamp,
        BytesSource args,
        BytesSink error
    )
    {
        // Piece together the sender identity.
        Runtime.SenderIdentity = Identity.From(
            MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]).ToArray()
        );

        // Piece together the sender address.
        Runtime.SenderAddress = Address.From(MemoryMarshal.AsBytes([address_0, address_1]).ToArray());

        try
        {
            Runtime.Random = new((int)timestamp.MicrosecondsSinceEpoch);

            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            reducers[(int)id](reader);
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            return Errno.OK; /* no exception */
        }
        catch (Exception e)
        {
            var error_str = e.ToString();
            var error_bytes = Encoding.UTF8.GetBytes(error_str);
            error.Write(error_bytes);
            return Errno.HOST_CALL_FAILURE;
        }
    }
}
