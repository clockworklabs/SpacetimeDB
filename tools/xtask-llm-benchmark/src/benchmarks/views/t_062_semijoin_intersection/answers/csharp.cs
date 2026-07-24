using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Member", Public = true)] public partial struct Member { [PrimaryKey] public ulong Id; public string Name; }
    [Table(Accessor = "Eligibility", Public = true)] public partial struct Eligibility { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong MemberId; }
    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Member.Insert(new Member { Id = 1, Name = "Ada" });
        ctx.Db.Member.Insert(new Member { Id = 2, Name = "Grace" });
        ctx.Db.Eligibility.Insert(new Eligibility { Id = 10, MemberId = 1 });
    }
    [SpacetimeDB.View(Accessor = "EligibleMember", Public = true)]
    public static IQuery<Member> EligibleMember(ViewContext ctx) =>
        ctx.From.Eligibility().RightSemijoin(ctx.From.Member(), (eligibility, member) => eligibility.MemberId.Eq(member.Id));
}
