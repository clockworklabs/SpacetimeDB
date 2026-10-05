use spacetimedb::{reducer, table, view, Query, ReducerContext, Table, ViewContext};

#[table(accessor = member, public)]
pub struct Member {
    #[primary_key]
    pub id: u64,
    pub name: String,
}
#[table(accessor = eligibility, public)]
pub struct Eligibility {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub member_id: u64,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.member().insert(Member {
        id: 1,
        name: "Ada".into(),
    });
    ctx.db.member().insert(Member {
        id: 2,
        name: "Grace".into(),
    });
    ctx.db.eligibility().insert(Eligibility { id: 10, member_id: 1 });
}

#[view(accessor = eligible_member, public)]
pub fn eligible_member(ctx: &ViewContext) -> impl Query<Member> {
    ctx.from
        .eligibility()
        .right_semijoin(ctx.from.member(), |eligibility, member| {
            eligibility.member_id.eq(member.id)
        })
}
