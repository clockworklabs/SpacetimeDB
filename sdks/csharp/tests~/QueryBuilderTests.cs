namespace SpacetimeDB.Tests;

using System;
using Xunit;

public sealed class QueryBuilderTests
{
    private sealed class Row { }

    private enum Color : long
    {
        Red = 1,
    }

    private sealed class RowCols
    {
        public Col<Row, string> Name { get; }
        public Col<Row, string> Weird { get; }
        public Col<Row, int> Age { get; }
        public Col<Row, bool> IsAdmin { get; }
        public Col<Row, Color> Color { get; }

        public RowCols(string tableName)
        {
            Name = new Col<Row, string>(tableName, "Name");
            Weird = new Col<Row, string>(tableName, "we\"ird");
            Age = new Col<Row, int>(tableName, "Age");
            IsAdmin = new Col<Row, bool>(tableName, "IsAdmin");
            Color = new Col<Row, Color>(tableName, "Color");
        }
    }

    private sealed class RowIxCols
    {
        public IxCol<Row, string> Name { get; }

        public RowIxCols(string tableName)
        {
            Name = new IxCol<Row, string>(tableName, "Name");
        }
    }

    private static Table<Row, RowCols, RowIxCols> MakeTable(string tableName) =>
        new(tableName, new RowCols(tableName), new RowIxCols(tableName));

    [Fact]
    public void All_QuotesTableName()
    {
        var table = MakeTable("My\"Table");
        Assert.Equal("SELECT * FROM \"My\"\"Table\"", table.All().Sql);
    }

    [Fact]
    public void Where_Eq_String_EscapesSingleQuote()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Name.Eq("O'Reilly")).Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 'O''Reilly'", sql);
    }

    [Fact]
    public void Where_Eq_Null_UsesIsNull()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Name.Eq(null!)).Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Name\" IS NULL", sql);
    }

    [Fact]
    public void Where_Neq_Null_UsesIsNotNull()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Name.Neq(null!)).Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Name\" IS NOT NULL", sql);
    }

    [Fact]
    public void Where_Gt_Int_FormatsInvariant()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Age.Gt(123)).Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Age\" > 123", sql);
    }

    [Fact]
    public void Where_Eq_Bool_FormatsAsTrueFalse()
    {
        var table = MakeTable("T");
        Assert.Equal(
            "SELECT * FROM \"T\" WHERE \"T\".\"IsAdmin\" = TRUE",
            table.Where(c => c.IsAdmin.Eq(true)).Sql
        );
        Assert.Equal(
            "SELECT * FROM \"T\" WHERE \"T\".\"IsAdmin\" = FALSE",
            table.Where(c => c.IsAdmin.Eq(false)).Sql
        );
    }

    [Fact]
    public void Where_Eq_Enum_FormatsAsInt64()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Color.Eq(Color.Red)).Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Color\" = 1", sql);
    }

    [Fact]
    public void BoolExpr_AndOrNot_AddsParens()
    {
        var table = MakeTable("T");
        var expr = table.Cols.Age.Gt(1).And(table.Cols.Name.Neq("x")).Or(table.Cols.IsAdmin.Eq(true)).Not();

        Assert.Equal(
            "NOT (((\"T\".\"Age\" > 1) AND (\"T\".\"Name\" <> 'x')) OR (\"T\".\"IsAdmin\" = TRUE))",
            expr.Sql
        );
    }

    [Fact]
    public void QuoteIdent_EscapesDoubleQuotesInColumnName()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Weird.Eq("x")).Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"we\"\"ird\" = 'x'", sql);
    }

    [Fact]
    public void FormatLiteral_SpacetimeDbTypes_AreQuoted()
    {
        var table = MakeTable("T");

        var identity = Identity.FromHexString(new string('0', 64));
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = '{identity}'",
            table.Where(new Col<Row, Identity>("T", "Name").Eq(identity)).Sql
        );

        var connId = ConnectionId.FromHexString(new string('0', 31) + "1") ?? throw new InvalidOperationException();
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = '{connId}'",
            table.Where(new Col<Row, ConnectionId>("T", "Name").Eq(connId)).Sql
        );

        var uuid = Uuid.Parse("00000000-0000-0000-0000-000000000000");
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = '{uuid}'",
            table.Where(new Col<Row, Uuid>("T", "Name").Eq(uuid)).Sql
        );

        var u128 = new U128(upper: 0, lower: 5);
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = '{u128}'",
            table.Where(new Col<Row, U128>("T", "Name").Eq(u128)).Sql
        );
    }

    [Fact]
    public void IxCol_EqNeq_HasNullSemantics()
    {
        var table = MakeTable("T");

        Assert.Equal(
            "\"T\".\"Name\" = 'x'",
            table.IxCols.Name.Eq("x").Sql
        );

        Assert.Equal(
            "\"T\".\"Name\" IS NULL",
            table.IxCols.Name.Eq(null!).Sql
        );

        Assert.Equal(
            "\"T\".\"Name\" <> 'x'",
            table.IxCols.Name.Neq("x").Sql
        );

        Assert.Equal(
            "\"T\".\"Name\" IS NOT NULL",
            table.IxCols.Name.Neq(null!).Sql
        );
    }
}
