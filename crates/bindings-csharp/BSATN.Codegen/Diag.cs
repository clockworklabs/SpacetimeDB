namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Linq.Expressions;
using System.Runtime.CompilerServices;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static SpacetimeDB.Codegen.Utils;

public record ErrorDescriptorGroup(string Tag, string Name)
{
    private int idCounter = 0;

    public string NextId() => $"{Tag}{++idCounter:D4}";
}

// Roslyn `Diagnostics` requires declaration using old-school string format ("foo {0} bar {1}")
// which is not very type-safe when you want to pass arguments from arbitrary places in the codebase.
//
// This helper class allows us to define diagnostics in a type-safe way by using LINQ expressions
// to extract the format and arguments from a normal-looking lambda.
public class ErrorDescriptor<TContext>
{
    private readonly DiagnosticDescriptor descriptor;
    private readonly Func<TContext, Location?> toLocation;
    private readonly Func<TContext, object[]> makeFormatArgs;

    public ErrorDescriptor(
        ErrorDescriptorGroup group,
        string title,
        Expression<Func<TContext, FormattableString>> interpolate,
        Func<TContext, Location?> toLocation
    )
    {
        this.toLocation = toLocation;
        if (
            interpolate.Body
                is not MethodCallExpression
                {
                    Method:
                    {
                        DeclaringType: var declaringType,
                        Name: nameof(FormattableStringFactory.Create)
                    },
                    Arguments: [
                        ConstantExpression { Value: string messageFormat },
                        NewArrayExpression args,
                    ]
                }
            || declaringType != typeof(FormattableStringFactory)
        )
        {
            throw new InvalidOperationException(
                $"Expected an interpolated string as a lambda body but got {interpolate.Body}."
            );
        }
        descriptor = new(
            id: group.NextId(),
            title: title,
            messageFormat: messageFormat,
            category: group.Name,
            defaultSeverity: DiagnosticSeverity.Error,
            isEnabledByDefault: true
        );
        makeFormatArgs = Expression
            .Lambda<Func<TContext, object[]>>(args, interpolate.Parameters)
            .Compile();
    }

    public ErrorDescriptor(
        ErrorDescriptorGroup group,
        string title,
        Expression<Func<TContext, FormattableString>> interpolate,
        Func<TContext, SyntaxToken> toLocation
    )
        : this(group, title, interpolate, ctx => toLocation(ctx).GetLocation()) { }

    public ErrorDescriptor(
        ErrorDescriptorGroup group,
        string title,
        Expression<Func<TContext, FormattableString>> interpolate,
        Func<TContext, SyntaxNode> toLocation
    )
        : this(group, title, interpolate, ctx => toLocation(ctx).GetLocation()) { }

    public ErrorDescriptor(
        ErrorDescriptorGroup group,
        string title,
        Expression<Func<TContext, FormattableString>> interpolate,
        Func<TContext, ISymbol> toLocation
    )
        : this(group, title, interpolate, ctx => toLocation(ctx).Locations.FirstOrDefault()) { }

    public ErrorDescriptor(
        ErrorDescriptorGroup group,
        string title,
        Expression<Func<TContext, FormattableString>> interpolate,
        Func<TContext, AttributeData> toLocation
    )
        : this(
            group,
            title,
            interpolate,
            ctx =>
                toLocation(ctx).ApplicationSyntaxReference is { } r
                    ? r.SyntaxTree.GetLocation(r.Span)
                    : null
        )
    { }

    public Diagnostic ToDiag(TContext ctx) =>
        Diagnostic.Create(descriptor, toLocation(ctx), makeFormatArgs(ctx));
}

/// <summary>
/// No-op error descriptor placeholder.
///
/// <para>Error descriptors must have strong ID to avoid breaking semver, since they are used for diagnostic suppression by users.</para>
/// <para>To ensure this, we cannot reorder to delete unused diagnostics - instead, we need to put some placeholders where they used to be.</para>
/// <para>This class serves that purpose - it's a no-op error descriptor that you can instantiate just to reserve said ID.</para>
/// </summary>
public sealed class UnusedErrorDescriptor
{
    public UnusedErrorDescriptor(ErrorDescriptorGroup group)
    {
        group.NextId();
    }
}

internal static class ErrorDescriptor
{
    private static readonly ErrorDescriptorGroup group = new("BSATN", "SpacetimeDB.BSATN");

    public static readonly ErrorDescriptor<(
        ISymbol member,
        ITypeSymbol type,
        Exception e
    )> UnsupportedType =
        new(
            group,
            "Unsupported type",
            ctx => $"BSATN implementation for {ctx.type} is not found: {ctx.e.Message}",
            ctx => ctx.member
        );

    public static readonly ErrorDescriptor<(
        EqualsValueClauseSyntax equalsValue,
        EnumMemberDeclarationSyntax enumMember,
        EnumDeclarationSyntax @enum
    )> EnumWithExplicitValues =
        new(
            group,
            "[SpacetimeDB.Type] enums cannot have explicit values",
            ctx =>
                $"{ctx.@enum.Identifier}.{ctx.enumMember.Identifier} has an explicit value {ctx.equalsValue.Value} which is not allowed in SpacetimeDB enums.",
            ctx => ctx.equalsValue
        );

    public static readonly ErrorDescriptor<EnumDeclarationSyntax> EnumTooManyVariants =
        new(
            group,
            "[SpacetimeDB.Type] enums are limited to 256 variants",
            @enum =>
                $"{@enum.Identifier} has {@enum.Members.Count} variants which is more than the allowed 256 variants for SpacetimeDB enums.",
            @enum => @enum.Members[256]
        );

    public static readonly ErrorDescriptor<INamedTypeSymbol> TaggedEnumInlineTuple =
        new(
            group,
            "Tagged enum variants must be declared with inline tuples",
            baseType =>
                $"{baseType} does not have the expected format SpacetimeDB.TaggedEnum<(TVariant1 v1, ..., TVariantN vN)>.",
            baseType => baseType
        );

    public static readonly ErrorDescriptor<IFieldSymbol> TaggedEnumField =
        new(
            group,
            "Tagged enums cannot have instance fields",
            field =>
                $"{field.Name} is an instance field, which are not permitted inside SpacetimeDB tagged enums.",
            field => field
        );

    public static readonly ErrorDescriptor<TypeParameterListSyntax> TypeParams =
        new(
            group,
            "Type parameters are not yet supported",
            typeParams => $"Type parameters {typeParams} are not supported in SpacetimeDB types.",
            typeParams => typeParams
        );
}

// This class is used to collect diagnostics during parsing and return them as a combined result.
//
// It's necessary because Roslyn doesn't let incremental generators emit diagnostics while parsing,
// only while emitting - which means we need to collect them during parsing and store in some data
// structure (this one) and report them later, when we have the emitter context.
//
// Diagnostics are not kept inside the parsed data structure to make the main data cacheable even
// when the diagnostic locations and details change between reruns.
public record ParseResult<T>(T? Parsed, EquatableArray<Diagnostic> Diag)
    where T : IEquatable<T>;

public class DiagReporter
{
    private readonly ImmutableArray<Diagnostic>.Builder builder =
        ImmutableArray.CreateBuilder<Diagnostic>();

    public void Report<TContext>(ErrorDescriptor<TContext> descriptor, TContext ctx)
    {
        builder.Add(descriptor.ToDiag(ctx));
    }

    private DiagReporter() { }

    private static readonly ErrorDescriptor<(Location location, Exception e)> InternalError =
        new(
            new("STDBINT", "SpacetimeDB.Internal"),
            "Internal SpacetimeDB codegen error",
            ctx => $"An internal error occurred during codegen: {ctx.e.Message}",
            ctx => ctx.location
        );

    public static ParseResult<T> With<T>(Location location, Func<DiagReporter, T> build)
        where T : IEquatable<T>
    {
        var reporter = new DiagReporter();
        T? parsed;
        try
        {
            parsed = build(reporter);
        }
        // Catch any unexpected exceptions not covered by proper diagnostics.
        // This is the last resort to prevent the generator from crashing and being skipped altogether.
        // Instead, it will limit the damage to skipping one particular syntax node.
        catch (Exception e)
        {
            reporter.Report(InternalError, (location, e));
            parsed = default;
        }
        return new(parsed, new(reporter.builder.ToImmutable()));
    }

    public static ParseResult<T> With<T>(SyntaxNode node, Func<DiagReporter, T> build)
        where T : IEquatable<T>
    {
        return With(node.GetLocation(), build);
    }
}

public static class DiagExtensions
{
    public static ParseResult<T> ParseWithDiags<T>(
        this GeneratorAttributeSyntaxContext context,
        Func<DiagReporter, T> build
    )
        where T : IEquatable<T>
    {
        return DiagReporter.With(context.TargetNode, build);
    }

    public static IncrementalValuesProvider<T> ReportDiagnostics<T>(
        this IncrementalValuesProvider<ParseResult<T>> diagnosticHolders,
        IncrementalGeneratorInitializationContext context
    )
        where T : IEquatable<T>
    {
        context.RegisterSourceOutput(
            diagnosticHolders.SelectMany((result, ct) => result.Diag),
            (context, diag) => context.ReportDiagnostic(diag)
        );
        return diagnosticHolders
            .Select((result, ct) => result.Parsed!)
            .Where(parsed => parsed is not null);
    }
}
