namespace SpacetimeDB.Testing.Tests;

using TestingModule;
using Chat = ChatModule.Module;

public class QueryTests
{
    [Fact]
    public void ViewsReadCommittedRowsForTheirCaller()
    {
        using var test = ModuleTestContext.Create(typeof(Chat).Assembly);
        var alice = TestAuth.FromJwtPayload(
            """{"iss":"test","sub":"alice"}""",
            ConnectionId.FromHexString("00000000000000000000000000000001")!.Value
        );
        var view = test.CreateViewContext(alice);
        var anonymous = test.CreateAnonymousViewContext();
        Assert.Equal(test.DatabaseIdentity, test.CreateViewContext(TestAuth.Internal()).Sender);
        Assert.Empty(test.RunQuery(view.From.Message()));
        test.WithReducerTx(alice, ctx => Chat.SendMessage(ctx, "Alice"));
        test.WithReducerTx(TestAuth.Internal(), ctx => Chat.SendMessage(ctx, "Internal"));
        var mine = test.RunQuery(Chat.MyMessages(view));
        Assert.Equal("Alice", Assert.Single(mine).Text);
        Assert.Equal(2, test.RunQuery(anonymous.From.Message()).Count);
        Assert.Equal(2UL, view.Db.Message.Count);

        var query = anonymous.From.Message();
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => test.RunQuery(query));
        Assert.Throws<ObjectDisposedException>(() => view.Db.Message.Count);
        Assert.Throws<ObjectDisposedException>(() => anonymous.From.Message());
        Assert.Throws<ObjectDisposedException>(() => test.CreateAnonymousViewContext());
    }

    [Fact]
    public void SemijoinsMaterializeTypedRows()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var entry = test.Db.Entries.Insert(new Entry { Text = "matched" });
        test.Db.Entries.Insert(new Entry { Text = "unmatched" });
        test.Db.Accounts.Insert(new Account { Id = entry.Id, Name = "account" });
        var from = test.CreateAnonymousViewContext().From;
        var query = from.Entries()
            .LeftSemijoin(from.Accounts(), (entry, account) => entry.Id.Eq(account.Id));
        Assert.Equal("matched", Assert.Single(test.RunQuery(query)).Text);
    }

    [Fact]
    public void QueryFailuresLeaveTheDatastoreUsable()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var query = test.CreateAnonymousViewContext().From.Entries();
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                ctx.Db.Entries.Insert(new Entry { Text = "committed" });
                Assert.Throws<InvalidOperationException>(() => test.RunQuery(query));
            }
        );
        Assert.Throws<InvalidOperationException>(
            () => test.RunQuery(new SqlQuery("DELETE FROM Entries"))
        );
        Assert.Throws<EndOfStreamException>(
            () => test.RunQuery(new SqlQuery("SELECT Id FROM Entries"))
        );
        Assert.Equal("committed", Assert.Single(test.RunQuery(query)).Text);
        test.Db.Entries.Insert(new Entry { Text = "still writable" });
        Assert.Equal(2, test.RunQuery(query).Count);
    }

    private sealed record SqlQuery(string Sql) : IQuery<Entry>
    {
        public string ToSql() => Sql;
    }
}
