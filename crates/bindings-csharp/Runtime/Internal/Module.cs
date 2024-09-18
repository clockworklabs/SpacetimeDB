namespace SpacetimeDB.Internal;

using System;
using System.Runtime.InteropServices;
using SpacetimeDB;
using SpacetimeDB.BSATN;

partial class RawConstraintDefV8
{
    public RawConstraintDefV8(string tableName, ushort colIndex, string colName, ColumnAttrs attrs)
        : this(
            ConstraintName: $"ct_{tableName}_{colName}_{attrs}",
            Constraints: (byte)attrs,
            Columns: [colIndex]
        ) { }
}

partial class RawModuleDefV8
{
    // Note: this intends to generate a valid identifier, but it's not guaranteed to be unique as it's not proper mangling.
    // Fix it up to a different mangling scheme if it causes problems.
    private static string GetFriendlyName(Type type) =>
        type.IsGenericType
            ? $"{type.Name.Remove(type.Name.IndexOf('`'))}_{string.Join("_", type.GetGenericArguments().Select(GetFriendlyName))}"
            : type.Name;

    private void RegisterTypeName<T>(AlgebraicType.Ref typeRef)
    {
        MiscExports.Add(
            new MiscModuleExport.TypeAlias(new(GetFriendlyName(typeof(T)), (uint)typeRef.Ref_))
        );
    }

    internal AlgebraicType.Ref RegisterType<T>(Func<AlgebraicType.Ref, AlgebraicType> makeType)
    {
        var types = Typespace.Types;
        var typeRef = new AlgebraicType.Ref(types.Count);
        // Put a dummy self-reference just so that we get stable index even if `makeType` recursively adds more types.
        types.Add(typeRef);
        // Now we can safely call `makeType` and assign the result to the reserved slot.
        types[typeRef.Ref_] = makeType(typeRef);
        RegisterTypeName<T>(typeRef);
        return typeRef;
    }

    internal void RegisterReducer(ReducerDef reducer) => Reducers.Add(reducer);

    internal void RegisterTable(TableDesc table) => Tables.Add(table);
}

public static class Module
{
    private static readonly RawModuleDefV8 moduleDef = new();
    private static readonly List<IReducer> reducers = [];

    readonly struct TypeRegistrar() : ITypeRegistrar
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
            var versioned = new RawModuleDef.V8BackCompat(moduleDef);
            var moduleBytes = IStructuralWrite.ToBytes(versioned);
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
        var sender = Identity.From(
            MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]).ToArray()
        );

        // Piece together the sender address.
        var address = Address.From(MemoryMarshal.AsBytes([address_0, address_1]).ToArray());

        try
        {
            Runtime.Random = new((int)timestamp.MicrosecondsSinceEpoch);

            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            reducers[(int)id].Invoke(reader, new(sender, address, timestamp.ToStd()));
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            return Errno.OK; /* no exception */
        }
        catch (Exception e)
        {
            var error_str = e.ToString();
            var error_bytes = System.Text.Encoding.UTF8.GetBytes(error_str);
            error.Write(error_bytes);
            return Errno.HOST_CALL_FAILURE;
        }
    }
}
