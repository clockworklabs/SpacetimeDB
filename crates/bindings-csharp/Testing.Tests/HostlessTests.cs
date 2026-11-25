namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;
using Module = ChatModule.Module;

/// <summary>Behavior of module code that runs outside both a host and a test fixture.</summary>
public class HostlessTests
{
    [Fact]
    public void UnboundHandlesRejectDatabaseAccess()
    {
        var table = new SpacetimeDB.Local().Entries;
        Assert.Throws<InvalidOperationException>(() => table.Count);
        Assert.Throws<InvalidOperationException>(
            () => table.Insert(new Entry { Text = "unbound" })
        );
        Assert.Throws<InvalidOperationException>(() => table.Id.Find(1));
        Assert.Throws<InvalidOperationException>(() => new LocalReadOnly().Entries.Count);
        Assert.Throws<InvalidOperationException>(() => default(QueryBuilder).Entries());
    }

    [Fact]
    public void UnsupportedServicesFailWithoutHostImports()
    {
        Assert.Throws<NotSupportedException>(() => ProcedureContext.Identity);
        Assert.Throws<NotSupportedException>(() => IReducerContext.GetDatabaseIdentity());
        Assert.Throws<NotSupportedException>(() => IViewContext.GetIdentity());
        var connection = ConnectionId.FromHexString("00000000000000000000000000000001")!.Value;
        Assert.Throws<NotSupportedException>(
            () => AuthCtx.BuildFromSystemTables(connection, default)
        );
#pragma warning disable STDB_UNSTABLE
        Assert.Contains(
            "VolatileNonatomicScheduleImmediate",
            Assert
                .Throws<NotSupportedException>(
                    () => Module.VolatileNonatomicScheduleImmediateSendMessage("message")
                )
                .Message
        );
        Assert.Contains(
            "VolatileNonatomicScheduleImmediate",
            Assert
                .Throws<NotSupportedException>(Module.VolatileNonatomicScheduleImmediateCheckStatus)
                .Message
        );
#pragma warning restore STDB_UNSTABLE
    }
}
