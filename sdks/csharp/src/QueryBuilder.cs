using System;
using System.Globalization;

#nullable enable

namespace SpacetimeDB
{
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
        public BoolExpr<TRow> Or(BoolExpr<TRow> other) => new($"({Sql}) OR ({other.Sql})");
        public BoolExpr<TRow> Not() => new($"NOT ({Sql})");

        public override string ToString() => Sql;
    }

    public readonly struct Col<TRow, TValue>
    {
        private readonly string tableName;
        private readonly string columnName;

        public Col(string tableName, string columnName)
        {
            this.tableName = tableName;
            this.columnName = columnName;
        }

        internal string RefSql => $"{SqlFormat.QuoteIdent(tableName)}.{SqlFormat.QuoteIdent(columnName)}";

        public BoolExpr<TRow> Eq(TValue value)
        {
            if (value is null)
            {
                return IsNull();
            }

            return new BoolExpr<TRow>($"{RefSql} = {SqlFormat.FormatLiteral(value)}");
        }

        public BoolExpr<TRow> Neq(TValue value)
        {
            if (value is null)
            {
                return IsNotNull();
            }

            return new BoolExpr<TRow>($"{RefSql} <> {SqlFormat.FormatLiteral(value)}");
        }

        public BoolExpr<TRow> Lt(TValue value) => new($"{RefSql} < {SqlFormat.FormatLiteral(value)}");
        public BoolExpr<TRow> Lte(TValue value) => new($"{RefSql} <= {SqlFormat.FormatLiteral(value)}");
        public BoolExpr<TRow> Gt(TValue value) => new($"{RefSql} > {SqlFormat.FormatLiteral(value)}");
        public BoolExpr<TRow> Gte(TValue value) => new($"{RefSql} >= {SqlFormat.FormatLiteral(value)}");

        public BoolExpr<TRow> IsNull() => new($"{RefSql} IS NULL");
        public BoolExpr<TRow> IsNotNull() => new($"{RefSql} IS NOT NULL");

        public override string ToString() => RefSql;
    }

    public readonly struct IxCol<TRow, TValue>
    {
        private readonly string tableName;
        private readonly string columnName;

        public IxCol(string tableName, string columnName)
        {
            this.tableName = tableName;
            this.columnName = columnName;
        }

        internal string RefSql => $"{SqlFormat.QuoteIdent(tableName)}.{SqlFormat.QuoteIdent(columnName)}";

        public BoolExpr<TRow> Eq(TValue value)
        {
            if (value is null)
            {
                return new BoolExpr<TRow>($"{RefSql} IS NULL");
            }

            return new BoolExpr<TRow>($"{RefSql} = {SqlFormat.FormatLiteral(value)}");
        }

        public BoolExpr<TRow> Neq(TValue value)
        {
            if (value is null)
            {
                return new BoolExpr<TRow>($"{RefSql} IS NOT NULL");
            }

            return new BoolExpr<TRow>($"{RefSql} <> {SqlFormat.FormatLiteral(value)}");
        }

        public override string ToString() => RefSql;
    }

    public sealed class Table<TRow, TCols, TIxCols>
    {
        private readonly string tableName;

        public TCols Cols { get; }
        public TIxCols IxCols { get; }

        public Table(string tableName, TCols cols, TIxCols ixCols)
        {
            this.tableName = tableName;
            Cols = cols;
            IxCols = ixCols;
        }

        public string ToSql() => $"SELECT * FROM {SqlFormat.QuoteIdent(tableName)}";

        public Query<TRow> Build() => new(ToSql());

        public Query<TRow> Where(Func<TCols, BoolExpr<TRow>> predicate) => Where(predicate(Cols));

        public Query<TRow> Where(BoolExpr<TRow> predicate) => new($"{ToSql()} WHERE {predicate.Sql}");
    }

    internal static class SqlFormat
    {
        public static string QuoteIdent(string ident)
        {
            ident ??= string.Empty;
            return $"\"{ident.Replace("\"", "\"\"")}\"";
        }

        private static string EscapeString(string s) => s.Replace("'", "''");

        public static string FormatLiteral(object? value)
        {
            if (value is null)
            {
                return "NULL";
            }

            if (value is string s)
            {
                return $"'{EscapeString(s)}'";
            }

            if (value is bool b)
            {
                return b ? "TRUE" : "FALSE";
            }

            if (value is char c)
            {
                return $"'{EscapeString(c.ToString())}'";
            }

            var t = value.GetType();
            if (t.IsEnum)
            {
                return Convert.ToInt64(value, CultureInfo.InvariantCulture).ToString(CultureInfo.InvariantCulture);
            }

            if (value is IFormattable f)
            {
                return f.ToString(null, CultureInfo.InvariantCulture) ?? "NULL";
            }

            return $"'{EscapeString(value.ToString() ?? string.Empty)}'";
        }
    }
}

#nullable disable
