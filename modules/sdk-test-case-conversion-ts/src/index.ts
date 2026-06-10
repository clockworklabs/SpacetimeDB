
import { schema, t, table } from 'spacetimedb/server';

const Player2Status = t.enum('Player2Status', {
  Active1: t.unit(),
  BannedUntil: t.u32(),
});

const Person3Info = t.object('Person3Info', {
  AgeValue1: t.u8(),
  ScoreTotal: t.u32(),
});


const PlayerRow = t.row('PlayerRow', {
  Player1Id: t.u32().primaryKey().autoInc(),
  player_name: t.string(),
  currentLevel2: t.u32(),
  status3Field: Player2Status,
})

const PersonRow = t.row({
  Person2Id: t.u32().primaryKey().autoInc(),
  FirstName: t.string(),
  playerRef: t.u32().index(),
  personInfo: Person3Info,
})

const Player1 = table(
  { name: 'Player1Canonical', public: true },
  PlayerRow
);

const Person2 = table(
  { public: true },
  PersonRow
);

const spacetimedb = schema({ Player1, Person2 });
export default spacetimedb;

export const CreatePlayer1 = spacetimedb.reducer(
  { Player1Name: t.string(), Start2Level: t.u32() },
  (ctx, { Player1Name, Start2Level }) => {
    ctx.db.Player1.insert({
      Player1Id: 0,
      player_name: Player1Name,
      currentLevel2: Start2Level,
      status3Field: { tag: 'Active1' },
    });
  }
);

export const AddPerson2 = spacetimedb.reducer(
  { First3Name: t.string(), playerRef: t.u32(), AgeValue: t.u8(), ScoreTotal: t.u32() },
  (ctx, { First3Name, playerRef, AgeValue, ScoreTotal }) => {
    ctx.db.Person2.insert({
      Person2Id: 0,
      FirstName: First3Name,
      playerRef,
      personInfo: { AgeValue1: AgeValue, ScoreTotal },
    });
  }
);

export const BanPlayer1 = spacetimedb.reducer(
  { name: "banPlayer1" },
  { Player1Id: t.u32(), BanUntil6: t.u32() },
  (ctx, { Player1Id, BanUntil6 }) => {
    const player = ctx.db.Player1.Player1Id.find(Player1Id);
    if (player) {
      ctx.db.Player1.Player1Id.update({
        ...player,
        status3Field: { tag: 'BannedUntil', value: BanUntil6 },
      });
    }
  }
);


export const PersonAtLevel2 = spacetimedb.view(
  { name: "Level2Person", public: true },
  t.array(Person2.rowType),
  (ctx) => {
    const person = ctx.from.Player1.where((p) => p.currentLevel2.eq(2)).rightSemijoin(ctx.from.Person2, (player, person) => player.Player1Id.eq(person.playerRef));
    return person
  }
);
