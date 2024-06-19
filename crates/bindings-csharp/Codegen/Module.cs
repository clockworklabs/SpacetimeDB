namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using SpacetimeDB.Codegen.Utils;

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

record ColumnDeclaration : FieldDeclaration
{
    public readonly ColumnAttrs Attrs;
    public readonly bool IsEquatable;

    public ColumnDeclaration(IFieldSymbol field)
        : base(field)
    {
        Attrs = field
            .GetAttributes()
            .Where(a => a.AttributeClass?.ToString() == "SpacetimeDB.ColumnAttribute")
            .Select(a => (ColumnAttrs)a.ConstructorArguments[0].Value!)
            .SingleOrDefault();

        var type = field.Type;

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

        if (Attrs.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
        {
            throw new System.Exception(
                $"{type} {this} is not valid for AutoInc or Identity as it's not an integer."
            );
        }

        IsEquatable =
            (
                isInteger
                || type.SpecialType switch
                {
                    SpecialType.System_String or SpecialType.System_Boolean => true,
                    SpecialType.None
                        => type.ToString()
                            is "SpacetimeDB.Runtime.Address"
                                or "SpacetimeDB.Runtime.Identity",
                    _ => false,
                }
            )
            && type.NullableAnnotation != NullableAnnotation.Annotated;

        if (Attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            throw new System.Exception(
                $"{type} {this} is not valid for Identity, PrimaryKey or PrimaryKeyAuto as it's not an equatable primitive."
            );
        }
    }
}

record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly string FullName;
    public readonly bool IsPublic;

    public TableDeclaration(GeneratorAttributeSyntaxContext context)
        : base(context)
    {
        FullName = context.TargetSymbol.GetFullName();

        IsPublic = context
            .Attributes.Single()
            .NamedArguments.Any(pair => pair is { Key: "Public", Value.Value: true });
    }

    protected override ColumnDeclaration ConvertMember(IFieldSymbol field) => new(field);

    public override string ToExtensions()
    {
        var autoIncFields = Members
            .Where(f => f.Attrs.HasFlag(ColumnAttrs.AutoInc))
            .Select(f => f.Name);

        var extensions = $$"""
            {{base.ToExtensions()}}

            private static readonly Lazy<SpacetimeDB.RawBindings.TableId> tableId = new (() => SpacetimeDB.Runtime.GetTableId(nameof({{this}})));

            public static IEnumerable<{{this}}> Iter() =>
                new SpacetimeDB.Runtime.RawTableIter(tableId.Value)
                .Parse<{{this}}>();

            public static SpacetimeDB.Module.TableDesc MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                new (
                    nameof({{this}}),
                    new SpacetimeDB.Module.ColumnDefWithAttrs[] { {{string.Join(",", Members.Select(f =>
                        $"""
                        new (
                            new SpacetimeDB.Module.ColumnDef(nameof({f}), BSATN.{f}.GetAlgebraicType(registrar)),
                            SpacetimeDB.Module.ColumnAttrs.{f.Attrs}
                        )
                        """
                    ))}} },
                    {{(IsPublic ? "true" : "false")}}
                ),
                (SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)
            );

            private static readonly Lazy<KeyValuePair<string, Action<BinaryWriter, object?>>[]> fieldTypeInfos = new (() => new KeyValuePair<string, Action<BinaryWriter, object?>>[] {
                {{Members.Join("\n", f => $"new (nameof({f}), (w, v) => BSATN.{f}.Write(w, ({f.Type}) v!)),")}}
            });

            public static IEnumerable<{{this}}> Query(System.Linq.Expressions.Expression<Func<{{this}}, bool>> filter) =>
                new SpacetimeDB.Runtime.RawTableIterFiltered(tableId.Value, SpacetimeDB.Filter.Filter.Compile<{{this}}>(fieldTypeInfos.Value, filter))
                .Parse<{{this}}>();

            public void Insert() {
                var bytes = SpacetimeDB.Runtime.Insert(tableId.Value, this);
                // bytes should contain modified value now with autoinc fields updated
                {{(autoIncFields.Any() ?
                $"""
                using var stream = new System.IO.MemoryStream(bytes);
                using var reader = new System.IO.BinaryReader(stream);
                ReadFields(reader);
                """
                : "")}}
            }
            """;

        foreach (
            var (f, i) in Members
                .Select((field, i) => (field, i))
                .Where(pair => pair.field.IsEquatable)
        )
        {
            var index = $"new SpacetimeDB.RawBindings.ColId({i})";

            extensions += $"""
                public static IEnumerable<{this}> FilterBy{f}({f.Type} {f}) =>
                    new SpacetimeDB.Runtime.RawTableIterByColEq(tableId.Value, {index}, SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.{f}, {f}))
                    .Parse<{this}>();
                """;

            if (f.Attrs.HasFlag(ColumnAttrs.Unique))
            {
                extensions += $"""
                    public static {this}? FindBy{f}({f.Type} {f}) =>
                        FilterBy{f}({f})
                        .Cast<{this}?>()
                        .SingleOrDefault();

                    public static bool DeleteBy{f}({f.Type} {f}) =>
                        SpacetimeDB.Runtime.DeleteByColEq(tableId.Value, {index}, SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.{f}, {f})) > 0;

                    public static bool UpdateBy{f}({f.Type} {f}, {this} value) =>
                        SpacetimeDB.Runtime.UpdateByColEq(tableId.Value, {index}, SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.{f}, {f}), value);
                    """;
            }
        }

        return extensions;
    }
}

record ReducerParamDeclaration : MemberDeclaration
{
    public readonly bool IsContextArg;

    public ReducerParamDeclaration(string name, ITypeSymbol typeSymbol)
        : base(name, typeSymbol)
    {
        IsContextArg = Type.FullName == "SpacetimeDB.Runtime.ReducerContext";
    }
}

record ReducerDeclaration : SourceOutput
{
    public readonly string FullName;
    public readonly string ExportName;
    public readonly EquatableArray<ReducerParamDeclaration> Args;

    public ReducerDeclaration(
        MethodDeclarationSyntax methodSyntax,
        IMethodSymbol method,
        AttributeData attr
    )
        : base(new Scope(methodSyntax.Parent as MemberDeclarationSyntax), method)
    {
        if (!method.ReturnsVoid)
        {
            Diagnostics.ReducerReturnType.Report(Diagnostics, methodSyntax);
        }

        FullName = method.GetFullName();

        ExportName = attr.ConstructorArguments.Select(arg => arg.Value).SingleOrDefault() switch
        {
            { } name => (string)name!,
            _ => Name
        };

        Args = new(
            method
                .Parameters.Select(p => new ReducerParamDeclaration(p.Name, p.Type))
                .ToImmutableArray()
        );
    }

    public IEnumerable<ReducerParamDeclaration> GetNonContextArgs() =>
        Args.Where(a => !a.IsContextArg);

    public override string ToExtensions()
    {
        return $$"""
            // We need to generate a class, but we don't want it to be visible to users in autocomplete.
            [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Advanced)]
            internal class {{this}}_BSATN: IReducer {
                {{GetNonContextArgs().Join("\n", a =>
                    $"internal static readonly {a.Type.BSATN} {a} = new();"
                )}}

                SpacetimeDB.Module.ReducerDef IReducer.MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) {
                    return new (
                        "{{ExportName}}"
                        {{GetNonContextArgs().Join("", a =>
                            $",\nnew SpacetimeDB.BSATN.AggregateElement(nameof({a}), {a}.GetAlgebraicType(registrar))"
                        )}}
                    );
                }

                void IReducer.Invoke(BinaryReader reader, SpacetimeDB.Runtime.ReducerContext ctx) {
                    {{this}}({{Args.Join(", ", a => a.IsContextArg ? "ctx" : $"{a}.Read(reader)")}});
                }
            }

            public static SpacetimeDB.Runtime.ScheduleToken Schedule{{this}}(DateTimeOffset time{{GetNonContextArgs().Join("", a => $", {a.Type} {a}")}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{GetNonContextArgs().Join("\n", a =>
                    $"{this}_BSATN.{a}.Write(writer, {a});"
                )}}
                return new(nameof({{this}}), stream, time);
            }
            """;
    }
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context.HandleDerives(
            "SpacetimeDB.Table",
            context => new TableDeclaration(context)
        );

        var reducers = context.HandleDerives(
            "SpacetimeDB.Reducer",
            context => new ReducerDeclaration(
                (MethodDeclarationSyntax)context.TargetNode,
                (IMethodSymbol)context.TargetSymbol,
                context.Attributes.Single()
            )
        );

        var tableNames = tables.Select((t, ct) => t.FullName).Collect();

        var reducerNames = reducers.Select((r, ct) => r.FullName).Collect();

        context.RegisterSourceOutput(
            tableNames.Combine(reducerNames),
            (context, tuple) =>
            {
                // Sort tables and reducers by name to match Rust behaviour.
                // Not really important outside of testing, but for testing
                // it matters because we commit module-bindings
                // so they need to match 1:1 between different langs.
                var tableNames = tuple.Left.Sort();
                var reducerNames = tuple.Right.Sort();

                context.AddSource(
                    "FFI.g.cs",
                    $$"""
                    #nullable enable

                    using static SpacetimeDB.RawBindings;
                    using SpacetimeDB.Module;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;
                    using static SpacetimeDB.Runtime;
                    using System.Diagnostics.CodeAnalysis;

                    using Buffer = SpacetimeDB.RawBindings.Buffer;

                    static class ModuleRegistration {
                    #if EXPERIMENTAL_WASM_AOT
                        // In AOT mode we're building a library.
                        // Main method won't be called automatically, so we need to export it as a preinit function.
                        [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
                    #else
                        // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                        [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(FFI))]
                    #endif
                        public static void Main() {
                            {{reducerNames.Join("\n", r =>
                                $"FFI.RegisterReducer<{r}>();"
                            )}}
                            {{tableNames.Join("\n", t =>
                                $"FFI.RegisterTable({t}.MakeTableDesc(FFI.TypeRegistrar));"
                            )}}
                        }

                    // Exports only work from the main assembly, so we need to generate forwarding methods.
                    #if EXPERIMENTAL_WASM_AOT
                        [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
                        public static Buffer __describe_module__() => FFI.__describe_module__();

                        [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                        public static Buffer __call_reducer__(
                            uint id,
                            Buffer caller_identity,
                            Buffer caller_address,
                            ulong timestamp,
                            Buffer args
                        ) => FFI.__call_reducer__(id, caller_identity, caller_address, timestamp, args);
                    #endif
                    }
                    """
                );
            }
        );
    }
}
