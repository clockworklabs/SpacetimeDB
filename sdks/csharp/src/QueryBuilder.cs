#nullable enable

namespace SpacetimeDB
{
    using System;
    using System.Globalization;

    /// <summary>
    /// A pre-formatted SQL literal for the typed query builder.
    /// This wrapper exists so callers cannot accidentally generate unsupported subscription SQL,
    /// such as NULL literals or NULL-specific predicates.
    /// </summary>
    public readonly struct SqlLiteral<T>
    {
        internal string Sql { get; }

        internal SqlLiteral(string sql)
        {
            Sql = sql;
        }

        public override string ToString() => Sql;
    }

    public readonly struct JoinRow<TLeftRow, TRightRow> { }

    /// <summary>
    /// Factory methods for producing <see cref="SqlLiteral{T}"/> values.
    /// Centralizing literal formatting keeps the typed query builder safe and consistent
    /// with the supported subscription SQL subset.
    /// </summary>
    public static class SqlLit
    {
        public static SqlLiteral<string> String(ReadOnlySpan<char> value) => new(SqlFormat.FormatStringLiteral(value));

        public static SqlLiteral<bool> Bool(bool value) => new(value ? "TRUE" : "FALSE");

        public static SqlLiteral<sbyte> Int(sbyte value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<byte> Int(byte value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<short> Int(short value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<ushort> Int(ushort value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<int> Int(int value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<uint> Int(uint value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<long> Int(long value) => new(value.ToString(CultureInfo.InvariantCulture));
        public static SqlLiteral<ulong> Int(ulong value) => new(value.ToString(CultureInfo.InvariantCulture));

        public static SqlLiteral<U128> Int(U128 value) => new(value.ToString());

        public static SqlLiteral<Identity> Identity(Identity value) => new(SqlFormat.FormatHexLiteral(value.ToString()));
        public static SqlLiteral<ConnectionId> ConnectionId(ConnectionId value) => new(SqlFormat.FormatHexLiteral(value.ToString()));
        public static SqlLiteral<Uuid> Uuid(Uuid value) => new(SqlFormat.FormatHexLiteral(value.ToString()));
    }

    public readonly struct Query<TRow>
    {
        public string Sql { get; }

        public Query(string sql)
        {
            Sql = sql;
        }

        public string ToSql() => Sql;

        public override string ToString() => Sql;
    }

    public readonly struct BoolExpr<TRow>
    {
        public string Sql { get; }

        public BoolExpr(string sql)
        {
            Sql = sql;
        }

        public BoolExpr<TRow> And(BoolExpr<TRow> other) => new($"({Sql}) AND ({other.Sql})");
        public BoolExpr<TRow> Or(BoolExpr<TRow> other) => new($"(({Sql}) OR ({other.Sql}))");

        public override string ToString() => Sql;
    }

    public readonly struct IxJoinEq<TLeftRow, TRightRow>
    {
        internal string LeftRefSql { get; }
        internal string RightRefSql { get; }

        internal IxJoinEq(string leftRefSql, string rightRefSql)
        {
            LeftRefSql = leftRefSql;
            RightRefSql = rightRefSql;
        }
    }

    public readonly struct Col<TRow, TValue>
        where TValue : notnull
    {
        private readonly string tableName;
        private readonly string columnName;

        public Col(string tableName, string columnName)
        {
            this.tableName = tableName;
            this.columnName = columnName;
        }

        internal string RefSql => $"{SqlFormat.QuoteIdent(tableName)}.{SqlFormat.QuoteIdent(columnName)}";

        public BoolExpr<TRow> Eq(SqlLiteral<TValue> value) => new($"{RefSql} = {value.Sql}");

        public BoolExpr<TRow> Eq(Col<TRow, TValue> other) => new($"{RefSql} = {other.RefSql}");

        public BoolExpr<TRow> Neq(SqlLiteral<TValue> value) => new($"{RefSql} <> {value.Sql}");

        public BoolExpr<TRow> Neq(Col<TRow, TValue> other) => new($"{RefSql} <> {other.RefSql}");

        public BoolExpr<TRow> Lt(SqlLiteral<TValue> value) => new($"{RefSql} < {value.Sql}");
        public BoolExpr<TRow> Lte(SqlLiteral<TValue> value) => new($"{RefSql} <= {value.Sql}");
        public BoolExpr<TRow> Gt(SqlLiteral<TValue> value) => new($"{RefSql} > {value.Sql}");
        public BoolExpr<TRow> Gte(SqlLiteral<TValue> value) => new($"{RefSql} >= {value.Sql}");

        public BoolExpr<TRow> Lt(NullableCol<TRow, TValue> other) => new($"{RefSql} < {other.RefSql}");
        public BoolExpr<TRow> Lte(NullableCol<TRow, TValue> other) => new($"{RefSql} <= {other.RefSql}");
        public BoolExpr<TRow> Gt(NullableCol<TRow, TValue> other) => new($"{RefSql} > {other.RefSql}");
        public BoolExpr<TRow> Gte(NullableCol<TRow, TValue> other) => new($"{RefSql} >= {other.RefSql}");

        public BoolExpr<TRow> Lt(Col<TRow, TValue> other) => new($"{RefSql} < {other.RefSql}");
        public BoolExpr<TRow> Lte(Col<TRow, TValue> other) => new($"{RefSql} <= {other.RefSql}");
        public BoolExpr<TRow> Gt(Col<TRow, TValue> other) => new($"{RefSql} > {other.RefSql}");
        public BoolExpr<TRow> Gte(Col<TRow, TValue> other) => new($"{RefSql} >= {other.RefSql}");

        public override string ToString() => RefSql;
    }

    public readonly struct NullableCol<TRow, TValue>
        where TValue : notnull
    {
        private readonly string tableName;
        private readonly string columnName;

        public NullableCol(string tableName, string columnName)
        {
            this.tableName = tableName;
            this.columnName = columnName;
        }

        internal string RefSql => $"{SqlFormat.QuoteIdent(tableName)}.{SqlFormat.QuoteIdent(columnName)}";

        public BoolExpr<TRow> Eq(SqlLiteral<TValue> value) => new($"{RefSql} = {value.Sql}");

        public BoolExpr<TRow> Eq(NullableCol<TRow, TValue> other) => new($"{RefSql} = {other.RefSql}");

        public BoolExpr<TRow> Neq(SqlLiteral<TValue> value) => new($"{RefSql} <> {value.Sql}");

        public BoolExpr<TRow> Neq(NullableCol<TRow, TValue> other) => new($"{RefSql} <> {other.RefSql}");

        public BoolExpr<TRow> Lt(SqlLiteral<TValue> value) => new($"{RefSql} < {value.Sql}");
        public BoolExpr<TRow> Lte(SqlLiteral<TValue> value) => new($"{RefSql} <= {value.Sql}");
        public BoolExpr<TRow> Gt(SqlLiteral<TValue> value) => new($"{RefSql} > {value.Sql}");
        public BoolExpr<TRow> Gte(SqlLiteral<TValue> value) => new($"{RefSql} >= {value.Sql}");

        public BoolExpr<TRow> Lt(NullableCol<TRow, TValue> other) => new($"{RefSql} < {other.RefSql}");
        public BoolExpr<TRow> Lte(NullableCol<TRow, TValue> other) => new($"{RefSql} <= {other.RefSql}");
        public BoolExpr<TRow> Gt(NullableCol<TRow, TValue> other) => new($"{RefSql} > {other.RefSql}");
        public BoolExpr<TRow> Gte(NullableCol<TRow, TValue> other) => new($"{RefSql} >= {other.RefSql}");

        public override string ToString() => RefSql;
    }

    public readonly struct IxCol<TRow, TValue>
        where TValue : notnull
    {
        private readonly string tableName;
        private readonly string columnName;

        public IxCol(string tableName, string columnName)
        {
            this.tableName = tableName;
            this.columnName = columnName;
        }

        internal string RefSql => $"{SqlFormat.QuoteIdent(tableName)}.{SqlFormat.QuoteIdent(columnName)}";

        public BoolExpr<TRow> Eq(SqlLiteral<TValue> value) => new($"{RefSql} = {value.Sql}");

        public IxJoinEq<TRow, TOtherRow> Eq<TOtherRow>(IxCol<TOtherRow, TValue> other) =>
            new(RefSql, other.RefSql);

        public BoolExpr<TRow> Neq(SqlLiteral<TValue> value) => new($"{RefSql} <> {value.Sql}");

        public override string ToString() => RefSql;
    }

    public readonly struct NullableIxCol<TRow, TValue>
        where TValue : notnull
    {
        private readonly string tableName;
        private readonly string columnName;

        public NullableIxCol(string tableName, string columnName)
        {
            this.tableName = tableName;
            this.columnName = columnName;
        }

        internal string RefSql => $"{SqlFormat.QuoteIdent(tableName)}.{SqlFormat.QuoteIdent(columnName)}";

        public BoolExpr<TRow> Eq(SqlLiteral<TValue> value) => new($"{RefSql} = {value.Sql}");

        public IxJoinEq<TRow, TOtherRow> Eq<TOtherRow>(NullableIxCol<TOtherRow, TValue> other) =>
            new(RefSql, other.RefSql);

        public BoolExpr<TRow> Neq(SqlLiteral<TValue> value) => new($"{RefSql} <> {value.Sql}");

        public override string ToString() => RefSql;
    }

    public sealed class Table<TRow, TCols, TIxCols>
    {
        private readonly string tableName;

        private readonly TCols cols;
        private readonly TIxCols ixCols;

        public Table(string tableName, TCols cols, TIxCols ixCols)
        {
            this.tableName = tableName;
            this.cols = cols;
            this.ixCols = ixCols;
        }

        internal string TableRefSql => SqlFormat.QuoteIdent(tableName);

        internal TCols Cols => cols;

        internal TIxCols IxCols => ixCols;

        public string ToSql() => $"SELECT * FROM {SqlFormat.QuoteIdent(tableName)}";

        public Query<TRow> Build() => new(ToSql());

        public FromWhere<TRow, TCols, TIxCols> Where(Func<TCols, BoolExpr<TRow>> predicate) =>
            new(this, predicate(cols));

        public FromWhere<TRow, TCols, TIxCols> Where(Func<TCols, TIxCols, BoolExpr<TRow>> predicate) =>
            new(this, predicate(cols, ixCols));

        public FromWhere<TRow, TCols, TIxCols> Filter(Func<TCols, BoolExpr<TRow>> predicate) => Where(predicate);

        public FromWhere<TRow, TCols, TIxCols> Filter(Func<TCols, TIxCols, BoolExpr<TRow>> predicate) => Where(predicate);

        public LeftSemiJoin<TRow, TCols, TIxCols, TRightRow, TRightCols, TRightIxCols> LeftSemijoin<
            TRightRow,
            TRightCols,
            TRightIxCols
        >(
            Table<TRightRow, TRightCols, TRightIxCols> right,
            Func<TIxCols, TRightIxCols, IxJoinEq<TRow, TRightRow>> on
        )
            => new(this, right, on(ixCols, right.ixCols), whereExpr: null);

        public RightSemiJoin<TRow, TCols, TIxCols, TRightRow, TRightCols, TRightIxCols> RightSemijoin<
            TRightRow,
            TRightCols,
            TRightIxCols
        >(
            Table<TRightRow, TRightCols, TRightIxCols> right,
            Func<TIxCols, TRightIxCols, IxJoinEq<TRow, TRightRow>> on
        )
            => new(this, right, on(ixCols, right.ixCols), leftWhereExpr: null);

        public Join<TRow, TCols, TIxCols, TRightRow, TRightCols, TRightIxCols> Join<TRightRow, TRightCols, TRightIxCols>(
            Table<TRightRow, TRightCols, TRightIxCols> right,
            Func<TIxCols, TRightIxCols, IxJoinEq<TRow, TRightRow>> on
        )
            => new(this, right, on(ixCols, right.ixCols), leftWhereExpr: null, rightWhereExpr: null);
    }

    public sealed class FromWhere<TRow, TCols, TIxCols>
    {
        private readonly Table<TRow, TCols, TIxCols> table;
        private readonly BoolExpr<TRow> expr;

        internal FromWhere(Table<TRow, TCols, TIxCols> table, BoolExpr<TRow> expr)
        {
            this.table = table;
            this.expr = expr;
        }

        public FromWhere<TRow, TCols, TIxCols> Where(Func<TCols, BoolExpr<TRow>> predicate) =>
            new(table, expr.And(predicate(table.Cols)));

        public FromWhere<TRow, TCols, TIxCols> Where(Func<TCols, TIxCols, BoolExpr<TRow>> predicate) =>
            new(table, expr.And(predicate(table.Cols, table.IxCols)));

        public FromWhere<TRow, TCols, TIxCols> Filter(Func<TCols, BoolExpr<TRow>> predicate) => Where(predicate);

        public FromWhere<TRow, TCols, TIxCols> Filter(Func<TCols, TIxCols, BoolExpr<TRow>> predicate) => Where(predicate);

        public Query<TRow> Build() => new($"{table.ToSql()} WHERE {expr.Sql}");

        public LeftSemiJoin<TRow, TCols, TIxCols, TRightRow, TRightCols, TRightIxCols> LeftSemijoin<
            TRightRow,
            TRightCols,
            TRightIxCols
        >(
            Table<TRightRow, TRightCols, TRightIxCols> right,
            Func<TIxCols, TRightIxCols, IxJoinEq<TRow, TRightRow>> on
        )
            => new(table, right, on(table.IxCols, right.IxCols), expr);

        public RightSemiJoin<TRow, TCols, TIxCols, TRightRow, TRightCols, TRightIxCols> RightSemijoin<
            TRightRow,
            TRightCols,
            TRightIxCols
        >(
            Table<TRightRow, TRightCols, TRightIxCols> right,
            Func<TIxCols, TRightIxCols, IxJoinEq<TRow, TRightRow>> on
        )
            => new(table, right, on(table.IxCols, right.IxCols), expr);

        public Join<TRow, TCols, TIxCols, TRightRow, TRightCols, TRightIxCols> Join<TRightRow, TRightCols, TRightIxCols>(
            Table<TRightRow, TRightCols, TRightIxCols> right,
            Func<TIxCols, TRightIxCols, IxJoinEq<TRow, TRightRow>> on
        )
            => new(table, right, on(table.IxCols, right.IxCols), leftWhereExpr: expr, rightWhereExpr: null);
    }

    public sealed class Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>
    {
        private readonly Table<TLeftRow, TLeftCols, TLeftIxCols> left;
        private readonly Table<TRightRow, TRightCols, TRightIxCols> right;
        private readonly string leftJoinRefSql;
        private readonly string rightJoinRefSql;
        private readonly BoolExpr<TLeftRow>? leftWhereExpr;
        private readonly BoolExpr<TRightRow>? rightWhereExpr;

        internal Join(
            Table<TLeftRow, TLeftCols, TLeftIxCols> left,
            Table<TRightRow, TRightCols, TRightIxCols> right,
            IxJoinEq<TLeftRow, TRightRow> join,
            BoolExpr<TLeftRow>? leftWhereExpr,
            BoolExpr<TRightRow>? rightWhereExpr
        )
        {
            this.left = left;
            this.right = right;
            leftJoinRefSql = join.LeftRefSql;
            rightJoinRefSql = join.RightRefSql;
            this.leftWhereExpr = leftWhereExpr;
            this.rightWhereExpr = rightWhereExpr;
        }

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> WhereLeft(
            Func<TLeftCols, BoolExpr<TLeftRow>> predicate
        )
        {
            var extra = predicate(left.Cols);
            BoolExpr<TLeftRow>? nextLeft = leftWhereExpr.HasValue ? leftWhereExpr.Value.And(extra) : extra;
            return new(
                left,
                right,
                new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql),
                nextLeft,
                rightWhereExpr
            );
        }

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> WhereLeft(
            Func<TLeftCols, TLeftIxCols, BoolExpr<TLeftRow>> predicate
        )
        {
            var extra = predicate(left.Cols, left.IxCols);
            BoolExpr<TLeftRow>? nextLeft = leftWhereExpr.HasValue ? leftWhereExpr.Value.And(extra) : extra;
            return new(
                left,
                right,
                new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql),
                nextLeft,
                rightWhereExpr
            );
        }

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> FilterLeft(
            Func<TLeftCols, BoolExpr<TLeftRow>> predicate
        ) => WhereLeft(predicate);

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> FilterLeft(
            Func<TLeftCols, TLeftIxCols, BoolExpr<TLeftRow>> predicate
        ) => WhereLeft(predicate);

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> WhereRight(
            Func<TRightCols, BoolExpr<TRightRow>> predicate
        )
        {
            var extra = predicate(right.Cols);
            BoolExpr<TRightRow>? nextRight = rightWhereExpr.HasValue ? rightWhereExpr.Value.And(extra) : extra;
            return new(
                left,
                right,
                new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql),
                leftWhereExpr,
                nextRight
            );
        }

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> WhereRight(
            Func<TRightCols, TRightIxCols, BoolExpr<TRightRow>> predicate
        )
        {
            var extra = predicate(right.Cols, right.IxCols);
            BoolExpr<TRightRow>? nextRight = rightWhereExpr.HasValue ? rightWhereExpr.Value.And(extra) : extra;
            return new(
                left,
                right,
                new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql),
                leftWhereExpr,
                nextRight
            );
        }

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> FilterRight(
            Func<TRightCols, BoolExpr<TRightRow>> predicate
        ) => WhereRight(predicate);

        public Join<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> FilterRight(
            Func<TRightCols, TRightIxCols, BoolExpr<TRightRow>> predicate
        ) => WhereRight(predicate);

        public Query<JoinRow<TLeftRow, TRightRow>> Build()
        {
            var whereClause = string.Empty;

            if (leftWhereExpr.HasValue && rightWhereExpr.HasValue)
            {
                whereClause = $" WHERE {leftWhereExpr.Value.Sql} AND {rightWhereExpr.Value.Sql}";
            }
            else if (leftWhereExpr.HasValue)
            {
                whereClause = $" WHERE {leftWhereExpr.Value.Sql}";
            }
            else if (rightWhereExpr.HasValue)
            {
                whereClause = $" WHERE {rightWhereExpr.Value.Sql}";
            }

            return new(
                $"SELECT * FROM {left.TableRefSql} JOIN {right.TableRefSql} ON {leftJoinRefSql} = {rightJoinRefSql}{whereClause}"
            );
        }
    }

    public sealed class LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>
    {
        private readonly Table<TLeftRow, TLeftCols, TLeftIxCols> left;
        private readonly Table<TRightRow, TRightCols, TRightIxCols> right;
        private readonly string leftJoinRefSql;
        private readonly string rightJoinRefSql;
        private readonly BoolExpr<TLeftRow>? whereExpr;

        internal LeftSemiJoin(
            Table<TLeftRow, TLeftCols, TLeftIxCols> left,
            Table<TRightRow, TRightCols, TRightIxCols> right,
            IxJoinEq<TLeftRow, TRightRow> join,
            BoolExpr<TLeftRow>? whereExpr
        )
        {
            this.left = left;
            this.right = right;
            leftJoinRefSql = join.LeftRefSql;
            rightJoinRefSql = join.RightRefSql;
            this.whereExpr = whereExpr;
        }

        public LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Where(
            Func<TLeftCols, BoolExpr<TLeftRow>> predicate
        )
        {
            var extra = predicate(left.Cols);
            BoolExpr<TLeftRow>? next = whereExpr.HasValue ? whereExpr.Value.And(extra) : extra;
            return new(left, right, new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql), next);
        }

        public LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Where(
            Func<TLeftCols, TLeftIxCols, BoolExpr<TLeftRow>> predicate
        )
        {
            var extra = predicate(left.Cols, left.IxCols);
            BoolExpr<TLeftRow>? next = whereExpr.HasValue ? whereExpr.Value.And(extra) : extra;
            return new(left, right, new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql), next);
        }

        public LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Filter(
            Func<TLeftCols, BoolExpr<TLeftRow>> predicate
        ) => Where(predicate);

        public LeftSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Filter(
            Func<TLeftCols, TLeftIxCols, BoolExpr<TLeftRow>> predicate
        ) => Where(predicate);

        public Query<TLeftRow> Build()
        {
            var whereClause = whereExpr.HasValue ? $" WHERE {whereExpr.Value.Sql}" : string.Empty;
            return new(
                $"SELECT {left.TableRefSql}.* FROM {left.TableRefSql} JOIN {right.TableRefSql} ON {leftJoinRefSql} = {rightJoinRefSql}{whereClause}"
            );
        }
    }

    public sealed class RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols>
    {
        private readonly Table<TLeftRow, TLeftCols, TLeftIxCols> left;
        private readonly Table<TRightRow, TRightCols, TRightIxCols> right;
        private readonly string leftJoinRefSql;
        private readonly string rightJoinRefSql;
        private readonly BoolExpr<TLeftRow>? leftWhereExpr;
        private readonly BoolExpr<TRightRow>? rightWhereExpr;

        internal RightSemiJoin(
            Table<TLeftRow, TLeftCols, TLeftIxCols> left,
            Table<TRightRow, TRightCols, TRightIxCols> right,
            IxJoinEq<TLeftRow, TRightRow> join,
            BoolExpr<TLeftRow>? leftWhereExpr,
            BoolExpr<TRightRow>? rightWhereExpr
        )
        {
            this.left = left;
            this.right = right;
            leftJoinRefSql = join.LeftRefSql;
            rightJoinRefSql = join.RightRefSql;
            this.leftWhereExpr = leftWhereExpr;
            this.rightWhereExpr = rightWhereExpr;
        }

        internal RightSemiJoin(
            Table<TLeftRow, TLeftCols, TLeftIxCols> left,
            Table<TRightRow, TRightCols, TRightIxCols> right,
            IxJoinEq<TLeftRow, TRightRow> join,
            BoolExpr<TLeftRow>? leftWhereExpr
        )
            : this(left, right, join, leftWhereExpr, rightWhereExpr: null)
        {
        }

        public RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Where(
            Func<TRightCols, BoolExpr<TRightRow>> predicate
        )
        {
            var extra = predicate(right.Cols);
            BoolExpr<TRightRow>? nextRight = rightWhereExpr.HasValue ? rightWhereExpr.Value.And(extra) : extra;
            return new(
                left,
                right,
                new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql),
                leftWhereExpr,
                nextRight
            );
        }

        public RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Where(
            Func<TRightCols, TRightIxCols, BoolExpr<TRightRow>> predicate
        )
        {
            var extra = predicate(right.Cols, right.IxCols);
            BoolExpr<TRightRow>? nextRight = rightWhereExpr.HasValue ? rightWhereExpr.Value.And(extra) : extra;
            return new(
                left,
                right,
                new IxJoinEq<TLeftRow, TRightRow>(leftJoinRefSql, rightJoinRefSql),
                leftWhereExpr,
                nextRight
            );
        }

        public RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Filter(
            Func<TRightCols, BoolExpr<TRightRow>> predicate
        ) => Where(predicate);

        public RightSemiJoin<TLeftRow, TLeftCols, TLeftIxCols, TRightRow, TRightCols, TRightIxCols> Filter(
            Func<TRightCols, TRightIxCols, BoolExpr<TRightRow>> predicate
        ) => Where(predicate);

        public Query<TRightRow> Build()
        {
            var whereClause = string.Empty;

            if (leftWhereExpr.HasValue && rightWhereExpr.HasValue)
            {
                whereClause = $" WHERE {leftWhereExpr.Value.Sql} AND {rightWhereExpr.Value.Sql}";
            }
            else if (leftWhereExpr.HasValue)
            {
                whereClause = $" WHERE {leftWhereExpr.Value.Sql}";
            }
            else if (rightWhereExpr.HasValue)
            {
                whereClause = $" WHERE {rightWhereExpr.Value.Sql}";
            }

            return new(
                $"SELECT {right.TableRefSql}.* FROM {left.TableRefSql} JOIN {right.TableRefSql} ON {leftJoinRefSql} = {rightJoinRefSql}{whereClause}"
            );
        }
    }

    /// <summary>
    /// Ergonomic overloads for comparisons (e.g. <c>col.Eq("x")</c>) that still route through
    /// <see cref="SqlLit"/> and <see cref="SqlLiteral{T}"/>, preventing raw/NULL literals from
    /// being embedded into subscription SQL.
    /// </summary>
    public static class SqlLitExtensions
    {
        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, string> col, ReadOnlySpan<char> value) => col.Eq(SqlLit.String(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, string> col, ReadOnlySpan<char> value) => col.Neq(SqlLit.String(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, string> col, ReadOnlySpan<char> value) => col.Eq(SqlLit.String(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, string> col, ReadOnlySpan<char> value) => col.Neq(SqlLit.String(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, bool> col, bool value) => col.Eq(SqlLit.Bool(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, bool> col, bool value) => col.Neq(SqlLit.Bool(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, bool> col, bool value) => col.Eq(SqlLit.Bool(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, bool> col, bool value) => col.Neq(SqlLit.Bool(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, sbyte> col, sbyte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, sbyte> col, sbyte value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, sbyte> col, sbyte value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, sbyte> col, sbyte value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, sbyte> col, sbyte value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, sbyte> col, sbyte value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, sbyte> col, sbyte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, sbyte> col, sbyte value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, sbyte> col, sbyte value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, sbyte> col, sbyte value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, sbyte> col, sbyte value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, sbyte> col, sbyte value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, byte> col, byte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, byte> col, byte value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, byte> col, byte value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, byte> col, byte value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, byte> col, byte value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, byte> col, byte value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, byte> col, byte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, byte> col, byte value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, byte> col, byte value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, byte> col, byte value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, byte> col, byte value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, byte> col, byte value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, short> col, short value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, short> col, short value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, short> col, short value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, short> col, short value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, short> col, short value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, short> col, short value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, short> col, short value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, short> col, short value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, short> col, short value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, short> col, short value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, short> col, short value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, short> col, short value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, ushort> col, ushort value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, ushort> col, ushort value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, ushort> col, ushort value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, ushort> col, ushort value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, ushort> col, ushort value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, ushort> col, ushort value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, ushort> col, ushort value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, ushort> col, ushort value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, ushort> col, ushort value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, ushort> col, ushort value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, ushort> col, ushort value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, ushort> col, ushort value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, int> col, int value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, int> col, int value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, int> col, int value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, int> col, int value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, int> col, int value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, int> col, int value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, int> col, int value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, int> col, int value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, int> col, int value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, int> col, int value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, int> col, int value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, int> col, int value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, uint> col, uint value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, uint> col, uint value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, uint> col, uint value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, uint> col, uint value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, uint> col, uint value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, uint> col, uint value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, uint> col, uint value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, uint> col, uint value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, uint> col, uint value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, uint> col, uint value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, uint> col, uint value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, uint> col, uint value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, long> col, long value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, long> col, long value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, long> col, long value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, long> col, long value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, long> col, long value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, long> col, long value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, long> col, long value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, long> col, long value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, long> col, long value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, long> col, long value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, long> col, long value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, long> col, long value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, ulong> col, ulong value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, ulong> col, ulong value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, ulong> col, ulong value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, ulong> col, ulong value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, ulong> col, ulong value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, ulong> col, ulong value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, ulong> col, ulong value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, ulong> col, ulong value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, ulong> col, ulong value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, ulong> col, ulong value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, ulong> col, ulong value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, ulong> col, ulong value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, U128> col, U128 value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, U128> col, U128 value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this Col<TRow, U128> col, U128 value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this Col<TRow, U128> col, U128 value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this Col<TRow, U128> col, U128 value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this Col<TRow, U128> col, U128 value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, U128> col, U128 value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, U128> col, U128 value) => col.Neq(SqlLit.Int(value));
        public static BoolExpr<TRow> Lt<TRow>(this NullableCol<TRow, U128> col, U128 value) => col.Lt(SqlLit.Int(value));
        public static BoolExpr<TRow> Lte<TRow>(this NullableCol<TRow, U128> col, U128 value) => col.Lte(SqlLit.Int(value));
        public static BoolExpr<TRow> Gt<TRow>(this NullableCol<TRow, U128> col, U128 value) => col.Gt(SqlLit.Int(value));
        public static BoolExpr<TRow> Gte<TRow>(this NullableCol<TRow, U128> col, U128 value) => col.Gte(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, Identity> col, Identity value) => col.Eq(SqlLit.Identity(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, Identity> col, Identity value) => col.Neq(SqlLit.Identity(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, Identity> col, Identity value) => col.Eq(SqlLit.Identity(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, Identity> col, Identity value) => col.Neq(SqlLit.Identity(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, ConnectionId> col, ConnectionId value) => col.Eq(SqlLit.ConnectionId(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, ConnectionId> col, ConnectionId value) => col.Neq(SqlLit.ConnectionId(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, ConnectionId> col, ConnectionId value) => col.Eq(SqlLit.ConnectionId(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, ConnectionId> col, ConnectionId value) => col.Neq(SqlLit.ConnectionId(value));

        public static BoolExpr<TRow> Eq<TRow>(this Col<TRow, Uuid> col, Uuid value) => col.Eq(SqlLit.Uuid(value));
        public static BoolExpr<TRow> Neq<TRow>(this Col<TRow, Uuid> col, Uuid value) => col.Neq(SqlLit.Uuid(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableCol<TRow, Uuid> col, Uuid value) => col.Eq(SqlLit.Uuid(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableCol<TRow, Uuid> col, Uuid value) => col.Neq(SqlLit.Uuid(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, string> col, ReadOnlySpan<char> value) => col.Eq(SqlLit.String(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, string> col, ReadOnlySpan<char> value) => col.Neq(SqlLit.String(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, string> col, ReadOnlySpan<char> value) => col.Eq(SqlLit.String(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, string> col, ReadOnlySpan<char> value) => col.Neq(SqlLit.String(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, bool> col, bool value) => col.Eq(SqlLit.Bool(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, bool> col, bool value) => col.Neq(SqlLit.Bool(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, bool> col, bool value) => col.Eq(SqlLit.Bool(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, bool> col, bool value) => col.Neq(SqlLit.Bool(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, sbyte> col, sbyte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, sbyte> col, sbyte value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, sbyte> col, sbyte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, sbyte> col, sbyte value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, byte> col, byte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, byte> col, byte value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, byte> col, byte value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, byte> col, byte value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, short> col, short value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, short> col, short value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, short> col, short value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, short> col, short value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, ushort> col, ushort value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, ushort> col, ushort value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, ushort> col, ushort value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, ushort> col, ushort value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, int> col, int value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, int> col, int value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, int> col, int value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, int> col, int value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, uint> col, uint value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, uint> col, uint value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, uint> col, uint value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, uint> col, uint value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, long> col, long value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, long> col, long value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, long> col, long value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, long> col, long value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, ulong> col, ulong value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, ulong> col, ulong value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, ulong> col, ulong value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, ulong> col, ulong value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, U128> col, U128 value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, U128> col, U128 value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, U128> col, U128 value) => col.Eq(SqlLit.Int(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, U128> col, U128 value) => col.Neq(SqlLit.Int(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, Identity> col, Identity value) => col.Eq(SqlLit.Identity(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, Identity> col, Identity value) => col.Neq(SqlLit.Identity(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, Identity> col, Identity value) => col.Eq(SqlLit.Identity(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, Identity> col, Identity value) => col.Neq(SqlLit.Identity(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, ConnectionId> col, ConnectionId value) => col.Eq(SqlLit.ConnectionId(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, ConnectionId> col, ConnectionId value) => col.Neq(SqlLit.ConnectionId(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, ConnectionId> col, ConnectionId value) => col.Eq(SqlLit.ConnectionId(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, ConnectionId> col, ConnectionId value) => col.Neq(SqlLit.ConnectionId(value));

        public static BoolExpr<TRow> Eq<TRow>(this IxCol<TRow, Uuid> col, Uuid value) => col.Eq(SqlLit.Uuid(value));
        public static BoolExpr<TRow> Neq<TRow>(this IxCol<TRow, Uuid> col, Uuid value) => col.Neq(SqlLit.Uuid(value));

        public static BoolExpr<TRow> Eq<TRow>(this NullableIxCol<TRow, Uuid> col, Uuid value) => col.Eq(SqlLit.Uuid(value));
        public static BoolExpr<TRow> Neq<TRow>(this NullableIxCol<TRow, Uuid> col, Uuid value) => col.Neq(SqlLit.Uuid(value));
    }

    internal static class SqlFormat
    {
        public static string QuoteIdent(string ident)
        {
            ident ??= string.Empty;
            return $"\"{ident.Replace("\"", "\"\"")}\"";
        }

        private static string EscapeString(string s) => s.Replace("'", "''");

        public static string FormatStringLiteral(ReadOnlySpan<char> value) => $"'{EscapeString(value.ToString())}'";

        public static string FormatHexLiteral(string hex)
        {
            if (hex is null)
            {
                throw new ArgumentNullException(nameof(hex));
            }

            var s = hex;
            if (s.StartsWith("0x", StringComparison.OrdinalIgnoreCase))
            {
                s = s.Substring(2);
            }

            s = s.Replace("-", string.Empty);

            for (var i = 0; i < s.Length; i++)
            {
                var c = s[i];
                var isHex = c is >= '0' and <= '9' or >= 'a' and <= 'f' or >= 'A' and <= 'F';
                if (!isHex)
                {
                    throw new ArgumentOutOfRangeException(nameof(hex), $"Invalid hex character '{c}'.");
                }
            }

            return $"0x{s}";
        }
    }
}

#nullable disable
