namespace SpacetimeDB.Internal;

using System;
using System.Runtime.CompilerServices;
using System.Text;
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
    public partial struct ReducerDef(string name, params AggregateElement[] args)
    {
        string Name = name;
        AggregateElement[] Args = args;
    }

    [Type]
    public partial struct TypeAlias(string name, int typeRef)
    {
        string Name = name;
        int TypeRef = typeRef;
    }

    [Type]
    public partial record MiscModuleExport : TaggedEnum<(TypeAlias TypeAlias, Unit _Reserved)>;

    [Type]
    public partial struct RawModuleDefV8()
    {
        internal AlgebraicType[] Types = [];
        internal TableDesc[] Tables = [];
        internal ReducerDef[] Reducers = [];
        internal MiscModuleExport[] MiscExports = [];
    }

    [Type]
    internal partial record RawModuleDef
        : TaggedEnum<(RawModuleDefV8 V8BackCompat, Unit _Reserved)>;

    public delegate void CallReducer(BinaryReader reader);

    private static RawModuleDefV8 moduleDef = new();

#pragma warning disable CS8618 // Non-nullable field must contain a non-null value when exiting constructor.
    private static CallReducer[] reducers;

    static FFI.TableId?[] tableIds;
#pragma warning restore CS8618

    public static void Initialize(
        AlgebraicType[] types,
        MiscModuleExport.TypeAlias[] aliases,
        TableDesc[] tableDescs,
        ReducerDef[] reducerDefs,
        CallReducer[] reducerCalls
    )
    {
        moduleDef.Types = types;
        moduleDef.Tables = tableDescs;
        moduleDef.Reducers = reducerDefs;
        moduleDef.MiscExports = aliases;
        reducers = reducerCalls;
        tableIds = new FFI.TableId?[tableDescs.Length];
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public static void Insert<T>(FFI.TableId id, in T row)
        where T : struct, IStructuralReadWrite
    {
        var bytes = IStructuralReadWrite.ToBytes(row);
        FFI._insert(id, bytes, (uint)bytes.Length);
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public static bool Update<T, C, RW>(FFI.TableId id, FFI.ColId colId, in C col, in T row)
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
    public static bool Delete<C, RW>(FFI.TableId id, FFI.ColId colId, in C col)
        where RW : struct, IReadWrite<C>
    {
        var bytes = IStructuralReadWrite.ToBytes(new RW(), col);
        FFI._delete_by_col_eq(id, colId, bytes!, (uint)bytes.Length, out var result);
        return result > 0;
    }

    public static FFI.TableId GetTableId(int idx, string name)
    {
        var id = tableIds[idx];
        if (id == null)
        {
            var bytes = Encoding.UTF8.GetBytes(name);
            FFI._get_table_id(bytes, (uint)bytes.Length, out var newId);
            tableIds[idx] = id = newId;
        }
        return id.Value;
    }

    static readonly MemoryStream stream = new();
    static readonly BinaryReader reader = new(stream);

    static void Produce(this BytesSource buffer, MemoryStream into)
    {
        into.Position = 0;

        uint len = 0;
        var ret = FFI._bytes_source_read(buffer, null, ref len);
        if (ret != 0)
        {
            throw new InvalidOperationException();
        }
        if (len > into.Capacity)
        {
            into.Capacity = (int)len;
        }

        into.SetLength(len);
        ret = FFI._bytes_source_read(buffer, into.GetBuffer(), ref len);
        if (ret != -1)
        {
            throw new Exception("Failed to read host buffer");
        }
    }

#pragma warning disable IDE1006 // Naming Styles - methods below are meant for FFI.

    public static Buffer __describe_module__()
    {
        try
        {
            RawModuleDef raw = new RawModuleDef.V8BackCompat(moduleDef);
            var moduleBytes = IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), raw);
            return FFI._buffer_alloc(moduleBytes, (uint)moduleBytes.Length);
        }
        catch (Exception e)
        {
            Runtime.Log($"Error while describing the module: {e}", Runtime.LogLevel.Error);
            return Buffer.INVALID;
        }
    }

    public static unsafe Buffer __call_reducer__(
        uint id,
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        ulong address_0,
        ulong address_1,
        DateTimeOffsetRepr timestamp,
        BytesSource args
    )
    {
        var identityBytes = new byte[32];
        var addressBytes = new byte[16];
        fixed (byte* b = identityBytes)
        {
            var p = (ulong*)b;
            p[0] = sender_0;
            p[1] = sender_1;
            p[2] = sender_2;
            p[3] = sender_3;
        }
        fixed (byte* b = addressBytes)
        {
            var p = (ulong*)b;
            p[0] = address_0;
            p[1] = address_1;
        }

        try
        {
            Runtime.Random = new((int)timestamp.MicrosecondsSinceEpoch);
            Runtime.SenderIdentity = Identity.From(identityBytes);
            Runtime.SenderAddress = Address.From(addressBytes);
            Runtime.Timestamp = timestamp.ToStd();

            args.Produce(stream);
            reducers[(int)id](reader);

            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            else
            {
                return /* no exception */
                Buffer.INVALID;
            }
        }
        catch (Exception e)
        {
            var error_str = e.ToString();
            var error_bytes = Encoding.UTF8.GetBytes(error_str);
            return FFI._buffer_alloc(error_bytes, (uint)error_bytes.Length);
        }
    }
}
