namespace SpacetimeDB.Internal;

using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.BSATN;

public sealed class ModuleBuilder
{
    private Module.TypeRegistrar TypeRegistrar { get; }

    public ModuleBuilder() => TypeRegistrar = new Module.TypeRegistrar(this);

    private static class ReducerCache<R>
        where R : IReducer, new()
    {
        public static readonly R Instance = new();
    }

    private static class ProcedureCache<P>
        where P : IProcedure, new()
    {
        public static readonly P Instance = new();
    }

    private static class HttpHandlerCache<H>
        where H : IHttpHandler, new()
    {
        public static readonly H Instance = new();
    }

    private static class ViewDispatcherCache<TDispatcher>
        where TDispatcher : IView, new()
    {
        public static readonly TDispatcher Instance = new();
    }

    private static class AnonymousViewDispatcherCache<TDispatcher>
        where TDispatcher : IAnonymousView, new()
    {
        public static readonly TDispatcher Instance = new();
    }

    private readonly Typespace typespace = new();
    private readonly List<RawSubmoduleV10> submoduleDefs = [];
    private readonly List<RawTypeDefV10> typeDefs = [];
    private readonly List<RawTableDefV10> tableDefs = [];
    private readonly List<RawScheduleDefV10> scheduleDefs = [];
    private readonly List<RawReducerDefV10> reducerDefs = [];
    private readonly List<RawLifeCycleReducerDefV10> lifecycleReducerDefs = [];
    private readonly List<RawProcedureDefV10> procedureDefs = [];
    private readonly List<RawHttpHandlerDefV10> httpHandlerDefs = [];
    private readonly List<RawHttpRouteDefV10> httpRouteDefs = [];
    private readonly List<RawViewDefV10> viewDefs = [];
    private readonly List<RawViewPrimaryKeyDefV10> viewPrimaryKeyDefs = [];
    private readonly List<RawRowLevelSecurityDefV9> rowLevelSecurityDefs = [];
    private readonly Dictionary<string, List<RawColumnDefaultValueV10>> defaultValuesByTable =
        new(StringComparer.Ordinal);

    private SpacetimeDB.CaseConversionPolicy? caseConversionPolicy = null;
    private readonly List<ExplicitNameEntry> explicitNames = [];

    // Note: this intends to generate a valid identifier, but it's not guaranteed to be unique as it's not proper mangling.
    // Fix it up to a different mangling scheme if it causes problems.
    private static string GetFriendlyName(Type type) =>
        type.IsGenericType
            ? $"{type.Name[..type.Name.IndexOf('`')]}_{string.Join("_", type.GetGenericArguments().Select(GetFriendlyName))}"
            : type.Name;

    private static RawScopedTypeNameV10 MakeScopedTypeName(Type type) =>
        new([], GetFriendlyName(type));

    internal void RegisterSubmodule(RawSubmoduleV10 submodule) => submoduleDefs.Add(submodule);

    // Receives types to store the reference in the dictionary so that we can resolve it later and to avoid infinite recursion inside `makeType`.
    internal AlgebraicType.Ref RegisterType<T>(
        Dictionary<Type, AlgebraicType.Ref> types,
        Func<AlgebraicType.Ref, AlgebraicType> makeType
    )
    {
        var typeList = typespace.Types;
        var typeRef = new AlgebraicType.Ref(typeList.Count);
        types.Add(typeof(T), typeRef);
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

    public void RegisterReducer<R>()
        where R : IReducer, new()
    {
        var reducer = ReducerCache<R>.Instance;
        RegisterReducer(reducer.MakeReducerDef(TypeRegistrar), reducer.Lifecycle);
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

    public void RegisterProcedure<P>()
        where P : IProcedure, new()
    {
        var procedure = ProcedureCache<P>.Instance;
        RegisterProcedure(procedure.MakeProcedureDef(TypeRegistrar));
    }

    internal void RegisterProcedure(RawProcedureDefV10 procedure) => procedureDefs.Add(procedure);

    public void RegisterHttpHandler<H>()
        where H : IHttpHandler, new()
    {
        var handler = HttpHandlerCache<H>.Instance;
        RegisterHttpHandler(handler.MakeHandlerDef());
    }

    internal void RegisterHttpHandler(RawHttpHandlerDefV10 handler) => httpHandlerDefs.Add(handler);

    public void RegisterHttpRouter(SpacetimeDB.Router router)
    {
        foreach (var route in router.GetRoutes())
        {
            if (!HasHttpHandler(route.HandlerFunction))
            {
                throw new ArgumentException(
                    $"HTTP router references unknown handler `{route.HandlerFunction}`",
                    nameof(router)
                );
            }

            RegisterHttpRoute(
                new RawHttpRouteDefV10(
                    HandlerFunction: route.HandlerFunction,
                    Method: route.Method,
                    Path: route.Path
                )
            );
        }
    }

    internal bool HasHttpHandler(string sourceName) =>
        httpHandlerDefs.Any(handler => handler.SourceName == sourceName);

    internal void RegisterHttpRoute(RawHttpRouteDefV10 route) => httpRouteDefs.Add(route);

    public void RegisterTable<T, View>()
        where T : IStructuralReadWrite, new()
        where View : ITableView<View, T>, new() =>
        RegisterTable(View.MakeTableDesc(TypeRegistrar), View.MakeScheduleDesc());

    internal void RegisterTable(RawTableDefV10 table, RawScheduleDefV10? schedule)
    {
        tableDefs.Add(table);
        if (schedule is { } scheduleDef)
        {
            scheduleDefs.Add(scheduleDef);
        }
    }

    public void RegisterView<TDispatcher>()
        where TDispatcher : IView, new()
    {
        var dispatcher = ViewDispatcherCache<TDispatcher>.Instance;
        var def = dispatcher.MakeViewDef(TypeRegistrar);
        RegisterView(def);
    }

    public void RegisterAnonymousView<TDispatcher>()
        where TDispatcher : IAnonymousView, new()
    {
        var dispatcher = AnonymousViewDispatcherCache<TDispatcher>.Instance;
        var def = dispatcher.MakeAnonymousViewDef(TypeRegistrar);
        RegisterView(def);
    }

    internal void RegisterView(RawViewDefV10 view) => viewDefs.Add(view);

    public void RegisterViewPrimaryKey(string viewSourceName, IEnumerable<string> columns) =>
        viewPrimaryKeyDefs.Add(new RawViewPrimaryKeyDefV10(viewSourceName, [.. columns]));

    public void RegisterClientVisibilityFilter(Filter rlsFilter)
    {
        if (rlsFilter is Filter.Sql(var rlsSql))
        {
            RegisterRowLevelSecurity(new RawRowLevelSecurityDefV9 { Sql = rlsSql });
        }
        else
        {
            throw new Exception($"Unimplemented row level security type: {rlsFilter}");
        }
    }

    internal void RegisterRowLevelSecurity(RawRowLevelSecurityDefV9 rls) =>
        rowLevelSecurityDefs.Add(rls);

    public void RegisterTableDefaultValue(string table, ushort colId, byte[] value)
    {
        if (!defaultValuesByTable.TryGetValue(table, out var defaults))
        {
            defaults = [];
            defaultValuesByTable.Add(table, defaults);
        }
        defaults.Add(new RawColumnDefaultValueV10(colId, [.. value]));
    }

    public void SetCaseConversionPolicy(SpacetimeDB.CaseConversionPolicy policy) =>
        caseConversionPolicy = policy;

    public void RegisterExplicitTableName(string sourceName, string canonicalName) =>
        explicitNames.Add(new ExplicitNameEntry.Table(new NameMapping(sourceName, canonicalName)));

    public void RegisterExplicitFunctionName(string sourceName, string canonicalName) =>
        explicitNames.Add(
            new ExplicitNameEntry.Function(new NameMapping(sourceName, canonicalName))
        );

    public void RegisterExplicitIndexName(string sourceName, string canonicalName) =>
        explicitNames.Add(new ExplicitNameEntry.Index(new NameMapping(sourceName, canonicalName)));

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
                    DefaultValues: defaults is null ? [] : [.. defaults],
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

        if (submoduleDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Submodules([.. submoduleDefs]));
        }
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
        if (httpHandlerDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.HttpHandlers(httpHandlerDefs));
        }
        if (httpRouteDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.HttpRoutes(httpRouteDefs));
        }
        if (viewDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.Views(viewDefs));
        }
        if (viewPrimaryKeyDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.ViewPrimaryKeys(viewPrimaryKeyDefs));
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
        if (caseConversionPolicy is { } policy)
        {
            sections.Add(new RawModuleDefV10Section.CaseConversionPolicy(policy));
        }
        if (explicitNames.Count > 0)
        {
            sections.Add(
                new RawModuleDefV10Section.ExplicitNames(new ExplicitNames([.. explicitNames]))
            );
        }
        if (rowLevelSecurityDefs.Count > 0)
        {
            sections.Add(new RawModuleDefV10Section.RowLevelSecurity(rowLevelSecurityDefs));
        }

        return new RawModuleDefV10(sections);
    }
}
