#![allow(non_snake_case, non_camel_case_types)]

use spacetimedb::{reducer, table, view, AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table};

#[derive(Clone, PartialEq, Eq, Hash, SpacetimeType)]
pub enum Player2Status {
    Active1,
    BannedUntil(u32),
}

#[derive(Clone, SpacetimeType)]
pub struct Person3Info {
    pub AgeValue1: u8,
    pub ScoreTotal: u32,
}

#[table(name = "Player1Canonical", accessor = player1, public)]
pub struct Player1 {
    #[primary_key]
    #[auto_inc]
    pub Player1Id: u32,
    pub player_name: String,
    pub currentLevel2: u32,
    pub status3Field: Player2Status,
}

#[table(accessor = person2, public)]
pub struct Person2 {
    #[primary_key]
    #[auto_inc]
    pub Person2Id: u32,
    pub FirstName: String,
    #[index(btree)]
    pub playerRef: u32,
    pub personInfo: Person3Info,
}

#[reducer]
pub fn CreatePlayer1(ctx: &ReducerContext, Player1Name: String, Start2Level: u32) {
    ctx.db.player1().insert(Player1 {
        Player1Id: 0,
        player_name: Player1Name,
        currentLevel2: Start2Level,
        status3Field: Player2Status::Active1,
    });
}

#[reducer]
pub fn AddPerson2(ctx: &ReducerContext, First3Name: String, playerRef: u32, AgeValue: u8, ScoreTotal: u32) {
    ctx.db.person2().insert(Person2 {
        Person2Id: 0,
        FirstName: First3Name,
        playerRef,
        personInfo: Person3Info {
            AgeValue1: AgeValue,
            ScoreTotal,
        },
    });
}

#[reducer(name = "banPlayer1")]
pub fn BanPlayer1(ctx: &ReducerContext, Player1Id: u32, BanUntil6: u32) {
    if let Some(player) = ctx.db.player1().Player1Id().find(Player1Id) {
        ctx.db.player1().Player1Id().update(Player1 {
            status3Field: Player2Status::BannedUntil(BanUntil6),
            ..player
        });
    }
}

#[view(name = "Level2Person", accessor = person_at_level_2, public)]
pub fn level2_person(ctx: &AnonymousViewContext) -> impl Query<Person2> {
    ctx.from
        .player1()
        .r#where(|pl| pl.currentLevel2.eq(2))
        .right_semijoin(ctx.from.person2(), |pl, per| pl.Player1Id.eq(per.playerRef))
}
