---
title: Hide information from players
slug: /how-to/games/hide-information
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll build the hidden information of a card game: each player sees their own hand, but only how many cards the other players hold, and nobody sees the order of the deck. You'll keep every table private, and give each player [views](../../00200-core-concepts/00200-functions/00500-views.md) that return only what they're allowed to see.

## Keep tables private

Any client can subscribe to any row of a public table, whatever your game shows on screen. Hiding something in your client's UI doesn't hide it: a modified client can read the whole table, including other players' cards.

Private tables are the opposite. Clients can't subscribe to them or query them, and they don't even appear in the client bindings. Only your module's code, and you as the database owner, can read them. Tables are private unless you mark them public.

To give clients the data they need, add views. A view runs on the server for the player who subscribes to it, and returns only the rows and columns that player may see. A modified client can't get anything more out of it.

:::tip Best practice
Keep your tables private, and expose what clients need through views. Only make a table public if every row and every column is meant for every player, and will stay that way: it's easy to add a secret column to a public table later and forget that everyone can read it.
:::

## How it works

- Four private tables hold each player's game, the deck, each player's hand and the cards played.
- A `start_game` function shuffles the deck and deals a hand to each player.
- The `draw_card` and `play_card` reducers move cards from the deck to a hand, and from a hand to the table.
- Three views give each player their own hand, how many cards each player in their game holds, and the cards played in their game.

## Define the tables

Add four tables, all private:

- `game_player` records which game each player is in.
- `deck_card` holds the cards left in each game's deck. The card with the lowest `id` is the top of the deck.
- `hand_card` holds the cards in each player's hand, with an index on the player holding them.
- `played_card` holds the cards played face up in each game.

A card is a number from 0 to 51, which clients turn into a rank and a suit.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import type { Identity } from 'spacetimedb';
import { schema, table, t, SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';

// Cards in each player's starting hand.
const HAND_SIZE = 5;

// Each player's game.
const gamePlayer = table(
  { name: 'game_player' },
  {
    identity: t.identity().primaryKey(),
    gameId: t.u64().index('btree'),
  }
);

// The cards left in each game's deck. The card with the lowest id is on top.
const deckCard = table(
  { name: 'deck_card' },
  {
    id: t.u64().primaryKey().autoInc(),
    gameId: t.u64().index('btree'),
    // 0 to 51: the suit is card / 13, the rank is card % 13.
    card: t.u8(),
  }
);

// The cards in each player's hand.
const handCard = table(
  { name: 'hand_card' },
  {
    id: t.u64().primaryKey().autoInc(),
    holder: t.identity().index('btree'),
    card: t.u8(),
  }
);

// The cards played face up in each game.
const playedCard = table(
  { name: 'played_card' },
  {
    id: t.u64().primaryKey().autoInc(),
    gameId: t.u64().index('btree'),
    card: t.u8(),
    playedBy: t.identity(),
  }
);

const spacetimedb = schema({ gamePlayer, deckCard, handCard, playedCard });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // Cards in each player's starting hand.
    const int HandSize = 5;

    // Each player's game.
    [SpacetimeDB.Table(Accessor = "GamePlayer")]
    public partial struct GamePlayer
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;

        [SpacetimeDB.Index.BTree]
        public ulong GameId;
    }

    // The cards left in each game's deck. The card with the lowest id is on top.
    [SpacetimeDB.Table(Accessor = "DeckCard")]
    public partial struct DeckCard
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Index.BTree]
        public ulong GameId;

        // 0 to 51: the suit is Card / 13, the rank is Card % 13.
        public byte Card;
    }

    // The cards in each player's hand.
    [SpacetimeDB.Table(Accessor = "HandCard")]
    public partial struct HandCard
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Index.BTree]
        public Identity Holder;

        public byte Card;
    }

    // The cards played face up in each game.
    [SpacetimeDB.Table(Accessor = "PlayedCard")]
    public partial struct PlayedCard
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Index.BTree]
        public ulong GameId;

        public byte Card;
        public Identity PlayedBy;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::rand::seq::SliceRandom;
use spacetimedb::{reducer, table, view, Identity, Query, ReducerContext, SpacetimeType, Table, ViewContext};

// Cards in each player's starting hand.
const HAND_SIZE: usize = 5;

// Each player's game.
#[table(accessor = game_player)]
pub struct GamePlayer {
    #[primary_key]
    identity: Identity,
    #[index(btree)]
    game_id: u64,
}

// The cards left in each game's deck. The card with the lowest id is on top.
#[table(accessor = deck_card)]
pub struct DeckCard {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    game_id: u64,
    // 0 to 51: the suit is card / 13, the rank is card % 13.
    card: u8,
}

// The cards in each player's hand.
#[table(accessor = hand_card)]
pub struct HandCard {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    holder: Identity,
    card: u8,
}

// The cards played face up in each game.
#[table(accessor = played_card)]
pub struct PlayedCard {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    game_id: u64,
    card: u8,
    played_by: Identity,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
#include <numeric>
#include <vector>
using namespace SpacetimeDB;

// Cards in each player's starting hand.
constexpr size_t HAND_SIZE = 5;

// Each player's game.
struct GamePlayer {
    Identity identity;
    uint64_t game_id;
};
SPACETIMEDB_STRUCT(GamePlayer, identity, game_id)
SPACETIMEDB_TABLE(GamePlayer, game_player, Private)
FIELD_PrimaryKey(game_player, identity)
FIELD_Index(game_player, game_id)

// The cards left in each game's deck. The card with the lowest id is on top.
struct DeckCard {
    uint64_t id;
    uint64_t game_id;
    // 0 to 51: the suit is card / 13, the rank is card % 13.
    uint8_t card;
};
SPACETIMEDB_STRUCT(DeckCard, id, game_id, card)
SPACETIMEDB_TABLE(DeckCard, deck_card, Private)
FIELD_PrimaryKeyAutoInc(deck_card, id)
FIELD_Index(deck_card, game_id)

// The cards in each player's hand.
struct HandCard {
    uint64_t id;
    Identity holder;
    uint8_t card;
};
SPACETIMEDB_STRUCT(HandCard, id, holder, card)
SPACETIMEDB_TABLE(HandCard, hand_card, Private)
FIELD_PrimaryKeyAutoInc(hand_card, id)
FIELD_Index(hand_card, holder)

// The cards played face up in each game.
struct PlayedCard {
    uint64_t id;
    uint64_t game_id;
    uint8_t card;
    Identity played_by;
};
SPACETIMEDB_STRUCT(PlayedCard, id, game_id, card, played_by)
SPACETIMEDB_TABLE(PlayedCard, played_card, Private)
FIELD_PrimaryKeyAutoInc(played_card, id)
FIELD_Index(played_card, game_id)
```

</TabItem>
</Tabs>

## Deal the cards

Add a `start_game` function that shuffles a 52-card deck, deals 5 cards to each player, and stores the rest of the deck in shuffled order.

`start_game` isn't a reducer, so clients can't call it. Call it from your matchmaking when a match is created, with the match's id and its players. See the [matchmaking queue](./00300-matchmaking-queue.md) guide.

When a game ends, delete its players' `game_player` and `hand_card` rows, and its `deck_card` and `played_card` rows. Otherwise its players can't start another game: `start_game` would fail to add them to `game_player` again.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
/** Shuffles a deck and deals a hand to each player. Call it from your matchmaking. */
function startGame(ctx: Ctx, gameId: bigint, players: Identity[]) {
  const deck = Array.from({ length: 52 }, (_, i) => i);
  // Shuffle the deck.
  for (let i = deck.length - 1; i > 0; i--) {
    const j = ctx.random.integerInRange(0, i);
    [deck[i], deck[j]] = [deck[j], deck[i]];
  }

  for (const player of players) {
    ctx.db.gamePlayer.insert({ identity: player, gameId });
    for (const card of deck.splice(0, HAND_SIZE)) {
      ctx.db.handCard.insert({ id: 0n, holder: player, card });
    }
  }
  // The rest of the deck, in shuffled order.
  for (const card of deck) {
    ctx.db.deckCard.insert({ id: 0n, gameId, card });
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    /// Shuffles a deck and deals a hand to each player. Call it from your matchmaking.
    public static void StartGame(ReducerContext ctx, ulong gameId, List<Identity> players)
    {
        var deck = Enumerable.Range(0, 52).Select(i => (byte)i).ToArray();
        ctx.Rng.Shuffle(deck);

        var next = 0;
        foreach (var player in players)
        {
            ctx.Db.GamePlayer.Insert(new GamePlayer { Identity = player, GameId = gameId });
            for (var i = 0; i < HandSize; i++)
            {
                ctx.Db.HandCard.Insert(new HandCard { Id = 0, Holder = player, Card = deck[next++] });
            }
        }
        // The rest of the deck, in shuffled order.
        for (; next < deck.Length; next++)
        {
            ctx.Db.DeckCard.Insert(new DeckCard { Id = 0, GameId = gameId, Card = deck[next] });
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Shuffles a deck and deals a hand to each player. Call it from your matchmaking.
pub fn start_game(ctx: &ReducerContext, game_id: u64, players: &[Identity]) {
    let mut deck: Vec<u8> = (0..52).collect();
    deck.shuffle(&mut ctx.rng());
    let mut deck = deck.into_iter();

    for &player in players {
        ctx.db.game_player().insert(GamePlayer { identity: player, game_id });
        for card in deck.by_ref().take(HAND_SIZE) {
            ctx.db.hand_card().insert(HandCard { id: 0, holder: player, card });
        }
    }
    // The rest of the deck, in shuffled order.
    for card in deck {
        ctx.db.deck_card().insert(DeckCard { id: 0, game_id, card });
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Shuffles a deck and deals a hand to each player. Call it from your matchmaking.
void start_game(ReducerContext& ctx, uint64_t game_id, const std::vector<Identity>& players) {
    std::vector<uint8_t> deck(52);
    std::iota(deck.begin(), deck.end(), 0);
    ctx.rng().shuffle(deck.begin(), deck.end());

    size_t next = 0;
    for (const auto& player : players) {
        ctx.db[game_player].insert(GamePlayer{player, game_id});
        for (size_t i = 0; i < HAND_SIZE; ++i) {
            ctx.db[hand_card].insert(HandCard{0, player, deck[next++]});
        }
    }
    // The rest of the deck, in shuffled order.
    for (; next < deck.size(); ++next) {
        ctx.db[deck_card].insert(DeckCard{0, game_id, deck[next]});
    }
}
```

</TabItem>
</Tabs>

:::caution The shuffle can be predicted
The module's random number generator (`ctx.random` in TypeScript, `ctx.Rng` in C#, `ctx.rng()` in Rust and C++) is seeded from the reducer's timestamp, so it isn't cryptographically secure. A determined cheater who knows roughly when the cards were dealt could try every possible timestamp, find the one that deals the hand they hold, and work out the whole deck. That's fine for casual games, but not for games played for money or ranking.
:::

## Draw and play cards

Add two reducers:

- `draw_card` moves the top card of the caller's deck into their hand.
- `play_card` moves a card from the caller's hand to the cards played in their game. It rejects cards the caller doesn't hold.

Errors are sent back to the player who called the reducer, so they must not reveal anything about other players. `play_card` returns the same error whether the card doesn't exist or is in another player's hand.

Other players don't see which reducers you call or their arguments. They only see the changes to the views they subscribe to.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const drawCard = spacetimedb.reducer(ctx => {
  const me = ctx.db.gamePlayer.identity.find(ctx.sender);
  if (!me) {
    throw new SenderError('not in a game');
  }
  let top;
  for (const c of ctx.db.deckCard.gameId.filter(me.gameId)) {
    if (!top || c.id < top.id) {
      top = c;
    }
  }
  if (!top) {
    throw new SenderError('the deck is empty');
  }
  ctx.db.deckCard.id.delete(top.id);
  ctx.db.handCard.insert({ id: 0n, holder: ctx.sender, card: top.card });
});

export const playCard = spacetimedb.reducer({ cardId: t.u64() }, (ctx, { cardId }) => {
  const me = ctx.db.gamePlayer.identity.find(ctx.sender);
  if (!me) {
    throw new SenderError('not in a game');
  }
  // The same error whether the card doesn't exist or is in another player's hand.
  const card = ctx.db.handCard.id.find(cardId);
  if (!card || !card.holder.isEqual(ctx.sender)) {
    throw new SenderError("you don't have this card");
  }
  ctx.db.handCard.id.delete(card.id);
  ctx.db.playedCard.insert({ id: 0n, gameId: me.gameId, card: card.card, playedBy: ctx.sender });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void DrawCard(ReducerContext ctx)
    {
        var me = ctx.Db.GamePlayer.Identity.Find(ctx.Sender) ?? throw new Exception("Not in a game");
        var deck = ctx.Db.DeckCard.GameId.Filter(me.GameId).ToList();
        if (deck.Count == 0)
        {
            throw new Exception("The deck is empty");
        }
        var top = deck.MinBy(c => c.Id);
        ctx.Db.DeckCard.Id.Delete(top.Id);
        ctx.Db.HandCard.Insert(new HandCard { Id = 0, Holder = ctx.Sender, Card = top.Card });
    }

    [SpacetimeDB.Reducer]
    public static void PlayCard(ReducerContext ctx, ulong cardId)
    {
        var me = ctx.Db.GamePlayer.Identity.Find(ctx.Sender) ?? throw new Exception("Not in a game");
        // The same error whether the card doesn't exist or is in another player's hand.
        if (ctx.Db.HandCard.Id.Find(cardId) is not HandCard card || card.Holder != ctx.Sender)
        {
            throw new Exception("You don't have this card");
        }
        ctx.Db.HandCard.Id.Delete(card.Id);
        ctx.Db.PlayedCard.Insert(new PlayedCard
        {
            Id = 0,
            GameId = me.GameId,
            Card = card.Card,
            PlayedBy = ctx.Sender,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn draw_card(ctx: &ReducerContext) -> Result<(), String> {
    let me = ctx.db.game_player().identity().find(ctx.sender()).ok_or("not in a game")?;
    let top = ctx
        .db
        .deck_card()
        .game_id()
        .filter(me.game_id)
        .min_by_key(|c| c.id)
        .ok_or("the deck is empty")?;
    ctx.db.deck_card().id().delete(top.id);
    ctx.db.hand_card().insert(HandCard {
        id: 0,
        holder: ctx.sender(),
        card: top.card,
    });
    Ok(())
}

#[reducer]
pub fn play_card(ctx: &ReducerContext, card_id: u64) -> Result<(), String> {
    let me = ctx.db.game_player().identity().find(ctx.sender()).ok_or("not in a game")?;
    // The same error whether the card doesn't exist or is in another player's hand.
    let card = ctx
        .db
        .hand_card()
        .id()
        .find(card_id)
        .filter(|c| c.holder == ctx.sender())
        .ok_or("you don't have this card")?;
    ctx.db.hand_card().id().delete(card.id);
    ctx.db.played_card().insert(PlayedCard {
        id: 0,
        game_id: me.game_id,
        card: card.card,
        played_by: ctx.sender(),
    });
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(draw_card, ReducerContext ctx) {
    auto me = ctx.db[game_player_identity].find(ctx.sender());
    if (!me) {
        return Err("not in a game");
    }
    std::optional<DeckCard> top;
    for (const auto& c : ctx.db[deck_card_game_id].filter(me->game_id)) {
        if (!top || c.id < top->id) {
            top = c;
        }
    }
    if (!top) {
        return Err("the deck is empty");
    }
    ctx.db[deck_card_id].delete_by_key(top->id);
    ctx.db[hand_card].insert(HandCard{0, ctx.sender(), top->card});
    return Ok();
}

SPACETIMEDB_REDUCER(play_card, ReducerContext ctx, uint64_t card_id) {
    auto me = ctx.db[game_player_identity].find(ctx.sender());
    if (!me) {
        return Err("not in a game");
    }
    // The same error whether the card doesn't exist or is in another player's hand.
    auto card = ctx.db[hand_card_id].find(card_id);
    if (!card || card->holder != ctx.sender()) {
        return Err("you don't have this card");
    }
    ctx.db[hand_card_id].delete_by_key(card->id);
    ctx.db[played_card].insert(PlayedCard{0, me->game_id, card->card, ctx.sender()});
    return Ok();
}
```

</TabItem>
</Tabs>

## Give each player their own views

Add three public views. Each one depends on who is asking, so SpacetimeDB computes it separately for each subscribed player:

- `my_hand` returns the caller's cards. It's a single filter, so it returns a query instead of rows: SpacetimeDB then keeps it up to date the same way as a subscription query, without re-running the view.
- `played_cards` returns the cards played in the caller's game. It's also a query: it finds the caller's `game_player` row, then the played cards with the same game id.
- `my_table` returns every player in the caller's game with the number of cards they hold. It returns a `Seat` type instead of `hand_card` rows, so other players' cards never leave the server: a view decides which columns players see, not just which rows.

Because SpacetimeDB computes these views for each player, keep them small: each one reads a few rows through indexes. See [Views](../../00200-core-concepts/00200-functions/00500-views.md#performance-why-anonymousviewcontext-matters) for how they're evaluated.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const myHand = spacetimedb.view(
  { name: 'my_hand', public: true },
  t.array(handCard.rowType),
  ctx => ctx.from.handCard.where(c => c.holder.eq(ctx.sender))
);

export const playedCards = spacetimedb.view(
  { name: 'played_cards', public: true },
  t.array(playedCard.rowType),
  ctx =>
    ctx.from.gamePlayer
      .where(p => p.identity.eq(ctx.sender))
      .rightSemijoin(ctx.from.playedCard, (p, c) => p.gameId.eq(c.gameId))
);

// What every player can know about a player: who they are, and how many cards they hold.
const seat = t.row('Seat', {
  identity: t.identity(),
  handSize: t.u32(),
});

export const myTable = spacetimedb.view({ name: 'my_table', public: true }, t.array(seat), ctx => {
  const me = ctx.db.gamePlayer.identity.find(ctx.sender);
  if (!me) {
    return [];
  }
  return Array.from(ctx.db.gamePlayer.gameId.filter(me.gameId), p => ({
    identity: p.identity,
    handSize: Array.from(ctx.db.handCard.holder.filter(p.identity)).length,
  }));
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.View(Accessor = "MyHand", Public = true)]
    public static IQuery<HandCard> MyHand(ViewContext ctx)
    {
        return ctx.From.HandCard().Where(c => c.Holder.Eq(ctx.Sender));
    }

    [SpacetimeDB.View(Accessor = "PlayedCards", Public = true)]
    public static IQuery<PlayedCard> PlayedCards(ViewContext ctx)
    {
        return ctx.From.GamePlayer()
            .Where(p => p.Identity.Eq(ctx.Sender))
            .RightSemijoin(ctx.From.PlayedCard(), (p, c) => p.GameId.Eq(c.GameId));
    }

    // What every player can know about a player: who they are, and how many cards they hold.
    [SpacetimeDB.Type]
    public partial struct Seat
    {
        public Identity Identity;
        public uint HandSize;
    }

    [SpacetimeDB.View(Accessor = "MyTable", Public = true)]
    public static List<Seat> MyTable(ViewContext ctx)
    {
        if (ctx.Db.GamePlayer.Identity.Find(ctx.Sender) is not GamePlayer me)
        {
            return new List<Seat>();
        }
        return ctx.Db.GamePlayer.GameId.Filter(me.GameId)
            .Select(p => new Seat
            {
                Identity = p.Identity,
                HandSize = (uint)ctx.Db.HandCard.Holder.Filter(p.Identity).Count(),
            })
            .ToList();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[view(accessor = my_hand, public)]
fn my_hand(ctx: &ViewContext) -> impl Query<HandCard> {
    let me = ctx.sender();
    ctx.from.hand_card().r#where(move |c| c.holder.eq(me))
}

#[view(accessor = played_cards, public)]
fn played_cards(ctx: &ViewContext) -> impl Query<PlayedCard> {
    let me = ctx.sender();
    ctx.from
        .game_player()
        .r#where(move |p| p.identity.eq(me))
        .right_semijoin(ctx.from.played_card(), |p, c| p.game_id.eq(c.game_id))
}

// What every player can know about a player: who they are, and how many cards they hold.
#[derive(SpacetimeType)]
pub struct Seat {
    identity: Identity,
    hand_size: u32,
}

#[view(accessor = my_table, public)]
fn my_table(ctx: &ViewContext) -> Vec<Seat> {
    let Some(me) = ctx.db.game_player().identity().find(ctx.sender()) else {
        return vec![];
    };
    ctx.db
        .game_player()
        .game_id()
        .filter(me.game_id)
        .map(|p| Seat {
            identity: p.identity,
            hand_size: ctx.db.hand_card().holder().filter(p.identity).count() as u32,
        })
        .collect()
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_VIEW(Query<HandCard>, my_hand, Public, ViewContext ctx) {
    Identity me = ctx.sender();
    return ctx.from[hand_card].where([me](const auto& c) {
        return c.holder.eq(me);
    });
}

SPACETIMEDB_VIEW(Query<PlayedCard>, played_cards, Public, ViewContext ctx) {
    Identity me = ctx.sender();
    return ctx.from[game_player]
        .where([me](const auto& p) {
            return p.identity.eq(me);
        })
        .right_semijoin(ctx.from[played_card], [](const auto& p, const auto& c) {
            return p.game_id.eq(c.game_id);
        });
}

// What every player can know about a player: who they are, and how many cards they hold.
struct Seat {
    Identity identity;
    uint32_t hand_size;
};
SPACETIMEDB_STRUCT(Seat, identity, hand_size)

SPACETIMEDB_VIEW(std::vector<Seat>, my_table, Public, ViewContext ctx) {
    auto me = ctx.db[game_player_identity].find(ctx.sender());
    if (!me) {
        return {};
    }
    std::vector<Seat> seats;
    for (const auto& p : ctx.db[game_player_game_id].filter(me->game_id)) {
        auto hand = ctx.db[hand_card_holder].filter(p.identity);
        seats.push_back(Seat{p.identity, static_cast<uint32_t>(hand.size())});
    }
    return seats;
}
```

</TabItem>
</Tabs>

## Show the game on the client

On the client, subscribe to the three views. They're all the client can subscribe to: subscribing to a private table such as `hand_card` fails with "no such table", as if it didn't exist.

The client cache then holds the player's hand, the hand sizes and the played cards. Draw the game from it each frame. The client cache doesn't keep the order the rows were added in, so sort the hand by card and the played cards by id.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const RANKS = ['A', '2', '3', '4', '5', '6', '7', '8', '9', '10', 'J', 'Q', 'K'];
const SUITS = ['♠', '♥', '♦', '♣'];

// Turns a card, from 0 to 51, into a name such as "Q♥".
function cardName(card: number): string {
  return RANKS[card % 13] + SUITS[Math.floor(card / 13)];
}

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.myHand, tables.myTable, tables.playedCards]);
  })
  .build();

// Call this each frame to draw the game from the client cache.
function drawGame() {
  const hand = Array.from(conn.db.myHand.iter()).sort((a, b) => a.card - b.card);
  console.log(`Your hand: ${hand.map(c => cardName(c.card)).join(' ')}`);

  for (const seat of conn.db.myTable.iter()) {
    if (!seat.identity.isEqual(conn.identity!)) {
      console.log(`${seat.identity.toHexString().slice(0, 8)} holds ${seat.handSize} cards`);
    }
  }

  const played = Array.from(conn.db.playedCards.iter()).sort((a, b) => (a.id < b.id ? -1 : 1));
  console.log(`Played: ${played.map(c => cardName(c.card)).join(' ')}`);
}

// Call this when the player clicks the deck.
function draw() {
  conn.reducers.drawCard({});
}

// Call this when the player clicks a card in their hand.
function play(cardId: bigint) {
  conn.reducers.playCard({ cardId });
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

string[] ranks = ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"];
string[] suits = ["♠", "♥", "♦", "♣"];

// Turns a card, from 0 to 51, into a name such as "Q♥".
string CardName(byte card) => ranks[card % 13] + suits[card / 13];

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.MyHand())
            .AddQuery(q => q.From.MyTable())
            .AddQuery(q => q.From.PlayedCards())
            .Subscribe();
    })
    .Build();

// Call this each frame to draw the game from the client cache.
void DrawGame()
{
    var hand = conn.Db.MyHand.Iter().OrderBy(c => c.Card).Select(c => CardName(c.Card));
    Console.WriteLine($"Your hand: {string.Join(" ", hand)}");

    foreach (var seat in conn.Db.MyTable.Iter())
    {
        if (seat.Identity != conn.Identity)
        {
            Console.WriteLine($"{seat.Identity.ToString()[..8]} holds {seat.HandSize} cards");
        }
    }

    var played = conn.Db.PlayedCards.Iter().OrderBy(c => c.Id).Select(c => CardName(c.Card));
    Console.WriteLine($"Played: {string.Join(" ", played)}");
}

// Call this when the player clicks the deck.
void Draw() => conn.Reducers.DrawCard();

// Call this when the player clicks a card in their hand.
void Play(ulong cardId) => conn.Reducers.PlayCard(cardId);
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the client cache stays up to date.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table};

const RANKS: [&str; 13] = ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"];
const SUITS: [&str; 4] = ["♠", "♥", "♦", "♣"];

// Turns a card, from 0 to 51, into a name such as "Q♥".
fn card_name(card: u8) -> String {
    format!("{}{}", RANKS[(card % 13) as usize], SUITS[(card / 13) as usize])
}

// Call this each frame to draw the game from the client cache.
fn draw_game(conn: &DbConnection) {
    let mut hand: Vec<_> = conn.db().my_hand().iter().collect();
    hand.sort_by_key(|c| c.card);
    let hand: Vec<_> = hand.iter().map(|c| card_name(c.card)).collect();
    println!("Your hand: {}", hand.join(" "));

    for seat in conn.db().my_table().iter() {
        if seat.identity != conn.identity() {
            println!("{} holds {} cards", &seat.identity.to_hex()[..8], seat.hand_size);
        }
    }

    let mut played: Vec<_> = conn.db().played_cards().iter().collect();
    played.sort_by_key(|c| c.id);
    let played: Vec<_> = played.iter().map(|c| card_name(c.card)).collect();
    println!("Played: {}", played.join(" "));
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.my_hand())
                .add_query(|q| q.from.my_table())
                .add_query(|q| q.from.played_cards())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here, and calls draw_game(&conn) each frame.
}

// Call this when the player clicks the deck.
fn draw(conn: &DbConnection) {
    conn.reducers().draw_card().unwrap();
}

// Call this when the player clicks a card in their hand.
fn play(conn: &DbConnection, card_id: u64) {
    conn.reducers().play_card(card_id).unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

Each player now sees exactly what they're allowed to, and a modified client can't see more:

- All four tables are private, so clients can't read the deck or anyone's hand directly.
- `my_hand` and `played_cards` return only the caller's cards and the cards played in their game.
- `my_table` tells each player how many cards the others hold, without their cards.
- Errors from `play_card` don't reveal other players' cards.

To adapt it to your game:

- **Show how many cards are left in the deck.** Add it to a view the same way as the hand sizes: count the `deck_card` rows of the caller's game.
- **Reveal hands at the end of a round.** A view can check the state of the game before returning other players' cards, for example only once a showdown has started.
- **Hide other kinds of information.** The same pattern hides units under fog of war, or a hidden role in a social deduction game: keep the data in a private table, and return only what the caller may see from a view. To send each player only what's near them, without hiding anything, see [Only sync what's near the player](./00500-sync-nearby-entities.md).
- **Take turns.** See [Make a turn-based game with turn timeouts](./00400-turn-based-game.md).

To learn more about the features used here, see [Access Permissions](../../00200-core-concepts/00300-tables/00400-access-permissions.md) and [Views](../../00200-core-concepts/00200-functions/00500-views.md).

Next, you can [reset a leaderboard each season](./00700-seasonal-leaderboard-reset.md).
