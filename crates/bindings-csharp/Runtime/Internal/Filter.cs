namespace SpacetimeDB.Internal;

using System.Linq.Expressions;
using System.Reflection;
using SpacetimeDB.BSATN;

public partial class Filter(KeyValuePair<string, Action<BinaryWriter, object?>>[] fieldTypeInfos)
{
    readonly record struct ErasedValue(Action<BinaryWriter> Write)
    {
        public readonly struct BSATN : IReadWrite<ErasedValue>
        {
            public ErasedValue Read(BinaryReader reader) => throw new NotSupportedException();

            public void Write(BinaryWriter writer, ErasedValue value) => value.Write(writer);
        }
    }

    [SpacetimeDB.Type]
    partial record Rhs : SpacetimeDB.TaggedEnum<(ErasedValue Value, ushort Field)>;

    [SpacetimeDB.Type]
    partial struct CmpArgs(ushort lhsField, Rhs rhs)
    {
        public ushort LhsField = lhsField;
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

    public byte[] Compile<T>(Expression<Func<T, bool>> query)
    {
        var expr = HandleExpr(query.Body);
        return IStructuralReadWrite.ToBytes(new Expr.BSATN(), expr);
    }

    static FieldInfo ExprAsTableField(Expression expr) =>
        expr switch
        {
            // LINQ inserts spurious conversions in comparisons, so we need to unwrap them
            UnaryExpression { NodeType: ExpressionType.Convert, Operand: var arg } =>
                ExprAsTableField(arg),
            MemberExpression { Expression: ParameterExpression, Member: FieldInfo field } => field,
            _ => throw new NotSupportedException(
                "expected table field access in the left-hand side of a comparison"
            ),
        };

    static object? ExprAsRhs(Expression expr) =>
        expr switch
        {
            ConstantExpression { Value: var value } => value,
            _ => Expression.Lambda(expr).Compile().DynamicInvoke(),
        };

    Cmp HandleCmp(BinaryExpression expr)
    {
        var field = ExprAsTableField(expr.Left);
        var lhsFieldIndex = (ushort)Array.FindIndex(fieldTypeInfos, x => x.Key == field.Name);

        var rhs = ExprAsRhs(expr.Right);
        rhs = Convert.ChangeType(rhs, field.FieldType);
        var erasedRhs = new ErasedValue(
            (writer) => fieldTypeInfos[lhsFieldIndex].Value(writer, rhs)
        );

        var args = new CmpArgs(lhsFieldIndex, new Rhs.Value(erasedRhs));

        var op = expr.NodeType switch
        {
            ExpressionType.Equal => OpCmp.Eq,
            ExpressionType.NotEqual => OpCmp.NotEq,
            ExpressionType.LessThan => OpCmp.Lt,
            ExpressionType.LessThanOrEqual => OpCmp.LtEq,
            ExpressionType.GreaterThan => OpCmp.Gt,
            ExpressionType.GreaterThanOrEqual => OpCmp.GtEq,
            _ => throw new NotSupportedException("unsupported comparison operation"),
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
            _ => throw new NotSupportedException("unsupported logic operation"),
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
            } => new Expr.Cmp(HandleCmp(expr)),
            BinaryExpression { NodeType: ExpressionType.And or ExpressionType.Or } =>
                new Expr.Logic(HandleLogic(expr)),
            _ => throw new NotSupportedException("unsupported expression"),
        };

    Expr.Unary HandleUnary(UnaryExpression expr)
    {
        var arg = HandleExpr(expr.Operand);

        var op = expr.NodeType switch
        {
            ExpressionType.Not => OpUnary.Not,
            _ => throw new NotSupportedException("unsupported unary operation"),
        };

        return new(new Unary(op, arg));
    }

    Expr HandleExpr(Expression expr) =>
        expr switch
        {
            BinaryExpression binExpr => HandleBinary(binExpr),
            UnaryExpression unExpr => HandleUnary(unExpr),
            _ => throw new NotSupportedException("unsupported expression"),
        };
}
