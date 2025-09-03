## Quickstart Chat Module Version
This quickstart chat Client is built on from the [C# Module Quickstart](https://spacetimedb.com/docs/modules/c-sharp/quickstart). \
If you have started with the [Rust Module Quickstart](https://spacetimedb.com/docs/modules/rust/quickstart) the reducer names will differ, send_message instead of SendMessage and set_name instead of SetName. /



## How to Use the Quickstart Chat Example

1. Follow the instructions in `Develop.md` in in "sdk-unreal" directory to integrate the `SpacetimeDbSdk` plugin into your project.
2. Launch the project in Unreal Engine.
3. Create a new level. Press add in the Content Browser, then select "Level" to create a new level.
   - Alternatively, you can open an existing level if you already have one.
4. In the Content Browser, locate the `BP` folder. Inside, you’ll find an actor named `BP_ChatClientActor`.

### Using the Blueprint Version

- Drag `BP_ChatClientActor` into your level.
- Press the Play button to start the game.
- `BP_ChatClientActor` actor connects to `localhost` and uses the module name `quickstart-chat`.
- It automatically subscribes to all tables.
- In the Details panel, you can manually call functions to start/end subscriptions and invoke reducers, only while game is running.

### Using the C++ Version

- Click "Quickly add to Project" button (cube with a plus icon), select `ChatClientActor`, and add it to the level.
- Press the Play button to start the game.
- This actor provides the same functionality as the Blueprint version but is implemented in C++.
