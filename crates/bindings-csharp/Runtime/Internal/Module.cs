namespace SpacetimeDB.Internal;

using SpacetimeDB;
using SpacetimeDB.BSATN;

public static partial class Module
{
    [SpacetimeDB.Type]
    public enum IndexType : byte
    {
        BTree,
        Hash,
    }

    [SpacetimeDB.Type]
    public partial struct IndexDef(string name, IndexType type, bool isUnique, uint[] columnIds)
    {
        string IndexName = name;
        bool IsUnique = isUnique;
        IndexType Type = type;
        uint[] ColumnIds = columnIds;
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

        string? ScheduledReducer = scheduledReducer;
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
    internal partial struct TypeAlias(string name, AlgebraicType.Ref typeRef)
    {
        string Name = name;
        int TypeRef = typeRef.Ref_;
    }

    [SpacetimeDB.Type]
    internal partial record MiscModuleExport
        : SpacetimeDB.TaggedEnum<(TypeAlias TypeAlias, Unit _Reserved)>;

    [SpacetimeDB.Type]
    public partial struct RawModuleDefV8()
    {
        List<AlgebraicType> Types = [];
        List<TableDesc> Tables = [];
        List<ReducerDef> Reducers = [];
        List<MiscModuleExport> MiscExports = [];

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

        internal void RegisterReducer(ReducerDef reducer) => Reducers.Add(reducer);

        internal void RegisterTable(TableDesc table) => Tables.Add(table);
    }

    [SpacetimeDB.Type]
    internal partial record RawModuleDef
        : SpacetimeDB.TaggedEnum<(RawModuleDefV8 V8BackCompat, Unit _Reserved)>;

    private static readonly RawModuleDefV8 moduleDef = new();
    private static readonly List<IReducer> reducers = [];

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

    static readonly TypeRegistrar typeRegistrar = new();

    public static void RegisterReducer<R>()
        where R : IReducer, new()
    {
        var reducer = new R();
        reducers.Add(reducer);
        moduleDef.RegisterReducer(reducer.MakeReducerDef(typeRegistrar));
    }

    public static void RegisterTable<T>()
        where T : ITable<T>, new()
    {
        moduleDef.RegisterTable(T.MakeTableDesc(typeRegistrar));
    }

    private static byte[] Consume(this Buffer buffer)
    {
        var len = FFI._buffer_len(buffer);
        var result = new byte[len];
        FFI._buffer_consume(buffer, result, len);
        return result;
    }

#pragma warning disable IDE1006 // Naming Styles - methods below are meant for FFI.

    public static Buffer __describe_module__()
    {
        // replace `module` with a temporary internal module that will register RawModuleDefV8, AlgebraicType and other internal types
        // during the RawModuleDefV8.GetSatsTypeInfo() instead of exposing them via user's module.
        try
        {
            // We need this explicit cast here to make `ToBytes` understand the types correctly.
            RawModuleDef versioned = new RawModuleDef.V8BackCompat(moduleDef);
            var moduleBytes = IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), versioned);
            var res = FFI._buffer_alloc(moduleBytes, (uint)moduleBytes.Length);
            return res;
        }
        catch (Exception e)
        {
            Runtime.Log($"Error while describing the module: {e}", Runtime.LogLevel.Error);
            return Buffer.INVALID;
        }
    }

    public static Buffer __call_reducer__(
        uint id,
        Buffer caller_identity,
        Buffer caller_address,
        DateTimeOffsetRepr timestamp,
        Buffer args
    )
    {
        try
        {
            Runtime.Random = new((int)timestamp.MicrosecondsSinceEpoch);
            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            reducers[(int)id]
                .Invoke(
                    reader,
                    new(
                        Identity.From(caller_identity.Consume()),
                        Address.From(caller_address.Consume()),
                        timestamp.ToStd()
                    )
                );
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            return /* no exception */
            Buffer.INVALID;
        }
        catch (Exception e)
        {
            var error_str = e.ToString();
            var error_bytes = System.Text.Encoding.UTF8.GetBytes(error_str);
            return FFI._buffer_alloc(error_bytes, (uint)error_bytes.Length);
        }
    }
}
