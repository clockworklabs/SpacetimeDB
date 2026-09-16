namespace SpacetimeDB.Tests;

using System;
using System.Globalization;
using System.Threading;
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
        public Col<Row, Timestamp> CreatedAt { get; }

        public RowCols(string tableName)
        {
            Name = new Col<Row, string>(tableName, "Name");
            Weird = new Col<Row, string>(tableName, "we\"ird");
            Age = new Col<Row, int>(tableName, "Age");
            IsAdmin = new Col<Row, bool>(tableName, "IsAdmin");
            CreatedAt = new Col<Row, Timestamp>(tableName, "CreatedAt");
        }
    }

    private sealed class RowIxCols
    {
        public IxCol<Row, string> Name { get; }
        public IxCol<Row, Timestamp> CreatedAt { get; }

        public RowIxCols(string tableName)
        {
            Name = new IxCol<Row, string>(tableName, "Name");
            CreatedAt = new IxCol<Row, Timestamp>(tableName, "CreatedAt");
        }
    }

    private static Table<Row, RowCols, RowIxCols> MakeTable(string tableName) =>
        new(tableName, new RowCols(tableName), new RowIxCols(tableName));


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


    [Fact]
    public void QualifiedName_QuotesSegmentsAndCopiesInput()
    {
        var segments = new[] { "select", "we\"ird.namespace" };
        var name = new SqlTableName(segments, "users.with.dots");
        segments[0] = "changed";
        Assert.Equal(new[] { "select", "we\"ird.namespace" }, name.NamespaceSegments);
        Assert.Equal("users.with.dots", name.LocalName);
        Assert.Equal("\"select\".\"we\"\"ird.namespace\".\"users.with.dots\"", name.ToString());
        Assert.Throws<NotSupportedException>(() =>
            ((System.Collections.Generic.IList<string>)name.NamespaceSegments)[0] = "changed");
        Assert.Equal("SELECT * FROM \"auth.users\"", MakeTable("auth.users").ToSql());
        Assert.Equal("\"auth.users\".\"id\"", new Col<Row, int>("auth.users", "id").ToString());
        Assert.Equal("\"auth.users\".\"id\"", new IxCol<Row, int>("auth.users", "id").ToString());
    }

    [Fact]
    public void QualifiedName_FiltersAndBothSemijoinsKeepSameLeafNamesDistinct()
    {
        var leftName = new SqlTableName(new[] { "auth" }, "users");
        var rightName = new SqlTableName(new[] { "audit" }, "users");
        var left = new Table<LeftRow, Col<LeftRow, int>, IxCol<LeftRow, int>>(
            leftName, new(leftName, "id"), new(leftName, "id"));
        var right = new Table<RightRow, Col<RightRow, int>, IxCol<RightRow, int>>(
            rightName, new(rightName, "id"), new(rightName, "id"));
        Assert.Equal("SELECT * FROM \"auth\".\"users\" WHERE (\"auth\".\"users\".\"id\" = 1)",
            left.Where(c => c.Eq(1)).ToSql());
        Assert.Equal("SELECT * FROM \"auth\".\"users\" WHERE (\"auth\".\"users\".\"id\" = 1)",
            left.Where((_, ix) => ix.Eq(SqlLit.Int(1))).ToSql());
        const string join = " FROM \"auth\".\"users\" JOIN \"audit\".\"users\" ON \"auth\".\"users\".\"id\" = \"audit\".\"users\".\"id\"";
        Assert.Equal("SELECT \"auth\".\"users\".*" + join,
            left.LeftSemijoin(right, (l, r) => l.Eq(r)).ToSql());
        Assert.Equal("SELECT \"audit\".\"users\".*" + join,
            left.RightSemijoin(right, (l, r) => l.Eq(r)).ToSql());
        var escaped = new SqlTableName(new[] { "auth\"data" }, "select");
        Assert.Equal("\"auth\"\"data\".\"select\".\"i\"\"d\"", new Col<Row, int>(escaped, "i\"d").ToString());
        Assert.Equal("\"auth\"\"data\".\"select\".\"i\"\"d\"", new IxCol<Row, int>(escaped, "i\"d").ToString());
    }

    [Fact]
    public void All_QuotesTableName()
    {
        var table = MakeTable("My\"Table");
        Assert.Equal("SELECT * FROM \"My\"\"Table\"", table.ToSql());
    }

    [Fact]
    public void Where_Eq_String_EscapesSingleQuote()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Name.Eq("O'Reilly")).ToSql();
        Assert.Equal("SELECT * FROM \"T\" WHERE (\"T\".\"Name\" = 'O''Reilly')", sql);
    }

    [Fact]
    public void Where_Gt_Int_FormatsInvariant()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Age.Gt(123)).ToSql();
        Assert.Equal("SELECT * FROM \"T\" WHERE (\"T\".\"Age\" > 123)", sql);
    }

    [Fact]
    public void Where_Eq_Bool_FormatsAsTrueFalse()
    {
        var table = MakeTable("T");
        Assert.Equal(
            "SELECT * FROM \"T\" WHERE (\"T\".\"IsAdmin\" = TRUE)",
            table.Where(c => c.IsAdmin.Eq(true)).ToSql()
        );
        Assert.Equal(
            "SELECT * FROM \"T\" WHERE (\"T\".\"IsAdmin\" = FALSE)",
            table.Where(c => c.IsAdmin.Eq(false)).ToSql()
        );
    }

    [Fact]
    public void Where_BoolColumn_FormatsCorrectly()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.IsAdmin.Eq(true)).ToSql();
        Assert.Equal("SELECT * FROM \"T\" WHERE (\"T\".\"IsAdmin\" = TRUE)", sql);
    }

    [Fact]
    public void Where_WithIxColsOverload_FormatsCorrectly()
    {
        var table = MakeTable("T");
        var sql = table.Where((_, ix) => ix.Name.Eq(SqlLit.String("x"))).ToSql();
        Assert.Equal("SELECT * FROM \"T\" WHERE (\"T\".\"Name\" = 'x')", sql);
    }

    [Fact]
    public void Where_ChainingWhere_AddsAnd()
    {
        var table = MakeTable("T");
        var sql = table.Where(c => c.Age.Gt(1)).Where(c => c.IsAdmin.Eq(true)).ToSql();

        Assert.Equal(
            "SELECT * FROM \"T\" WHERE ((\"T\".\"Age\" > 1) AND (\"T\".\"IsAdmin\" = TRUE))",
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
        var sql = table.Where(c => c.Weird.Eq("x")).ToSql();
        Assert.Equal("SELECT * FROM \"T\" WHERE (\"T\".\"we\"\"ird\" = 'x')", sql);
    }

    [Fact]
    public void FormatLiteral_SpacetimeDbTypes_AreQuoted()
    {
        var table = MakeTable("T");

        var identity = Identity.FromHexString(new string('0', 64));
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"Name\" = 0x{identity})",
            table.Where(_ => new Col<Row, Identity>("T", "Name").Eq(identity)).ToSql()
        );

        var connId = ConnectionId.FromHexString(new string('0', 31) + "1") ?? throw new InvalidOperationException();
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"Name\" = 0x{connId})",
            table.Where(_ => new Col<Row, ConnectionId>("T", "Name").Eq(connId)).ToSql()
        );

        var uuid = Uuid.Parse("00000000-0000-0000-0000-000000000000");
        var uuidHex = uuid.ToString().Replace("-", string.Empty);
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"Name\" = 0x{uuidHex})",
            table.Where(_ => new Col<Row, Uuid>("T", "Name").Eq(uuid)).ToSql()
        );

        var u128 = new U128(upper: 0, lower: 5);
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"Name\" = 5)",
            table.Where(_ => new Col<Row, U128>("T", "Name").Eq(u128)).ToSql()
        );
    }

    [Fact]
    public void FormatLiteral_Timestamp_UsesQuotedIsoString()
    {
        var table = MakeTable("T");
        var timestamp = new Timestamp(1_737_582_793_990_639L);
        const string expected = "2025-01-22T21:53:13.990639Z";

        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" = '{expected}')",
            table.Where(c => c.CreatedAt.Eq(timestamp)).ToSql()
        );
    }

    [Fact]
    public void Where_Timestamp_ComparisonOperators_FormatCorrectly()
    {
        var table = MakeTable("T");
        var timestamp = new Timestamp(1_737_582_793_990_639L);
        const string expected = "2025-01-22T21:53:13.990639Z";

        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" <> '{expected}')",
            table.Where(c => c.CreatedAt.Neq(timestamp)).ToSql()
        );
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" < '{expected}')",
            table.Where(c => c.CreatedAt.Lt(timestamp)).ToSql()
        );
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" <= '{expected}')",
            table.Where(c => c.CreatedAt.Lte(timestamp)).ToSql()
        );
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" > '{expected}')",
            table.Where(c => c.CreatedAt.Gt(timestamp)).ToSql()
        );
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" >= '{expected}')",
            table.Where(c => c.CreatedAt.Gte(timestamp)).ToSql()
        );
    }

    [Fact]
    public void IxCol_EqNeq_FormatsCorrectly()
    {
        var ix = new IxCol<Row, string>("T", "Name");
        Assert.Equal(
            "(\"T\".\"Name\" = 'x')",
            ix.Eq("x").Sql
        );

        Assert.Equal(
            "(\"T\".\"Name\" <> 'x')",
            ix.Neq("x").Sql
        );
    }

    [Fact]
    public void IxCol_Timestamp_EqNeq_FormatsCorrectly()
    {
        var table = MakeTable("T");
        var timestamp = new Timestamp(1_737_582_793_990_639L);
        const string expected = "2025-01-22T21:53:13.990639Z";

        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" = '{expected}')",
            table.Where((_, ix) => ix.CreatedAt.Eq(timestamp)).ToSql()
        );
        Assert.Equal(
            $"SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" <> '{expected}')",
            table.Where((_, ix) => ix.CreatedAt.Neq(timestamp)).ToSql()
        );
    }

    [Fact]
    public void FormatLiteral_Timestamp_UsesInvariantCulture()
    {
        var table = MakeTable("T");
        var timestamp = new Timestamp(1_737_582_793_990_639L);
        const string expectedSql =
            "SELECT * FROM \"T\" WHERE (\"T\".\"CreatedAt\" = '2025-01-22T21:53:13.990639Z')";
        var originalCulture = Thread.CurrentThread.CurrentCulture;

        try
        {
            // Ensure the format is agnostic to the culture. Using ar-SA because it's different than Gregorian, which is used in UTC.
            Thread.CurrentThread.CurrentCulture = CultureInfo.GetCultureInfo("ar-SA");

            Assert.Equal(
                expectedSql,
                table.Where(c => c.CreatedAt.Eq(timestamp)).ToSql()
            );
        }
        finally
        {
            Thread.CurrentThread.CurrentCulture = originalCulture;
        }
    }

    [Fact]
    public void LeftSemijoin_Build_FormatsCorrectly()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");

        var sql = left.LeftSemijoin(right, (l, r) => l.Id.Eq(r.Uid)).ToSql();
        Assert.Equal(
            "SELECT \"users\".* FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\"",
            sql
        );
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
            .ToSql();

        Assert.Equal(
            "SELECT \"other\".* FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\" WHERE (\"users\".\"id\" = 1) AND (\"other\".\"uid\" > 10)",
            sql
        );
    }

    [Fact]
    public void BoolExpr_Not_FormatsCorrectly()
    {
        var age = new Col<Row, int>("T", "Age");
        var expr = age.Gt(18).Not();
        Assert.Equal("(NOT (\"T\".\"Age\" > 18))", expr.Sql);
    }

    [Fact]
    public void BoolExpr_NotWithAnd_FormatsCorrectly()
    {
        var age = new Col<Row, int>("T", "Age");
        var isAdmin = new Col<Row, bool>("T", "IsAdmin");
        var expr = age.Gt(18).Not().And(isAdmin.Eq(true));
        Assert.Equal("((NOT (\"T\".\"Age\" > 18)) AND (\"T\".\"IsAdmin\" = TRUE))", expr.Sql);
    }

    [Fact]
    public void Table_ImplementsIQuery()
    {
        var table = MakeTable("T");
        IQuery<Row> query = table;
        Assert.Equal("SELECT * FROM \"T\"", query.ToSql());
    }

    [Fact]
    public void FromWhere_ImplementsIQuery()
    {
        var table = MakeTable("T");
        IQuery<Row> query = table.Where(c => c.Age.Gt(18));
        Assert.Equal("SELECT * FROM \"T\" WHERE (\"T\".\"Age\" > 18)", query.ToSql());
    }

    [Fact]
    public void LeftSemijoin_ImplementsIQuery()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");
        IQuery<LeftRow> query = left.LeftSemijoin(right, (l, r) => l.Id.Eq(r.Uid));
        Assert.Equal(
            "SELECT \"users\".* FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\"",
            query.ToSql()
        );
    }

    [Fact]
    public void RightSemijoin_ImplementsIQuery()
    {
        var left = MakeLeftTable("users");
        var right = MakeRightTable("other");
        IQuery<RightRow> query = left.RightSemijoin(right, (l, r) => l.Id.Eq(r.Uid));
        Assert.Equal(
            "SELECT \"other\".* FROM \"users\" JOIN \"other\" ON \"users\".\"id\" = \"other\".\"uid\"",
            query.ToSql()
        );
    }
}
