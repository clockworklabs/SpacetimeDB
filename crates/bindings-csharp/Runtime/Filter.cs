namespace SpacetimeDB.Filter;

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq.Expressions;
using SpacetimeDB.BSATN;

readonly record struct ErasedValue(Action<BinaryWriter> write)
{
    public readonly struct BSATN : SpacetimeDB.BSATN.IReadWrite<ErasedValue>
    {
        public ErasedValue Read(BinaryReader reader) => throw new NotSupportedException();

        public void Write(BinaryWriter writer, ErasedValue value) => value.write(writer);

        public AlgebraicType GetAlgebraicType(ITypeRegistrar _) =>
            throw new NotSupportedException();
    }
}

[SpacetimeDB.Type]
partial record Rhs : SpacetimeDB.TaggedEnum<(ErasedValue Value, byte Field)>;

[SpacetimeDB.Type]
partial struct CmpArgs(byte lhsField, Rhs rhs)
{
    public byte LhsField = lhsField;
    public Rhs Rhs = rhs;
}

[SpacetimeDB.Type]
enum OpCmp
{
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

[SpacetimeDB.Type]
partial struct Cmp(OpCmp op, CmpArgs args)
{
    public OpCmp op = op;
    public CmpArgs args = args;
}

[SpacetimeDB.Type]
enum OpLogic
{
    And,
    Or,
}

[SpacetimeDB.Type]
partial struct Logic(Expr lhs, OpLogic op, Expr rhs)
{
    public Expr lhs = lhs;

    public OpLogic op = op;
    public Expr rhs = rhs;
}

[SpacetimeDB.Type]
enum OpUnary
{
    Not,
}

[SpacetimeDB.Type]
partial struct Unary(OpUnary op, Expr arg)
{
    public OpUnary op = op;
    public Expr arg = arg;
}

[SpacetimeDB.Type]
partial record Expr : SpacetimeDB.TaggedEnum<(Cmp Cmp, Logic Logic, Unary Unary)>;

public class Filter(Runtime.IQueryField[] fieldTypeInfos)
{
    public static byte[] Compile<T>(
        Runtime.IQueryField[] fieldTypeInfos,
        Expression<Func<T, bool>> rowFilter
    )
    {
        var filter = new Filter(fieldTypeInfos);
        var expr = filter.HandleExpr(rowFilter.Body);
        return IStructuralReadWrite.ToBytes(new Expr.BSATN(), expr);
    }

    (byte, Type) ExprAsTableField(Expression expr) =>
        expr switch
        {
            // LINQ inserts spurious conversions in comparisons, so we need to unwrap them
            UnaryExpression { NodeType: ExpressionType.Convert, Operand: var arg }
                => ExprAsTableField(arg),
            MemberExpression
            {
                Expression: ParameterExpression,
                Member.Name: var memberName,
                Type: var type
            }
                => ((byte)Array.FindIndex(fieldTypeInfos, pair => pair.Name == memberName), type),
            _
                => throw new NotSupportedException(
                    "expected table field access in the left-hand side of a comparison"
                )
        };

    static object? ExprAsRhs(Expression expr) =>
        expr switch
        {
            ConstantExpression { Value: var value } => value,
            _ => Expression.Lambda(expr).Compile().DynamicInvoke()
        };

    Cmp HandleCmp(BinaryExpression expr)
    {
        var (lhsFieldIndex, type) = ExprAsTableField(expr.Left);

        var rhs = ExprAsRhs(expr.Right);
        rhs = Convert.ChangeType(rhs, type);
        var rhsField = fieldTypeInfos[lhsFieldIndex];
        var erasedRhs = new ErasedValue((writer) => rhsField.Write(writer, rhs));

        var args = new CmpArgs(lhsFieldIndex, new Rhs.Value(erasedRhs));

        var op = expr.NodeType switch
        {
            ExpressionType.Equal => OpCmp.Eq,
            ExpressionType.NotEqual => OpCmp.NotEq,
            ExpressionType.LessThan => OpCmp.Lt,
            ExpressionType.LessThanOrEqual => OpCmp.LtEq,
            ExpressionType.GreaterThan => OpCmp.Gt,
            ExpressionType.GreaterThanOrEqual => OpCmp.GtEq,
            _ => throw new NotSupportedException("unsupported comparison operation")
        };

        return new Cmp(op, args);
    }

    Logic HandleLogic(BinaryExpression expr)
    {
        var lhs = HandleExpr(expr.Left);
        var rhs = HandleExpr(expr.Right);

        var op = expr.NodeType switch
        {
            ExpressionType.And => OpLogic.And,
            ExpressionType.Or => OpLogic.Or,
            _ => throw new NotSupportedException("unsupported logic operation")
        };

        return new Logic(lhs, op, rhs);
    }

    Expr HandleBinary(BinaryExpression expr) =>
        expr switch
        {
            BinaryExpression
            {
                NodeType: ExpressionType.Equal
                    or ExpressionType.NotEqual
                    or ExpressionType.LessThan
                    or ExpressionType.LessThanOrEqual
                    or ExpressionType.GreaterThan
                    or ExpressionType.GreaterThanOrEqual
            }
                => new Expr.Cmp(HandleCmp(expr)),
            BinaryExpression { NodeType: ExpressionType.And or ExpressionType.Or }
                => new Expr.Logic(HandleLogic(expr)),
            _ => throw new NotSupportedException("unsupported expression")
        };

    Expr HandleUnary(UnaryExpression expr)
    {
        var arg = HandleExpr(expr.Operand);

        var op = expr.NodeType switch
        {
            ExpressionType.Not => OpUnary.Not,
            _ => throw new NotSupportedException("unsupported unary operation")
        };

        return new Expr.Unary(new Unary(op, arg));
    }

    Expr HandleExpr(Expression expr) =>
        expr switch
        {
            BinaryExpression binExpr => HandleBinary(binExpr),
            UnaryExpression unExpr => HandleUnary(unExpr),
            _ => throw new NotSupportedException("unsupported expression")
        };
}
