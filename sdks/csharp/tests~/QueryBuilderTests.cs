namespace SpacetimeDB.Tests;

using System;
using Xunit;

public sealed class QueryBuilderTests
{
    private sealed class Row { }

    private sealed class LeftRow { }

    private sealed class RightRow { }

    private sealed class RowCols
    {
        public Col<Row, string> Name { get; }
        public Col<Row, string> Weird { get; }
        public Col<Row, int> Age { get; }
        public Col<Row, bool> IsAdmin { get; }

        public RowCols(string tableName)
        {
            Name = new Col<Row, string>(tableName, "Name");
            Weird = new Col<Row, string>(tableName, "we\"ird");
            Age = new Col<Row, int>(tableName, "Age");
            IsAdmin = new Col<Row, bool>(tableName, "IsAdmin");
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

    private sealed class RowNullableCols
    {
        public NullableCol<Row, string> Name { get; }
        public NullableCol<Row, int> Age { get; }

        public RowNullableCols(string tableName)
        {
            Name = new NullableCol<Row, string>(tableName, "Name");
            Age = new NullableCol<Row, int>(tableName, "Age");
        }
    }

    private sealed class RowNullableIxCols
    {
        public NullableIxCol<Row, string> Name { get; }

        public RowNullableIxCols(string tableName)
        {
            Name = new NullableIxCol<Row, string>(tableName, "Name");
        }
    }

    private static Table<Row, RowNullableCols, RowNullableIxCols> MakeNullableTable(string tableName) =>
        new(tableName, new RowNullableCols(tableName), new RowNullableIxCols(tableName));

    private sealed class LeftCols
    {
        public Col<LeftRow, int> Id { get; }

        public LeftCols(string tableName)
        {
            Id = new Col<LeftRow, int>(tableName, "id");
        }
    }

    private sealed class LeftIxCols
    {
        public IxCol<LeftRow, int> Id { get; }

        public LeftIxCols(string tableName)
        {
            Id = new IxCol<LeftRow, int>(tableName, "id");
        }
    }

    private sealed class RightCols
    {
        public Col<RightRow, int> Uid { get; }

        public RightCols(string tableName)
        {
            Uid = new Col<RightRow, int>(tableName, "uid");
        }
    }

    private sealed class RightIxCols
    {
        public IxCol<RightRow, int> Uid { get; }

        public RightIxCols(string tableName)
        {
            Uid = new IxCol<RightRow, int>(tableName, "uid");
        }
    }

    private static Table<LeftRow, LeftCols, LeftIxCols> MakeLeftTable(string tableName) =>
        new(tableName, new LeftCols(tableName), new LeftIxCols(tableName));

    private static Table<RightRow, RightCols, RightIxCols> MakeRightTable(string tableName) =>
        new(tableName, new RightCols(tableName), new RightIxCols(tableName));

    private sealed class LeftNullableIxCols
    {
        public NullableIxCol<LeftRow, int> Id { get; }

        public LeftNullableIxCols(string tableName)
        {
            Id = new NullableIxCol<LeftRow, int>(tableName, "id");
        }
    }

    private sealed class RightNullableIxCols
    {
        public NullableIxCol<RightRow, int> Uid { get; }

        public RightNullableIxCols(string tableName)
        {
            Uid = new NullableIxCol<RightRow, int>(tableName, "uid");
        }
    }

    private static Table<LeftRow, LeftCols, LeftNullableIxCols> MakeLeftNullableIxTable(string tableName) =>
        new(tableName, new LeftCols(tableName), new LeftNullableIxCols(tableName));

    private static Table<RightRow, RightCols, RightNullableIxCols> MakeRightNullableIxTable(string tableName) =>
        new(tableName, new RightCols(tableName), new RightNullableIxCols(tableName));

    [Fact]
    public void All_QuotesTableName()
    {
        var table = MakeTable("My\"Table");
        Assert.Equal("SELECT * FROM \"My\"\"Table\"", table.Build().Sql);
    }

    [Fact]
    public void Where_Eq_String_EscapesSingleQuote()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Name.Eq("O'Reilly")).Build().Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 'O''Reilly'", sql);
    }

    [Fact]
    public void Where_Gt_Int_FormatsInvariant()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Age.Gt(123)).Build().Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Age\" > 123", sql);
    }

    [Fact]
    public void Where_Eq_Bool_FormatsAsTrueFalse()
    {
        var table = MakeTable("T");
        Assert.Equal(
            "SELECT * FROM \"T\" WHERE \"T\".\"IsAdmin\" = TRUE",
            table.Where(c => c.IsAdmin.Eq(true)).Build().Sql
        );
        Assert.Equal(
            "SELECT * FROM \"T\" WHERE \"T\".\"IsAdmin\" = FALSE",
            table.Where(c => c.IsAdmin.Eq(false)).Build().Sql
        );
    }

    [Fact]
    public void Where_WithIxColsOverload_FormatsCorrectly()
    {
        var table = MakeTable("T");
        var sql = table.Where((_, ix) => ix.Name.Eq(SqlLit.String("x"))).Build().Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 'x'", sql);
    }

    [Fact]
    public void Where_ChainingWhere_AddsAnd()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Age.Gt(1)).Where(c => c.IsAdmin.Eq(true)).Build().Sql;

        Assert.Equal(
            "SELECT * FROM \"T\" WHERE (\"T\".\"Age\" > 1) AND (\"T\".\"IsAdmin\" = TRUE)",
            sql
        );
    }

    [Fact]
    public void BoolExpr_AndOrNot_AddsParens()
    {
        var age = new Col<Row, int>("T", "Age");
        var name = new Col<Row, string>("T", "Name");
        var isAdmin = new Col<Row, bool>("T", "IsAdmin");
        var expr = age.Gt(1).And(name.Neq("x")).Or(isAdmin.Eq(true));

        Assert.Equal(
            "(((\"T\".\"Age\" > 1) AND (\"T\".\"Name\" <> 'x')) OR (\"T\".\"IsAdmin\" = TRUE))",
            expr.Sql
        );
    }

    [Fact]
    public void QuoteIdent_EscapesDoubleQuotesInColumnName()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Weird.Eq("x")).Build().Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"we\"\"ird\" = 'x'", sql);
    }

    [Fact]
    public void FormatLiteral_SpacetimeDbTypes_AreQuoted()
    {
        var table = MakeTable("T");

        var identity = Identity.FromHexString(new string('0', 64));
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 0x{identity}",
            table.Where(_ => new Col<Row, Identity>("T", "Name").Eq(identity)).Build().Sql
        );

        var connId = ConnectionId.FromHexString(new string('0', 31) + "1") ?? throw new InvalidOperationException();
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 0x{connId}",
            table.Where(_ => new Col<Row, ConnectionId>("T", "Name").Eq(connId)).Build().Sql
        );

        var uuid = Uuid.Parse("00000000-0000-0000-0000-000000000000");
        var uuidHex = uuid.ToString().Replace("-", string.Empty);
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 0x{uuidHex}",
            table.Where(_ => new Col<Row, Uuid>("T", "Name").Eq(uuid)).Build().Sql
        );

        var u128 = new U128(upper: 0, lower: 5);
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 5",
            table.Where(_ => new Col<Row, U128>("T", "Name").Eq(u128)).Build().Sql
        );
    }

    [Fact]
    public void IxCol_EqNeq_FormatsCorrectly()
    {
        var ix = new IxCol<Row, string>("T", "Name");
        Assert.Equal(
            "\"T\".\"Name\" = 'x'",
            ix.Eq("x").Sql
        );

        Assert.Equal(
            "\"T\".\"Name\" <> 'x'",
            ix.Neq("x").Sql
        );
    }

    [Fact]
    public void LeftSemijoin_Build_FormatsCorrectly()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");

        var sql = left.LeftSemijoin(right, (l, r) => l.Id.Eq(r.Uid)).Build().Sql;
        Assert.Equal(
            "SELECT \"users\".* FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\"",
            sql
        );
    }

    [Fact]
    public void Where_NullableCol_Eq_FormatsCorrectly()
    {
        var table = MakeNullableTable("T");
        var sql = table.Where(c => c.Name.Eq("x")).Build().Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Name\" = 'x'", sql);
    }

    [Fact]
    public void Where_NullableCol_Gt_FormatsCorrectly()
    {
        var table = MakeNullableTable("T");
        var sql = table.Where(c => c.Age.Gt(123)).Build().Sql;
        Assert.Equal("SELECT * FROM \"T\" WHERE \"T\".\"Age\" > 123", sql);
    }

    [Fact]
    public void RightSemijoin_WithLeftAndRightWhere_FormatsCorrectly()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");

        var sql = left
            .Where(c => c.Id.Eq(1))
            .RightSemijoin(right, (l, r) => l.Id.Eq(r.Uid))
            .Where(c => c.Uid.Gt(10))
            .Build()
            .Sql;

        Assert.Equal(
            "SELECT \"other\".* FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\" WHERE \"users\".\"id\" = 1 AND \"other\".\"uid\" > 10",
            sql
        );
    }

    [Fact]
    public void Join_Build_FormatsCorrectly()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");

        var sql = left.Join(right, (l, r) => l.Id.Eq(r.Uid)).Build().Sql;
        Assert.Equal(
            "SELECT * FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\"",
            sql
        );
    }

    [Fact]
    public void Join_WithLeftAndRightWhere_FormatsCorrectly()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");

        var sql = left
            .Where(c => c.Id.Eq(1))
            .Join(right, (l, r) => l.Id.Eq(r.Uid))
            .WhereRight(c => c.Uid.Gt(10))
            .Build()
            .Sql;

        Assert.Equal(
            "SELECT * FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\" WHERE \"users\".\"id\" = 1 AND \"other\".\"uid\" > 10",
            sql
        );
    }

    [Fact]
    public void Join_WithWhereLeftChaining_FormatsCorrectly()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");

        var sql = left
            .Join(right, (l, r) => l.Id.Eq(r.Uid))
            .WhereLeft(c => c.Id.Gt(0))
            .WhereLeft(c => c.Id.Eq(1))
            .WhereRight(c => c.Uid.Gt(10))
            .Build()
            .Sql;

        Assert.Equal(
            "SELECT * FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\" WHERE (\"users\".\"id\" > 0) AND (\"users\".\"id\" = 1) AND \"other\".\"uid\" > 10",
            sql
        );
    }

    [Fact]
    public void Join_OnNullableIxCol_FormatsCorrectly()
    {
        var left = MakeLeftNullableIxTable("users");
        var right = MakeRightNullableIxTable("other");

        var sql = left.Join(right, (l, r) => l.Id.Eq(r.Uid)).Build().Sql;
        Assert.Equal(
            "SELECT * FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\"",
            sql
        );
    }
}
