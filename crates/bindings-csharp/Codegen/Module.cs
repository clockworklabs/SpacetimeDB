namespace SpacetimeDB.Codegen;

using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

[System.Flags]
enum ColumnAttrs : byte
{
    UnSet = 0b0000,
    Indexed = 0b0001,
    AutoInc = 0b0010,
    Unique = Indexed | 0b0100,
    Identity = Unique | AutoInc,
    PrimaryKey = Unique | 0b1000,
    PrimaryKeyAuto = PrimaryKey | AutoInc,
}

readonly record struct ColumnDeclaration
{
    public readonly string Name;
    public readonly string Type;
    public readonly string TypeInfo;
    public readonly ColumnAttrs IndexKind;
    public readonly bool IsEquatable;

    public ColumnDeclaration(string name, ITypeSymbol type, ColumnAttrs indexKind)
    {
        var isInteger = type.SpecialType switch
        {
            SpecialType.System_Byte
            or SpecialType.System_SByte
            or SpecialType.System_Int16
            or SpecialType.System_UInt16
            or SpecialType.System_Int32
            or SpecialType.System_UInt32
            or SpecialType.System_Int64
            or SpecialType.System_UInt64
                => true,
            SpecialType.None => type.ToString() is "System.Int128" or "System.UInt128",
            _ => false
        };

        if (indexKind.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
        {
            throw new System.Exception(
                $"{type} {name} is not valid for AutoInc or Identity as it's not an integer."
            );
        }

        IsEquatable =
            isInteger
            || type.SpecialType switch
            {
                SpecialType.System_String or SpecialType.System_Boolean => true,
                SpecialType.None
                    => type.ToString()
                        is "SpacetimeDB.Runtime.Address"
                            or "SpacetimeDB.Runtime.Identity",
                _ => false,
            };

        if (indexKind.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            throw new System.Exception(
                $"{type} {name} is not valid for Identity, PrimaryKey or PrimaryKeyAuto as it's not an equatable primitive."
            );
        }

        Name = name;
        Type = SymbolToName(type);
        TypeInfo = GetTypeInfo(type);
        IndexKind = indexKind;
    }
}

record TableDeclaration
{
    public readonly Scope Scope;
    public readonly string Name;
    public readonly string FullName;
    public readonly EquatableArray<ColumnDeclaration> Fields;
    public readonly bool Public;

    public TableDeclaration(
        TypeDeclarationSyntax tableSyntax,
        INamedTypeSymbol table,
        IEnumerable<ColumnDeclaration> columns,
        bool isPublic
    )
    {
        Scope = new Scope(tableSyntax);
        Name = table.Name;
        FullName = SymbolToName(table);
        Fields = new EquatableArray<ColumnDeclaration>(columns.ToImmutableArray());
        Public = isPublic;
    }
}

readonly record struct ReducerParamDeclaration
{
    public readonly string Name;
    public readonly string Type;
    public readonly string TypeInfo;
    public readonly bool IsContextArg;

    public ReducerParamDeclaration(string name, ITypeSymbol type)
    {
        Name = name;
        Type = SymbolToName(type);
        TypeInfo = GetTypeInfo(type);
        IsContextArg = Type == "SpacetimeDB.Runtime.ReducerContext";
    }
}

record ReducerDeclaration
{
    public readonly string Name;
    public readonly string ExportName;
    public readonly string FullName;
    public readonly EquatableArray<ReducerParamDeclaration> Args;
    public readonly Scope Scope;

    public ReducerDeclaration(
        MethodDeclarationSyntax methodSyntax,
        IMethodSymbol method,
        string? exportName
    )
    {
        Name = method.Name;
        ExportName = exportName ?? Name;
        FullName = SymbolToName(method);
        Args = new(
            method
                .Parameters.Select(p => new ReducerParamDeclaration(p.Name, p.Type))
                .ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.TableAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                {
                    var tableSyntax = (TypeDeclarationSyntax)context.TargetNode;

                    var table = context.SemanticModel.GetDeclaredSymbol(tableSyntax)!;

                    var fields = GetFields(table)
                        .Select(f =>
                        {
                            var indexKind = f.GetAttributes()
                                .Where(a =>
                                    a.AttributeClass?.ToString() == "SpacetimeDB.ColumnAttribute"
                                )
                                .Select(a => (ColumnAttrs)a.ConstructorArguments[0].Value!)
                                .SingleOrDefault();

                            return new ColumnDeclaration(f.Name, f.Type, indexKind);
                        });

                    var isPublic = context
                        .Attributes.SelectMany(attr => attr.NamedArguments)
                        .Any(pair => pair.Key == "Public" && pair.Value.Value is true);

                    return new TableDeclaration(tableSyntax, table, fields, isPublic);
                }
            )
            .WithTrackingName("SpacetimeDB.Table.Parse");

        tables
            .Select(
                (t, ct) =>
                {
                    var autoIncFields = t.Fields.Where(f =>
                        f.IndexKind.HasFlag(ColumnAttrs.AutoInc)
                    )
                        .Select(f => f.Name);

                    var extensions =
                        $@"
                            private static readonly Lazy<SpacetimeDB.RawBindings.TableId> tableId = new (() => SpacetimeDB.Runtime.GetTableId(nameof({t.Name})));

                            public static IEnumerable<{t.Name}> Iter() =>
                                new SpacetimeDB.Runtime.RawTableIter(tableId.Value)
                                .Parse<{t.Name}>();

                            public static SpacetimeDB.Module.TableDesc MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                                new (
                                    nameof({t.Name}),
                                    new SpacetimeDB.Module.ColumnDefWithAttrs[] {{ {string.Join(",", t.Fields.Select(f => $@"
                                        new (
                                            new SpacetimeDB.Module.ColumnDef(nameof({f.Name}), BSATN.{f.Name}.GetAlgebraicType(registrar)),
                                            SpacetimeDB.Module.ColumnAttrs.{f.IndexKind}
                                        )
                                    "))} }},
                                    {(t.Public ? "true" : "false")}
                                ),
                                (SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)
                            );

                            private static readonly Lazy<KeyValuePair<string, Action<BinaryWriter, object?>>[]> fieldTypeInfos = new (() => new KeyValuePair<string, Action<BinaryWriter, object?>>[] {{
                                {string.Join("\n", t.Fields.Select(f => $"new (nameof({f.Name}), (w, v) => BSATN.{f.Name}.Write(w, ({f.Type}) v!)),"))}
                            }});

                            public static IEnumerable<{t.Name}> Query(System.Linq.Expressions.Expression<Func<{t.Name}, bool>> filter) =>
                                new SpacetimeDB.Runtime.RawTableIterFiltered(tableId.Value, SpacetimeDB.Filter.Filter.Compile<{t.Name}>(fieldTypeInfos.Value, filter))
                                .Parse<{t.Name}>();

                            public void Insert() {{
                                var bytes = SpacetimeDB.Runtime.Insert(tableId.Value, this);
                                // bytes should contain modified value now with autoinc fields updated
                                {(autoIncFields.Any() ? $@"
                                    using var stream = new System.IO.MemoryStream(bytes);
                                    using var reader = new System.IO.BinaryReader(stream);
                                    ReadFields(reader);
                                " : "")}
                            }}
                        ";

                    foreach (
                        var (f, i) in t.Fields.Select((field, i) => (field, i))
                            .Where(pair => pair.field.IsEquatable)
                    )
                    {
                        var index = $"new SpacetimeDB.RawBindings.ColId({i})";

                        extensions +=
                            $@"
                                public static IEnumerable<{t.Name}> FilterBy{f.Name}({f.Type} {f.Name}) =>
                                    new SpacetimeDB.Runtime.RawTableIterByColEq(tableId.Value, {index}, SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.{f.Name}, {f.Name}))
                                    .Parse<{t.Name}>();
                            ";

                        if (f.IndexKind.HasFlag(ColumnAttrs.Unique))
                        {
                            extensions +=
                                $@"
                                    public static {t.Name}? FindBy{f.Name}({f.Type} {f.Name}) =>
                                        FilterBy{f.Name}({f.Name})
                                        .Cast<{t.Name}?>()
                                        .SingleOrDefault();

                                    public static bool DeleteBy{f.Name}({f.Type} {f.Name}) =>
                                        SpacetimeDB.Runtime.DeleteByColEq(tableId.Value, {index}, SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.{f.Name}, {f.Name})) > 0;

                                    public static bool UpdateBy{f.Name}({f.Type} {f.Name}, {t.Name} value) =>
                                        SpacetimeDB.Runtime.UpdateByColEq(tableId.Value, {index}, SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.{f.Name}, {f.Name}), value);
                                ";
                        }
                    }

                    return new KeyValuePair<string, string>(
                        t.FullName,
                        t.Scope.GenerateExtensions(extensions)
                    );
                }
            )
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var reducers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: "SpacetimeDB.ReducerAttribute",
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                {
                    var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;

                    var method = context.SemanticModel.GetDeclaredSymbol(methodSyntax)!;

                    if (!method.ReturnsVoid)
                    {
                        throw new System.Exception($"Reducer {method} must return void");
                    }

                    var exportName = (string?)
                        context
                            .Attributes.SingleOrDefault()
                            ?.ConstructorArguments
                            .SingleOrDefault()
                            .Value;

                    return new ReducerDeclaration(methodSyntax, method, exportName);
                }
            )
            .WithTrackingName("SpacetimeDB.Reducer.Parse");

        var addReducers = reducers
            .Select(
                (r, ct) =>
                    (
                        r.Name,
                        Class: $@"
                            class {r.Name}: IReducer {{
                                {string.Join("\n", r.Args.Where(a => !a.IsContextArg).Select(a => $"{a.TypeInfo} {a.Name} = new();"))}

                                SpacetimeDB.Module.ReducerDef IReducer.MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) {{
                                    return new (
                                        ""{r.ExportName}""
                                        {string.Join("", r.Args.Where(a => !a.IsContextArg).Select(a => $",\nnew SpacetimeDB.BSATN.AggregateElement(nameof({a.Name}), {a.Name}.GetAlgebraicType(registrar))"))}
                                    );
                                }}

                                void IReducer.Invoke(BinaryReader reader, SpacetimeDB.Runtime.ReducerContext ctx) {{
                                    {r.FullName}({string.Join(", ", r.Args.Select(a => a.IsContextArg ? "ctx" : $"{a.Name}.Read(reader)"))});
                                }}
                            }}
                        "
                    )
            )
            .WithTrackingName("SpacetimeDB.Reducer.GenerateClass")
            .Collect();

        context.RegisterSourceOutput(
            tableNames.Combine(addReducers),
            (context, tuple) =>
            {
                // Sort tables and reducers by name to match Rust behaviour.
                // Not really important outside of testing, but for testing
                // it matters because we commit module-bindings
                // so they need to match 1:1 between different langs.
                var tableNames = tuple.Left.Sort();
                var addReducers = tuple.Right.Sort((a, b) => a.Name.CompareTo(b.Name));
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (tableNames.IsEmpty && addReducers.IsEmpty)
                    return;
                context.AddSource(
                    "FFI.cs",
                    $@"
            // <auto-generated />
            #nullable enable

            using static SpacetimeDB.RawBindings;
            using SpacetimeDB.Module;
            using System.Runtime.CompilerServices;
            using System.Runtime.InteropServices;
            using static SpacetimeDB.Runtime;
            using System.Diagnostics.CodeAnalysis;

            using Buffer = SpacetimeDB.RawBindings.Buffer;

            static class ModuleRegistration {{
                {string.Join("\n", addReducers.Select(r => r.Class))}

#if EXPERIMENTAL_WASM_AOT
                // In AOT mode we're building a library.
                // Main method won't be called automatically, so we need to export it as a preinit function.
                [UnmanagedCallersOnly(EntryPoint = ""__preinit__10_init_csharp"")]
#else
                // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(FFI))]
#endif
                public static void Main() {{
                    {string.Join("\n", addReducers.Select(r => $"FFI.RegisterReducer(new {r.Name}());"))}
                    {string.Join("\n", tableNames.Select(t => $"FFI.RegisterTable({t}.MakeTableDesc(FFI.TypeRegistrar));"))}
                }}

// Exports only work from the main assembly, so we need to generate forwarding methods.
#if EXPERIMENTAL_WASM_AOT
                [UnmanagedCallersOnly(EntryPoint = ""__describe_module__"")]
                public static Buffer __describe_module__() => FFI.__describe_module__();

                [UnmanagedCallersOnly(EntryPoint = ""__call_reducer__"")]
                public static Buffer __call_reducer__(
                    uint id,
                    Buffer caller_identity,
                    Buffer caller_address,
                    ulong timestamp,
                    Buffer args
                ) => FFI.__call_reducer__(id, caller_identity, caller_address, timestamp, args);
#endif
            }}
            "
                );
            }
        );

        reducers
            .Select(
                (r, ct) =>
                    new KeyValuePair<string, string>(
                        r.FullName,
                        r.Scope.GenerateExtensions(
                            $@"
                            public static SpacetimeDB.Runtime.ScheduleToken Schedule{r.Name}(DateTimeOffset time{string.Join("", r.Args.Where(a => !a.IsContextArg).Select(a => $", {a.Type} {a.Name}"))}) {{
                                using var stream = new MemoryStream();
                                using var writer = new BinaryWriter(stream);
                                {string.Join("\n", r.Args.Where(a => !a.IsContextArg).Select(a => $"new {a.TypeInfo}().Write(writer, {a.Name});"))}
                                return new(nameof({r.Name}), stream.ToArray(), time);
                            }}
                        "
                        )
                    )
            )
            .WithTrackingName("SpacetimeDB.Reducer.GenerateSchedule")
            .RegisterSourceOutputs(context);
    }
}
