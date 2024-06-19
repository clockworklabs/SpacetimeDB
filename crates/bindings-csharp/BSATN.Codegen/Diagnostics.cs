using System.Linq.Expressions;
using System.Reflection;
using System.Runtime.CompilerServices;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using SpacetimeDB.Codegen.Utils;

public record Diagnostics : EquatableCollection<Diagnostic, List<Diagnostic>>
{
    private void Report(Diagnostic diag) => Collection.Add(diag);

    public void Emit(SourceProductionContext context)
    {
        foreach (var diag in this)
        {
            context.ReportDiagnostic(diag);
        }
    }

    // Note: all diagnostics must be defined in the same place to ensure unique IDs.
    // If we spread them out across the codebase, we could end up accidentally reusing IDs or shifting them around.
    public static readonly ErrorDescriptor<(
        EqualsValueClauseSyntax equalsValue,
        EnumMemberDeclarationSyntax enumMember,
        EnumDeclarationSyntax @enum
    )> EnumWithExplicitValues =
        new(
            1,
            "[SpacetimeDB.Type] enums cannot have explicit values",
            ctx => ctx.equalsValue,
            ctx =>
                $"{ctx.@enum.Identifier}.{ctx.enumMember.Identifier} has an explicit value {ctx.equalsValue.Value} which is not allowed in SpacetimeDB enums."
        );

    public static readonly ErrorDescriptor<EnumDeclarationSyntax> EnumTooManyVariants =
        new(
            2,
            "[SpacetimeDB.Type] enums are limited to 256 variants",
            @enum => @enum,
            @enum =>
                $"{@enum.Identifier} has {@enum.Members.Count} variants which is more than the allowed 256 variants for SpacetimeDB enums."
        );

    public static readonly ErrorDescriptor<BaseTypeSyntax> TaggedEnumInlineTuple =
        new(
            3,
            "Tagged enum variants must be declared with inline tuples",
            baseType => baseType,
            baseType =>
                $"{baseType} does not have the expected format SpacetimeDB.TaggedEnum<(TVariant1 v1, ..., TVariantN vN)>."
        );

    public static readonly ErrorDescriptor<IFieldSymbol> TaggedEnumField =
        new(
            4,
            "Tagged enums cannot have instance fields",
            field => field.DeclaringSyntaxReferences[0].GetSyntax(),
            field =>
                $"{field.ContainingType.Name}.{field.Name} is an instance field, which are not permitted inside SpacetimeDB tagged enums."
        );

    public static readonly ErrorDescriptor<TypeParameterListSyntax> TypeParams =
        new(
            5,
            "Type parameters are not yet supported",
            typeParams => typeParams,
            typeParams => $"Type parameters {typeParams} are not supported in SpacetimeDB types."
        );

    public static readonly ErrorDescriptor<MethodDeclarationSyntax> ReducerReturnType =
        new(
            6,
            "[SpacetimeDB.Reducer] methods must return void",
            method => method.ReturnType,
            method =>
                $"Reducer method {method.Identifier} returns {method.ReturnType} instead of void."
        );

    public class ErrorDescriptor<TContext>
    {
        private readonly DiagnosticDescriptor descriptor;
        private readonly Func<TContext, SyntaxNode> toSource;
        private readonly Func<TContext, object[]> makeFormatArgs;

        internal ErrorDescriptor(
            int id,
            string title,
            Func<TContext, SyntaxNode> toSource,
            Expression<Func<TContext, FormattableString>> interpolate
        )
        {
            this.toSource = toSource;
            if (
                interpolate.Body
                    is not MethodCallExpression
                    {
                        Method: { DeclaringType: var declaringType, Name: nameof(FormattableStringFactory.Create) },
                        Arguments: [
                            ConstantExpression { Value: string messageFormat },
                            NewArrayExpression args
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
                id: $"STDB{id:D4}",
                title: title,
                messageFormat: messageFormat,
                category: "SpacetimeDB",
                defaultSeverity: DiagnosticSeverity.Error,
                isEnabledByDefault: true
            );
            makeFormatArgs = Expression
                .Lambda<Func<TContext, object[]>>(args, interpolate.Parameters)
                .Compile();
        }

        public void Report(Diagnostics diag, TContext ctx)
        {
            diag.Report(
                Diagnostic.Create(descriptor, toSource(ctx).GetLocation(), makeFormatArgs(ctx))
            );
        }
    }
}
