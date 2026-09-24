namespace SpacetimeDB.Internal;

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using SpacetimeDB;
using SpacetimeDB.BSATN;

public static class Module
{
    // Workaround for NativeAOT-LLVM IL scanner bug:
    // The scanner fails to compute vtables for TaggedEnum<T> base types when
    // concrete subtypes are only encountered indirectly (e.g., through Equals
    // calls on types containing TaggedEnum fields). This occurs when no user
    // table has indexes/primary keys, so RawIndexAlgorithm is never directly
    // constructed in user code.
    // By referencing concrete TaggedEnum subtypes here, we ensure the IL scanner
    // always processes their vtables. One variant per TaggedEnum is sufficient.
    [System.Runtime.CompilerServices.MethodImpl(
        System.Runtime.CompilerServices.MethodImplOptions.NoInlining
    )]
    private static void EnsureNativeAotTypeRoots()
    {
        // These constructions are never executed at runtime — they exist solely
        // to make the IL scanner compute vtables for TaggedEnum subtypes.
        // The condition is always false but the scanner must assume it could be true.
#pragma warning disable IDE0078 // Keep this opaque to the NativeAOT IL scanner.
        if (Environment.TickCount < 0 && Environment.TickCount > 0)
#pragma warning restore IDE0078
        {
            _ = new RawIndexAlgorithm.BTree(null!);
            _ = new RawConstraintDataV9.Unique(null!);
            _ = new RawModuleDef.V10(null!);
            _ = new RawModuleDefV10Section.Typespace(null!);
            _ = new RawModuleDefV10Section.ViewPrimaryKeys(null!);
            _ = new ExplicitNameEntry.Table(null!);
            _ = new MiscModuleExport.TypeAlias(null!);
            _ = new RawMiscModuleExportV9.ColumnDefaultValue(null!);
            _ = new SpacetimeDB.Filter.Sql(null!);
            _ = new ViewResultHeader.RowData(default);
        }
    }

    public static readonly ModuleBuilder RootBuilder = new();

    private static NamespaceRegistry? namespaces;

    public static void InstallNamespaces(NamespaceRegistry registry)
    {
        if (namespaces is not null)
        {
            throw new InvalidOperationException("Module namespaces have already been installed.");
        }

        namespaces = registry;
    }

    public static string ResolveName(string assemblyIdentity, string localName) =>
        (
            namespaces
            ?? throw new InvalidOperationException("Module namespaces have not been installed.")
        ).Resolve(assemblyIdentity, localName);

    public static SqlTableName ResolveSqlName(string assemblyIdentity, string localName) =>
        (
            namespaces
            ?? throw new InvalidOperationException("Module namespaces have not been installed.")
        ).ResolveSqlName(assemblyIdentity, localName);

    private static Func<
        Identity,
        ConnectionId?,
        Random,
        Timestamp,
        IReducerContext
    >? newReducerContext =
#if NET10_0_OR_GREATER
    (identity, connectionId, random, time) =>
        new SpacetimeDB.ReducerContext(identity, connectionId, random, time);
#else
        null;
#endif
    private static Func<Identity, IViewContext>? newViewContext =
#if NET10_0_OR_GREATER
    identity => new ViewContext(identity, new LocalReadOnly());
#else
        null;
#endif
    private static Func<IAnonymousViewContext>? newAnonymousViewContext =
#if NET10_0_OR_GREATER
    () => new AnonymousViewContext(new LocalReadOnly());
#else
        null;
#endif
    private static Func<Random, Timestamp, SpacetimeDB.HandlerContextBase>? newHandlerContext =
#if NET10_0_OR_GREATER
    (
        random,
        time
    ) => new HandlerContext(random, time);
#else
        null;
#endif

    private static Func<
        Identity,
        ConnectionId?,
        Random,
        Timestamp,
        IProcedureContext
    >? newProcedureContext =
#if NET10_0_OR_GREATER
    (identity, connectionId, random, time) =>
        new ProcedureContext(identity, connectionId, random, time);
#else
        null;
#endif

    public static void SetReducerContextConstructor(
        Func<Identity, ConnectionId?, Random, Timestamp, IReducerContext> ctor
    ) => newReducerContext = ctor;

    public static void SetProcedureContextConstructor(
        Func<Identity, ConnectionId?, Random, Timestamp, IProcedureContext> ctor
    ) => newProcedureContext = ctor;

    public static void SetHandlerContextConstructor(
        Func<Random, Timestamp, SpacetimeDB.HandlerContextBase> ctor
    ) => newHandlerContext = ctor;

    public static void SetViewContextConstructor(Func<Identity, IViewContext> ctor) =>
        newViewContext = ctor;

    public static void SetAnonymousViewContextConstructor(Func<IAnonymousViewContext> ctor) =>
        newAnonymousViewContext = ctor;

    public readonly struct TypeRegistrar : ITypeRegistrar
    {
        private readonly Dictionary<Type, AlgebraicType.Ref> types = [];
        private readonly ModuleBuilder target;

        public TypeRegistrar()
            : this(RootBuilder) { }

        internal TypeRegistrar(ModuleBuilder target) => this.target = target;

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
            if (types.TryGetValue(typeof(T), out var existingTypeRef))
            {
                return existingTypeRef;
            }

            // Passes types down to register the type reference in the dictionary so that we can resolve it later and to avoid infinite recursion inside `makeType`.
            return target.RegisterType<T>(types, makeType);
        }
    }

    public static void RegisterReducer<R>()
        where R : IReducer, new() => RootBuilder.RegisterReducer<R>();

    public static void RegisterProcedure<P>()
        where P : IProcedure, new() => RootBuilder.RegisterProcedure<P>();

    public static void RegisterHttpHandler<H>()
        where H : IHttpHandler, new() => RootBuilder.RegisterHttpHandler<H>();

    public static void RegisterHttpRouter(SpacetimeDB.Router router) =>
        RootBuilder.RegisterHttpRouter(router);

    public static void RegisterTable<T, View>()
        where T : IStructuralReadWrite, new()
        where View : ITableView<View, T>, new() => RootBuilder.RegisterTable<T, View>();

    public static void RegisterView<TDispatcher>()
        where TDispatcher : IView, new() => RootBuilder.RegisterView<TDispatcher>();

    public static void RegisterAnonymousView<TDispatcher>()
        where TDispatcher : IAnonymousView, new() =>
        RootBuilder.RegisterAnonymousView<TDispatcher>();

    public static void RegisterEnvironment(EnvironmentDeclaration declaration) =>
        RootBuilder.RegisterEnvironment(declaration);

    public static void RegisterViewPrimaryKey(string viewSourceName, string[] columns) =>
        RootBuilder.RegisterViewPrimaryKey(viewSourceName, columns);

    public static void RegisterClientVisibilityFilter(Filter rlsFilter) =>
        RootBuilder.RegisterClientVisibilityFilter(rlsFilter);

    public static void RegisterTableDefaultValue(string table, ushort colId, byte[] value) =>
        RootBuilder.RegisterTableDefaultValue(table, colId, value);

    public static void SetCaseConversionPolicy(SpacetimeDB.CaseConversionPolicy policy) =>
        RootBuilder.SetCaseConversionPolicy(policy);

    public static void RegisterExplicitTableName(string sourceName, string canonicalName) =>
        RootBuilder.RegisterExplicitTableName(sourceName, canonicalName);

    public static void RegisterExplicitFunctionName(string sourceName, string canonicalName) =>
        RootBuilder.RegisterExplicitFunctionName(sourceName, canonicalName);

    public static void RegisterExplicitIndexName(string sourceName, string canonicalName) =>
        RootBuilder.RegisterExplicitIndexName(sourceName, canonicalName);

    public static byte[] Consume(this BytesSource source)
    {
        var buffer = Array.Empty<byte>();
        using var stream = source.Consume(ref buffer);
        return stream.ToArray();
    }

    internal static MemoryStream Consume(this BytesSource source, ref byte[] buffer)
    {
        if (source == BytesSource.INVALID)
        {
            return new();
        }

        var len = (uint)0;
        var ret = FFI.bytes_source_remaining_length(source, ref len);
        switch (ret)
        {
            case Errno.OK:
                break;
            case Errno.NO_SUCH_BYTES:
                throw new NoSuchBytesException();
            default:
                throw new UnknownException(ret);
        }

        var requiredLen = checked((int)len);
        if (buffer.Length < requiredLen)
        {
            Array.Resize(ref buffer, requiredLen);
        }

        var written = 0;
        while (true)
        {
            var spare = buffer.AsSpan(written);
            var buf_len = spare.Length;
            ret = FFI.bytes_source_read(source, spare, ref buf_len);
            written += buf_len;
            switch (ret)
            {
                case Errno.EXHAUSTED:
                    return new MemoryStream(
                        buffer,
                        0,
                        written,
                        writable: false,
                        publiclyVisible: true
                    );
                case Errno.OK when written == buffer.Length:
                    Array.Resize(ref buffer, buffer.Length + 1024);
                    break;
                case Errno.OK:
                    break;
                case Errno.NO_SUCH_BYTES:
                    throw new NoSuchBytesException();
                default:
                    throw new UnknownException(ret);
            }
        }
    }

    public static void WriteBytes(BytesSink sink, ReadOnlySpan<byte> bytes) => sink.Write(bytes);

    public static MemoryStream ConsumeBytes(BytesSource source, ref byte[] buffer) =>
        source.Consume(ref buffer);

    public static SpacetimeDB.HttpRequest ReadHttpRequest(
        BytesSource request,
        ref byte[] requestBuffer,
        BytesSource requestBody,
        ref byte[] requestBodyBuffer
    )
    {
        using var stream = ConsumeBytes(request, ref requestBuffer);
        using var reader = new BinaryReader(stream);
        var requestWire = new HttpRequestWire.BSATN().Read(reader);
        EnsureNoUnreadBytes(stream, "HTTP handler request");

        using var requestBodyStream = ConsumeBytes(requestBody, ref requestBodyBuffer);
        return SpacetimeDB.HttpClient.FromWire(requestWire, requestBodyStream.ToArray());
    }

    public static void WriteHttpResponse(
        BytesSink responseSink,
        BytesSink responseBodySink,
        SpacetimeDB.HttpResponse response
    )
    {
        var (responseWire, responseBody) = SpacetimeDB.HttpClient.ToWire(response);
        responseSink.Write(
            IStructuralReadWrite.ToBytes(new HttpResponseWire.BSATN(), responseWire)
        );
        responseBodySink.Write(responseBody);
    }

    public static IReducerContext CreateReducerContext(
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        ulong conn_id_0,
        ulong conn_id_1,
        Timestamp timestamp
    )
    {
        var senderIdentity = Identity.From(
            MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3])
        );
        var connectionId = ConnectionId.From(MemoryMarshal.AsBytes([conn_id_0, conn_id_1]));
        var random = new Random((int)timestamp.MicrosecondsSinceUnixEpoch);
        var time = timestamp.ToStd();

        return newReducerContext!(senderIdentity, connectionId, random, time);
    }

    public static IProcedureContext CreateProcedureContext(
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        ulong conn_id_0,
        ulong conn_id_1,
        Timestamp timestamp
    )
    {
        var sender = Identity.From(MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]));
        var connectionId = ConnectionId.From(MemoryMarshal.AsBytes([conn_id_0, conn_id_1]));
        var random = new Random((int)timestamp.MicrosecondsSinceUnixEpoch);
        var time = timestamp.ToStd();

        return newProcedureContext!(sender, connectionId, random, time);
    }

    public static SpacetimeDB.HandlerContextBase CreateHandlerContext(Timestamp timestamp)
    {
        var random = new Random((int)timestamp.MicrosecondsSinceUnixEpoch);
        var time = timestamp.ToStd();
        return newHandlerContext!(random, time);
    }

    public static IViewContext CreateViewContext(
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3
    )
    {
        var sender = Identity.From(MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]));
        return newViewContext!(sender);
    }

    public static IAnonymousViewContext CreateAnonymousViewContext() => newAnonymousViewContext!();

    public static void EnsureNoUnreadBytes(MemoryStream stream, string description)
    {
        if (stream.Position != stream.Length)
        {
            throw new Exception($"Unrecognised extra bytes in the {description}");
        }
    }

    public static Errno WriteReducerError(BytesSink error, Exception e)
    {
        var error_str = e.Message ?? e.GetType().FullName ?? e.GetType().Name;
        var error_bytes = System.Text.Encoding.UTF8.GetBytes(error_str);
        error.Write(error_bytes);
        return Errno.HOST_CALL_FAILURE;
    }

    private static void Write(this BytesSink sink, ReadOnlySpan<byte> bytes)
    {
        while (!bytes.IsEmpty)
        {
            var written = bytes.Length;
            FFI.bytes_sink_write(sink, bytes, ref written);
            bytes = bytes[written..];
        }
    }

#pragma warning disable IDE1006 // Naming Styles - methods below are meant for FFI.

    public static void __describe_module__(BytesSink description)
    {
        EnsureNativeAotTypeRoots();
        try
        {
            var module = RootBuilder.BuildModuleDefinition();
            RawModuleDef versioned = new RawModuleDef.V10(module);
            var moduleBytes = IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), versioned);
            description.Write(moduleBytes);
        }
        catch (Exception e)
        {
            Log.Error($"Error while describing the module: {e}");
        }
    }
}

/// <summary>
/// Read-write database access for procedure contexts.
/// The code generator will extend this partial class with table accessors.
/// </summary>
public partial class Local
{
    // Intentionally empty – generated code adds table handles here.
}

/// <summary>
/// Read-only database access for view contexts.
/// On .NET 10 the generator provides assembly-scoped extension properties.
/// On .NET 8 generated modules declare their own type with table accessors.
/// </summary>
public sealed partial class LocalReadOnly { }
