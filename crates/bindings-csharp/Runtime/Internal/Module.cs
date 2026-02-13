namespace SpacetimeDB.Internal;

using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using SpacetimeDB;
using SpacetimeDB.BSATN;

partial class RawModuleDefV10
{
    private readonly Typespace typespace = new();
    private readonly List<RawTypeDefV10> typeDefs = [];
    private readonly List<RawTableDefV10> tableDefs = [];
    private readonly List<RawScheduleDefV10> scheduleDefs = [];
    private readonly List<RawReducerDefV10> reducerDefs = [];
    private readonly List<RawLifeCycleReducerDefV10> lifecycleReducerDefs = [];
    private readonly List<RawProcedureDefV10> procedureDefs = [];
    private readonly List<RawViewDefV10> viewDefs = [];
    private readonly List<RawRowLevelSecurityDefV9> rowLevelSecurityDefs = [];
    private readonly Dictionary<string, List<RawColumnDefaultValueV10>> defaultValuesByTable =
        new(StringComparer.Ordinal);

    // Note: this intends to generate a valid identifier, but it's not guaranteed to be unique as it's not proper mangling.
    // Fix it up to a different mangling scheme if it causes problems.
    private static string GetFriendlyName(Type type) =>
        type.IsGenericType
            ? $"{type.Name.Remove(type.Name.IndexOf('`'))}_{string.Join("_", type.GetGenericArguments().Select(GetFriendlyName))}"
            : type.Name;

    private static RawScopedTypeNameV10 MakeScopedTypeName(Type type) =>
        new(new List<string>(), GetFriendlyName(type));

    internal AlgebraicType.Ref RegisterType<T>(Func<AlgebraicType.Ref, AlgebraicType> makeType)
    {
        var typeList = typespace.Types;
        var typeRef = new AlgebraicType.Ref(typeList.Count);
        // Put a dummy self-reference just so that we get stable index even if `makeType` recursively adds more types.
        typeList.Add(typeRef);
        typeList[typeRef.Ref_] = makeType(typeRef);
        typeDefs.Add(
            new RawTypeDefV10(
                SourceName: MakeScopedTypeName(typeof(T)),
                Ty: (uint)typeRef.Ref_,
                CustomOrdering: true
            )
        );
        return typeRef;
    }

    internal void RegisterReducer(RawReducerDefV10 reducer, Lifecycle? lifecycle)
    {
        reducerDefs.Add(reducer);
        if (lifecycle is { } lifecycleSpec)
        {
            lifecycleReducerDefs.Add(
                new RawLifeCycleReducerDefV10(lifecycleSpec, reducer.SourceName)
            );
            reducer.Visibility = FunctionVisibility.Private;
        }
    }

    internal void RegisterProcedure(RawProcedureDefV10 procedure) => procedureDefs.Add(procedure);

    internal void RegisterTable(RawTableDefV10 table, RawScheduleDefV10? schedule)
    {
        tableDefs.Add(table);
        if (schedule is { } scheduleDef)
        {
            scheduleDefs.Add(scheduleDef);
        }
    }

    internal void RegisterView(RawViewDefV10 view) => viewDefs.Add(view);

    internal void RegisterRowLevelSecurity(RawRowLevelSecurityDefV9 rls) =>
        rowLevelSecurityDefs.Add(rls);

    internal void RegisterTableDefaultValue(string table, ushort colId, byte[] value)
    {
        if (!defaultValuesByTable.TryGetValue(table, out var defaults))
        {
            defaults = [];
            defaultValuesByTable.Add(table, defaults);
        }
        defaults.Add(new RawColumnDefaultValueV10(colId, new List<byte>(value)));
    }

    internal RawModuleDefV10 BuildModuleDefinition()
    {
        var builtTables = new List<RawTableDefV10>(tableDefs.Count);
        foreach (var table in tableDefs)
        {
            defaultValuesByTable.TryGetValue(table.SourceName, out var defaults);
            builtTables.Add(
                new RawTableDefV10(
                    SourceName: table.SourceName,
                    ProductTypeRef: table.ProductTypeRef,
                    PrimaryKey: table.PrimaryKey,
                    Indexes: table.Indexes,
                    Constraints: table.Constraints,
                    Sequences: table.Sequences,
                    TableType: table.TableType,
                    TableAccess: table.TableAccess,
                    DefaultValues: defaults is null
                        ? []
                        : new List<RawColumnDefaultValueV10>(defaults),
                    IsEvent: table.IsEvent
                )
            );
        }

        var internalFunctions = lifecycleReducerDefs
            .Select(l => l.FunctionName)
            .Concat(scheduleDefs.Select(s => s.FunctionName))
            .ToHashSet(StringComparer.Ordinal);

        foreach (var reducer in reducerDefs)
        {
            if (internalFunctions.Contains(reducer.SourceName))
            {
                reducer.Visibility = FunctionVisibility.Private;
            }
        }

        foreach (var procedure in procedureDefs)
        {
            if (internalFunctions.Contains(procedure.SourceName))
            {
                procedure.Visibility = FunctionVisibility.Private;
            }
        }

        var sections = new List<RawModuleDefV10Section>
        {
            new RawModuleDefV10Section.Typespace(typespace),
        };

        if (typeDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Types(typeDefs));
        }
        if (builtTables.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Tables(builtTables));
        }
        if (reducerDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Reducers(reducerDefs));
        }
        if (procedureDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Procedures(procedureDefs));
        }
        if (viewDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Views(viewDefs));
        }
        if (scheduleDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Schedules(scheduleDefs));
        }
        if (lifecycleReducerDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.LifeCycleReducers(lifecycleReducerDefs));
        }
        // TODO: Add sections for Event tables and Case conversion policy (mirrors Rust `raw_def/v10.rs` TODO).
        if (rowLevelSecurityDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.RowLevelSecurity(rowLevelSecurityDefs));
        }

        Sections = sections;
        return this;
    }
}

public static class Module
{
    private static readonly RawModuleDefV10 moduleDef = new();

    private static readonly List<IReducer> reducers = [];
    private static readonly List<IProcedure> procedures = [];
    private static readonly List<IView> viewDispatchers = [];
    private static readonly List<IAnonymousView> anonymousViewDispatchers = [];

    private static Func<
        Identity,
        ConnectionId?,
        Random,
        Timestamp,
        IReducerContext
    >? newReducerContext = null;
    private static Func<Identity, IViewContext>? newViewContext = null;
    private static Func<IAnonymousViewContext>? newAnonymousViewContext = null;

    private static Func<
        Identity,
        ConnectionId?,
        Random,
        Timestamp,
        IProcedureContext
    >? newProcedureContext = null;

    public static void SetReducerContextConstructor(
        Func<Identity, ConnectionId?, Random, Timestamp, IReducerContext> ctor
    ) => newReducerContext = ctor;

    public static void SetProcedureContextConstructor(
        Func<Identity, ConnectionId?, Random, Timestamp, IProcedureContext> ctor
    ) => newProcedureContext = ctor;

    public static void SetViewContextConstructor(Func<Identity, IViewContext> ctor) =>
        newViewContext = ctor;

    public static void SetAnonymousViewContextConstructor(Func<IAnonymousViewContext> ctor) =>
        newAnonymousViewContext = ctor;

    public readonly struct TypeRegistrar() : ITypeRegistrar
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
        moduleDef.RegisterReducer(reducer.MakeReducerDef(typeRegistrar), reducer.Lifecycle);
    }

    public static void RegisterProcedure<P>()
        where P : IProcedure, new()
    {
        var procedure = new P();
        procedures.Add(procedure);
        moduleDef.RegisterProcedure(procedure.MakeProcedureDef(typeRegistrar));
    }

    public static void RegisterTable<T, View>()
        where T : IStructuralReadWrite, new()
        where View : ITableView<View, T>, new()
    {
        moduleDef.RegisterTable(View.MakeTableDesc(typeRegistrar), View.MakeScheduleDesc());
    }

    public static void RegisterView<TDispatcher>()
        where TDispatcher : IView, new()
    {
        var dispatcher = new TDispatcher();
        var def = dispatcher.MakeViewDef(typeRegistrar);
        viewDispatchers.Add(dispatcher);
        moduleDef.RegisterView(def);
    }

    public static void RegisterAnonymousView<TDispatcher>()
        where TDispatcher : IAnonymousView, new()
    {
        var dispatcher = new TDispatcher();
        var def = dispatcher.MakeAnonymousViewDef(typeRegistrar);
        anonymousViewDispatchers.Add(dispatcher);
        moduleDef.RegisterView(def);
    }

    public static void RegisterClientVisibilityFilter(Filter rlsFilter)
    {
        if (rlsFilter is Filter.Sql(var rlsSql))
        {
            moduleDef.RegisterRowLevelSecurity(new RawRowLevelSecurityDefV9 { Sql = rlsSql });
        }
        else
        {
            throw new Exception($"Unimplemented row level security type: {rlsFilter}");
        }
    }

    public static void RegisterTableDefaultValue(string table, ushort colId, byte[] value) =>
        moduleDef.RegisterTableDefaultValue(table, colId, value);

    public static byte[] Consume(this BytesSource source)
    {
        if (source == BytesSource.INVALID)
        {
            return [];
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

        var buffer = new byte[len];
        var written = 0U;
        // Because we've reserved space in our buffer already, this loop should be unnecessary.
        // We expect the first call to `bytes_source_read` to always return `-1`.
        // I (pgoldman 2025-09-26) am leaving the loop here because there's no downside to it,
        // and in the future we may want to support `BytesSource`s which don't have a known length ahead of time
        // (i.e. put arbitrary streams in `BytesSource` on the host side rather than just `Bytes` buffers),
        // at which point the loop will become useful again.
        while (true)
        {
            // Write into the spare capacity of the buffer.
            var spare = buffer.AsSpan((int)written);
            var buf_len = (uint)spare.Length;
            ret = FFI.bytes_source_read(source, spare, ref buf_len);
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
            FFI.bytes_sink_write(sink, buffer, ref written);
            start += written;
        }
    }

#pragma warning disable IDE1006 // Naming Styles - methods below are meant for FFI.

    public static void __describe_module__(BytesSink description)
    {
        try
        {
            var module = moduleDef.BuildModuleDefinition();
            RawModuleDef versioned = new RawModuleDef.V10(module);
            var moduleBytes = IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), versioned);
            description.Write(moduleBytes);
        }
        catch (Exception e)
        {
            Log.Error($"Error while describing the module: {e}");
        }
    }

    public static Errno __call_reducer__(
        uint id,
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        ulong conn_id_0,
        ulong conn_id_1,
        Timestamp timestamp,
        BytesSource args,
        BytesSink error
    )
    {
        try
        {
            var senderIdentity = Identity.From(
                MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]).ToArray()
            );
            var connectionId = ConnectionId.From(
                MemoryMarshal.AsBytes([conn_id_0, conn_id_1]).ToArray()
            );
            var random = new Random((int)timestamp.MicrosecondsSinceUnixEpoch);
            var time = timestamp.ToStd();

            var ctx = newReducerContext!(senderIdentity, connectionId, random, time);

            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            reducers[(int)id].Invoke(reader, ctx);
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the reducer arguments");
            }
            return Errno.OK; /* no exception */
        }
        catch (Exception e)
        {
            var error_str = e.Message ?? e.GetType().FullName;
            var error_bytes = System.Text.Encoding.UTF8.GetBytes(error_str);
            error.Write(error_bytes);
            return Errno.HOST_CALL_FAILURE;
        }
    }

    public static Errno __call_procedure__(
        uint id,
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        ulong conn_id_0,
        ulong conn_id_1,
        Timestamp timestamp,
        BytesSource args,
        BytesSink resultSink
    )
    {
        try
        {
            var sender = Identity.From(
                MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]).ToArray()
            );
            var connectionId = ConnectionId.From(
                MemoryMarshal.AsBytes([conn_id_0, conn_id_1]).ToArray()
            );
            var random = new Random((int)timestamp.MicrosecondsSinceUnixEpoch);
            var time = timestamp.ToStd();

            var ctx = newProcedureContext!(sender, connectionId, random, time);

            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            var bytes = procedures[(int)id].Invoke(reader, ctx);
            if (stream.Position != stream.Length)
            {
                throw new Exception("Unrecognised extra bytes in the procedure arguments");
            }
            resultSink.Write(bytes);

            return Errno.OK;
        }
        catch (Exception e)
        {
            // Host contract __call_procedure__ must either return Errno.OK or trap.
            // Returning other errno values here can put the host/runtime in an unexpected state,
            // so we log and rethrow to trap on any exception.
            Log.Error($"Error while invoking procedure: {e}");
            throw;
        }
    }

    /// <summary>
    /// Called by the host to execute a view when the sender calls the view identified by <paramref name="id" />.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The sender identity is passed as 4 <see cref="ulong" /> values (<paramref name="sender_0" /> through
    /// <paramref name="sender_3" />) representing a little-endian <see cref="SpacetimeDB.Identity" />.
    /// </para>
    /// <para>
    /// <paramref name="args" /> is a host-registered <see cref="BytesSource" /> containing the BSATN-encoded
    /// view arguments. For empty arguments, <paramref name="args" /> will be invalid.
    /// </para>
    /// <para>
    /// The view output is written to <paramref name="rows" />, a host-registered <see cref="BytesSink" />.
    /// </para>
    /// <para>
    /// Note: a previous view ABI wrote the return rows directly to the sink.
    /// The current ABI writes a BSATN-encoded <see cref="ViewResultHeader" /> first, in order to distinguish
    /// between views that return row data and views that return queries.
    /// </para>
    /// <para>
    /// The current ABI is identified by returning error code <c>2</c>.
    /// </para>
    /// </remarks>
    public static Errno __call_view__(
        uint id,
        ulong sender_0,
        ulong sender_1,
        ulong sender_2,
        ulong sender_3,
        BytesSource args,
        BytesSink rows
    )
    {
        try
        {
            var sender = Identity.From(
                MemoryMarshal.AsBytes([sender_0, sender_1, sender_2, sender_3]).ToArray()
            );
            var ctx = newViewContext!(sender);
            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            var bytes = viewDispatchers[(int)id].Invoke(reader, ctx);
            rows.Write(bytes);
            return (Errno)2;
        }
        catch (Exception e)
        {
            Log.Error($"Error while invoking view: {e}");
            return Errno.HOST_CALL_FAILURE;
        }
    }

    /// <summary>
    /// Called by the host to execute an anonymous view.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <paramref name="args" /> is a host-registered <see cref="BytesSource" /> containing the BSATN-encoded
    /// view arguments. For empty arguments, <paramref name="args" /> will be invalid.
    /// </para>
    /// <para>
    /// The view output is written to <paramref name="rows" />, a host-registered <see cref="BytesSink" />.
    /// </para>
    /// <para>
    /// Note: a previous view ABI wrote the return rows directly to the sink.
    /// The current ABI writes a BSATN-encoded <see cref="ViewResultHeader" /> first, in order to distinguish
    /// between views that return row data and views that return queries.
    /// </para>
    /// <para>
    /// The current ABI is identified by returning error code <c>2</c>.
    /// </para>
    /// </remarks>
    public static Errno __call_view_anon__(uint id, BytesSource args, BytesSink rows)
    {
        try
        {
            var ctx = newAnonymousViewContext!();
            using var stream = new MemoryStream(args.Consume());
            using var reader = new BinaryReader(stream);
            var bytes = anonymousViewDispatchers[(int)id].Invoke(reader, ctx);
            rows.Write(bytes);
            return (Errno)2;
        }
        catch (Exception e)
        {
            Log.Error($"Error while invoking anonymous view: {e}");
            return Errno.HOST_CALL_FAILURE;
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
/// The code generator will extend this partial class to add table accessors.
/// </summary>
public sealed partial class LocalReadOnly
{
    // This class is intentionally empty - the code generator will add
    // read-only table accessors for each table in the module.
    // Example generated code:
    // public Internal.ViewHandles.UserReadOnly User => new();
}
