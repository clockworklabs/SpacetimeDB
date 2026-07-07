Get a competitive SpacetimeDB Hangman game running with React and TypeScript.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a TypeScript SpacetimeDB module and React client.

This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the React development server.

```bash
spacetime dev --template hangman-react-ts
```

## Open your app

Navigate to [http://localhost:5173](http://localhost:5173) to play Hangman.

Every player guesses the same hidden word on a private board. A round lasts 60 seconds, followed by 10 seconds of revealed results and rankings.

## Explore the project structure

Your project contains both server and client code.

Edit `spacetimedb/src/index.ts` to change game rules or the word list. Edit `src/App.tsx` to change the game interface.

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Game tables, reducers, and round transitions
├── src/                  # React frontend
│   ├── App.tsx
│   └── module_bindings/  # Auto-generated types
└── package.json
```

## Understand tables and reducers

Open `spacetimedb/src/index.ts` to see the module code. The template includes public round and result tables, private player progress, and scheduled round transitions. The `set_name` reducer registers a nickname and `guess_letter` submits one letter for the current round.

Tables store game state. Reducers are functions that modify data - they are the only way to write to the database.

```typescript
export const set_name = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmedName = name.trim();
    if (trimmedName.length === 0 || trimmedName.length > 20) {
      throw new SenderError('Names must be between 1 and 20 characters');
    }

    const existing = ctx.db.player.identity.find(ctx.sender);
    if (existing) {
      ctx.db.player.identity.update({ ...existing, name: trimmedName });
    } else {
      ctx.db.player.insert({ identity: ctx.sender, name: trimmedName });
    }
  }
);

export const guess_letter = spacetimedb.reducer(
  { letter: t.string() },
  (ctx, { letter }) => {
    const guess = letter.trim().toUpperCase();
    if (!/^[A-Z]$/.test(guess)) {
      throw new SenderError('Guess one letter from A to Z');
    }

    // Update this player's private progress for the active round.
  }
);
```

## Test with the CLI

Open a new terminal and navigate to your project directory. Then use the SpacetimeDB CLI to join a round, guess letters, and inspect your state.

```bash
cd my-spacetime-app

# Pick a nickname before guessing
spacetime call set_name '"Ada"'

# Guess one letter
spacetime call guess_letter '"A"'

# Inspect the active public round and your private board view
spacetime sql "SELECT * FROM current_round"
spacetime sql "SELECT * FROM my_progress"
```

## Understand round state and privacy

The module runs one shared competitive round at a time:

- `current_round` publishes the timer, difficulty, and word length. It reveals the answer only during results.
- `my_progress` exposes each player only to their own masked word and guesses during an active round.
- `round_result` publishes the completed round standings once the timer ends.
- `transition_timer` schedules the results phase and the start of the next round.

## Customize the game

The built-in word list and round durations are at the top of `spacetimedb/src/index.ts`. Add words, adjust difficulty labels, or change the active and results durations there.

The React UI in `src/App.tsx` includes the gallows drawing, masked-word board, keyboard, timer, nickname form, and standings panel. Edit those components and `src/App.css` to change how the game looks and plays.

## Next steps

- Read the [TypeScript SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/typescript-reference) for detailed API docs
- See the [Chat App Tutorial](https://spacetimedb.com/docs/intro/tutorials/chat-app) for another complete React example
