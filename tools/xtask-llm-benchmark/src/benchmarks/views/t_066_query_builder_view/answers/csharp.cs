using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "Ticket", Public = true)] public partial struct Ticket { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public string Status; public string Title; }
    [Reducer] public static void Seed(ReducerContext ctx) { ctx.Db.Ticket.Insert(new Ticket { Id = 1, Status = "open", Title = "A" }); ctx.Db.Ticket.Insert(new Ticket { Id = 2, Status = "closed", Title = "B" }); }
    [SpacetimeDB.View(Accessor = "OpenTicket", Public = true)]
    public static IQuery<Ticket> OpenTicket(ViewContext ctx) => ctx.From.Ticket().Where(ticket => ticket.Status.Eq("open"));
}
