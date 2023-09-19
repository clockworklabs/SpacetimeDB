export const docsConfig = {
  "sections": [
    {
      "title": "Overview",
      "identifier": "Overview",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Overview/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "SpacetimeDB Documentation",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# SpacetimeDB Documentation\r\n\r\n## Installation\r\n\r\nYou can run SpacetimeDB as a standalone database server via the `spacetime` CLI tool.\r\n\r\nYou can find the instructions to install the CLI tool for your platform [here](/install).\r\n\r\n<button to=\"/install\">Click here to install</button>\r\n\r\nTo get started running your own standalone instance of SpacetimeDB check out our [Getting Started Guide](/docs/getting-started).\r\n\r\n<button to=\"/docs/getting-started\">Getting Started</button>\r\n\r\n## What is SpacetimeDB?\r\n\r\nYou can think of SpacetimeDB as a database that is also a server.\r\n\r\nIt is a relational database system that lets you upload your application logic directly into the database by way of very fancy stored procedures called \"modules\".\r\n\r\nInstead of deploying a web or game server that sits in between your clients and your database, your clients connect directly to the database and execute your application logic inside the database itself. You can write all of your permission and authorization logic right inside your module just as you would in a normal server.\r\n\r\nThis means that you can write your entire application in a single language, Rust, and deploy it as a single binary. No more microservices, no more containers, no more Kubernetes, no more Docker, no more VMs, no more DevOps, no more infrastructure, no more ops, no more servers.\r\n\r\n<figure>\r\n    <img src=\"/images/basic-architecture-diagram.png\" alt=\"SpacetimeDB Architecture\" style=\"width:100%\">\r\n    <figcaption style=\"margin-top: -55px;\" align=\"center\">\r\n        <b align=\"center\">SpacetimeDB application architecture</b>\r\n        <span style=\"font-size: 14px\">(elements in white are provided by SpacetimeDB)</span>\r\n    </figcaption>\r\n</figure>\r\n\r\nIt's actually similar to the idea of smart contracts, except that SpacetimeDB is a database, has nothing to do with blockchain, and it's a lot faster than any smart contract system.\r\n\r\nSo fast, in fact, that the entire backend our MMORPG [BitCraft Online](https://bitcraftonline.com) is just a SpacetimeDB module. We don't have any other servers or services running, which means that everything in the game, all of the chat messages, items, resources, terrain, and even the locations of the players are stored and processed by the database before being synchronized out to all of the clients in real-time.\r\n\r\nSpacetimeDB is optimized for maximum speed and minimum latency rather than batch processing or OLAP workloads. It is designed to be used for real-time applications like games, chat, and collaboration tools.\r\n\r\nThis speed and latency is achieved by holding all of application state in memory, while persisting the data in a write-ahead-log (WAL) which is used to recover application state.\r\n\r\n## State Synchronization\r\n\r\nSpacetimeDB syncs client and server state for you so that you can just write your application as though you're accessing the database locally. No more messing with sockets for a week before actually writing your game.\r\n\r\n## Identities\r\n\r\nAn important concept in SpacetimeDB is that of an `Identity`. An `Identity` represents who someone is. It is a unique identifier that is used to authenticate and authorize access to the database. Importantly, while it represents who someone is, does NOT represent what they can do. Your application's logic will determine what a given identity is able to do by allowing or disallowing a transaction based on the `Identity`.\r\n\r\nSpacetimeDB associates each client with a 256-bit (32-byte) integer `Identity`. These identities are usually formatted as 64-digit hexadecimal strings. Identities are public information, and applications can use them to identify users. Identities are a global resource, so a user can use the same identity with multiple applications, so long as they're hosted by the same SpacetimeDB instance.\r\n\r\nEach identity has a corresponding authentication token. The authentication token is private, and should never be shared with anyone. Specifically, authentication tokens are [JSON Web Tokens](https://datatracker.ietf.org/doc/html/rfc7519) signed by a secret unique to the SpacetimeDB instance.\r\n\r\nAdditionally, each database has an owner `Identity`. Many database maintenance operations, like publishing a new version or evaluating arbitrary SQL queries, are restricted to only authenticated connections by the owner.\r\n\r\nSpacetimeDB provides tools in the CLI and the [client SDKs](/docs/client-languages/client-sdk-overview) for managing credentials.\r\n\r\n## Language Support\r\n\r\n### Server-side Libraries\r\n\r\nCurrently, Rust is the best-supported language for writing SpacetimeDB modules. Support for lots of other languages is in the works!\r\n\r\n- [Rust](/docs/server-languages/rust/rust-module-reference) - [(Quickstart)](/docs/server-languages/rust/rust-module-quickstart-guide)\r\n- [C#](/docs/server-languages/csharp/csharp-module-reference) - [(Quickstart)](/docs/server-languages/csharp/csharp-module-quickstart-guide)\r\n- Python (Coming soon)\r\n- C# (Coming soon)\r\n- Typescript (Coming soon)\r\n- C++ (Planned)\r\n- Lua (Planned)\r\n\r\n### Client-side SDKs\r\n\r\n- [Rust](/docs/client-languages/rust/rust-sdk-reference) - [(Quickstart)](/docs/client-languages/rust/rust-sdk-quickstart-guide)\r\n- [C#](/docs/client-languages/csharp/csharp-sdk-reference) - [(Quickstart)](/docs/client-languages/csharp/csharp-sdk-quickstart-guide)\r\n- [TypeScript](/docs/client-languages/typescript/typescript-sdk-reference) - [(Quickstart)](client-languages/typescript/typescript-sdk-quickstart-guide)\r\n- [Python](/docs/client-languages/python/python-sdk-reference) - [(Quickstart)](/docs/python/python-sdk-quickstart-guide)\r\n- C++ (Planned)\r\n- Lua (Planned)\r\n\r\n### Unity\r\n\r\nSpacetimeDB was designed first and foremost as the backend for multiplayer Unity games. To learn more about using SpacetimeDB with Unity, jump on over to the [SpacetimeDB Unity Tutorial](/docs/unity-tutorial/unity-tutorial-part-1).\r\n\r\n## FAQ\r\n\r\n1. What is SpacetimeDB?\r\n   It's a whole cloud platform within a database that's fast enough to run real-time games.\r\n\r\n1. How do I use SpacetimeDB?\r\n   Install the `spacetime` command line tool, choose your favorite language, import the SpacetimeDB library, write your application, compile it to WebAssembly, and upload it to the SpacetimeDB cloud platform. Once it's uploaded you can call functions directly on your application and subscribe to changes in application state.\r\n\r\n1. How do I get/install SpacetimeDB?\r\n   Just install our command line tool and then upload your application to the cloud.\r\n\r\n1. How do I create a new database with SpacetimeDB?\r\n   Follow our [Quick Start](/docs/quick-start) guide!\r\n\r\nTL;DR in an empty directory:\r\n\r\n```bash\r\nspacetime init --lang=rust\r\nspacetime publish\r\n```\r\n\r\n5. How do I create a Unity game with SpacetimeDB?\r\n   Follow our [Unity Project](/docs/unity-project) guide!\r\n\r\nTL;DR in an empty directory:\r\n\r\n```bash\r\nspacetime init --lang=rust\r\nspacetime publish\r\nspacetime generate --out-dir <path-to-unity-project> --lang=csharp\r\n```\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "SpacetimeDB Documentation",
              "route": "spacetimedb-documentation",
              "depth": 1
            },
            {
              "title": "Installation",
              "route": "installation",
              "depth": 2
            },
            {
              "title": "What is SpacetimeDB?",
              "route": "what-is-spacetimedb-",
              "depth": 2
            },
            {
              "title": "State Synchronization",
              "route": "state-synchronization",
              "depth": 2
            },
            {
              "title": "Identities",
              "route": "identities",
              "depth": 2
            },
            {
              "title": "Language Support",
              "route": "language-support",
              "depth": 2
            },
            {
              "title": "Server-side Libraries",
              "route": "server-side-libraries",
              "depth": 3
            },
            {
              "title": "Client-side SDKs",
              "route": "client-side-sdks",
              "depth": 3
            },
            {
              "title": "Unity",
              "route": "unity",
              "depth": 3
            },
            {
              "title": "FAQ",
              "route": "faq",
              "depth": 2
            }
          ],
          "pages": []
        }
      ],
      "previousKey": null,
      "nextKey": {
        "title": "Getting Started",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "Getting Started",
      "identifier": "Getting Started",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Getting%20Started/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "Getting Started",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# Getting Started\r\n\r\nTo develop SpacetimeDB applications locally, you will need to run the Standalone version of the server.\r\n\r\n1. [Install](/install) the SpacetimeDB CLI (Command Line Interface).\r\n2. Run the start command\r\n\r\n```bash\r\nspacetime start\r\n```\r\n\r\nThe server listens on port `3000` by default. You can change this by using the `--listen-addr` option described below.\r\n\r\nSSL is not supported in standalone mode.\r\n\r\nTo set up your CLI to connect to the server, you can run the `spacetime server` command.\r\n\r\n```bash\r\nspacetime server set \"http://localhost:3000\"\r\n```\r\n\r\n## What's Next?\r\n\r\nYou are ready to start developing SpacetimeDB modules. We have a quickstart guide for each supported server-side language:\r\n\r\n- [Rust](/docs/server-languages/rust/rust-module-quickstart-guide)\r\n- [C#](/docs/server-languages/csharp/csharp-module-quickstart-guide)\r\n\r\nThen you can write your client application. We have a quickstart guide for each supported client-side language:\r\n\r\n- [Rust](/docs/client-languages/rust/rust-sdk-quickstart-guide)\r\n- [C#](/docs/client-languages/csharp/csharp-sdk-quickstart-guide)\r\n- [Typescript](/docs/client-languages/typescript/typescript-sdk-quickstart-guide)\r\n- [Python](/docs/client-languages/python/python-sdk-quickstart-guide)\r\n\r\nWe also have a [step-by-step tutorial](/docs/unity-tutorial/unity-tutorial-part-1) for building a multiplayer game in Unity3d.\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "Getting Started",
              "route": "getting-started",
              "depth": 1
            },
            {
              "title": "What's Next?",
              "route": "what-s-next-",
              "depth": 2
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "Overview",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "Cloud Testnet",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "Cloud Testnet",
      "identifier": "Cloud Testnet",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Cloud%20Testnet/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "SpacetimeDB Cloud Deployment",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# SpacetimeDB Cloud Deployment\r\n\r\nThe SpacetimeDB Cloud is a managed cloud service that provides developers an easy way to deploy their SpacetimeDB apps to the cloud.\r\n\r\nCurrently only the `testnet` is available for SpacetimeDB cloud which is subject to wipes. The `mainnet` will be available soon.\r\n\r\n## Deploy via CLI\r\n\r\n1. [Install](/install) the SpacetimeDB CLI.\r\n1. Configure your CLI to use the SpacetimeDB Cloud. To do this, run the `spacetime server` command:\r\n\r\n```bash\r\nspacetime server set \"https://testnet.spacetimedb.com\"\r\n```\r\n\r\n## Connecting your Identity to the Web Dashboard\r\n\r\nBy associating an email with your CLI identity, you can view your published modules on the web dashboard.\r\n\r\n1. Get your identity using the `spacetime identity list` command. Copy it to your clipboard.\r\n1. Connect your email address to your identity using the `spacetime identity set-email` command:\r\n\r\n```bash\r\nspacetime identity set-email <your-identity> <your-email>\r\n```\r\n\r\n1. Open the SpacetimeDB website and log in using your email address.\r\n1. Choose your identity from the dropdown menu.\r\n1. Validate your email address by clicking the link in the email you receive.\r\n1. You should now be able to see your published modules on the web dashboard.\r\n\r\n---\r\n\r\nWith SpacetimeDB Cloud, you benefit from automatic scaling, robust security, and the convenience of not having to manage the hosting environment.\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "SpacetimeDB Cloud Deployment",
              "route": "spacetimedb-cloud-deployment",
              "depth": 1
            },
            {
              "title": "Deploy via CLI",
              "route": "deploy-via-cli",
              "depth": 2
            },
            {
              "title": "Connecting your Identity to the Web Dashboard",
              "route": "connecting-your-identity-to-the-web-dashboard",
              "depth": 2
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "Getting Started",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "Unity Tutorial",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "Unity Tutorial",
      "identifier": "Unity Tutorial",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Unity%20Tutorial/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "Part 1 - Basic Multiplayer",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# Part 1 - Basic Multiplayer\r\n\r\n![UnityTutorial-HeroImage](/images/unity-tutorial/UnityTutorial-HeroImage.JPG)\r\n\r\nThe objective of this tutorial is to help you become acquainted with the basic features of SpacetimeDB. By the end of this tutorial you should have a basic understanding of what SpacetimeDB offers for developers making multiplayer games. It assumes that you have a basic understanding of the Unity Editor, using a command line terminal, and coding.\r\n\r\n## Setting up the Tutorial Unity Project\r\n\r\nIn this section, we will guide you through the process of setting up the Unity Project that will serve as the starting point for our tutorial. By the end of this section, you will have a basic Unity project ready to integrate SpacetimeDB functionality.\r\n\r\n### Step 1: Create a Blank Unity Project\r\n\r\n1. Open Unity and create a new project by selecting \"New\" from the Unity Hub or going to **File -> New Project**.\r\n\r\n![UnityHub-NewProject](/images/unity-tutorial/UnityHub-NewProject.JPG)\r\n\r\n2. Choose a suitable project name and location. For this tutorial, we recommend creating an empty folder for your tutorial project and selecting that as the project location, with the project being named \"Client\".\r\n\r\nThis allows you to have a single subfolder that contains both the Unity project in a folder called \"Client\" and the SpacetimeDB server module in a folder called \"Server\" which we will create later in this tutorial.\r\n\r\nEnsure that you have selected the **3D (URP)** template for this project.\r\n\r\n![UnityHub-3DURP](/images/unity-tutorial/UnityHub-3DURP.JPG)\r\n\r\n3. Click \"Create\" to generate the blank project.\r\n\r\n### Step 2: Adding Required Packages\r\n\r\nTo work with SpacetimeDB and ensure compatibility, we need to add some essential packages to our Unity project. Follow these steps:\r\n\r\n1. Open the Unity Package Manager by going to **Window -> Package Manager**.\r\n2. In the Package Manager window, select the \"Unity Registry\" tab to view unity packages.\r\n3. Search for and install the following package:\r\n   - **Input System**: Enables the use of Unity's new Input system used by this project.\r\n\r\n![PackageManager-InputSystem](/images/unity-tutorial/PackageManager-InputSystem.JPG)\r\n\r\n4. You may need to restart the Unity Editor to switch to the new Input system.\r\n\r\n![PackageManager-Restart](/images/unity-tutorial/PackageManager-Restart.JPG)\r\n\r\n### Step 3: Importing the Tutorial Package\r\n\r\nIn this step, we will import the provided Unity tutorial package that contains the basic single-player game setup. Follow these instructions:\r\n\r\n1. Download the tutorial package from the releases page on GitHub: [https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk/releases/latest](https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk/releases/latest)\r\n2. In Unity, go to **Assets -> Import Package -> Custom Package**.\r\n\r\n![Unity-ImportCustomPackageB](/images/unity-tutorial/Unity-ImportCustomPackageB.JPG)\r\n\r\n3. Browse and select the downloaded tutorial package file.\r\n4. Unity will prompt you with an import settings dialog. Ensure that all the files are selected and click \"Import\" to import the package into your project.\r\n\r\n![Unity-ImportCustomPackage2](/images/unity-tutorial/Unity-ImportCustomPackage2.JPG)\r\n\r\n### Step 4: Running the Project\r\n\r\nNow that we have everything set up, let's run the project and see it in action:\r\n\r\n1. Open the scene named \"Main\" in the Scenes folder provided in the project hierarchy by double-clicking it.\r\n\r\n![Unity-OpenSceneMain](/images/unity-tutorial/Unity-OpenSceneMain.JPG)\r\n\r\nNOTE: When you open the scene you may get a message saying you need to import TMP Essentials. When it appears, click the \"Import TMP Essentials\" button.\r\n\r\n![Unity Import TMP Essentials](/images/unity-tutorial/Unity-ImportTMPEssentials.JPG)\r\n\r\n2. Press the **Play** button located at the top of the Unity Editor.\r\n\r\n![Unity-Play](/images/unity-tutorial/Unity-Play.JPG)\r\n\r\n3. Enter any name and click \"Continue\"\r\n\r\n4. You should see a character loaded in the scene, and you can use the keyboard or mouse controls to move the character around.\r\n\r\nCongratulations! You have successfully set up the basic single-player game project. In the next section, we will start integrating SpacetimeDB functionality to enable multiplayer features.\r\n\r\n## Writing our SpacetimeDB Server Module\r\n\r\n### Step 1: Create the Module\r\n\r\n1. It is important that you already have SpacetimeDB [installed](/install).\r\n\r\n2. Run the SpacetimeDB standalone using the installed CLI. In your terminal or command window, run the following command:\r\n\r\n```bash\r\nspacetime start\r\n```\r\n\r\n3. Make sure your CLI is pointed to your local instance of SpacetimeDB. You can do this by running the following command:\r\n\r\n```bash\r\nspacetime server set http://localhost:3000\r\n```\r\n\r\n4. Open a new command prompt or terminal and navigate to the folder where your Unity project is located using the cd command. For example:\r\n\r\n```bash\r\ncd path/to/tutorial_project_folder\r\n```\r\n\r\n5. Run the following command to initialize the SpacetimeDB server project with Rust as the language:\r\n\r\n```bash\r\nspacetime init --lang=rust ./Server\r\n```\r\n\r\nThis command creates a new folder named \"Server\" within your Unity project directory and sets up the SpacetimeDB server project with Rust as the programming language.\r\n\r\n### Step 2: SpacetimeDB Tables\r\n\r\n1. Using your favorite code editor (we recommend VS Code) open the newly created lib.rs file in the Server folder.\r\n2. Erase everything in the file as we are going to be writing our module from scratch.\r\n\r\n---\r\n\r\n**Understanding ECS**\r\n\r\nECS is a game development architecture that separates game objects into components for better flexibility and performance. You can read more about the ECS design pattern [here](https://en.wikipedia.org/wiki/Entity_component_system).\r\n\r\nWe chose ECS for this example project because it promotes scalability, modularity, and efficient data management, making it ideal for building multiplayer games with SpacetimeDB.\r\n\r\n---\r\n\r\n3. Add the following code to lib.rs.\r\n\r\nWe are going to start by adding the global `Config` table. Right now it only contains the \"message of the day\" but it can be extended to store other configuration variables.\r\n\r\nYou'll notice we have a custom `spacetimedb(table)` attribute that tells SpacetimeDB that this is a SpacetimeDB table. SpacetimeDB automatically generates several functions for us for inserting, updating and querying the table created as a result of this attribute.\r\n\r\nThe `primarykey` attribute on the version not only ensures uniqueness, preventing duplicate values for the column, but also guides the client to determine whether an operation should be an insert or an update. NOTE: Our `version` column in this `Config` table is always 0. This is a trick we use to store\r\nglobal variables that can be accessed from anywhere.\r\n\r\nWe also use the built in rust `derive(Clone)` function to automatically generate a clone function for this struct that we use when updating the row.\r\n\r\n```rust\r\nuse spacetimedb::{spacetimedb, Identity, SpacetimeType, Timestamp, ReducerContext};\r\nuse log;\r\n\r\n#[spacetimedb(table)]\r\n#[derive(Clone)]\r\npub struct Config {\r\n    // Config is a global table with a single row. This table will be used to\r\n    // store configuration or global variables\r\n\r\n    #[primarykey]\r\n    // always 0\r\n    // having a table with a primarykey field which is always zero is a way to store singleton global state\r\n    pub version: u32,\r\n\r\n    pub message_of_the_day: String,\r\n}\r\n\r\n```\r\n\r\nThe next few tables are all components in the ECS system for our spawnable entities. Spawnable Entities are any objects in the game simulation that can have a world location. In this tutorial we will have only one type of spawnable entity, the Player.\r\n\r\nThe first component is the `SpawnableEntityComponent` that allows us to access any spawnable entity in the world by its entity_id. The `autoinc` attribute designates an auto-incrementing column in SpacetimeDB, generating sequential values for new entries. When inserting 0 with this attribute, it gets replaced by the next value in the sequence.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\npub struct SpawnableEntityComponent {\r\n    // All entities that can be spawned in the world will have this component.\r\n    // This allows us to find all objects in the world by iterating through\r\n    // this table. It also ensures that all world objects have a unique\r\n    // entity_id.\r\n\r\n    #[primarykey]\r\n    #[autoinc]\r\n    pub entity_id: u64,\r\n}\r\n```\r\n\r\nThe `PlayerComponent` table connects this entity to a SpacetimeDB identity - a user's \"public key.\" In the context of this tutorial, each user is permitted to have just one Player entity. To guarantee this, we apply the `unique` attribute to the `owner_id` column. If a uniqueness constraint is required on a column aside from the `primarykey`, we make use of the `unique` attribute. This mechanism makes certain that no duplicate values exist within the designated column.\r\n\r\n```rust\r\n#[derive(Clone)]\r\n#[spacetimedb(table)]\r\npub struct PlayerComponent {\r\n    // All players have this component and it associates the spawnable entity\r\n    // with the user's identity. It also stores their username.\r\n\r\n    #[primarykey]\r\n    pub entity_id: u64,\r\n    #[unique]\r\n    pub owner_id: Identity,\r\n\r\n    // username is provided to the create_player reducer\r\n    pub username: String,\r\n    // this value is updated when the user logs in and out\r\n    pub logged_in: bool,\r\n}\r\n```\r\n\r\nThe next component, `MobileLocationComponent`, is used to store the last known location and movement direction for spawnable entities that can move smoothly through the world.\r\n\r\nUsing the `derive(SpacetimeType)` attribute, we define a custom SpacetimeType, StdbVector2, that stores 2D positions. Marking it a `SpacetimeType` allows it to be used in SpacetimeDB columns and reducer calls.\r\n\r\nWe are also making use of the SpacetimeDB `Timestamp` type for the `move_start_timestamp` column. Timestamps represent the elapsed time since the Unix epoch (January 1, 1970, at 00:00:00 UTC) and are not dependent on any specific timezone.\r\n\r\n```rust\r\n#[derive(SpacetimeType, Clone)]\r\npub struct StdbVector2 {\r\n    // A spacetime type which can be used in tables and reducers to represent\r\n    // a 2d position.\r\n    pub x: f32,\r\n    pub z: f32,\r\n}\r\n\r\nimpl StdbVector2 {\r\n    // this allows us to use StdbVector2::ZERO in reducers\r\n    pub const ZERO: StdbVector2 = StdbVector2 { x: 0.0, z: 0.0 };\r\n}\r\n\r\n#[spacetimedb(table)]\r\n#[derive(Clone)]\r\npub struct MobileLocationComponent {\r\n    // This component will be created for all world objects that can move\r\n    // smoothly throughout the world. It keeps track of the position the last\r\n    // time the component was updated and the direction the mobile object is\r\n    // currently moving.\r\n\r\n    #[primarykey]\r\n    pub entity_id: u64,\r\n\r\n    // The last known location of this entity\r\n    pub location: StdbVector2,\r\n    // Movement direction, {0,0} if not moving at all.\r\n    pub direction: StdbVector2,\r\n    // Timestamp when movement started. Timestamp::UNIX_EPOCH if not moving.\r\n    pub move_start_timestamp: Timestamp,\r\n}\r\n```\r\n\r\nNext we write our very first reducer, `create_player`. This reducer is called by the client after the user enters a username.\r\n\r\n---\r\n\r\n**SpacetimeDB Reducers**\r\n\r\n\"Reducer\" is a term coined by SpacetimeDB that \"reduces\" a single function call into one or more database updates performed within a single transaction. Reducers can be called remotely using a client SDK or they can be scheduled to be called at some future time from another reducer call.\r\n\r\n---\r\n\r\nThe first argument to all reducers is the `ReducerContext`. This struct contains: `sender` the identity of the user that called the reducer and `timestamp` which is the `Timestamp` when the reducer was called.\r\n\r\nBefore we begin creating the components for the player entity, we pass the sender identity to the auto-generated function `filter_by_owner_id` to see if there is already a player entity associated with this user's identity. Because the `owner_id` column is unique, the `filter_by_owner_id` function returns a `Option<PlayerComponent>` that we can check to see if a matching row exists.\r\n\r\n---\r\n\r\n**Rust Options**\r\n\r\nRust programs use Option in a similar way to how C#/Unity programs use nullable types. Rust's Option is an enumeration type that represents the possibility of a value being either present (Some) or absent (None), providing a way to handle optional values and avoid null-related errors. For more information, refer to the official Rust documentation: [Rust Option](https://doc.rust-lang.org/std/option/).\r\n\r\n---\r\n\r\nThe first component we create and insert, `SpawnableEntityComponent`, automatically increments the `entity_id` property. When we use the insert function, it returns a result that includes the newly generated `entity_id`. We will utilize this generated `entity_id` in all other components associated with the player entity.\r\n\r\nNote the Result that the insert function returns can fail with a \"DuplicateRow\" error if we insert two rows with the same unique column value. In this example we just use the rust `expect` function to check for this.\r\n\r\n---\r\n\r\n**Rust Results**\r\n\r\nA Result is like an Option where the None is augmented with a value describing the error. Rust programs use Result and return Err in situations where Unity/C# programs would signal an exception. For more information, refer to the official Rust documentation: [Rust Result](https://doc.rust-lang.org/std/result/).\r\n\r\n---\r\n\r\nWe then create and insert our `PlayerComponent` and `MobileLocationComponent` using the same `entity_id`.\r\n\r\nWe use the log crate to write to the module log. This can be viewed using the CLI command `spacetime logs <module-domain-or-address>`. If you add the -f switch it will continuously tail the log.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\npub fn create_player(ctx: ReducerContext, username: String) -> Result<(), String> {\r\n    // This reducer is called when the user logs in for the first time and\r\n    // enters a username\r\n\r\n    let owner_id = ctx.sender;\r\n    // We check to see if there is already a PlayerComponent with this identity.\r\n    // this should never happen because the client only calls it if no player\r\n    // is found.\r\n    if PlayerComponent::filter_by_owner_id(&owner_id).is_some() {\r\n        log::info!(\"Player already exists\");\r\n        return Err(\"Player already exists\".to_string());\r\n    }\r\n\r\n    // Next we create the SpawnableEntityComponent. The entity_id for this\r\n    // component automatically increments and we get it back from the result\r\n    // of the insert call and use it for all components.\r\n\r\n    let entity_id = SpawnableEntityComponent::insert(SpawnableEntityComponent { entity_id: 0 })\r\n        .expect(\"Failed to create player spawnable entity component.\")\r\n        .entity_id;\r\n    // The PlayerComponent uses the same entity_id and stores the identity of\r\n    // the owner, username, and whether or not they are logged in.\r\n    PlayerComponent::insert(PlayerComponent {\r\n        entity_id,\r\n        owner_id,\r\n        username: username.clone(),\r\n        logged_in: true,\r\n    })\r\n    .expect(\"Failed to insert player component.\");\r\n    // The MobileLocationComponent is used to calculate the current position\r\n    // of an entity that can move smoothly in the world. We are using 2d\r\n    // positions and the client will use the terrain height for the y value.\r\n    MobileLocationComponent::insert(MobileLocationComponent {\r\n        entity_id,\r\n        location: StdbVector2::ZERO,\r\n        direction: StdbVector2::ZERO,\r\n        move_start_timestamp: Timestamp::UNIX_EPOCH,\r\n    })\r\n    .expect(\"Failed to insert player mobile entity component.\");\r\n\r\n    log::info!(\"Player created: {}({})\", username, entity_id);\r\n\r\n    Ok(())\r\n}\r\n```\r\n\r\nSpacetimeDB also gives you the ability to define custom reducers that automatically trigger when certain events occur.\r\n\r\n- `init` - Called the very first time you publish your module and anytime you clear the database. We'll learn about publishing a little later.\r\n- `connect` - Called when a user connects to the SpacetimeDB module. Their identity can be found in the `sender` member of the `ReducerContext`.\r\n- `disconnect` - Called when a user disconnects from the SpacetimeDB module.\r\n\r\nNext we are going to write a custom `init` reducer that inserts the default message of the day into our `Config` table. The `Config` table only ever contains a single row with version 0, which we retrieve using `Config::filter_by_version(0)`.\r\n\r\n```rust\r\n#[spacetimedb(init)]\r\npub fn init() {\r\n    // Called when the module is initially published\r\n\r\n\r\n    // Create our global config table.\r\n    Config::insert(Config {\r\n        version: 0,\r\n        message_of_the_day: \"Hello, World!\".to_string(),\r\n    })\r\n    .expect(\"Failed to insert config.\");\r\n}\r\n```\r\n\r\nWe use the `connect` and `disconnect` reducers to update the logged in state of the player. The `update_player_login_state` helper function looks up the `PlayerComponent` row using the user's identity and if it exists, it updates the `logged_in` variable and calls the auto-generated `update` function on `PlayerComponent` to update the row.\r\n\r\n```rust\r\n#[spacetimedb(connect)]\r\npub fn identity_connected(ctx: ReducerContext) {\r\n    // called when the client connects, we update the logged_in state to true\r\n    update_player_login_state(ctx, true);\r\n}\r\n\r\n\r\n#[spacetimedb(disconnect)]\r\npub fn identity_disconnected(ctx: ReducerContext) {\r\n    // Called when the client disconnects, we update the logged_in state to false\r\n    update_player_login_state(ctx, false);\r\n}\r\n\r\n\r\npub fn update_player_login_state(ctx: ReducerContext, logged_in: bool) {\r\n    // This helper function gets the PlayerComponent, sets the logged\r\n    // in variable and updates the SpacetimeDB table row.\r\n    if let Some(player) = PlayerComponent::filter_by_owner_id(&ctx.sender) {\r\n        let entity_id = player.entity_id;\r\n        // We clone the PlayerComponent so we can edit it and pass it back.\r\n        let mut player = player.clone();\r\n        player.logged_in = logged_in;\r\n        PlayerComponent::update_by_entity_id(&entity_id, player);\r\n    }\r\n}\r\n```\r\n\r\nOur final two reducers handle player movement. In `move_player` we look up the `PlayerComponent` using the user identity. If we don't find one, we return an error because the client should not be sending moves without creating a player entity first.\r\n\r\nUsing the `entity_id` in the `PlayerComponent` we retrieved, we can lookup the `MobileLocationComponent` that stores the entity's locations in the world. We update the values passed in from the client and call the auto-generated `update` function.\r\n\r\n---\r\n\r\n**Server Validation**\r\n\r\nIn a fully developed game, the server would typically perform server-side validation on player movements to ensure they comply with game boundaries, rules, and mechanics. This validation, which we omit for simplicity in this tutorial, is essential for maintaining game integrity, preventing cheating, and ensuring a fair gaming experience. Remember to incorporate appropriate server-side validation in your game's development to ensure a secure and fair gameplay environment.\r\n\r\n---\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\npub fn move_player(\r\n    ctx: ReducerContext,\r\n    start: StdbVector2,\r\n    direction: StdbVector2,\r\n) -> Result<(), String> {\r\n    // Update the MobileLocationComponent with the current movement\r\n    // values. The client will call this regularly as the direction of movement\r\n    // changes. A fully developed game should validate these moves on the server\r\n    // before committing them, but that is beyond the scope of this tutorial.\r\n\r\n    let owner_id = ctx.sender;\r\n    // First, look up the player using the sender identity, then use that\r\n    // entity_id to retrieve and update the MobileLocationComponent\r\n    if let Some(player) = PlayerComponent::filter_by_owner_id(&owner_id) {\r\n        if let Some(mut mobile) = MobileLocationComponent::filter_by_entity_id(&player.entity_id) {\r\n            mobile.location = start;\r\n            mobile.direction = direction;\r\n            mobile.move_start_timestamp = ctx.timestamp;\r\n            MobileLocationComponent::update_by_entity_id(&player.entity_id, mobile);\r\n\r\n\r\n            return Ok(());\r\n        }\r\n    }\r\n\r\n\r\n    // If we can not find the PlayerComponent for this user something went wrong.\r\n    // This should never happen.\r\n    return Err(\"Player not found\".to_string());\r\n}\r\n\r\n\r\n#[spacetimedb(reducer)]\r\npub fn stop_player(ctx: ReducerContext, location: StdbVector2) -> Result<(), String> {\r\n    // Update the MobileLocationComponent when a player comes to a stop. We set\r\n    // the location to the current location and the direction to {0,0}\r\n    let owner_id = ctx.sender;\r\n    if let Some(player) = PlayerComponent::filter_by_owner_id(&owner_id) {\r\n        if let Some(mut mobile) = MobileLocationComponent::filter_by_entity_id(&player.entity_id) {\r\n            mobile.location = location;\r\n            mobile.direction = StdbVector2::ZERO;\r\n            mobile.move_start_timestamp = Timestamp::UNIX_EPOCH;\r\n            MobileLocationComponent::update_by_entity_id(&player.entity_id, mobile);\r\n\r\n\r\n            return Ok(());\r\n        }\r\n    }\r\n\r\n\r\n    return Err(\"Player not found\".to_string());\r\n}\r\n```\r\n\r\n4. Now that we've written the code for our server module, we need to publish it to SpacetimeDB. This will create the database and call the init reducer. Make sure your domain name is unique. You will get an error if someone has already created a database with that name. In your terminal or command window, run the following commands.\r\n\r\n```bash\r\ncd Server\r\n\r\nspacetime publish -c yourname-bitcraftmini\r\n```\r\n\r\nIf you get any errors from this command, double check that you correctly entered everything into lib.rs. You can also look at the Troubleshooting section at the end of this tutorial.\r\n\r\n## Updating our Unity Project to use SpacetimeDB\r\n\r\nNow we are ready to connect our bitcraft mini project to SpacetimeDB.\r\n\r\n### Step 1: Import the SDK and Generate Module Files\r\n\r\n1. Add the SpacetimeDB Unity Package using the Package Manager. Open the Package Manager window by clicking on Window -> Package Manager. Click on the + button in the top left corner of the window and select \"Add package from git URL\". Enter the following URL and click Add.\r\n\r\n```bash\r\nhttps://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.git\r\n```\r\n\r\n![Unity-PackageManager](/images/unity-tutorial/Unity-PackageManager.JPG)\r\n\r\n3. The next step is to generate the module specific client files using the SpacetimeDB CLI. The files created by this command provide an interface for retrieving values from the local client cache of the database and for registering for callbacks to events. In your terminal or command window, run the following commands.\r\n\r\n```bash\r\nmkdir -p ../Client/Assets/module_bindings\r\n\r\nspacetime generate --out-dir ../Client/Assets/module_bindings --lang=csharp\r\n```\r\n\r\n### Step 2: Connect to the SpacetimeDB Module\r\n\r\n1. The Unity SpacetimeDB SDK relies on there being a `NetworkManager` somewhere in the scene. Click on the GameManager object in the scene, and in the inspector, add the `NetworkManager` component.\r\n\r\n![Unity-AddNetworkManager](/images/unity-tutorial/Unity-AddNetworkManager.JPG)\r\n\r\n2. Next we are going to connect to our SpacetimeDB module. Open BitcraftMiniGameManager.cs in your editor of choice and add the following code at the top of the file:\r\n\r\n`SpacetimeDB.Types` is the namespace that your generated code is in. You can change this by specifying a namespace in the generate command using `--namespace`.\r\n\r\n```csharp\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n```\r\n\r\n3. Inside the class definition add the following members:\r\n\r\n```csharp\r\n    // These are connection variables that are exposed on the GameManager\r\n    // inspector. The cloud version of SpacetimeDB needs sslEnabled = true\r\n    [SerializeField] private string moduleAddress = \"YOUR_MODULE_DOMAIN_OR_ADDRESS\";\r\n    [SerializeField] private string hostName = \"localhost:3000\";\r\n    [SerializeField] private bool sslEnabled = false;\r\n\r\n    // This is the identity for this player that is automatically generated\r\n    // the first time you log in. We set this variable when the\r\n    // onIdentityReceived callback is triggered by the SDK after connecting\r\n    private Identity local_identity;\r\n```\r\n\r\nThe first three fields will appear in your Inspector so you can update your connection details without editing the code. The `moduleAddress` should be set to the domain you used in the publish command. You should not need to change `hostName` or `sslEnabled` if you are using the standalone version of SpacetimeDB.\r\n\r\n4. Add the following code to the `Start` function. **Be sure to remove the line `UIUsernameChooser.instance.Show();`** since we will call this after we get the local state and find that the player for us.\r\n\r\nIn our `onConnect` callback we are calling `Subscribe` with a list of queries. This tells SpacetimeDB what rows we want in our local client cache. We will also not get row update callbacks or event callbacks for any reducer that does not modify a row that matches these queries.\r\n\r\n---\r\n\r\n**Local Client Cache**\r\n\r\nThe \"local client cache\" is a client-side view of the database, defined by the supplied queries to the Subscribe function. It contains relevant data, allowing efficient access without unnecessary server queries. Accessing data from the client cache is done using the auto-generated iter and filter_by functions for each table, and it ensures that update and event callbacks are limited to the subscribed rows.\r\n\r\n---\r\n\r\n```csharp\r\n        // When we connect to SpacetimeDB we send our subscription queries\r\n        // to tell SpacetimeDB which tables we want to get updates for.\r\n        SpacetimeDBClient.instance.onConnect += () =>\r\n        {\r\n            Debug.Log(\"Connected.\");\r\n\r\n            SpacetimeDBClient.instance.Subscribe(new List<string>()\r\n            {\r\n                \"SELECT * FROM Config\",\r\n                \"SELECT * FROM SpawnableEntityComponent\",\r\n                \"SELECT * FROM PlayerComponent\",\r\n                \"SELECT * FROM MobileLocationComponent\",\r\n            });\r\n        };\r\n\r\n        // called when we have an error connecting to SpacetimeDB\r\n        SpacetimeDBClient.instance.onConnectError += (error, message) =>\r\n        {\r\n            Debug.LogError($\"Connection error: \" + message);\r\n        };\r\n\r\n        // called when we are disconnected from SpacetimeDB\r\n        SpacetimeDBClient.instance.onDisconnect += (closeStatus, error) =>\r\n        {\r\n            Debug.Log(\"Disconnected.\");\r\n        };\r\n\r\n\r\n        // called when we receive the client identity from SpacetimeDB\r\n        SpacetimeDBClient.instance.onIdentityReceived += (token, identity) => {\r\n            AuthToken.SaveToken(token);\r\n            local_identity = identity;\r\n        };\r\n\r\n\r\n        // called after our local cache is populated from a Subscribe call\r\n        SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;\r\n\r\n        // now that we’ve registered all our callbacks, lets connect to\r\n        // spacetimedb\r\n        SpacetimeDBClient.instance.Connect(AuthToken.Token, hostName, moduleAddress, sslEnabled);\r\n```\r\n\r\n5. Next we write the `OnSubscriptionUpdate` callback. When this event occurs for the first time, it signifies that our local client cache is fully populated. At this point, we can verify if a player entity already exists for the corresponding user. If we do not have a player entity, we need to show the `UserNameChooser` dialog so the user can enter a username. We also put the message of the day into the chat window. Finally we unsubscribe from the callback since we only need to do this once.\r\n\r\n```csharp\r\nvoid OnSubscriptionApplied()\r\n{\r\n    // If we don't have any data for our player, then we are creating a\r\n    // new one. Let's show the username dialog, which will then call the\r\n    // create player reducer\r\n    var player = PlayerComponent.FilterByOwnerId(local_identity);\r\n    if (player == null)\r\n    {\r\n       // Show username selection\r\n       UIUsernameChooser.instance.Show();\r\n    }\r\n\r\n    // Show the Message of the Day in our Config table of the Client Cache\r\n    UIChatController.instance.OnChatMessageReceived(\"Message of the Day: \" + Config.FilterByVersion(0).MessageOfTheDay);\r\n\r\n    // Now that we've done this work we can unregister this callback\r\n    SpacetimeDBClient.instance.onSubscriptionApplied -= OnSubscriptionApplied;\r\n}\r\n```\r\n\r\n### Step 3: Adding the Multiplayer Functionality\r\n\r\n1. Now we have to change what happens when you press the \"Continue\" button in the name dialog window. Instead of calling start game like we did in the single player version, we call the `create_player` reducer on the SpacetimeDB module using the auto-generated code. Open `UIUsernameChooser`, **add `using SpacetimeDB.Types;`** at the top of the file, and replace:\r\n\r\n```csharp\r\n    LocalPlayer.instance.username = _usernameField.text;\r\n    BitcraftMiniGameManager.instance.StartGame();\r\n```\r\n\r\nwith:\r\n\r\n```csharp\r\n    // Call the SpacetimeDB CreatePlayer reducer\r\n    Reducer.CreatePlayer(_usernameField.text);\r\n```\r\n\r\n2. We need to create a `RemotePlayer` component that we attach to remote player objects. In the same folder as `LocalPlayer`, create a new C# script called `RemotePlayer`. In the start function, we will register an OnUpdate callback for the `MobileLocationComponent` and query the local cache to get the player’s initial position. **Make sure you include a `using SpacetimeDB.Types;`** at the top of the file.\r\n\r\n```csharp\r\n    public ulong EntityId;\r\n\r\n    public TMP_Text UsernameElement;\r\n\r\n    public string Username { set { UsernameElement.text = value; } }\r\n\r\n    void Start()\r\n    {\r\n        // initialize overhead name\r\n        UsernameElement = GetComponentInChildren<TMP_Text>();\r\n        var canvas = GetComponentInChildren<Canvas>();\r\n        canvas.worldCamera = Camera.main;\r\n\r\n        // get the username from the PlayerComponent for this object and set it in the UI\r\n        PlayerComponent playerComp = PlayerComponent.FilterByEntityId(EntityId);\r\n        Username = playerComp.Username;\r\n\r\n        // get the last location for this player and set the initial\r\n        // position\r\n        MobileLocationComponent mobPos = MobileLocationComponent.FilterByEntityId(EntityId);\r\n        Vector3 playerPos = new Vector3(mobPos.Location.X, 0.0f, mobPos.Location.Z);\r\n        transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);\r\n\r\n        // register for a callback that is called when the client gets an\r\n        // update for a row in the MobileLocationComponent table\r\n        MobileLocationComponent.OnUpdate += MobileLocationComponent_OnUpdate;\r\n    }\r\n```\r\n\r\n3. We now write the `MobileLocationComponent_OnUpdate` callback which sets the movement direction in the `MovementController` for this player. We also set the position to the current location when we stop moving (`DirectionVec` is zero)\r\n\r\n```csharp\r\n    private void MobileLocationComponent_OnUpdate(MobileLocationComponent oldObj, MobileLocationComponent obj, ReducerEvent callInfo)\r\n    {\r\n        // if the update was made to this object\r\n        if(obj.EntityId == EntityId)\r\n        {\r\n            // update the DirectionVec in the PlayerMovementController component with the updated values\r\n            var movementController = GetComponent<PlayerMovementController>();\r\n            movementController.DirectionVec = new Vector3(obj.Direction.X, 0.0f, obj.Direction.Z);\r\n            // if DirectionVec is {0,0,0} then we came to a stop so correct our position to match the server\r\n            if (movementController.DirectionVec == Vector3.zero)\r\n            {\r\n                Vector3 playerPos = new Vector3(obj.Location.X, 0.0f, obj.Location.Z);\r\n                transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);\r\n            }\r\n        }\r\n    }\r\n```\r\n\r\n4. Next we need to handle what happens when a `PlayerComponent` is added to our local cache. We will handle it differently based on if it’s our local player entity or a remote player. We are going to register for the `OnInsert` event for our `PlayerComponent` table. Add the following code to the `Start` function in `BitcraftMiniGameManager`.\r\n\r\n```csharp\r\n    PlayerComponent.OnInsert += PlayerComponent_OnInsert;\r\n```\r\n\r\n5. Create the `PlayerComponent_OnInsert` function which does something different depending on if it's the component for the local player or a remote player. If it's the local player, we set the local player object's initial position and call `StartGame`. If it's a remote player, we instantiate a `PlayerPrefab` with the `RemotePlayer` component. The start function of `RemotePlayer` handles initializing the player position.\r\n\r\n```csharp\r\n    private void PlayerComponent_OnInsert(PlayerComponent obj, ReducerEvent callInfo)\r\n    {\r\n        // if the identity of the PlayerComponent matches our user identity then this is the local player\r\n        if(obj.OwnerId == local_identity)\r\n        {\r\n            // Set the local player username\r\n            LocalPlayer.instance.Username = obj.Username;\r\n\r\n            // Get the MobileLocationComponent for this object and update the position to match the server\r\n            MobileLocationComponent mobPos = MobileLocationComponent.FilterByEntityId(obj.EntityId);\r\n            Vector3 playerPos = new Vector3(mobPos.Location.X, 0.0f, mobPos.Location.Z);\r\n            LocalPlayer.instance.transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);\r\n\r\n            // Now that we have our initial position we can start the game\r\n            StartGame();\r\n        }\r\n        // otherwise this is a remote player\r\n        else\r\n        {\r\n            // spawn the player object and attach the RemotePlayer component\r\n            var remotePlayer = Instantiate(PlayerPrefab);\r\n            remotePlayer.AddComponent<RemotePlayer>().EntityId = obj.EntityId;\r\n        }\r\n    }\r\n```\r\n\r\n6. Next, we need to update the `FixedUpdate` function in `LocalPlayer` to call the `move_player` and `stop_player` reducers using the auto-generated functions. **Don’t forget to add `using SpacetimeDB.Types;`** to LocalPlayer.cs\r\n\r\n```csharp\r\n    private Vector3? lastUpdateDirection;\r\n\r\n    private void FixedUpdate()\r\n    {\r\n        var directionVec = GetDirectionVec();\r\n        PlayerMovementController.Local.DirectionVec = directionVec;\r\n\r\n        // first get the position of the player\r\n        var ourPos = PlayerMovementController.Local.GetModelTransform().position;\r\n        // if we are moving , and we haven't updated our destination yet, or we've moved more than .1 units, update our destination\r\n        if (directionVec.sqrMagnitude != 0 && (!lastUpdateDirection.HasValue || (directionVec - lastUpdateDirection.Value).sqrMagnitude > .1f))\r\n        {\r\n            Reducer.MovePlayer(new StdbVector2() { X = ourPos.x, Z = ourPos.z }, new StdbVector2() { X = directionVec.x, Z = directionVec.z });\r\n            lastUpdateDirection = directionVec;\r\n        }\r\n        // if we stopped moving, send the update\r\n        else if(directionVec.sqrMagnitude == 0 && lastUpdateDirection != null)\r\n        {\r\n            Reducer.StopPlayer(new StdbVector2() { X = ourPos.x, Z = ourPos.z });\r\n            lastUpdateDirection = null;\r\n        }\r\n    }\r\n```\r\n\r\n7. Finally, we need to update our connection settings in the inspector for our GameManager object in the scene. Click on the GameManager in the Hierarchy tab. The the inspector tab you should now see fields for `Module Address`, `Host Name` and `SSL Enabled`. Set the `Module Address` to the name you used when you ran `spacetime publish`. If you don't remember, you can go back to your terminal and run `spacetime publish` again from the `Server` folder.\r\n\r\n![GameManager-Inspector2](/images/unity-tutorial/GameManager-Inspector2.JPG)\r\n\r\n### Step 4: Play the Game!\r\n\r\n1. Go to File -> Build Settings... Replace the SampleScene with the Main scene we have been working in.\r\n\r\n![Unity-AddOpenScenes](/images/unity-tutorial/Unity-AddOpenScenes.JPG)\r\n\r\nWhen you hit the `Build` button, it will kick off a build of the game which will use a different identity than the Unity Editor. Create your character in the build and in the Unity Editor by entering a name and clicking `Continue`. Now you can see each other in game running around the map.\r\n\r\n### Step 5: Implement Player Logout\r\n\r\nSo far we have not handled the `logged_in` variable of the `PlayerComponent`. This means that remote players will not despawn on your screen when they disconnect. To fix this we need to handle the `OnUpdate` event for the `PlayerComponent` table in addition to `OnInsert`. We are going to use a common function that handles any time the `PlayerComponent` changes.\r\n\r\n1. Open `BitcraftMiniGameManager.cs` and add the following code to the `Start` function:\r\n\r\n```csharp\r\n    PlayerComponent.OnUpdate += PlayerComponent_OnUpdate;\r\n```\r\n\r\n2. We are going to add a check to determine if the player is logged for remote players. If the player is not logged in, we search for the RemotePlayer object with the corresponding `EntityId` and destroy it. Add `using System.Linq;` to the top of the file and replace the `PlayerComponent_OnInsert` function with the following code.\r\n\r\n```csharp\r\n    private void PlayerComponent_OnUpdate(PlayerComponent oldValue, PlayerComponent newValue, ReducerEvent dbEvent)\r\n    {\r\n        OnPlayerComponentChanged(newValue);\r\n    }\r\n\r\n    private void PlayerComponent_OnInsert(PlayerComponent obj, ReducerEvent dbEvent)\r\n    {\r\n        OnPlayerComponentChanged(obj);\r\n    }\r\n\r\n    private void OnPlayerComponentChanged(PlayerComponent obj)\r\n    {\r\n        // if the identity of the PlayerComponent matches our user identity then this is the local player\r\n        if (obj.OwnerId == local_identity)\r\n        {\r\n            // Set the local player username\r\n            LocalPlayer.instance.Username = obj.Username;\r\n\r\n            // Get the MobileLocationComponent for this object and update the position to match the server\r\n            MobileLocationComponent mobPos = MobileLocationComponent.FilterByEntityId(obj.EntityId);\r\n            Vector3 playerPos = new Vector3(mobPos.Location.X, 0.0f, mobPos.Location.Z);\r\n            LocalPlayer.instance.transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);\r\n\r\n            // Now that we have our initial position we can start the game\r\n            StartGame();\r\n        }\r\n        // otherwise this is a remote player\r\n        else\r\n        {\r\n            // if the remote player is logged in, spawn it\r\n            if (obj.LoggedIn)\r\n            {\r\n                // spawn the player object and attach the RemotePlayer component\r\n                var remotePlayer = Instantiate(PlayerPrefab);\r\n                remotePlayer.AddComponent<RemotePlayer>().EntityId = obj.EntityId;\r\n            }\r\n            // otherwise we need to look for the remote player object in the scene (if it exists) and destroy it\r\n            else\r\n            {\r\n                var remotePlayer = FindObjectsOfType<RemotePlayer>().FirstOrDefault(item => item.EntityId == obj.EntityId);\r\n                if (remotePlayer != null)\r\n                {\r\n                    Destroy(remotePlayer.gameObject);\r\n                }\r\n            }\r\n        }\r\n    }\r\n```\r\n\r\n3. Now you when you play the game you should see remote players disappear when they log out.\r\n\r\n### Step 6: Add Chat Support\r\n\r\nThe project has a chat window but so far all it's used for is the message of the day. We are going to add the ability for players to send chat messages to each other.\r\n\r\n1. First lets add a new `ChatMessage` table to the SpacetimeDB module. Add the following code to lib.rs.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\npub struct ChatMessage {\r\n    // The primary key for this table will be auto-incremented\r\n    #[primarykey]\r\n    #[autoinc]\r\n    pub chat_entity_id: u64,\r\n\r\n    // The entity id of the player (or NPC) that sent the message\r\n    pub source_entity_id: u64,\r\n    // Message contents\r\n    pub chat_text: String,\r\n    // Timestamp of when the message was sent\r\n    pub timestamp: Timestamp,\r\n}\r\n```\r\n\r\n2. Now we need to add a reducer to handle inserting new chat messages. Add the following code to lib.rs.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\npub fn chat_message(ctx: ReducerContext, message: String) -> Result<(), String> {\r\n    // Add a chat entry to the ChatMessage table\r\n\r\n    // Get the player component based on the sender identity\r\n    let owner_id = ctx.sender;\r\n    if let Some(player) = PlayerComponent::filter_by_owner_id(&owner_id) {\r\n        // Now that we have the player we can insert the chat message using the player entity id.\r\n        ChatMessage::insert(ChatMessage {\r\n            // this column auto-increments so we can set it to 0\r\n            chat_entity_id: 0,\r\n            source_entity_id: player.entity_id,\r\n            chat_text: message,\r\n            timestamp: ctx.timestamp,\r\n        })\r\n        .unwrap();\r\n\r\n        return Ok(());\r\n    }\r\n\r\n    Err(\"Player not found\".into())\r\n}\r\n```\r\n\r\n3. Before updating the client, let's generate the client files and publish our module.\r\n\r\n```bash\r\nspacetime generate --out-dir ../Client/Assets/module_bindings --lang=csharp\r\n\r\nspacetime publish -c yourname-bitcraftmini\r\n```\r\n\r\n4. On the client, let's add code to send the message when the chat button or enter is pressed. Update the `OnChatButtonPress` function in `UIChatController.cs`.\r\n\r\n```csharp\r\npublic void OnChatButtonPress()\r\n{\r\n    Reducer.ChatMessage(_chatInput.text);\r\n    _chatInput.text = \"\";\r\n}\r\n```\r\n\r\n5. Next let's add the `ChatMessage` table to our list of subscriptions.\r\n\r\n```csharp\r\n            SpacetimeDBClient.instance.Subscribe(new List<string>()\r\n            {\r\n                \"SELECT * FROM Config\",\r\n                \"SELECT * FROM SpawnableEntityComponent\",\r\n                \"SELECT * FROM PlayerComponent\",\r\n                \"SELECT * FROM MobileLocationComponent\",\r\n                \"SELECT * FROM ChatMessage\",\r\n            });\r\n```\r\n\r\n6. Now we need to add a reducer to handle inserting new chat messages. First register for the ChatMessage reducer in the `Start` function using the auto-generated function:\r\n\r\n```csharp\r\n        Reducer.OnChatMessageEvent += OnChatMessageEvent;\r\n```\r\n\r\nThen we write the `OnChatMessageEvent` function. We can find the `PlayerComponent` for the player who sent the message using the `Identity` of the sender. Then we get the `Username` and prepend it to the message before sending it to the chat window.\r\n\r\n```csharp\r\n    private void OnChatMessageEvent(ReducerEvent dbEvent, string message)\r\n    {\r\n        var player = PlayerComponent.FilterByOwnerId(dbEvent.Identity);\r\n        if (player != null)\r\n        {\r\n            UIChatController.instance.OnChatMessageReceived(player.Username + \": \" + message);\r\n        }\r\n    }\r\n```\r\n\r\n7. Now when you run the game you should be able to send chat messages to other players. Be sure to make a new Unity client build and run it in a separate window so you can test chat between two clients.\r\n\r\n## Conclusion\r\n\r\nThis concludes the first part of the tutorial. We've learned about the basics of SpacetimeDB and how to use it to create a multiplayer game. In the next part of the tutorial we will add resource nodes to the game and learn about scheduled reducers.\r\n\r\n---\r\n\r\n### Troubleshooting\r\n\r\n- If you get an error when running the generate command, make sure you have an empty subfolder in your Unity project Assets folder called `module_bindings`\r\n\r\n- If you get this exception when running the project:\r\n\r\n```\r\nNullReferenceException: Object reference not set to an instance of an object\r\nBitcraftMiniGameManager.Start () (at Assets/_Project/Game/BitcraftMiniGameManager.cs:26)\r\n```\r\n\r\nCheck to see if your GameManager object in the Scene has the NetworkManager component attached.\r\n\r\n- If you get an error in your Unity console when starting the game, double check your connection settings in the Inspector for the `GameManager` object in the scene.\r\n\r\n```\r\nConnection error: Unable to connect to the remote server\r\n```\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "Part 1 - Basic Multiplayer",
              "route": "part-1-basic-multiplayer",
              "depth": 1
            },
            {
              "title": "Setting up the Tutorial Unity Project",
              "route": "setting-up-the-tutorial-unity-project",
              "depth": 2
            },
            {
              "title": "Step 1: Create a Blank Unity Project",
              "route": "step-1-create-a-blank-unity-project",
              "depth": 3
            },
            {
              "title": "Step 2: Adding Required Packages",
              "route": "step-2-adding-required-packages",
              "depth": 3
            },
            {
              "title": "Step 3: Importing the Tutorial Package",
              "route": "step-3-importing-the-tutorial-package",
              "depth": 3
            },
            {
              "title": "Step 4: Running the Project",
              "route": "step-4-running-the-project",
              "depth": 3
            },
            {
              "title": "Writing our SpacetimeDB Server Module",
              "route": "writing-our-spacetimedb-server-module",
              "depth": 2
            },
            {
              "title": "Step 1: Create the Module",
              "route": "step-1-create-the-module",
              "depth": 3
            },
            {
              "title": "Step 2: SpacetimeDB Tables",
              "route": "step-2-spacetimedb-tables",
              "depth": 3
            },
            {
              "title": "Updating our Unity Project to use SpacetimeDB",
              "route": "updating-our-unity-project-to-use-spacetimedb",
              "depth": 2
            },
            {
              "title": "Step 1: Import the SDK and Generate Module Files",
              "route": "step-1-import-the-sdk-and-generate-module-files",
              "depth": 3
            },
            {
              "title": "Step 2: Connect to the SpacetimeDB Module",
              "route": "step-2-connect-to-the-spacetimedb-module",
              "depth": 3
            },
            {
              "title": "Step 3: Adding the Multiplayer Functionality",
              "route": "step-3-adding-the-multiplayer-functionality",
              "depth": 3
            },
            {
              "title": "Step 4: Play the Game!",
              "route": "step-4-play-the-game-",
              "depth": 3
            },
            {
              "title": "Step 5: Implement Player Logout",
              "route": "step-5-implement-player-logout",
              "depth": 3
            },
            {
              "title": "Step 6: Add Chat Support",
              "route": "step-6-add-chat-support",
              "depth": 3
            },
            {
              "title": "Conclusion",
              "route": "conclusion",
              "depth": 2
            },
            {
              "title": "Troubleshooting",
              "route": "troubleshooting",
              "depth": 3
            }
          ],
          "pages": []
        },
        {
          "title": "Part 2 - Resources and Scheduling",
          "identifier": "Part 2 - Resources And Scheduling",
          "indexIdentifier": "Part 2 - Resources And Scheduling",
          "hasPages": false,
          "content": "# Part 2 - Resources and Scheduling\r\n\r\nIn this second part of the lesson, we'll add resource nodes to our project and learn about scheduled reducers. Then we will spawn the nodes on the client so they are visible to the player.\r\n\r\n## Add Resource Node Spawner\r\n\r\nIn this section we will add functionality to our server to spawn the resource nodes.\r\n\r\n### Step 1: Add the SpacetimeDB Tables for Resource Nodes\r\n\r\n1. Before we start adding code to the server, we need to add the ability to use the rand crate in our SpacetimeDB module so we can generate random numbers. Open the `Cargo.toml` file in the `Server` directory and add the following line to the `[dependencies]` section.\r\n\r\n```toml\r\nrand = \"0.8.5\"\r\n```\r\n\r\nWe also need to add the `getrandom` feature to our SpacetimeDB crate. Update the `spacetimedb` line to:\r\n\r\n```toml\r\nspacetimedb = { \"0.5\", features = [\"getrandom\"] }\r\n```\r\n\r\n2. The first entity component we are adding, `ResourceNodeComponent`, stores the resource type. We'll define an enum to describe a `ResourceNodeComponent`'s type. For now, we'll just have one resource type: Iron. In the future, though, we'll add more resources by adding variants to the `ResourceNodeType` enum. Since we are using a custom enum, we need to mark it with the `SpacetimeType` attribute. Add the following code to lib.rs.\r\n\r\n```rust\r\n#[derive(SpacetimeType, Clone)]\r\npub enum ResourceNodeType {\r\n    Iron,\r\n}\r\n\r\n#[spacetimedb(table)]\r\n#[derive(Clone)]\r\npub struct ResourceNodeComponent {\r\n    #[primarykey]\r\n    pub entity_id: u64,\r\n\r\n    // Resource type of this resource node\r\n    pub resource_type: ResourceNodeType,\r\n}\r\n```\r\n\r\nBecause resource nodes never move, the `MobileEntityComponent` is overkill. Instead, we will add a new entity component named `StaticLocationComponent` that only stores the position and rotation.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\n#[derive(Clone)]\r\npub struct StaticLocationComponent {\r\n    #[primarykey]\r\n    pub entity_id: u64,\r\n\r\n    pub location: StdbVector2,\r\n    pub rotation: f32,\r\n}\r\n```\r\n\r\n3. We are also going to add a couple of additional column to our Config table. `map_extents` let's our spawner know where it can spawn the nodes. `num_resource_nodes` is the maximum number of nodes to spawn on the map. Update the config table in lib.rs.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\npub struct Config {\r\n    // Config is a global table with a single row. This table will be used to\r\n    // store configuration or global variables\r\n\r\n    #[primarykey]\r\n    // always 0\r\n    // having a table with a primarykey field which is always zero is a way to store singleton global state\r\n    pub version: u32,\r\n\r\n    pub message_of_the_day: String,\r\n\r\n    // new variables for resource node spawner\r\n    // X and Z range of the map (-map_extents to map_extents)\r\n    pub map_extents: u32,\r\n    // maximum number of resource nodes to spawn on the map\r\n    pub num_resource_nodes: u32,\r\n}\r\n```\r\n\r\n4. In the `init` reducer, we need to set the initial values of our two new variables. Update the following code:\r\n\r\n```rust\r\n    Config::insert(Config {\r\n        version: 0,\r\n        message_of_the_day: \"Hello, World!\".to_string(),\r\n\r\n        // new variables for resource node spawner\r\n        map_extents: 25,\r\n        num_resource_nodes: 10,\r\n    })\r\n    .expect(\"Failed to insert config.\");\r\n```\r\n\r\n### Step 2: Write our Resource Spawner Repeating Reducer\r\n\r\n1. Add the following code to lib.rs. We are using a special attribute argument called repeat which will automatically schedule the reducer to run every 1000ms.\r\n\r\n```rust\r\n#[spacetimedb(reducer, repeat = 1000ms)]\r\npub fn resource_spawner_agent(_ctx: ReducerContext, _prev_time: Timestamp) -> Result<(), String> {\r\n    let config = Config::filter_by_version(&0).unwrap();\r\n\r\n    // Retrieve the maximum number of nodes we want to spawn from the Config table\r\n    let num_resource_nodes = config.num_resource_nodes as usize;\r\n\r\n    // Count the number of nodes currently spawned and exit if we have reached num_resource_nodes\r\n    let num_resource_nodes_spawned = ResourceNodeComponent::iter().count();\r\n    if num_resource_nodes_spawned >= num_resource_nodes {\r\n        log::info!(\"All resource nodes spawned. Skipping.\");\r\n        return Ok(());\r\n    }\r\n\r\n    // Pick a random X and Z based off the map_extents\r\n    let mut rng = rand::thread_rng();\r\n    let map_extents = config.map_extents as f32;\r\n    let location = StdbVector2 {\r\n        x: rng.gen_range(-map_extents..map_extents),\r\n        z: rng.gen_range(-map_extents..map_extents),\r\n    };\r\n    // Pick a random Y rotation in degrees\r\n    let rotation = rng.gen_range(0.0..360.0);\r\n\r\n    // Insert our SpawnableEntityComponent which assigns us our entity_id\r\n    let entity_id = SpawnableEntityComponent::insert(SpawnableEntityComponent { entity_id: 0 })\r\n        .expect(\"Failed to create resource spawnable entity component.\")\r\n        .entity_id;\r\n\r\n    // Insert our static location with the random position and rotation we selected\r\n    StaticLocationComponent::insert(StaticLocationComponent {\r\n        entity_id,\r\n        location: location.clone(),\r\n        rotation,\r\n    })\r\n    .expect(\"Failed to insert resource static location component.\");\r\n\r\n    // Insert our resource node component, so far we only have iron\r\n    ResourceNodeComponent::insert(ResourceNodeComponent {\r\n        entity_id,\r\n        resource_type: ResourceNodeType::Iron,\r\n    })\r\n    .expect(\"Failed to insert resource node component.\");\r\n\r\n    // Log that we spawned a node with the entity_id and location\r\n    log::info!(\r\n        \"Resource node spawned: {} at ({}, {})\",\r\n        entity_id,\r\n        location.x,\r\n        location.z,\r\n    );\r\n\r\n    Ok(())\r\n}\r\n```\r\n\r\n2. Since this reducer uses `rand::Rng` we need add include it. Add this `use` statement to the top of lib.rs.\r\n\r\n```rust\r\nuse rand::Rng;\r\n```\r\n\r\n3. Even though our reducer is set to repeat, we still need to schedule it the first time. Add the following code to the end of the `init` reducer. You can use this `schedule!` macro to schedule any reducer to run in the future after a certain amount of time.\r\n\r\n```rust\r\n    // Start our resource spawner repeating reducer\r\n    spacetimedb::schedule!(\"1000ms\", resource_spawner_agent(_, Timestamp::now()));\r\n```\r\n\r\n4. Next we need to generate our client code and publish the module. Since we changed the schema we need to make sure we include the `--clear-database` flag. Run the following commands from your Server directory:\r\n\r\n```bash\r\nspacetime generate --out-dir ../Assets/autogen --lang=csharp\r\n\r\nspacetime publish -c yourname/bitcraftmini\r\n```\r\n\r\nYour resource node spawner will start as soon as you publish since we scheduled it to run in our init reducer. You can watch the log output by using the `--follow` flag on the logs CLI command.\r\n\r\n```bash\r\nspacetime logs -f yourname/bitcraftmini\r\n```\r\n\r\n### Step 3: Spawn the Resource Nodes on the Client\r\n\r\n1. First we need to update the `GameResource` component in Unity to work for multiplayer. Open GameResource.cs and add `using SpacetimeDB.Types;` to the top of the file. Then change the variable `Type` to be of type `ResourceNodeType` instead of `int`. Also add a new variable called `EntityId` of type `ulong`.\r\n\r\n```csharp\r\n    public ulong EntityId;\r\n\r\n    public ResourceNodeType Type = ResourceNodeType.Iron;\r\n```\r\n\r\n2. Now that we've changed the `Type` variable, we need to update the code in the `PlayerAnimator` component that references it. Open PlayerAnimator.cs and update the following section of code. We need to add `using SpacetimeDB.Types;` to this file as well. This fixes the compile errors that result from changing the type of the `Type` variable to our new server generated enum.\r\n\r\n```csharp\r\n            var resourceType = res?.Type ?? ResourceNodeType.Iron;\r\n            switch (resourceType)\r\n            {\r\n                case ResourceNodeType.Iron:\r\n                    _animator.SetTrigger(\"Mine\");\r\n                    Interacting = true;\r\n                    break;\r\n                default:\r\n                    Interacting = false;\r\n                    break;\r\n            }\r\n            for (int i = 0; i < _tools.Length; i++)\r\n            {\r\n                _tools[i].SetActive(((int)resourceType) == i);\r\n            }\r\n            _target = res;\r\n```\r\n\r\n3. Now that our `GameResource` is ready to be spawned, lets update the `BitcraftMiniGameManager` component to actually create them. First, we need to add the new tables to our SpacetimeDB subscription. Open BitcraftMiniGameManager.cs and update the following code:\r\n\r\n```csharp\r\n            SpacetimeDBClient.instance.Subscribe(new List<string>()\r\n            {\r\n                \"SELECT * FROM Config\",\r\n                \"SELECT * FROM SpawnableEntityComponent\",\r\n                \"SELECT * FROM PlayerComponent\",\r\n                \"SELECT * FROM MobileEntityComponent\",\r\n                // Our new tables for part 2 of the tutorial\r\n                \"SELECT * FROM ResourceNodeComponent\",\r\n                \"SELECT * FROM StaticLocationComponent\"\r\n            });\r\n```\r\n\r\n4. Next let's add an `OnInsert` handler for the `ResourceNodeComponent`. Add the following line to the `Start` function.\r\n\r\n```csharp\r\n        ResourceNodeComponent.OnInsert += ResourceNodeComponent_OnInsert;\r\n```\r\n\r\n5. Finally we add the new function to handle the insert event. This function will be called whenever a new `ResourceNodeComponent` is inserted into our local client cache. We can use this to spawn the resource node in the world. Add the following code to the `BitcraftMiniGameManager` class.\r\n\r\nTo get the position and the rotation of the node, we look up the `StaticLocationComponent` for this entity by using the EntityId.\r\n\r\n```csharp\r\n    private void ResourceNodeComponent_OnInsert(ResourceNodeComponent insertedValue, ReducerEvent callInfo)\r\n    {\r\n        switch(insertedValue.ResourceType)\r\n        {\r\n            case ResourceNodeType.Iron:\r\n                var iron = Instantiate(IronPrefab);\r\n                StaticLocationComponent loc = StaticLocationComponent.FilterByEntityId(insertedValue.EntityId);\r\n                Vector3 nodePos = new Vector3(loc.Location.X, 0.0f, loc.Location.Z);\r\n                iron.transform.position = new Vector3(nodePos.x, MathUtil.GetTerrainHeight(nodePos), nodePos.z);\r\n                iron.transform.rotation = Quaternion.Euler(0.0f, loc.Rotation, 0.0f);\r\n                break;\r\n        }\r\n    }\r\n```\r\n\r\n### Step 4: Play the Game!\r\n\r\n6. Hit Play in the Unity Editor and you should now see your resource nodes spawning in the world!\r\n",
          "editUrl": "Part%202%20-%20Resources%20And%20Scheduling.md",
          "jumpLinks": [
            {
              "title": "Part 2 - Resources and Scheduling",
              "route": "part-2-resources-and-scheduling",
              "depth": 1
            },
            {
              "title": "Add Resource Node Spawner",
              "route": "add-resource-node-spawner",
              "depth": 2
            },
            {
              "title": "Step 1: Add the SpacetimeDB Tables for Resource Nodes",
              "route": "step-1-add-the-spacetimedb-tables-for-resource-nodes",
              "depth": 3
            },
            {
              "title": "Step 2: Write our Resource Spawner Repeating Reducer",
              "route": "step-2-write-our-resource-spawner-repeating-reducer",
              "depth": 3
            },
            {
              "title": "Step 3: Spawn the Resource Nodes on the Client",
              "route": "step-3-spawn-the-resource-nodes-on-the-client",
              "depth": 3
            },
            {
              "title": "Step 4: Play the Game!",
              "route": "step-4-play-the-game-",
              "depth": 3
            }
          ],
          "pages": []
        },
        {
          "title": "Part 3 - BitCraft Mini",
          "identifier": "Part 3 - BitCraft Mini",
          "indexIdentifier": "Part 3 - BitCraft Mini",
          "hasPages": false,
          "content": "# Part 3 - BitCraft Mini\r\n\r\nBitCraft Mini is a game that we developed which extends the code you've already developed in this tutorial. It is inspired by our game [BitCraft](https://bitcraftonline.com) and illustrates how you could build a more complex game from just the components we've discussed. Right now you can walk around, mine ore, and manage your inventory.\r\n\r\n## 1. Download\r\n\r\nYou can git-clone BitCraftMini from here:\r\n\r\n```plaintext\r\ngit clone ssh://git@github.com/clockworklabs/BitCraftMini\r\n```\r\n\r\nOnce you have downloaded BitCraftMini, you will need to compile the spacetime module.\r\n\r\n## 2. Compile the Spacetime Module\r\n\r\nIn order to compile the BitCraftMini module, you will need to install cargo. You can install cargo from here:\r\n\r\n> https://www.rust-lang.org/tools/install\r\n\r\nOnce you have cargo installed, you can compile and publish the module with these commands:\r\n\r\n```bash\r\ncd BitCraftMini/Server\r\nspacetime publish\r\n```\r\n\r\n`spacetime publish` will output an address where your module has been deployed to. You will want to copy/save this address because you will need it in step 3. Here is an example of what it should look like:\r\n\r\n```plaintext\r\n$ spacetime publish\r\ninfo: component 'rust-std' for target 'wasm32-unknown-unknown' is up to date\r\n    Finished release [optimized] target(s) in 0.03s\r\nPublish finished successfully.\r\nCreated new database with address: c91c17ecdcea8a05302be2bad9dd59b3\r\n```\r\n\r\nOptionally, you can specify a name when you publish the module:\r\n\r\n```bash\r\nspacetime publish \"unique-module-name\"\r\n```\r\n\r\nCurrently, all the named modules exist in the same namespace so if you get a message saying that database is not owned by you, it means that someone else has already published a module with that name. You can either choose a different name or you can use the address instead. If you specify a name when you publish, you can use that name in place of the autogenerated address in both the CLI and in the Unity client.\r\n\r\nIn the BitCraftMini module we have a function called `initialize()`. This function should be called immediately after publishing the module to spacetimedb. This function is in charge of generating some initial settings that are required for the server to operate. You can call this function like so:\r\n\r\n```bash\r\nspacetime call \"<YOUR DATABASE ADDRESS>\" \"initialize\" \"[]\"\r\n```\r\n\r\nHere we are telling spacetime to invoke the `initialize()` function on our module \"bitcraftmini\". If the function had some arguments, we would json encode them and put them into the \"[]\". Since `initialize()` requires no parameters, we just leave it empty.\r\n\r\nAfter you have called `initialize()` on the spacetime module you shouldgenerate the client files:\r\n\r\n```bash\r\nspacetime generate --out-dir ../Client/Assets/_Project/autogen --lang=cs\r\n```\r\n\r\nHere is some sample output:\r\n\r\n```plaintext\r\n$ spacetime generate --out-dir ../Client/Assets/_Project/autogen --lang cs\r\ninfo: component 'rust-std' for target 'wasm32-unknown-unknown' is up to date\r\n    Finished release [optimized] target(s) in 0.03s\r\ncompilation took 234.613518ms\r\nGenerate finished successfully.\r\n```\r\n\r\nIf you've gotten this message then everything should be working properly so far.\r\n\r\n## 3. Replace address in BitCraftMiniGameManager\r\n\r\nThe following settings are exposed in the `BitCraftMiniGameManager` inspector: Module Address, Host Name, and SSL Enabled.\r\n\r\nOpen the Main scene in Unity and click on the `GameManager` object in the heirarchy. The inspector window will look like this:\r\n\r\n![GameManager-Inspector](/images/unity-tutorial/GameManager-Inspector.JPG)\r\n\r\nUpdate the module address with the address you got from the `spacetime publish` command. If you are using SpacetimeDB Cloud `testnet`, the host name should be `testnet.spacetimedb.com` and SSL Enabled should be checked. If you are running SpacetimeDB Standalone locally, the host name should be `localhost:3000` and SSL Enabled should be unchecked. For instructions on how to deploy to these environments, see the [Deployment Section](/docs/DeploymentOverview.md)\r\n\r\n## 4. Play Mode\r\n\r\nYou should now be able to enter play mode and walk around! You can mine some rocks, cut down some trees and if you connect more clients you can trade with other players.\r\n\r\n## 5. Editing the Module\r\n\r\nIf you want to make further updates to the module, make sure to use this publish command instead:\r\n\r\n```bash\r\nspacetime publish <YOUR DATABASE ADDRESS>\r\n```\r\n\r\nWhere `<YOUR DATABASE ADDRESS>` is your own address. If you do this instead then you won't have to change the address inside of `BitCraftMiniGameManager.cs`\r\n\r\nWhen you change the server module you should also regenerate the client files as well:\r\n\r\n```bash\r\nspacetime generate --out-dir ../Client/Assets/_Project/autogen --lang=cs\r\n```\r\n\r\nYou may want to consider putting these 2 commands into a simple shell script to make the process a bit cleaner.\r\n",
          "editUrl": "Part%203%20-%20BitCraft%20Mini.md",
          "jumpLinks": [
            {
              "title": "Part 3 - BitCraft Mini",
              "route": "part-3-bitcraft-mini",
              "depth": 1
            },
            {
              "title": "1. Download",
              "route": "1-download",
              "depth": 2
            },
            {
              "title": "2. Compile the Spacetime Module",
              "route": "2-compile-the-spacetime-module",
              "depth": 2
            },
            {
              "title": "3. Replace address in BitCraftMiniGameManager",
              "route": "3-replace-address-in-bitcraftminigamemanager",
              "depth": 2
            },
            {
              "title": "4. Play Mode",
              "route": "4-play-mode",
              "depth": 2
            },
            {
              "title": "5. Editing the Module",
              "route": "5-editing-the-module",
              "depth": 2
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "Cloud Testnet",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "Server Module Languages",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "Server Module Languages",
      "identifier": "Server Module Languages",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Server%20Module%20Languages/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "C#",
          "identifier": "C#",
          "indexIdentifier": "index",
          "comingSoon": false,
          "tag": "Expiremental",
          "hasPages": true,
          "editUrl": "C%23/index.md",
          "jumpLinks": [],
          "pages": [
            {
              "title": "C# Module Quickstart",
              "identifier": "index",
              "indexIdentifier": "index",
              "content": "# C# Module Quickstart\r\n\r\nIn this tutorial, we'll implement a simple chat server as a SpacetimeDB module.\r\n\r\nA SpacetimeDB module is code that gets compiled to WebAssembly and is uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with the Spacetime relational database.\r\n\r\nEach SpacetimeDB module defines a set of tables and a set of reducers.\r\n\r\nEach table is defined as a C# `class` annotated with `[SpacetimeDB.Table]`, where an instance represents a row, and each field represents a column.\r\n\r\nA reducer is a function which traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. In C#, reducers are defined as functions annotated with `[SpacetimeDB.Reducer]`. If an exception is thrown, the reducer call fails, the database is not updated, and a failed message is reported to the client.\r\n\r\n## Install SpacetimeDB\r\n\r\nIf you haven't already, start by [installing SpacetimeDB](/install). This will install the `spacetime` command line interface (CLI), which contains all the functionality for interacting with SpacetimeDB.\r\n\r\n## Install .NET\r\n\r\nNext we need to [install .NET](https://dotnet.microsoft.com/en-us/download/dotnet) so that we can build and publish our module.\r\n\r\n## Project structure\r\n\r\nCreate and enter a directory `quickstart-chat`:\r\n\r\n```bash\r\nmkdir quickstart-chat\r\ncd quickstart-chat\r\n```\r\n\r\nNow create `server`, our module, which runs in the database:\r\n\r\n```bash\r\nspacetime init --lang csharp server\r\n```\r\n\r\n## Declare imports\r\n\r\n`spacetime init` should have pre-populated `server/Lib.cs` with a trivial module. Clear it out, so we can write a module that's still pretty simple: a bare-bones chat server.\r\n\r\nTo the top of `server/Lib.cs`, add some imports we'll be using:\r\n\r\n```C#\r\nusing System.Runtime.CompilerServices;\r\nusing SpacetimeDB.Module;\r\nusing static SpacetimeDB.Runtime;\r\n```\r\n\r\n- `System.Runtime.CompilerServices` allows us to use the `ModuleInitializer` attribute, which we'll use to register our `OnConnect` and `OnDisconnect` callbacks.\r\n- `SpacetimeDB.Module` contains the special attributes we'll use to define our module.\r\n- `SpacetimeDB.Runtime` contains the raw API bindings SpacetimeDB uses to communicate with the database.\r\n\r\nWe also need to create our static module class which all of the module code will live in. In `server/Lib.cs`, add:\r\n\r\n```csharp\r\nstatic partial class Module\r\n{\r\n}\r\n```\r\n\r\n## Define tables\r\n\r\nTo get our chat server running, we'll need to store two kinds of data: information about each user, and records of all the messages that have been sent.\r\n\r\nFor each `User`, we'll store the `Identity` of their client connection, an optional name they can set to identify themselves to other users, and whether they're online or not. We'll designate the `Identity` as our primary key, which enforces that it must be unique, indexes it for faster lookup, and allows clients to track updates.\r\n\r\nIn `server/Lib.cs`, add the definition of the table `User` to the `Module` class:\r\n\r\n```C#\r\n    [SpacetimeDB.Table]\r\n    public partial class User\r\n    {\r\n        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]\r\n        public Identity Identity;\r\n        public string? Name;\r\n        public bool Online;\r\n    }\r\n```\r\n\r\nFor each `Message`, we'll store the `Identity` of the user who sent it, the `Timestamp` when it was sent, and the text of the message.\r\n\r\nIn `server/Lib.cs`, add the definition of the table `Message` to the `Module` class:\r\n\r\n```C#\r\n    [SpacetimeDB.Table]\r\n    public partial class Message\r\n    {\r\n        public Identity Sender;\r\n        public long Sent;\r\n        public string Text = \"\";\r\n    }\r\n```\r\n\r\n## Set users' names\r\n\r\nWe want to allow users to set their names, because `Identity` is not a terribly user-friendly identifier. To that effect, we define a reducer `SetName` which clients can invoke to set their `User.Name`. It will validate the caller's chosen name, using a function `ValidateName` which we'll define next, then look up the `User` record for the caller and update it to store the validated name. If the name fails the validation, the reducer will fail.\r\n\r\nEach reducer may accept as its first argument a `DbEventArgs`, which includes the `Identity` of the client that called the reducer, and the `Timestamp` when it was invoked. For now, we only need the `Identity`, `dbEvent.Sender`.\r\n\r\nIt's also possible to call `SetName` via the SpacetimeDB CLI's `spacetime call` command without a connection, in which case no `User` record will exist for the caller. We'll return an error in this case, but you could alter the reducer to insert a `User` row for the module owner. You'll have to decide whether the module owner is always online or always offline, though.\r\n\r\nIn `server/Lib.cs`, add to the `Module` class:\r\n\r\n```C#\r\n    [SpacetimeDB.Reducer]\r\n    public static void SetName(DbEventArgs dbEvent, string name)\r\n    {\r\n        name = ValidateName(name);\r\n\r\n        var user = User.FindByIdentity(dbEvent.Sender);\r\n        if (user is not null)\r\n        {\r\n            user.Name = name;\r\n            User.UpdateByIdentity(dbEvent.Sender, user);\r\n        }\r\n    }\r\n```\r\n\r\nFor now, we'll just do a bare minimum of validation, rejecting the empty name. You could extend this in various ways, like:\r\n\r\n- Comparing against a blacklist for moderation purposes.\r\n- Unicode-normalizing names.\r\n- Rejecting names that contain non-printable characters, or removing characters or replacing them with a placeholder.\r\n- Rejecting or truncating long names.\r\n- Rejecting duplicate names.\r\n\r\nIn `server/Lib.cs`, add to the `Module` class:\r\n\r\n```C#\r\n    /// Takes a name and checks if it's acceptable as a user's name.\r\n    public static string ValidateName(string name)\r\n    {\r\n        if (string.IsNullOrEmpty(name))\r\n        {\r\n            throw new Exception(\"Names must not be empty\");\r\n        }\r\n        return name;\r\n    }\r\n```\r\n\r\n## Send messages\r\n\r\nWe define a reducer `SendMessage`, which clients will call to send messages. It will validate the message's text, then insert a new `Message` record using `Message.Insert`, with the `Sender` identity and `Time` timestamp taken from the `DbEventArgs`.\r\n\r\nIn `server/Lib.cs`, add to the `Module` class:\r\n\r\n```C#\r\n    [SpacetimeDB.Reducer]\r\n    public static void SendMessage(DbEventArgs dbEvent, string text)\r\n    {\r\n        text = ValidateMessage(text);\r\n        Log(text);\r\n        new Message\r\n        {\r\n            Sender = dbEvent.Sender,\r\n            Text = text,\r\n            Sent = dbEvent.Time.ToUnixTimeMilliseconds(),\r\n        }.Insert();\r\n    }\r\n```\r\n\r\nWe'll want to validate messages' texts in much the same way we validate users' chosen names. As above, we'll do the bare minimum, rejecting only empty messages.\r\n\r\nIn `server/Lib.cs`, add to the `Module` class:\r\n\r\n```C#\r\n    /// Takes a message's text and checks if it's acceptable to send.\r\n    public static string ValidateMessage(string text)\r\n    {\r\n        if (string.IsNullOrEmpty(text))\r\n        {\r\n            throw new ArgumentException(\"Messages must not be empty\");\r\n        }\r\n        return text;\r\n    }\r\n```\r\n\r\nYou could extend the validation in `ValidateMessage` in similar ways to `ValidateName`, or add additional checks to `SendMessage`, like:\r\n\r\n- Rejecting messages from senders who haven't set their names.\r\n- Rate-limiting users so they can't send new messages too quickly.\r\n\r\n## Set users' online status\r\n\r\nIn C# modules, you can register for OnConnect and OnDisconnect events in a special initializer function that uses the attribute `ModuleInitializer`. We'll use the `OnConnect` event to create a `User` record for the client if it doesn't yet exist, and to set its online status.\r\n\r\nWe'll use `User.FilterByOwnerIdentity` to look up a `User` row for `dbEvent.Sender`, if one exists. If we find one, we'll use `User.UpdateByOwnerIdentity` to overwrite it with a row that has `Online: true`. If not, we'll use `User.Insert` to insert a new row for our new user. All three of these methods are generated by the `[SpacetimeDB.Table]` attribute, with rows and behavior based on the row attributes. `FilterByOwnerIdentity` returns a nullable `User`, because the unique constraint from the `[SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]` attribute means there will be either zero or one matching rows. `Insert` will throw an exception if the insert violates this constraint; if we want to overwrite a `User` row, we need to do so explicitly using `UpdateByOwnerIdentity`.\r\n\r\nIn `server/Lib.cs`, add the definition of the connect reducer to the `Module` class:\r\n\r\n```C#\r\n    [ModuleInitializer]\r\n    public static void Init()\r\n    {\r\n        OnConnect += (dbEventArgs) =>\r\n        {\r\n            Log($\"Connect {dbEventArgs.Sender}\");\r\n            var user = User.FindByIdentity(dbEventArgs.Sender);\r\n\r\n            if (user is not null)\r\n            {\r\n                // If this is a returning user, i.e., we already have a `User` with this `Identity`,\r\n                // set `Online: true`, but leave `Name` and `Identity` unchanged.\r\n                user.Online = true;\r\n                User.UpdateByIdentity(dbEventArgs.Sender, user);\r\n            }\r\n            else\r\n            {\r\n                // If this is a new user, create a `User` object for the `Identity`,\r\n                // which is online, but hasn't set a name.\r\n                new User\r\n                {\r\n                    Name = null,\r\n                    Identity = dbEventArgs.Sender,\r\n                    Online = true,\r\n                }.Insert();\r\n            }\r\n        };\r\n    }\r\n```\r\n\r\nSimilarly, whenever a client disconnects, the module will execute the `OnDisconnect` event if it's registered. We'll use it to un-set the `Online` status of the `User` for the disconnected client.\r\n\r\nAdd the following code after the `OnConnect` lambda:\r\n\r\n```C#\r\n        OnDisconnect += (dbEventArgs) =>\r\n        {\r\n            var user = User.FindByIdentity(dbEventArgs.Sender);\r\n\r\n            if (user is not null)\r\n            {\r\n                // This user should exist, so set `Online: false`.\r\n                user.Online = false;\r\n                User.UpdateByIdentity(dbEventArgs.Sender, user);\r\n            }\r\n            else\r\n            {\r\n                // User does not exist, log warning\r\n                Log($\"Warning: No user found for disconnected client.\");\r\n            }\r\n        };\r\n```\r\n\r\n## Publish the module\r\n\r\nAnd that's all of our module code! We'll run `spacetime publish` to compile our module and publish it on SpacetimeDB. `spacetime publish` takes an optional name which will map to the database's unique address. Clients can connect either by name or by address, but names are much more pleasant. Come up with a unique name, and fill it in where we've written `<module-name>`.\r\n\r\nFrom the `quickstart-chat` directory, run:\r\n\r\n```bash\r\nspacetime publish --project-path server <module-name>\r\n```\r\n\r\n## Call Reducers\r\n\r\nYou can use the CLI (command line interface) to run reducers. The arguments to the reducer are passed in JSON format.\r\n\r\n```bash\r\nspacetime call <module-name> send_message '[\"Hello, World!\"]'\r\n```\r\n\r\nOnce we've called our `send_message` reducer, we can check to make sure it ran by running the `logs` command.\r\n\r\n```bash\r\nspacetime logs <module-name>\r\n```\r\n\r\nYou should now see the output that your module printed in the database.\r\n\r\n```bash\r\ninfo: Hello, World!\r\n```\r\n\r\n## SQL Queries\r\n\r\nSpacetimeDB supports a subset of the SQL syntax so that you can easily query the data of your database. We can run a query using the `sql` command.\r\n\r\n```bash\r\nspacetime sql <module-name> \"SELECT * FROM Message\"\r\n```\r\n\r\n```bash\r\n text\r\n---------\r\n \"Hello, World!\"\r\n```\r\n\r\n## What's next?\r\n\r\nYou've just set up your first database in SpacetimeDB! The next step would be to create a client module that interacts with this module. You can use any of SpacetimDB's supported client languages to do this. Take a look at the quick start guide for your client language of choice: [Rust](/docs/languages/rust/rust-sdk-quickstart-guide), [C#](/docs/languages/csharp/csharp-sdk-quickstart-guide), [TypeScript](/docs/languages/typescript/typescript-sdk-quickstart-guide) or [Python](/docs/languages/python/python-sdk-quickstart-guide).\r\n\r\nIf you are planning to use SpacetimeDB with the Unity3d game engine, you can skip right to the [Unity Comprehensive Tutorial](/docs/game-dev/unity-tutorial) or check out our example game, [BitcraftMini](/docs/game-dev/unity-tutorial-bitcraft-mini).\r\n",
              "hasPages": false,
              "editUrl": "index.md",
              "jumpLinks": [
                {
                  "title": "C# Module Quickstart",
                  "route": "c-module-quickstart",
                  "depth": 1
                },
                {
                  "title": "Install SpacetimeDB",
                  "route": "install-spacetimedb",
                  "depth": 2
                },
                {
                  "title": "Install .NET",
                  "route": "install-net",
                  "depth": 2
                },
                {
                  "title": "Project structure",
                  "route": "project-structure",
                  "depth": 2
                },
                {
                  "title": "Declare imports",
                  "route": "declare-imports",
                  "depth": 2
                },
                {
                  "title": "Define tables",
                  "route": "define-tables",
                  "depth": 2
                },
                {
                  "title": "Set users' names",
                  "route": "set-users-names",
                  "depth": 2
                },
                {
                  "title": "Send messages",
                  "route": "send-messages",
                  "depth": 2
                },
                {
                  "title": "Set users' online status",
                  "route": "set-users-online-status",
                  "depth": 2
                },
                {
                  "title": "Publish the module",
                  "route": "publish-the-module",
                  "depth": 2
                },
                {
                  "title": "Call Reducers",
                  "route": "call-reducers",
                  "depth": 2
                },
                {
                  "title": "SQL Queries",
                  "route": "sql-queries",
                  "depth": 2
                },
                {
                  "title": "What's next?",
                  "route": "what-s-next-",
                  "depth": 2
                }
              ],
              "pages": []
            },
            {
              "title": "SpacetimeDB C# Modules",
              "identifier": "ModuleReference",
              "indexIdentifier": "ModuleReference",
              "hasPages": false,
              "content": "# SpacetimeDB C# Modules\r\n\r\nYou can use the [C# SpacetimeDB library](https://github.com/clockworklabs/SpacetimeDBLibCSharp) to write modules in C# which interact with the SpacetimeDB database.\r\n\r\nIt uses [Roslyn incremental generators](https://github.com/dotnet/roslyn/blob/main/docs/features/incremental-generators.md) to add extra static methods to types, tables and reducers marked with special attributes and registers them with the database runtime.\r\n\r\n## Example\r\n\r\nLet's start with a heavily commented version of the default example from the landing page:\r\n\r\n```csharp\r\n// These imports bring into the scope common APIs you'll need to expose items from your module and to interact with the database runtime.\r\nusing SpacetimeDB.Module;\r\nusing static SpacetimeDB.Runtime;\r\n\r\n// Roslyn generators are statically generating extra code as-if they were part of the source tree, so,\r\n// in order to inject new methods, types they operate on as well as their parents have to be marked as `partial`.\r\n//\r\n// We start with the top-level `Module` class for the module itself.\r\nstatic partial class Module\r\n{\r\n    // `[SpacetimeDB.Table]` registers a struct or a class as a SpacetimeDB table.\r\n    //\r\n    // It generates methods to insert, filter, update, and delete rows of the given type in the table.\r\n    [SpacetimeDB.Table]\r\n    public partial struct Person\r\n    {\r\n        // `[SpacetimeDB.Column]` allows to specify column attributes / constraints such as\r\n        // \"this field should be unique\" or \"this field should get automatically assigned auto-incremented value\".\r\n        [SpacetimeDB.Column(ColumnAttrs.Unique | ColumnAttrs.AutoInc)]\r\n        public int Id;\r\n        public string Name;\r\n        public int Age;\r\n    }\r\n\r\n    // `[SpacetimeDB.Reducer]` marks a static method as a SpacetimeDB reducer.\r\n    //\r\n    // Reducers are functions that can be invoked from the database runtime.\r\n    // They can't return values, but can throw errors that will be caught and reported back to the runtime.\r\n    [SpacetimeDB.Reducer]\r\n    public static void Add(string name, int age)\r\n    {\r\n        // We can skip (or explicitly set to zero) auto-incremented fields when creating new rows.\r\n        var person = new Person { Name = name, Age = age };\r\n        // `Insert()` method is auto-generated and will insert the given row into the table.\r\n        person.Insert();\r\n        // After insertion, the auto-incremented fields will be populated with their actual values.\r\n        //\r\n        // `Log()` function is provided by the runtime and will print the message to the database log.\r\n        // It should be used instead of `Console.WriteLine()` or similar functions.\r\n        Log($\"Inserted {person.Name} under #{person.Id}\");\r\n    }\r\n\r\n    [SpacetimeDB.Reducer]\r\n    public static void SayHello()\r\n    {\r\n        // Each table type gets a static Iter() method that can be used to iterate over the entire table.\r\n        foreach (var person in Person.Iter())\r\n        {\r\n            Log($\"Hello, {person.Name}!\");\r\n        }\r\n        Log(\"Hello, World!\");\r\n    }\r\n}\r\n```\r\n\r\n## API reference\r\n\r\nNow we'll get into details on all the APIs SpacetimeDB provides for writing modules in C#.\r\n\r\n### Logging\r\n\r\nFirst of all, logging as we're likely going to use it a lot for debugging and reporting errors.\r\n\r\n`SpacetimeDB.Runtime` provides a `Log` function that will print the given message to the database log, along with the source location and a log level it was provided.\r\n\r\nSupported log levels are provided by the `LogLevel` enum:\r\n\r\n```csharp\r\npublic enum LogLevel\r\n{\r\n    Error,\r\n    Warn,\r\n    Info,\r\n    Debug,\r\n    Trace,\r\n    Panic\r\n}\r\n```\r\n\r\nIf omitted, the log level will default to `Info`, so these two forms are equivalent:\r\n\r\n```csharp\r\nLog(\"Hello, World!\");\r\nLog(\"Hello, World!\", LogLevel.Info);\r\n```\r\n\r\n### Supported types\r\n\r\n#### Built-in types\r\n\r\nThe following types are supported out of the box and can be stored in the database tables directly or as part of more complex types:\r\n\r\n- `bool`\r\n- `byte`, `sbyte`\r\n- `short`, `ushort`\r\n- `int`, `uint`\r\n- `long`, `ulong`\r\n- `float`, `double`\r\n- `string`\r\n- [`Int128`](https://learn.microsoft.com/en-us/dotnet/api/system.int128), [`UInt128`](https://learn.microsoft.com/en-us/dotnet/api/system.uint128)\r\n- `T[]` - arrays of supported values.\r\n- [`List<T>`](https://learn.microsoft.com/en-us/dotnet/api/system.collections.generic.list-1)\r\n- [`Dictionary<TKey, TValue>`](https://learn.microsoft.com/en-us/dotnet/api/system.collections.generic.dictionary-2)\r\n\r\nAnd a couple of special custom types:\r\n\r\n- `SpacetimeDB.SATS.Unit` - semantically equivalent to an empty struct, sometimes useful in generic contexts where C# doesn't permit `void`.\r\n- `Identity` (`SpacetimeDB.Runtime.Identity`) - a unique identifier for each connected client; internally a byte blob but can be printed, hashed and compared for equality.\r\n\r\n#### Custom types\r\n\r\n`[SpacetimeDB.Type]` attribute can be used on any `struct`, `class` or an `enum` to mark it as a SpacetimeDB type. It will implement serialization and deserialization for values of this type so that they can be stored in the database.\r\n\r\nAny `struct` or `class` marked with this attribute, as well as their respective parents, must be `partial`, as the code generator will add methods to them.\r\n\r\n```csharp\r\n[SpacetimeDB.Type]\r\npublic partial struct Point\r\n{\r\n    public int x;\r\n    public int y;\r\n}\r\n```\r\n\r\n`enum`s marked with this attribute must not use custom discriminants, as the runtime expects them to be always consecutive starting from zero. Unlike structs and classes, they don't use `partial` as C# doesn't allow to add methods to `enum`s.\r\n\r\n```csharp\r\n[SpacetimeDB.Type]\r\npublic enum Color\r\n{\r\n    Red,\r\n    Green,\r\n    Blue,\r\n}\r\n```\r\n\r\n#### Tagged enums\r\n\r\nSpacetimeDB has support for tagged enums which can be found in languages like Rust, but not C#.\r\n\r\nTo bridge the gap, a special marker interface `SpacetimeDB.TaggedEnum` can be used on any `SpacetimeDB.Type`-marked `struct` or `class` to mark it as a SpacetimeDB tagged enum. It accepts a tuple of 2 or more named items and will generate methods to check which variant is currently active, as well as accessors for each variant.\r\n\r\nIt is expected that you will use the `Is*` methods to check which variant is active before accessing the corresponding field, as the accessor will throw an exception on a state mismatch.\r\n\r\n```csharp\r\n// Example declaration:\r\n[SpacetimeDB.Type]\r\npartial struct Option<T> : SpacetimeDB.TaggedEnum<(T Some, Unit None)> { }\r\n\r\n// Usage:\r\nvar option = new Option<int> { Some = 42 };\r\nif (option.IsSome)\r\n{\r\n    Log($\"Value: {option.Some}\");\r\n}\r\n```\r\n\r\n### Tables\r\n\r\n`[SpacetimeDB.Table]` attribute can be used on any `struct` or `class` to mark it as a SpacetimeDB table. It will register a table in the database with the given name and fields as well as will generate C# methods to insert, filter, update, and delete rows of the given type.\r\n\r\nIt implies `[SpacetimeDB.Type]`, so you must not specify both attributes on the same type.\r\n\r\n```csharp\r\n[SpacetimeDB.Table]\r\npublic partial struct Person\r\n{\r\n    [SpacetimeDB.Column(ColumnAttrs.Unique | ColumnAttrs.AutoInc)]\r\n    public int Id;\r\n    public string Name;\r\n    public int Age;\r\n}\r\n```\r\n\r\nThe example above will generate the following extra methods:\r\n\r\n```csharp\r\npublic partial struct Person\r\n{\r\n    // Inserts current instance as a new row into the table.\r\n    public void Insert();\r\n\r\n    // Returns an iterator over all rows in the table, e.g.:\r\n    // `for (var person in Person.Iter()) { ... }`\r\n    public static IEnumerable<Person> Iter();\r\n\r\n    // Returns an iterator over all rows in the table that match the given filter, e.g.:\r\n    // `for (var person in Person.Query(p => p.Age >= 18)) { ... }`\r\n    public static IEnumerable<Person> Query(Expression<Func<Person, bool>> filter);\r\n\r\n    // Generated for each column:\r\n\r\n    // Returns an iterator over all rows in the table that have the given value in the `Name` column.\r\n    public static IEnumerable<Person> FilterByName(string name);\r\n    public static IEnumerable<Person> FilterByAge(int age);\r\n\r\n    // Generated for each unique column:\r\n\r\n    // Finds a row in the table with the given value in the `Id` column and returns it, or `null` if no such row exists.\r\n    public static Person? FindById(int id);\r\n    // Deletes a row in the table with the given value in the `Id` column and returns `true` if the row was found and deleted, or `false` if no such row exists.\r\n    public static bool DeleteById(int id);\r\n    // Updates a row in the table with the given value in the `Id` column and returns `true` if the row was found and updated, or `false` if no such row exists.\r\n    public static bool UpdateById(int oldId, Person newValue);\r\n}\r\n```\r\n\r\n#### Column attributes\r\n\r\nAttribute `[SpacetimeDB.Column]` can be used on any field of a `SpacetimeDB.Table`-marked `struct` or `class` to customize column attributes as seen above.\r\n\r\nThe supported column attributes are:\r\n\r\n- `ColumnAttrs.AutoInc` - this column should be auto-incremented.\r\n- `ColumnAttrs.Unique` - this column should be unique.\r\n- `ColumnAttrs.PrimaryKey` - this column should be a primary key, it implies `ColumnAttrs.Unique` but also allows clients to subscribe to updates via `OnUpdate` which will use this field to match the old and the new version of the row with each other.\r\n\r\nThese attributes are bitflags and can be combined together, but you can also use some predefined shortcut aliases:\r\n\r\n- `ColumnAttrs.Identity` - same as `ColumnAttrs.Unique | ColumnAttrs.AutoInc`.\r\n- `ColumnAttrs.PrimaryKeyAuto` - same as `ColumnAttrs.PrimaryKey | ColumnAttrs.AutoInc`.\r\n\r\n### Reducers\r\n\r\nAttribute `[SpacetimeDB.Reducer]` can be used on any `static void` method to register it as a SpacetimeDB reducer. The method must accept only supported types as arguments. If it throws an exception, those will be caught and reported back to the database runtime.\r\n\r\n```csharp\r\n[SpacetimeDB.Reducer]\r\npublic static void Add(string name, int age)\r\n{\r\n    var person = new Person { Name = name, Age = age };\r\n    person.Insert();\r\n    Log($\"Inserted {person.Name} under #{person.Id}\");\r\n}\r\n```\r\n\r\nIf a reducer has an argument with a type `DbEventArgs` (`SpacetimeDB.Runtime.DbEventArgs`), it will be provided with event details such as the sender identity (`SpacetimeDB.Runtime.Identity`) and the time (`DateTimeOffset`) of the invocation:\r\n\r\n```csharp\r\n[SpacetimeDB.Reducer]\r\npublic static void PrintInfo(DbEventArgs e)\r\n{\r\n    Log($\"Sender: {e.Sender}\");\r\n    Log($\"Time: {e.Time}\");\r\n}\r\n```\r\n\r\n`[SpacetimeDB.Reducer]` also generates a function to schedule the given reducer in the future.\r\n\r\nSince it's not possible to generate extension methods on existing methods, the codegen will instead add a `Schedule`-prefixed method colocated in the same namespace as the original method instead. The generated method will accept `DateTimeOffset` argument for the time when the reducer should be invoked, followed by all the arguments of the reducer itself, except those that have type `DbEventArgs`.\r\n\r\n```csharp\r\n// Example reducer:\r\n[SpacetimeDB.Reducer]\r\npublic static void Add(string name, int age) { ... }\r\n\r\n// Auto-generated by the codegen:\r\npublic static void ScheduleAdd(DateTimeOffset time, string name, int age) { ... }\r\n\r\n// Usage from another reducer:\r\n[SpacetimeDB.Reducer]\r\npublic static void AddIn5Minutes(DbEventArgs e, string name, int age)\r\n{\r\n    // Note that we're using `e.Time` instead of `DateTimeOffset.Now` which is not allowed in modules.\r\n    var scheduleToken = ScheduleAdd(e.Time.AddMinutes(5), name, age);\r\n\r\n    // We can cancel the scheduled reducer by calling `Cancel()` on the returned token.\r\n    scheduleToken.Cancel();\r\n}\r\n```\r\n\r\n#### Special reducers\r\n\r\nThese are two special kinds of reducers that can be used to respond to module lifecycle events. They're stored in the `SpacetimeDB.Module.ReducerKind` class and can be used as an argument to the `[SpacetimeDB.Reducer]` attribute:\r\n\r\n- `ReducerKind.Init` - this reducer will be invoked when the module is first published.\r\n- `ReducerKind.Update` - this reducer will be invoked when the module is updated.\r\n\r\nExample:\r\n\r\n```csharp\r\n[SpacetimeDB.Reducer(ReducerKind.Init)]\r\npublic static void Init()\r\n{\r\n    Log(\"...and we're live!\");\r\n}\r\n```\r\n\r\n### Connection events\r\n\r\n`OnConnect` and `OnDisconnect` `SpacetimeDB.Runtime` events are triggered when a client connects or disconnects from the database. They can be used to initialize per-client state or to clean up after the client disconnects. They get passed an instance of the earlier mentioned `DbEventArgs` which can be used to distinguish clients via its `Sender` field.\r\n\r\n```csharp\r\n[SpacetimeDB.Reducer(ReducerKind.Init)]\r\npublic static void Init()\r\n{\r\n    OnConnect += (e) => Log($\"Client {e.Sender} connected!\");\r\n    OnDisconnect += (e) => Log($\"Client {e.Sender} disconnected!\");\r\n}\r\n```\r\n",
              "editUrl": "ModuleReference.md",
              "jumpLinks": [
                {
                  "title": "SpacetimeDB C# Modules",
                  "route": "spacetimedb-c-modules",
                  "depth": 1
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 2
                },
                {
                  "title": "API reference",
                  "route": "api-reference",
                  "depth": 2
                },
                {
                  "title": "Logging",
                  "route": "logging",
                  "depth": 3
                },
                {
                  "title": "Supported types",
                  "route": "supported-types",
                  "depth": 3
                },
                {
                  "title": "Built-in types",
                  "route": "built-in-types",
                  "depth": 4
                },
                {
                  "title": "Custom types",
                  "route": "custom-types",
                  "depth": 4
                },
                {
                  "title": "Tagged enums",
                  "route": "tagged-enums",
                  "depth": 4
                },
                {
                  "title": "Tables",
                  "route": "tables",
                  "depth": 3
                },
                {
                  "title": "Column attributes",
                  "route": "column-attributes",
                  "depth": 4
                },
                {
                  "title": "Reducers",
                  "route": "reducers",
                  "depth": 3
                },
                {
                  "title": "Special reducers",
                  "route": "special-reducers",
                  "depth": 4
                },
                {
                  "title": "Connection events",
                  "route": "connection-events",
                  "depth": 3
                }
              ],
              "pages": []
            }
          ]
        },
        {
          "title": "Server Module Overview",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# Server Module Overview\r\n\r\nServer modules are the core of a SpacetimeDB application. They define the structure of the database and the server-side logic that processes and handles client requests. These functions are called reducers and are transactional, meaning they ensure data consistency and integrity. Reducers can perform operations such as inserting, updating, and deleting data in the database.\r\n\r\nIn the following sections, we'll cover the basics of server modules and how to create and deploy them.\r\n\r\n## Supported Languages\r\n\r\n### Rust\r\n\r\nAs of SpacetimeDB 0.6, Rust is the only fully supported language for server modules. Rust is a great option for server modules because it is fast, safe, and has a small runtime.\r\n\r\n- [Rust Module Reference](/docs/server-languages/rust/rust-module-reference)\r\n- [Rust Module Quickstart Guide](/docs/server-languages/rust/rust-module-quickstart-guide)\r\n\r\n### C#\r\n\r\nWe have C# support available in experimental status. C# can be a good choice for developers who are already using Unity or .net for their client applications.\r\n\r\n- [C# Module Reference](/docs/server-languages/csharp/csharp-module-reference)\r\n- [C# Module Quickstart Guide](/docs/server-languages/csharp/csharp-module-quickstart-guide)\r\n\r\n### Coming Soon\r\n\r\nWe have plans to support additional languages in the future.\r\n\r\n- Python\r\n- Typescript\r\n- C++\r\n- Lua\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "Server Module Overview",
              "route": "server-module-overview",
              "depth": 1
            },
            {
              "title": "Supported Languages",
              "route": "supported-languages",
              "depth": 2
            },
            {
              "title": "Rust",
              "route": "rust",
              "depth": 3
            },
            {
              "title": "C#",
              "route": "c-",
              "depth": 3
            },
            {
              "title": "Coming Soon",
              "route": "coming-soon",
              "depth": 3
            }
          ],
          "pages": []
        },
        {
          "title": "Rust",
          "identifier": "Rust",
          "indexIdentifier": "index",
          "comingSoon": false,
          "hasPages": true,
          "editUrl": "Rust/index.md",
          "jumpLinks": [],
          "pages": [
            {
              "title": "Rust Module Quickstart",
              "identifier": "index",
              "indexIdentifier": "index",
              "content": "# Rust Module Quickstart\r\n\r\nIn this tutorial, we'll implement a simple chat server as a SpacetimeDB module.\r\n\r\nA SpacetimeDB module is code that gets compiled to WebAssembly and is uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with the Spacetime relational database.\r\n\r\nEach SpacetimeDB module defines a set of tables and a set of reducers.\r\n\r\nEach table is defined as a Rust `struct` annotated with `#[spacetimedb(table)]`, where an instance represents a row, and each field represents a column.\r\n\r\nA reducer is a function which traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. In Rust, reducers are defined as functions annotated with `#[spacetimedb(reducer)]`, and may return a `Result<()>`, with an `Err` return aborting the transaction.\r\n\r\n## Install SpacetimeDB\r\n\r\nIf you haven't already, start by [installing SpacetimeDB](/install). This will install the `spacetime` command line interface (CLI), which contains all the functionality for interacting with SpacetimeDB.\r\n\r\n## Install Rust\r\n\r\nNext we need to [install Rust](https://www.rust-lang.org/tools/install) so that we can create our database module.\r\n\r\nOn MacOS and Linux run this command to install the Rust compiler:\r\n\r\n```bash\r\ncurl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\r\n```\r\n\r\nIf you're on Windows, go [here](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup).\r\n\r\n## Project structure\r\n\r\nCreate and enter a directory `quickstart-chat`:\r\n\r\n```bash\r\nmkdir quickstart-chat\r\ncd quickstart-chat\r\n```\r\n\r\nNow create `server`, our module, which runs in the database:\r\n\r\n```bash\r\nspacetime init --lang rust server\r\n```\r\n\r\n## Declare imports\r\n\r\n`spacetime init` should have pre-populated `server/src/lib.rs` with a trivial module. Clear it out, so we can write a module that's still pretty simple: a bare-bones chat server.\r\n\r\nTo the top of `server/src/lib.rs`, add some imports we'll be using:\r\n\r\n```rust\r\nuse spacetimedb::{spacetimedb, ReducerContext, Identity, Timestamp};\r\n```\r\n\r\nFrom `spacetimedb`, we import:\r\n\r\n- `spacetimedb`, an attribute macro we'll use to define tables and reducers.\r\n- `ReducerContext`, a special argument passed to each reducer.\r\n- `Identity`, a unique identifier for each connected client.\r\n- `Timestamp`, a point in time. Specifically, an unsigned 64-bit count of milliseconds since the UNIX epoch.\r\n\r\n## Define tables\r\n\r\nTo get our chat server running, we'll need to store two kinds of data: information about each user, and records of all the messages that have been sent.\r\n\r\nFor each `User`, we'll store the `Identity` of their client connection, an optional name they can set to identify themselves to other users, and whether they're online or not. We'll designate the `Identity` as our primary key, which enforces that it must be unique, indexes it for faster lookup, and allows clients to track updates.\r\n\r\nTo `server/src/lib.rs`, add the definition of the table `User`:\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\npub struct User {\r\n    #[primarykey]\r\n    identity: Identity,\r\n    name: Option<String>,\r\n    online: bool,\r\n}\r\n```\r\n\r\nFor each `Message`, we'll store the `Identity` of the user who sent it, the `Timestamp` when it was sent, and the text of the message.\r\n\r\nTo `server/src/lib.rs`, add the definition of the table `Message`:\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\npub struct Message {\r\n    sender: Identity,\r\n    sent: Timestamp,\r\n    text: String,\r\n}\r\n```\r\n\r\n## Set users' names\r\n\r\nWe want to allow users to set their names, because `Identity` is not a terribly user-friendly identifier. To that effect, we define a reducer `set_name` which clients can invoke to set their `User.name`. It will validate the caller's chosen name, using a function `validate_name` which we'll define next, then look up the `User` record for the caller and update it to store the validated name. If the name fails the validation, the reducer will fail.\r\n\r\nEach reducer may accept as its first argument a `ReducerContext`, which includes the `Identity` of the client that called the reducer, and the `Timestamp` when it was invoked. For now, we only need the `Identity`, `ctx.sender`.\r\n\r\nIt's also possible to call `set_name` via the SpacetimeDB CLI's `spacetime call` command without a connection, in which case no `User` record will exist for the caller. We'll return an error in this case, but you could alter the reducer to insert a `User` row for the module owner. You'll have to decide whether the module owner is always online or always offline, though.\r\n\r\nTo `server/src/lib.rs`, add:\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\n/// Clientss invoke this reducer to set their user names.\r\npub fn set_name(ctx: ReducerContext, name: String) -> Result<(), String> {\r\n    let name = validate_name(name)?;\r\n    if let Some(user) = User::filter_by_identity(&ctx.sender) {\r\n        User::update_by_identity(&ctx.sender, User { name: Some(name), ..user });\r\n        Ok(())\r\n    } else {\r\n        Err(\"Cannot set name for unknown user\".to_string())\r\n    }\r\n}\r\n```\r\n\r\nFor now, we'll just do a bare minimum of validation, rejecting the empty name. You could extend this in various ways, like:\r\n\r\n- Comparing against a blacklist for moderation purposes.\r\n- Unicode-normalizing names.\r\n- Rejecting names that contain non-printable characters, or removing characters or replacing them with a placeholder.\r\n- Rejecting or truncating long names.\r\n- Rejecting duplicate names.\r\n\r\nTo `server/src/lib.rs`, add:\r\n\r\n```rust\r\n/// Takes a name and checks if it's acceptable as a user's name.\r\nfn validate_name(name: String) -> Result<String, String> {\r\n    if name.is_empty() {\r\n        Err(\"Names must not be empty\".to_string())\r\n    } else {\r\n        Ok(name)\r\n    }\r\n}\r\n```\r\n\r\n## Send messages\r\n\r\nWe define a reducer `send_message`, which clients will call to send messages. It will validate the message's text, then insert a new `Message` record using `Message::insert`, with the `sender` identity and `sent` timestamp taken from the `ReducerContext`. Because `Message` does not have any columns with unique constraints, `Message::insert` is infallible; it does not return a `Result`.\r\n\r\nTo `server/src/lib.rs`, add:\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\n/// Clients invoke this reducer to send messages.\r\npub fn send_message(ctx: ReducerContext, text: String) -> Result<(), String> {\r\n    let text = validate_message(text)?;\r\n    log::info!(\"{}\", text);\r\n    Message::insert(Message {\r\n        sender: ctx.sender,\r\n        text,\r\n        sent: ctx.timestamp,\r\n    });\r\n    Ok(())\r\n}\r\n```\r\n\r\nWe'll want to validate messages' texts in much the same way we validate users' chosen names. As above, we'll do the bare minimum, rejecting only empty messages.\r\n\r\nTo `server/src/lib.rs`, add:\r\n\r\n```rust\r\n/// Takes a message's text and checks if it's acceptable to send.\r\nfn validate_message(text: String) -> Result<String, String> {\r\n    if text.is_empty() {\r\n        Err(\"Messages must not be empty\".to_string())\r\n    } else {\r\n        Ok(text)\r\n    }\r\n}\r\n```\r\n\r\nYou could extend the validation in `validate_message` in similar ways to `validate_name`, or add additional checks to `send_message`, like:\r\n\r\n- Rejecting messages from senders who haven't set their names.\r\n- Rate-limiting users so they can't send new messages too quickly.\r\n\r\n## Set users' online status\r\n\r\nWhenever a client connects, the module will run a special reducer, annotated with `#[spacetimedb(connect)]`, if it's defined. By convention, it's named `identity_connected`. We'll use it to create a `User` record for the client if it doesn't yet exist, and to set its online status.\r\n\r\nWe'll use `User::filter_by_identity` to look up a `User` row for `ctx.sender`, if one exists. If we find one, we'll use `User::update_by_identity` to overwrite it with a row that has `online: true`. If not, we'll use `User::insert` to insert a new row for our new user. All three of these methods are generated by the `#[spacetimedb(table)]` attribute, with rows and behavior based on the row attributes. `filter_by_identity` returns an `Option<User>`, because the unique constraint from the `#[primarykey]` attribute means there will be either zero or one matching rows. `insert` returns a `Result<(), UniqueConstraintViolation>` because of the same unique constraint; if we want to overwrite a `User` row, we need to do so explicitly using `update_by_identity`.\r\n\r\nTo `server/src/lib.rs`, add the definition of the connect reducer:\r\n\r\n```rust\r\n#[spacetimedb(connect)]\r\n// Called when a client connects to the SpacetimeDB\r\npub fn identity_connected(ctx: ReducerContext) {\r\n    if let Some(user) = User::filter_by_identity(&ctx.sender) {\r\n        // If this is a returning user, i.e. we already have a `User` with this `Identity`,\r\n        // set `online: true`, but leave `name` and `identity` unchanged.\r\n        User::update_by_identity(&ctx.sender, User { online: true, ..user });\r\n    } else {\r\n        // If this is a new user, create a `User` row for the `Identity`,\r\n        // which is online, but hasn't set a name.\r\n        User::insert(User {\r\n            name: None,\r\n            identity: ctx.sender,\r\n            online: true,\r\n        }).unwrap();\r\n    }\r\n}\r\n```\r\n\r\nSimilarly, whenever a client disconnects, the module will run the `#[spacetimedb(disconnect)]` reducer if it's defined. By convention, it's named `identity_disconnect`. We'll use it to un-set the `online` status of the `User` for the disconnected client.\r\n\r\n```rust\r\n#[spacetimedb(disconnect)]\r\n// Called when a client disconnects from SpacetimeDB\r\npub fn identity_disconnected(ctx: ReducerContext) {\r\n    if let Some(user) = User::filter_by_identity(&ctx.sender) {\r\n        User::update_by_identity(&ctx.sender, User { online: false, ..user });\r\n    } else {\r\n        // This branch should be unreachable,\r\n        // as it doesn't make sense for a client to disconnect without connecting first.\r\n        log::warn!(\"Disconnect event for unknown user with identity {:?}\", ctx.sender);\r\n    }\r\n}\r\n```\r\n\r\n## Publish the module\r\n\r\nAnd that's all of our module code! We'll run `spacetime publish` to compile our module and publish it on SpacetimeDB. `spacetime publish` takes an optional name which will map to the database's unique address. Clients can connect either by name or by address, but names are much more pleasant. Come up with a unique name that contains only URL-safe characters (letters, numbers, hyphens and underscores), and fill it in where we've written `<module-name>`.\r\n\r\nFrom the `quickstart-chat` directory, run:\r\n\r\n```bash\r\nspacetime publish --project-path server <module-name>\r\n```\r\n\r\n## Call Reducers\r\n\r\nYou can use the CLI (command line interface) to run reducers. The arguments to the reducer are passed in JSON format.\r\n\r\n```bash\r\nspacetime call <module-name> send_message '[\"Hello, World!\"]'\r\n```\r\n\r\nOnce we've called our `send_message` reducer, we can check to make sure it ran by running the `logs` command.\r\n\r\n```bash\r\nspacetime logs <module-name>\r\n```\r\n\r\nYou should now see the output that your module printed in the database.\r\n\r\n```bash\r\ninfo: Hello, World!\r\n```\r\n\r\n## SQL Queries\r\n\r\nSpacetimeDB supports a subset of the SQL syntax so that you can easily query the data of your database. We can run a query using the `sql` command.\r\n\r\n```bash\r\nspacetime sql <module-name> \"SELECT * FROM Message\"\r\n```\r\n\r\n```bash\r\n text\r\n---------\r\n \"Hello, World!\"\r\n```\r\n\r\n## What's next?\r\n\r\nYou can find the full code for this module [in the SpacetimeDB module examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/quickstart-chat).\r\n\r\nYou've just set up your first database in SpacetimeDB! The next step would be to create a client module that interacts with this module. You can use any of SpacetimDB's supported client languages to do this. Take a look at the quickstart guide for your client language of choice: [Rust](/docs/client-languages/rust/rust-sdk-quickstart-guide), [C#](/docs/client-languages/csharp/csharp-sdk-quickstart-guide), [TypeScript](/docs/client-languages/typescript/typescript-sdk-quickstart-guide) or [Python](/docs/client-languages/python/python-sdk-quickstart-guide).\r\n\r\nIf you are planning to use SpacetimeDB with the Unity3d game engine, you can skip right to the [Unity Comprehensive Tutorial](/docs/game-dev/unity-tutorial) or check out our example game, [BitcraftMini](/docs/game-dev/unity-tutorial-bitcraft-mini).\r\n",
              "hasPages": false,
              "editUrl": "index.md",
              "jumpLinks": [
                {
                  "title": "Rust Module Quickstart",
                  "route": "rust-module-quickstart",
                  "depth": 1
                },
                {
                  "title": "Install SpacetimeDB",
                  "route": "install-spacetimedb",
                  "depth": 2
                },
                {
                  "title": "Install Rust",
                  "route": "install-rust",
                  "depth": 2
                },
                {
                  "title": "Project structure",
                  "route": "project-structure",
                  "depth": 2
                },
                {
                  "title": "Declare imports",
                  "route": "declare-imports",
                  "depth": 2
                },
                {
                  "title": "Define tables",
                  "route": "define-tables",
                  "depth": 2
                },
                {
                  "title": "Set users' names",
                  "route": "set-users-names",
                  "depth": 2
                },
                {
                  "title": "Send messages",
                  "route": "send-messages",
                  "depth": 2
                },
                {
                  "title": "Set users' online status",
                  "route": "set-users-online-status",
                  "depth": 2
                },
                {
                  "title": "Publish the module",
                  "route": "publish-the-module",
                  "depth": 2
                },
                {
                  "title": "Call Reducers",
                  "route": "call-reducers",
                  "depth": 2
                },
                {
                  "title": "SQL Queries",
                  "route": "sql-queries",
                  "depth": 2
                },
                {
                  "title": "What's next?",
                  "route": "what-s-next-",
                  "depth": 2
                }
              ],
              "pages": []
            },
            {
              "title": "SpacetimeDB Rust Modules",
              "identifier": "ModuleReference",
              "indexIdentifier": "ModuleReference",
              "hasPages": false,
              "content": "# SpacetimeDB Rust Modules\r\n\r\nRust clients of SpacetimeDB use the [Rust SpacetimeDB module library][module library] to write modules which interact with the SpacetimeDB database.\r\n\r\nFirst, the `spacetimedb` library provides a number of macros for creating tables and Rust `struct`s corresponding to rows in those tables.\r\n\r\nThen the client API allows interacting with the database inside special functions called reducers.\r\n\r\nThis guide assumes you are familiar with some basics of Rust. At the very least, you should be familiar with the idea of using attribute macros. An extremely common example is `derive` macros.\r\n\r\nDerive macros look at the type they are attached to and generate some related code. In this example, `#[derive(Debug)]` generates the formatting code needed to print out a `Location` for debugging purposes.\r\n\r\n```rust\r\n#[derive(Debug)]\r\nstruct Location {\r\n    x: u32,\r\n    y: u32,\r\n}\r\n```\r\n\r\n## SpacetimeDB Macro basics\r\n\r\nLet's start with a highly commented example, straight from the [demo]. This Rust package defines a SpacetimeDB module, with types we can operate on and functions we can run.\r\n\r\n```rust\r\n// In this small example, we have two rust imports:\r\n// |spacetimedb::spacetimedb| is the most important attribute we'll be using.\r\n// |spacetimedb::println| is like regular old |println|, but outputting to the module's logs.\r\nuse spacetimedb::{spacetimedb, println};\r\n\r\n// This macro lets us interact with a SpacetimeDB table of Person rows.\r\n// We can insert and delete into, and query, this table by the collection\r\n// of functions generated by the macro.\r\n#[spacetimedb(table)]\r\npub struct Person {\r\n    name: String,\r\n}\r\n\r\n// This is the other key macro we will be using. A reducer is a\r\n// stored procedure that lives in the database, and which can\r\n// be invoked remotely.\r\n#[spacetimedb(reducer)]\r\npub fn add(name: String) {\r\n    // |Person| is a totally ordinary Rust struct. We can construct\r\n    // one from the given name as we typically would.\r\n    let person = Person { name };\r\n\r\n    // Here's our first generated function! Given a |Person| object,\r\n    // we can insert it into the table:\r\n    Person::insert(person)\r\n}\r\n\r\n// Here's another reducer. Notice that this one doesn't take any arguments, while\r\n// |add| did take one. Reducers can take any number of arguments, as long as\r\n// SpacetimeDB knows about all their types. Reducers also have to be top level\r\n// functions, not methods.\r\n#[spacetimedb(reducer)]\r\npub fn say_hello() {\r\n    // Here's the next of our generated functions: |iter()|. This\r\n    // iterates over all the columns in the |Person| table in SpacetimeDB.\r\n    for person in Person::iter() {\r\n        // Reducers run in a very constrained and sandboxed environment,\r\n        // and in particular, can't do most I/O from the Rust standard library.\r\n        // We provide an alternative |spacetimedb::println| which is just like\r\n        // the std version, excepted it is redirected out to the module's logs.\r\n        println!(\"Hello, {}!\", person.name);\r\n    }\r\n    println!(\"Hello, World!\");\r\n}\r\n\r\n// Reducers can't return values, but can return errors. To do so,\r\n// the reducer must have a return type of `Result<(), T>`, for any `T` that\r\n// implements `Debug`.  Such errors returned from reducers will be formatted and\r\n// printed out to logs.\r\n#[spacetimedb(reducer)]\r\npub fn add_person(name: String) -> Result<(), String> {\r\n    if name.is_empty() {\r\n        return Err(\"Name cannot be empty\");\r\n    }\r\n\r\n    Person::insert(Person { name })\r\n}\r\n```\r\n\r\n## Macro API\r\n\r\nNow we'll get into details on all the macro APIs SpacetimeDB provides, starting with all the variants of the `spacetimedb` attribute.\r\n\r\n### Defining tables\r\n\r\n`#[spacetimedb(table)]` takes no further arguments, and is applied to a Rust struct with named fields:\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Table {\r\n    field1: String,\r\n    field2: u32,\r\n}\r\n```\r\n\r\nThis attribute is applied to Rust structs in order to create corresponding tables in SpacetimeDB. Fields of the Rust struct correspond to columns of the database table.\r\n\r\nThe fields of the struct have to be types that spacetimedb knows how to encode into the database. This is captured in Rust by the `SpacetimeType` trait.\r\n\r\nThis is automatically defined for built in numeric types:\r\n\r\n- `bool`\r\n- `u8`, `u16`, `u32`, `u64`, `u128`\r\n- `i8`, `i16`, `i32`, `i64`, `i128`\r\n- `f32`, `f64`\r\n\r\nAnd common data structures:\r\n\r\n- `String` and `&str`, utf-8 string data\r\n- `()`, the unit type\r\n- `Option<T> where T: SpacetimeType`\r\n- `Vec<T> where T: SpacetimeType`\r\n\r\nAll `#[spacetimedb(table)]` types are `SpacetimeType`s, and accordingly, all of their fields have to be.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct AnotherTable {\r\n    // Fine, some builtin types.\r\n    id: u64,\r\n    name: Option<String>,\r\n\r\n    // Fine, another table type.\r\n    table: Table,\r\n\r\n    // Fine, another type we explicitly make serializable.\r\n    serial: Serial,\r\n}\r\n```\r\n\r\nIf you want to have a field that is not one of the above primitive types, and not a table of its own, you can derive the `SpacetimeType` attribute on it.\r\n\r\nWe can derive `SpacetimeType` on `struct`s and `enum`s with members that are themselves `SpacetimeType`s.\r\n\r\n```rust\r\n#[derive(SpacetimeType)]\r\nenum Serial {\r\n    Builtin(f64),\r\n    Compound {\r\n        s: String,\r\n        bs: Vec<bool>,\r\n    }\r\n}\r\n```\r\n\r\nOnce the table is created via the macro, other attributes described below can control more aspects of the table. For instance, a particular column can be indexed, or take on values of an automatically incremented counter. These are described in detail below.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Person {\r\n    #[unique]\r\n    id: u64,\r\n\r\n    name: String,\r\n    address: String,\r\n}\r\n```\r\n\r\n### Defining reducers\r\n\r\n`#[spacetimedb(reducer)]` optionally takes a single argument, which is a frequency at which the reducer will be automatically called by the database.\r\n\r\n`#[spacetimedb(reducer)]` is always applied to top level Rust functions. They can take arguments of types known to SpacetimeDB (just like fields of structs must be known to SpacetimeDB), and either return nothing, or return a `Result<(), E: Debug>`.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn give_player_item(player_id: u64, item_id: u64) -> Result<(), GameErr> {\r\n    // Notice how the exact name of the filter function derives from\r\n    // the name of the field of the struct.\r\n    let mut item = Item::filter_by_item_id(id).ok_or(GameErr::InvalidId)?;\r\n    item.owner = Some(player_id);\r\n    Item::update_by_id(id,  item);\r\n    Ok(())\r\n}\r\n\r\nstruct Item {\r\n    #[unique]\r\n    item_id: u64,\r\n\r\n    owner: Option<u64>,\r\n}\r\n```\r\n\r\nNote that reducers can call non-reducer functions, including standard library functions.\r\n\r\nReducers that are called periodically take an additional macro argument specifying the frequency at which they will be invoked. Durations are parsed according to https://docs.rs/humantime/latest/humantime/fn.parse_duration.html and will usually be a number of milliseconds or seconds.\r\n\r\nBoth of these examples are invoked every second.\r\n\r\n```rust\r\n#[spacetimedb(reducer, repeat = 1s)]\r\nfn every_second() {}\r\n\r\n#[spacetimedb(reducer, repeat = 1000ms)]\r\nfn every_thousand_milliseconds() {}\r\n```\r\n\r\nFinally, reducers can also receive a ReducerContext object, or the Timestamp at which they are invoked, just by taking parameters of those types first.\r\n\r\n```rust\r\n#[spacetimedb(reducer, repeat = 1s)]\r\nfn tick_timestamp(time: Timestamp) {\r\n    println!(\"tick at {time}\");\r\n}\r\n\r\n#[spacetimedb(reducer, repeat = 500ms)]\r\nfn tick_ctx(ctx: ReducerContext) {\r\n    println!(\"tick at {}\", ctx.timestamp)\r\n}\r\n```\r\n\r\nNote that each distinct time a repeating reducer is invoked, a seperate schedule is created for that reducer. So invoking `every_second` three times from the spacetimedb cli will result in the reducer being called times times each second.\r\n\r\nThere are several macros which modify the semantics of a column, which are applied to the members of the table struct. `#[unique]` and `#[autoinc]` are covered below, describing how those attributes affect the semantics of inserting, filtering, and so on.\r\n\r\n#[SpacetimeType]\r\n\r\n#[sats]\r\n\r\n## Client API\r\n\r\nBesides the macros for creating tables and reducers, there's two other parts of the Rust SpacetimeDB library. One is a collection of macros for logging, and the other is all the automatically generated functions for operating on those tables.\r\n\r\n### `println!` and friends\r\n\r\nBecause reducers run in a WASM sandbox, they don't have access to general purpose I/O from the Rust standard library. There's no filesystem or network access, and no input or output. This means no access to things like `std::println!`, which prints to standard output.\r\n\r\nSpacetimeDB modules have access to logging output. These are exposed as macros, just like their `std` equivalents. The names, and all the Rust formatting machinery, work the same; just the location of the output is different.\r\n\r\nLogs for a module can be viewed with the `spacetime logs` command from the CLI.\r\n\r\n```rust\r\nuse spacetimedb::{\r\n    println,\r\n    print,\r\n    eprintln,\r\n    eprint,\r\n    dbg,\r\n};\r\n\r\n#[spacetimedb(reducer)]\r\nfn output(i: i32) {\r\n    // These will be logged at log::Level::Info.\r\n    println!(\"an int with a trailing newline: {i}\");\r\n    print!(\"some more text...\\n\");\r\n\r\n    // These log at log::Level::Error.\r\n    eprint!(\"Oops...\");\r\n    eprintln!(\", we hit an error\");\r\n\r\n    // Just like std::dbg!, this prints its argument and returns the value,\r\n    // as a drop-in way to print expressions. So this will print out |i|\r\n    // before passing the value of |i| along to the calling function.\r\n    //\r\n    // The output is logged log::Level::Debug.\r\n    OutputtedNumbers::insert(dbg!(i));\r\n}\r\n```\r\n\r\n### Generated functions on a SpacetimeDB table\r\n\r\nWe'll work off these structs to see what functions SpacetimeDB generates:\r\n\r\nThis table has a plain old column.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Ordinary {\r\n    ordinary_field: u64,\r\n}\r\n```\r\n\r\nThis table has a unique column. Every row in the `Person` table must have distinct values of the `unique_field` column. Attempting to insert a row with a duplicate value will fail.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Unique {\r\n    // A unique column:\r\n    #[unique]\r\n    unique_field: u64,\r\n}\r\n```\r\n\r\nThis table has an automatically incrementing column. SpacetimeDB automatically provides an incrementing sequence of values for this field, and sets the field to that value when you insert the row.\r\n\r\nOnly integer types can be `#[unique]`: `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64` and `i128`.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Autoinc {\r\n    #[autoinc]\r\n    autoinc_field: u64,\r\n}\r\n```\r\n\r\nThese attributes can be combined, to create an automatically assigned ID usable for filtering.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Identity {\r\n    #[autoinc]\r\n    #[unique]\r\n    id_field: u64,\r\n}\r\n```\r\n\r\n### Insertion\r\n\r\nWe'll talk about insertion first, as there a couple of special semantics to know about.\r\n\r\nWhen we define |Ordinary| as a spacetimedb table, we get the ability to insert into it with the generated `Ordinary::insert` method.\r\n\r\nInserting takes a single argument, the row to insert. When there are no unique fields in the row, the return value is the inserted row.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn insert_ordinary(value: u64) {\r\n    let ordinary = Ordinary { ordinary_field: value };\r\n    let result = Ordinary::insert(ordinary);\r\n    assert_eq!(ordinary.ordinary_field, result.ordinary_field);\r\n}\r\n```\r\n\r\nWhen there is a unique column constraint on the table, insertion can fail if a uniqueness constraint is violated.\r\n\r\nIf we insert two rows which have the same value of a unique column, the second will fail.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn insert_unique(value: u64) {\r\n    let result = Ordinary::insert(Unique { unique_field: value });\r\n    assert!(result.is_ok());\r\n\r\n    let result = Ordinary::insert(Unique { unique_field: value });\r\n    assert!(result.is_err());\r\n}\r\n```\r\n\r\nWhen inserting a table with an `#[autoinc]` column, the database will automatically overwrite whatever we give it with an atomically increasing value.\r\n\r\nThe returned row has the `autoinc` column set to the value that was actually written into the database.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn insert_autoinc() {\r\n    for i in 1..=10 {\r\n        // These will have values of 1, 2, ..., 10\r\n        // at rest in the database, regardless of\r\n        // what value is actually present in the\r\n        // insert call.\r\n        let actual = Autoinc::insert(Autoinc { autoinc_field: 23 })\r\n        assert_eq!(actual.autoinc_field, i);\r\n    }\r\n}\r\n\r\n#[spacetimedb(reducer)]\r\nfn insert_id() {\r\n    for _ in 0..10 {\r\n        // These also will have values of 1, 2, ..., 10.\r\n        // There's no collision and silent failure to insert,\r\n        // because the value of the field is ignored and overwritten\r\n        // with the automatically incremented value.\r\n        Identity::insert(Identity { autoinc_field: 23 })\r\n    }\r\n}\r\n```\r\n\r\n### Iterating\r\n\r\nGiven a table, we can iterate over all the rows in it.\r\n\r\n```rust\r\n#[spacetimedb(table)]\r\nstruct Person {\r\n    #[unique]\r\n    id: u64,\r\n\r\n    age: u32,\r\n    name: String,\r\n    address: String,\r\n}\r\n```\r\n\r\n// Every table structure an iter function, like:\r\n\r\n```rust\r\nfn MyTable::iter() -> TableIter<MyTable>\r\n```\r\n\r\n`iter()` returns a regular old Rust iterator, giving us a sequence of `Person`. The database sends us over rows, one at a time, for each time through the loop. This means we get them by value, and own the contents of `String` fields and so on.\r\n\r\n```\r\n#[spacetimedb(reducer)]\r\nfn iteration() {\r\n    let mut addresses = HashSet::new();\r\n\r\n    for person in Person::iter() {\r\n        addresses.insert(person.address);\r\n    }\r\n\r\n    for address in addresses.iter() {\r\n        println!(\"{address}\");\r\n    }\r\n}\r\n```\r\n\r\n### Filtering\r\n\r\nOften, we don't need to look at the entire table, and instead are looking for rows with specific values in certain columns.\r\n\r\nOur `Person` table has a unique id column, so we can filter for a row matching that ID. Since it is unique, we will find either 0 or 1 matching rows in the database. This gets represented naturally as an `Option<Person>` in Rust. SpacetimeDB automatically creates and uses indexes for filtering on unique columns, so it is very efficient.\r\n\r\nThe name of the filter method just corresponds to the column name.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn filtering(id: u64) {\r\n    match Person::filter_by_id(&id) {\r\n        Some(person) => println!(\"Found {person}\"),\r\n        None => println!(\"No person with id {id}\"),\r\n    }\r\n}\r\n```\r\n\r\nOur `Person` table also has a column for age. Unlike IDs, ages aren't unique. Filtering for every person who is 21, then, gives us an `Iterator<Item = Person>` rather than an `Option<Person>`.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn filtering_non_unique() {\r\n    for person in Person::filter_by_age(&21) {\r\n        println!(\"{person} has turned 21\");\r\n    }\r\n}\r\n```\r\n\r\n### Deleting\r\n\r\nLike filtering, we can delete by a unique column instead of the entire row.\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\nfn delete_id(id: u64) {\r\n    Person::delete_by_id(&id)\r\n}\r\n```\r\n\r\n[macro library]: https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/bindings-macro\r\n[module library]: https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/lib\r\n[demo]: /#demo\r\n",
              "editUrl": "ModuleReference.md",
              "jumpLinks": [
                {
                  "title": "SpacetimeDB Rust Modules",
                  "route": "spacetimedb-rust-modules",
                  "depth": 1
                },
                {
                  "title": "SpacetimeDB Macro basics",
                  "route": "spacetimedb-macro-basics",
                  "depth": 2
                },
                {
                  "title": "Macro API",
                  "route": "macro-api",
                  "depth": 2
                },
                {
                  "title": "Defining tables",
                  "route": "defining-tables",
                  "depth": 3
                },
                {
                  "title": "Defining reducers",
                  "route": "defining-reducers",
                  "depth": 3
                },
                {
                  "title": "Client API",
                  "route": "client-api",
                  "depth": 2
                },
                {
                  "title": "`println!` and friends",
                  "route": "-println-and-friends",
                  "depth": 3
                },
                {
                  "title": "Generated functions on a SpacetimeDB table",
                  "route": "generated-functions-on-a-spacetimedb-table",
                  "depth": 3
                },
                {
                  "title": "Insertion",
                  "route": "insertion",
                  "depth": 3
                },
                {
                  "title": "Iterating",
                  "route": "iterating",
                  "depth": 3
                },
                {
                  "title": "Filtering",
                  "route": "filtering",
                  "depth": 3
                },
                {
                  "title": "Deleting",
                  "route": "deleting",
                  "depth": 3
                }
              ],
              "pages": []
            }
          ]
        }
      ],
      "previousKey": {
        "title": "Unity Tutorial",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "Client SDK Languages",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "Client SDK Languages",
      "identifier": "Client SDK Languages",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Client%20SDK%20Languages/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "C#",
          "identifier": "C#",
          "indexIdentifier": "index",
          "comingSoon": false,
          "hasPages": true,
          "editUrl": "C%23/index.md",
          "jumpLinks": [],
          "pages": [
            {
              "title": "C# Client SDK Quick Start",
              "identifier": "index",
              "indexIdentifier": "index",
              "content": "# C# Client SDK Quick Start\r\n\r\nIn this guide we'll show you how to get up and running with a simple SpacetimDB app with a client written in C#.\r\n\r\nWe'll implement a command-line client for the module created in our Rust or C# Module Quickstart guides. Make sure you follow one of these guides before you start on this one.\r\n\r\n## Project structure\r\n\r\nEnter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/server-languages/rust/rust-module-quickstart-guide) or [C# Module Quickstart](/docs/server-languages/csharp/csharp-module-quickstart-guide) guides:\r\n\r\n```bash\r\ncd quickstart-chat\r\n```\r\n\r\nWithin it, create a new C# console application project called `client` using either Visual Studio or the .NET CLI:\r\n\r\n```bash\r\ndotnet new console -o client\r\n```\r\n\r\nOpen the project in your IDE of choice.\r\n\r\n## Add the NuGet package for the C# SpacetimeDB SDK\r\n\r\nAdd the `spacetimedbsdk` [NuGet package](https://www.nuget.org/packages/spacetimedbsdk) using Visual Studio NuGet package manager or via the .NET CLI\r\n\r\n```bash\r\ndotnet add package spacetimedbsdk\r\n```\r\n\r\n## Generate your module types\r\n\r\nThe `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.\r\n\r\nIn your `quickstart-chat` directory, run:\r\n\r\n```bash\r\nmkdir -p client/module_bindings\r\nspacetime generate --lang csharp --out-dir client/module_bindings --project-path server\r\n```\r\n\r\nTake a look inside `client/module_bindings`. The CLI should have generated five files:\r\n\r\n```\r\nmodule_bindings\r\n├── Message.cs\r\n├── ReducerEvent.cs\r\n├── SendMessageReducer.cs\r\n├── SetNameReducer.cs\r\n└── User.cs\r\n```\r\n\r\n## Add imports to Program.cs\r\n\r\nOpen `client/Program.cs` and add the following imports:\r\n\r\n```csharp\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\nusing System.Collections.Concurrent;\r\n```\r\n\r\nWe will also need to create some global variables that will be explained when we use them later. Add the following to the top of `Program.cs`:\r\n\r\n```csharp\r\n// our local client SpacetimeDB identity\r\nIdentity? local_identity = null;\r\n// declare a thread safe queue to store commands in format (command, args)\r\nConcurrentQueue<(string,string)> input_queue = new ConcurrentQueue<(string, string)>();\r\n// declare a threadsafe cancel token to cancel the process loop\r\nCancellationTokenSource cancel_token = new CancellationTokenSource();\r\n```\r\n\r\n## Define Main function\r\n\r\nWe'll work outside-in, first defining our `Main` function at a high level, then implementing each behavior it needs. We need `Main` to do several things:\r\n\r\n1. Initialize the AuthToken module, which loads and stores our authentication token to/from local storage.\r\n2. Create the SpacetimeDBClient instance.\r\n3. Register callbacks on any events we want to handle. These will print to standard output messages received from the database and updates about users' names and online statuses.\r\n4. Start our processing thread, which connects to the SpacetimeDB module, updates the SpacetimeDB client and processes commands that come in from the input loop running in the main thread.\r\n5. Start the input loop, which reads commands from standard input and sends them to the processing thread.\r\n6. When the input loop exits, stop the processing thread and wait for it to exit.\r\n\r\n```csharp\r\nvoid Main()\r\n{\r\n    AuthToken.Init(\".spacetime_csharp_quickstart\");\r\n\r\n    // create the client, pass in a logger to see debug messages\r\n    SpacetimeDBClient.CreateInstance(new ConsoleLogger());\r\n\r\n    RegisterCallbacks();\r\n\r\n    // spawn a thread to call process updates and process commands\r\n    var thread = new Thread(ProcessThread);\r\n    thread.Start();\r\n\r\n    InputLoop();\r\n\r\n    // this signals the ProcessThread to stop\r\n    cancel_token.Cancel();\r\n    thread.Join();\r\n}\r\n```\r\n\r\n## Register callbacks\r\n\r\nWe need to handle several sorts of events:\r\n\r\n1. `onConnect`: When we connect, we will call `Subscribe` to tell the module what tables we care about.\r\n2. `onIdentityReceived`: When we receive our credentials, we'll use the `AuthToken` module to save our token so that the next time we connect, we can re-authenticate as the same user.\r\n3. `onSubscriptionApplied`: When we get the onSubscriptionApplied callback, that means our local client cache has been fully populated. At this time we'll print the user menu.\r\n4. `User.OnInsert`: When a new user joins, we'll print a message introducing them.\r\n5. `User.OnUpdate`: When a user is updated, we'll print their new name, or declare their new online status.\r\n6. `Message.OnInsert`: When we receive a new message, we'll print it.\r\n7. `Reducer.OnSetNameEvent`: If the server rejects our attempt to set our name, we'll print an error.\r\n8. `Reducer.OnSendMessageEvent`: If the server rejects a message we send, we'll print an error.\r\n\r\n```csharp\r\nvoid RegisterCallbacks()\r\n{\r\n    SpacetimeDBClient.instance.onConnect += OnConnect;\r\n    SpacetimeDBClient.instance.onIdentityReceived += OnIdentityReceived;\r\n    SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;\r\n\r\n    User.OnInsert += User_OnInsert;\r\n    User.OnUpdate += User_OnUpdate;\r\n\r\n    Message.OnInsert += Message_OnInsert;\r\n\r\n    Reducer.OnSetNameEvent += Reducer_OnSetNameEvent;\r\n    Reducer.OnSendMessageEvent += Reducer_OnSendMessageEvent;\r\n}\r\n```\r\n\r\n### Notify about new users\r\n\r\nFor each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `OnInsert` and `OnDelete` methods, which are automatically generated for each table by `spacetime generate`.\r\n\r\nThese callbacks can fire in two contexts:\r\n\r\n- After a reducer runs, when the client's cache is updated about changes to subscribed rows.\r\n- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.\r\n\r\nThis second case means that, even though the module only ever inserts online users, the client's `User.OnInsert` callbacks may be invoked with users who are offline. We'll only notify about online users.\r\n\r\n`OnInsert` and `OnDelete` callbacks take two arguments: the altered row, and a `ReducerEvent`. This will be `null` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is an enum autogenerated by `spacetime generate` with a variant for each reducer defined by the module. For now, we can ignore this argument.\r\n\r\nWhenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define a function `UserNameOrIdentity` to handle this.\r\n\r\n```csharp\r\nstring UserNameOrIdentity(User user) => user.Name ?? Identity.From(user.Identity).ToString()!.Substring(0, 8);\r\n\r\nvoid User_OnInsert(User insertedValue, ReducerEvent? dbEvent)\r\n{\r\n    if(insertedValue.Online)\r\n    {\r\n        Console.WriteLine($\"{UserNameOrIdentity(insertedValue)} is online\");\r\n    }\r\n}\r\n```\r\n\r\n### Notify about updated users\r\n\r\nBecause we declared a primary key column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User::update_by_identity` calls. We register these callbacks using the `OnUpdate` method, which is automatically implemented by `spacetime generate` for any table with a primary key column.\r\n\r\n`OnUpdate` callbacks take three arguments: the old row, the new row, and a `ReducerEvent`.\r\n\r\nIn our module, users can be updated for three reasons:\r\n\r\n1. They've set their name using the `SetName` reducer.\r\n2. They're an existing user re-connecting, so their `Online` has been set to `true`.\r\n3. They've disconnected, so their `Online` has been set to `false`.\r\n\r\nWe'll print an appropriate message in each of these cases.\r\n\r\n```csharp\r\nvoid User_OnUpdate(User oldValue, User newValue, ReducerEvent dbEvent)\r\n{\r\n    if(oldValue.Name != newValue.Name)\r\n    {\r\n        Console.WriteLine($\"{UserNameOrIdentity(oldValue)} renamed to {newValue.Name}\");\r\n    }\r\n    if(oldValue.Online != newValue.Online)\r\n    {\r\n        if(newValue.Online)\r\n        {\r\n            Console.WriteLine($\"{UserNameOrIdentity(newValue)} connected.\");\r\n        }\r\n        else\r\n        {\r\n            Console.WriteLine($\"{UserNameOrIdentity(newValue)} disconnected.\");\r\n        }\r\n    }\r\n}\r\n```\r\n\r\n### Print messages\r\n\r\nWhen we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `SendMessage` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `OnInsert` callback will check if its `ReducerEvent` argument is not `null`, and only print in that case.\r\n\r\nTo find the `User` based on the message's `Sender` identity, we'll use `User::FilterByIdentity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `FilterByIdentity` accepts a `byte[]`, rather than an `Identity`. The `Sender` identity stored in the message is also a `byte[]`, not an `Identity`, so we can just pass it to the filter method.\r\n\r\nWe'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.\r\n\r\n```csharp\r\nvoid PrintMessage(Message message)\r\n{\r\n    var sender = User.FilterByIdentity(message.Sender);\r\n    var senderName = \"unknown\";\r\n    if(sender != null)\r\n    {\r\n        senderName = UserNameOrIdentity(sender);\r\n    }\r\n\r\n    Console.WriteLine($\"{senderName}: {message.Text}\");\r\n}\r\n\r\nvoid Message_OnInsert(Message insertedValue, ReducerEvent? dbEvent)\r\n{\r\n    if(dbEvent != null)\r\n    {\r\n        PrintMessage(insertedValue);\r\n    }\r\n}\r\n```\r\n\r\n### Warn if our name was rejected\r\n\r\nWe can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `OnReducerEvent` method of the `Reducer` namespace, which is automatically implemented for each reducer by `spacetime generate`.\r\n\r\nEach reducer callback takes one fixed argument:\r\n\r\nThe ReducerEvent that triggered the callback. It contains several fields. The ones we care about are:\r\n\r\n1. The `Identity` of the client that called the reducer.\r\n2. The `Status` of the reducer run, one of `Committed`, `Failed` or `OutOfEnergy`.\r\n3. The error message, if any, that the reducer returned.\r\n\r\nIt also takes a variable amount of additional arguments that match the reducer's arguments.\r\n\r\nThese callbacks will be invoked in one of two cases:\r\n\r\n1. If the reducer was successful and altered any of our subscribed rows.\r\n2. If we requested an invocation which failed.\r\n\r\nNote that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.\r\n\r\nWe already handle successful `SetName` invocations using our `User.OnUpdate` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `Reducer_OnSetNameEvent` as a `Reducer.OnSetNameEvent` callback which checks if the reducer failed, and if it did, prints an error message including the rejected name.\r\n\r\nWe'll test both that our identity matches the sender and that the status is `Failed`, even though the latter implies the former, for demonstration purposes.\r\n\r\n```csharp\r\nvoid Reducer_OnSetNameEvent(ReducerEvent reducerEvent, string name)\r\n{\r\n    if(reducerEvent.Identity == local_identity && reducerEvent.Status == ClientApi.Event.Types.Status.Failed)\r\n    {\r\n        Console.Write($\"Failed to change name to {name}\");\r\n    }\r\n}\r\n```\r\n\r\n### Warn if our message was rejected\r\n\r\nWe handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.\r\n\r\n```csharp\r\nvoid Reducer_OnSendMessageEvent(ReducerEvent reducerEvent, string text)\r\n{\r\n    if (reducerEvent.Identity == local_identity && reducerEvent.Status == ClientApi.Event.Types.Status.Failed)\r\n    {\r\n        Console.Write($\"Failed to send message {text}\");\r\n    }\r\n}\r\n```\r\n\r\n## Connect callback\r\n\r\nOnce we are connected, we can send our subscription to the SpacetimeDB module. SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the \"chunk\" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.\r\n\r\n```csharp\r\nvoid OnConnect()\r\n{\r\n    SpacetimeDBClient.instance.Subscribe(new List<string> { \"SELECT * FROM User\", \"SELECT * FROM Message\" });\r\n}\r\n```\r\n\r\n## OnIdentityReceived callback\r\n\r\nThis callback is executed when we receive our credentials from the SpacetimeDB module. We'll use the `AuthToken` module to save our token to local storage, so that we can re-authenticate as the same user the next time we connect. We'll also store the identity in a global variable `local_identity` so that we can use it to check if we are the sender of a message or name change.\r\n\r\n```csharp\r\nvoid OnIdentityReceived(string authToken, Identity identity)\r\n{\r\n    local_identity = identity;\r\n    AuthToken.SaveToken(authToken);\r\n}\r\n```\r\n\r\n## OnSubscriptionApplied callback\r\n\r\nOnce our subscription is applied, we'll print all the previously sent messages. We'll define a function `PrintMessagesInOrder` to do this. `PrintMessagesInOrder` calls the automatically generated `Iter` function on our `Message` table, which returns an iterator over all rows in the table. We'll use the `OrderBy` method on the iterator to sort the messages by their `Sent` timestamp.\r\n\r\n```csharp\r\nvoid PrintMessagesInOrder()\r\n{\r\n    foreach (Message message in Message.Iter().OrderBy(item => item.Sent))\r\n    {\r\n        PrintMessage(message);\r\n    }\r\n}\r\n\r\nvoid OnSubscriptionApplied()\r\n{\r\n    Console.WriteLine(\"Connected\");\r\n    PrintMessagesInOrder();\r\n}\r\n```\r\n\r\n<!-- FIXME: isn't OnSubscriptionApplied invoked every time the subscription results change? -->\r\n\r\n## Process thread\r\n\r\nSince the input loop will be blocking, we'll run our processing code in a separate thread. This thread will:\r\n\r\n1. Connect to the module. We'll store the SpacetimeDB host name and our module name in constants `HOST` and `DB_NAME`. We will also store if SSL is enabled in a constant called `SSL_ENABLED`. This only needs to be `true` if we are using `SpacetimeDB Cloud`. Replace `<module-name>` with the name you chose when publishing your module during the module quickstart.\r\n\r\n`Connect` takes an auth token, which is `null` for a new connection, or a stored string for a returning user. We are going to use the optional AuthToken module which uses local storage to store the auth token. If you want to use your own way to associate an auth token with a user, you can pass in your own auth token here.\r\n\r\n2. Loop until the thread is signaled to exit, calling `Update` on the SpacetimeDBClient to process any updates received from the module, and `ProcessCommand` to process any commands received from the input loop.\r\n\r\n3. Finally, Close the connection to the module.\r\n\r\n```csharp\r\nconst string HOST = \"localhost:3000\";\r\nconst string DBNAME = \"chat\";\r\nconst bool SSL_ENABLED = false;\r\n\r\nvoid ProcessThread()\r\n{\r\n    SpacetimeDBClient.instance.Connect(AuthToken.Token, HOST, DBNAME, SSL_ENABLED);\r\n\r\n    // loop until cancellation token\r\n    while (!cancel_token.IsCancellationRequested)\r\n    {\r\n        SpacetimeDBClient.instance.Update();\r\n\r\n        ProcessCommands();\r\n\r\n        Thread.Sleep(100);\r\n    }\r\n\r\n    SpacetimeDBClient.instance.Close();\r\n}\r\n```\r\n\r\n## Input loop and ProcessCommands\r\n\r\nThe input loop will read commands from standard input and send them to the processing thread using the input queue. The `ProcessCommands` function is called every 100ms by the processing thread to process any pending commands.\r\n\r\nSupported Commands:\r\n\r\n1. Send a message: `message`, send the message to the module by calling `Reducer.SendMessage` which is automatically generated by `spacetime generate`.\r\n\r\n2. Set name: `name`, will send the new name to the module by calling `Reducer.SetName` which is automatically generated by `spacetime generate`.\r\n\r\n```csharp\r\nvoid InputLoop()\r\n{\r\n    while (true)\r\n    {\r\n        var input = Console.ReadLine();\r\n        if(input == null)\r\n        {\r\n            break;\r\n        }\r\n\r\n        if(input.StartsWith(\"/name \"))\r\n        {\r\n            input_queue.Enqueue((\"name\", input.Substring(6)));\r\n            continue;\r\n        }\r\n        else\r\n        {\r\n            input_queue.Enqueue((\"message\", input));\r\n        }\r\n    }\r\n}\r\n\r\nvoid ProcessCommands()\r\n{\r\n    // process input queue commands\r\n    while (input_queue.TryDequeue(out var command))\r\n    {\r\n        switch (command.Item1)\r\n        {\r\n            case \"message\":\r\n                Reducer.SendMessage(command.Item2);\r\n                break;\r\n            case \"name\":\r\n                Reducer.SetName(command.Item2);\r\n                break;\r\n        }\r\n    }\r\n}\r\n```\r\n\r\n## Run the client\r\n\r\nFinally we just need to add a call to `Main` in `Program.cs`:\r\n\r\n```csharp\r\nMain();\r\n```\r\n\r\nNow we can run the client, by hitting start in Visual Studio or running the following command in the `client` directory:\r\n\r\n```bash\r\ndotnet run --project client\r\n```\r\n\r\n## What's next?\r\n\r\nCongratulations! You've built a simple chat app using SpacetimeDB. You can look at the C# SDK Reference for more information about the client SDK. If you are interested in developing in the Unity3d game engine, check out our Unity3d Comprehensive Tutorial and BitcraftMini game example.\r\n",
              "hasPages": false,
              "editUrl": "index.md",
              "jumpLinks": [
                {
                  "title": "C# Client SDK Quick Start",
                  "route": "c-client-sdk-quick-start",
                  "depth": 1
                },
                {
                  "title": "Project structure",
                  "route": "project-structure",
                  "depth": 2
                },
                {
                  "title": "Add the NuGet package for the C# SpacetimeDB SDK",
                  "route": "add-the-nuget-package-for-the-c-spacetimedb-sdk",
                  "depth": 2
                },
                {
                  "title": "Generate your module types",
                  "route": "generate-your-module-types",
                  "depth": 2
                },
                {
                  "title": "Add imports to Program.cs",
                  "route": "add-imports-to-program-cs",
                  "depth": 2
                },
                {
                  "title": "Define Main function",
                  "route": "define-main-function",
                  "depth": 2
                },
                {
                  "title": "Register callbacks",
                  "route": "register-callbacks",
                  "depth": 2
                },
                {
                  "title": "Notify about new users",
                  "route": "notify-about-new-users",
                  "depth": 3
                },
                {
                  "title": "Notify about updated users",
                  "route": "notify-about-updated-users",
                  "depth": 3
                },
                {
                  "title": "Print messages",
                  "route": "print-messages",
                  "depth": 3
                },
                {
                  "title": "Warn if our name was rejected",
                  "route": "warn-if-our-name-was-rejected",
                  "depth": 3
                },
                {
                  "title": "Warn if our message was rejected",
                  "route": "warn-if-our-message-was-rejected",
                  "depth": 3
                },
                {
                  "title": "Connect callback",
                  "route": "connect-callback",
                  "depth": 2
                },
                {
                  "title": "OnIdentityReceived callback",
                  "route": "onidentityreceived-callback",
                  "depth": 2
                },
                {
                  "title": "OnSubscriptionApplied callback",
                  "route": "onsubscriptionapplied-callback",
                  "depth": 2
                },
                {
                  "title": "Process thread",
                  "route": "process-thread",
                  "depth": 2
                },
                {
                  "title": "Input loop and ProcessCommands",
                  "route": "input-loop-and-processcommands",
                  "depth": 2
                },
                {
                  "title": "Run the client",
                  "route": "run-the-client",
                  "depth": 2
                },
                {
                  "title": "What's next?",
                  "route": "what-s-next-",
                  "depth": 2
                }
              ],
              "pages": []
            },
            {
              "title": "The SpacetimeDB C# client SDK",
              "identifier": "SDK Reference",
              "indexIdentifier": "SDK Reference",
              "hasPages": false,
              "content": "# The SpacetimeDB C# client SDK\r\n\r\nThe SpacetimeDB client C# for Rust contains all the tools you need to build native clients for SpacetimeDB modules using C#.\r\n\r\n## Table of Contents\r\n\r\n- [The SpacetimeDB C# client SDK](#the-spacetimedb-c-client-sdk)\r\n  - [Table of Contents](#table-of-contents)\r\n  - [Install the SDK](#install-the-sdk)\r\n    - [Using the `dotnet` CLI tool](#using-the-dotnet-cli-tool)\r\n    - [Using Unity](#using-unity)\r\n  - [Generate module bindings](#generate-module-bindings)\r\n  - [Initialization](#initialization)\r\n    - [Static Method `SpacetimeDBClient.CreateInstance`](#static-method-spacetimedbclientcreateinstance)\r\n    - [Property `SpacetimeDBClient.instance`](#property-spacetimedbclientinstance)\r\n    - [Class `NetworkManager`](#class-networkmanager)\r\n    - [Method `SpacetimeDBClient.Connect`](#method-spacetimedbclientconnect)\r\n    - [Event `SpacetimeDBClient.onIdentityReceived`](#event-spacetimedbclientonidentityreceived)\r\n    - [Event `SpacetimeDBClient.onConnect`](#event-spacetimedbclientonconnect)\r\n  - [Subscribe to queries](#subscribe-to-queries)\r\n    - [Method `SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe)\r\n    - [Event `SpacetimeDBClient.onSubscriptionApplied`](#event-spacetimedbclientonsubscriptionapplied)\r\n  - [View rows of subscribed tables](#view-rows-of-subscribed-tables)\r\n    - [Class `{TABLE}`](#class-table)\r\n      - [Static Method `{TABLE}.Iter`](#static-method-tableiter)\r\n      - [Static Method `{TABLE}.FilterBy{COLUMN}`](#static-method-tablefilterbycolumn)\r\n      - [Static Method `{TABLE}.Count`](#static-method-tablecount)\r\n      - [Static Event `{TABLE}.OnInsert`](#static-event-tableoninsert)\r\n      - [Static Event `{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete)\r\n      - [Static Event `{TABLE}.OnDelete`](#static-event-tableondelete)\r\n      - [Static Event `{TABLE}.OnUpdate`](#static-event-tableonupdate)\r\n  - [Observe and invoke reducers](#observe-and-invoke-reducers)\r\n    - [Class `Reducer`](#class-reducer)\r\n      - [Static Method `Reducer.{REDUCER}`](#static-method-reducerreducer)\r\n      - [Static Event `Reducer.On{REDUCER}`](#static-event-reduceronreducer)\r\n    - [Class `ReducerEvent`](#class-reducerevent)\r\n      - [Enum `Status`](#enum-status)\r\n        - [Variant `Status.Committed`](#variant-statuscommitted)\r\n        - [Variant `Status.Failed`](#variant-statusfailed)\r\n        - [Variant `Status.OutOfEnergy`](#variant-statusoutofenergy)\r\n  - [Identity management](#identity-management)\r\n    - [Class `AuthToken`](#class-authtoken)\r\n      - [Static Method `AuthToken.Init`](#static-method-authtokeninit)\r\n      - [Static Property `AuthToken.Token`](#static-property-authtokentoken)\r\n      - [Static Method `AuthToken.SaveToken`](#static-method-authtokensavetoken)\r\n    - [Class `Identity`](#class-identity)\r\n  - [Customizing logging](#customizing-logging)\r\n    - [Interface `ISpacetimeDBLogger`](#interface-ispacetimedblogger)\r\n    - [Class `ConsoleLogger`](#class-consolelogger)\r\n    - [Class `UnityDebugLogger`](#class-unitydebuglogger)\r\n\r\n## Install the SDK\r\n\r\n### Using the `dotnet` CLI tool\r\n\r\nIf you would like to create a console application using .NET, you can create a new project using `dotnet new console` and add the SpacetimeDB SDK to your dependencies:\r\n\r\n```bash\r\ndotnet add package spacetimedbsdk\r\n```\r\n\r\n(See also the [CSharp Quickstart](./CSharpSDKQuickStart) for an in-depth example of such a console application.)\r\n\r\n### Using Unity\r\n\r\nTo install the SpacetimeDB SDK into a Unity project, download the SpacetimeDB SDK from the following link.\r\n\r\nhttps://sdk.spacetimedb.com/SpacetimeDBUnitySDK.unitypackage\r\n\r\nIn Unity navigate to the `Assets > Import Package > Custom Package...` menu in the menu bar. Select your `SpacetimeDBUnitySDK.unitypackage` file and leave all folders checked.\r\n\r\n(See also the [Unity Quickstart](./UnityQuickStart) and [Unity Tutorial](./UnityTutorialPart1).)\r\n\r\n## Generate module bindings\r\n\r\nEach SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's directory and generate the C# interface files using the Spacetime CLI. From your project directory, run:\r\n\r\n```bash\r\nmkdir -p module_bindings\r\nspacetime generate --lang cs --out-dir module_bindings --project-path PATH-TO-MODULE-DIRECTORY\r\n```\r\n\r\nReplace `PATH-TO-MODULE-DIRECTORY` with the path to your SpacetimeDB module.\r\n\r\n## Initialization\r\n\r\n### Static Method `SpacetimeDBClient.CreateInstance`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\npublic class SpacetimeDBClient {\r\n    public static void CreateInstance(ISpacetimeDBLogger loggerToUse);\r\n}\r\n\r\n}\r\n```\r\n\r\nCreate a global SpacetimeDBClient instance, accessible via [`SpacetimeDBClient.instance`](#property-spacetimedbclientinstance)\r\n\r\n| Argument      | Type                                                  | Meaning                           |\r\n| ------------- | ----------------------------------------------------- | --------------------------------- |\r\n| `loggerToUse` | [`ISpacetimeDBLogger`](#interface-ispacetimedblogger) | The logger to use to log messages |\r\n\r\nThere is a provided logger called [`ConsoleLogger`](#class-consolelogger) which logs to `System.Console`, and can be used as follows:\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\nSpacetimeDBClient.CreateInstance(new ConsoleLogger());\r\n```\r\n\r\n### Property `SpacetimeDBClient.instance`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\npublic class SpacetimeDBClient {\r\n    public static SpacetimeDBClient instance;\r\n}\r\n\r\n}\r\n```\r\n\r\nThis is the global instance of a SpacetimeDB client in a particular .NET/Unity process. Much of the SDK is accessible through this instance.\r\n\r\n### Class `NetworkManager`\r\n\r\nThe Unity SpacetimeDB SDK relies on there being a `NetworkManager` somewhere in the scene. Click on the GameManager object in the scene, and in the inspector, add the `NetworkManager` component.\r\n\r\n![Unity-AddNetworkManager](/images/unity-tutorial/Unity-AddNetworkManager.JPG)\r\n\r\nThis component will handle calling [`SpacetimeDBClient.CreateInstance`](#static-method-spacetimedbclientcreateinstance) for you, but will not call [`SpacetimeDBClient.Connect`](#method-spacetimedbclientconnect), you still need to handle that yourself. See the [Unity Quickstart](./UnityQuickStart) and [Unity Tutorial](./UnityTutorialPart1) for more information.\r\n\r\n### Method `SpacetimeDBClient.Connect`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass SpacetimeDBClient {\r\n    public void Connect(\r\n        string? token,\r\n        string host,\r\n        string addressOrName,\r\n        bool sslEnabled = true\r\n    );\r\n}\r\n\r\n}\r\n```\r\n\r\n<!-- FIXME: `token` is not currently marked as nullable in the API, but it should be. -->\r\n\r\nConnect to a database named `addressOrName` accessible over the internet at the URI `host`.\r\n\r\n| Argument        | Type      | Meaning                                                                    |\r\n| --------------- | --------- | -------------------------------------------------------------------------- |\r\n| `token`         | `string?` | Identity token to use, if one is available.                                |\r\n| `host`          | `string`  | URI of the SpacetimeDB instance running the module.                        |\r\n| `addressOrName` | `string`  | Address or name of the module.                                             |\r\n| `sslEnabled`    | `bool`    | Whether or not to use SSL when connecting to SpacetimeDB. Default: `true`. |\r\n\r\nIf a `token` is supplied, it will be passed to the new connection to identify and authenticate the user. Otherwise, a new token and [`Identity`](#class-identity) will be generated by the server and returned in [`onConnect`](#event-spacetimedbclientonconnect).\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\nconst string DBNAME = \"chat\";\r\n\r\n// Connect to a local DB with a fresh identity\r\nSpacetimeDBClient.instance.Connect(null, \"localhost:3000\", DBNAME, false);\r\n\r\n// Connect to cloud with a fresh identity\r\nSpacetimeDBClient.instance.Connect(null, \"dev.spacetimedb.net\", DBNAME, true);\r\n\r\n// Connect to cloud using a saved identity from the filesystem, or get a new one and save it\r\nAuthToken.Init();\r\nIdentity localIdentity;\r\nSpacetimeDBClient.instance.Connect(AuthToken.Token, \"dev.spacetimedb.net\", DBNAME, true);\r\nSpacetimeDBClient.instance.onIdentityReceived += (string authToken, Identity identity) {\r\n    AuthToken.SaveToken(authToken);\r\n    localIdentity = identity;\r\n}\r\n```\r\n\r\n(You should probably also store the returned `Identity` somewhere; see the [`onIdentityReceived`](#event-spacetimedbclientonidentityreceived) event.)\r\n\r\n### Event `SpacetimeDBClient.onIdentityReceived`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass SpacetimeDBClient {\r\n    public event Action<string, Identity> onIdentityReceived;\r\n}\r\n\r\n}\r\n```\r\n\r\nCalled when we receive an auth token and [`Identity`](#class-identity) from the server. The [`Identity`](#class-identity) serves as a unique public identifier for a client connected to the database. It can be for several purposes, such as filtering rows in a database for the rows created by a particular user. The auth token is a private access token that allows us to assume an identity.\r\n\r\nTo store the auth token to the filesystem, use the static method [`AuthToken.SaveToken`](#static-method-authtokensavetoken). You may also want to store the returned [`Identity`](#class-identity) in a local variable.\r\n\r\nIf an existing auth token is used to connect to the database, the same auth token and the identity it came with will be returned verbatim in `onIdentityReceived`.\r\n\r\n```cs\r\n// Connect to cloud using a saved identity from the filesystem, or get a new one and save it\r\nAuthToken.Init();\r\nIdentity localIdentity;\r\nSpacetimeDBClient.instance.Connect(AuthToken.Token, \"dev.spacetimedb.net\", DBNAME, true);\r\nSpacetimeDBClient.instance.onIdentityReceived += (string authToken, Identity identity) {\r\n    AuthToken.SaveToken(authToken);\r\n    localIdentity = identity;\r\n}\r\n```\r\n\r\n### Event `SpacetimeDBClient.onConnect`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass SpacetimeDBClient {\r\n    public event Action onConnect;\r\n}\r\n\r\n}\r\n```\r\n\r\nAllows registering delegates to be invoked upon authentication with the database.\r\n\r\nOnce this occurs, the SDK is prepared for calls to [`SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe).\r\n\r\n## Subscribe to queries\r\n\r\n### Method `SpacetimeDBClient.Subscribe`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass SpacetimeDBClient {\r\n    public void Subscribe(List<string> queries);\r\n}\r\n\r\n}\r\n```\r\n\r\n| Argument  | Type           | Meaning                      |\r\n| --------- | -------------- | ---------------------------- |\r\n| `queries` | `List<string>` | SQL queries to subscribe to. |\r\n\r\nSubscribe to a set of queries, to be notified when rows which match those queries are altered.\r\n\r\n`Subscribe` will return an error if called before establishing a connection with the [`SpacetimeDBClient.Connect`](#method-connect) function. In that case, the queries are not registered.\r\n\r\nThe `Subscribe` method does not return data directly. `spacetime generate` will generate classes [`SpacetimeDB.Types.{TABLE}`](#class-table) for each table in your module. These classes are used to reecive information from the database. See the section [View Rows of Subscribed Tables](#view-rows-of-subscribed-tables) for more information.\r\n\r\nA new call to `Subscribe` will remove all previous subscriptions and replace them with the new `queries`. If any rows matched the previous subscribed queries but do not match the new queries, those rows will be removed from the client cache, and [`{TABLE}.OnDelete`](#event-tableondelete) callbacks will be invoked for them.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\nvoid Main()\r\n{\r\n    AuthToken.Init();\r\n    SpacetimeDBClient.CreateInstance(new ConsoleLogger());\r\n\r\n    SpacetimeDBClient.instance.onConnect += OnConnect;\r\n\r\n    // Our module contains a table named \"Loot\"\r\n    Loot.OnInsert += Loot_OnInsert;\r\n\r\n    SpacetimeDBClient.instance.Connect(/* ... */);\r\n}\r\n\r\nvoid OnConnect()\r\n{\r\n    SpacetimeDBClient.instance.Subscribe(new List<string> {\r\n        \"SELECT * FROM Loot\"\r\n    });\r\n}\r\n\r\nvoid Loot_OnInsert(\r\n    Loot loot,\r\n    ReducerEvent? event\r\n) {\r\n    Console.Log($\"Loaded loot {loot.itemType} at coordinates {loot.position}\");\r\n}\r\n```\r\n\r\n### Event `SpacetimeDBClient.onSubscriptionApplied`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass SpacetimeDBClient {\r\n    public event Action onSubscriptionApplied;\r\n}\r\n\r\n}\r\n```\r\n\r\nRegister a delegate to be invoked when a subscription is registered with the database.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\n\r\nvoid OnSubscriptionApplied()\r\n{\r\n    Console.WriteLine(\"Now listening on queries.\");\r\n}\r\n\r\nvoid Main()\r\n{\r\n    // ...initialize...\r\n    SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;\r\n}\r\n```\r\n\r\n## View rows of subscribed tables\r\n\r\nThe SDK maintains a local view of the database called the \"client cache\". This cache contains whatever rows are selected via a call to [`SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe). These rows are represented in the SpacetimeDB .Net SDK as instances of [`SpacetimeDB.Types.{TABLE}`](#class-table).\r\n\r\nONLY the rows selected in a [`SpacetimeDBClient.Subscribe`](#method-spacetimedbclientsubscribe) call will be available in the client cache. All operations in the client sdk operate on these rows exclusively, and have no information about the state of the rest of the database.\r\n\r\nIn particular, SpacetimeDB does not support foreign key constraints. This means that if you are using a column as a foreign key, SpacetimeDB will not automatically bring in all of the rows that key might reference. You will need to manually subscribe to all tables you need information from.\r\n\r\nTo optimize network performance, prefer selecting as few rows as possible in your [`Subscribe`](#method-spacetimedbclientsubscribe) query. Processes that need to view the entire state of the database are better run inside the database -- that is, inside modules.\r\n\r\n### Class `{TABLE}`\r\n\r\nFor each table defined by a module, `spacetime generate` will generate a class [`SpacetimeDB.Types.{TABLE}`](#class-table) whose name is that table's name converted to `PascalCase`. The generated class contains a property for each of the table's columns, whose names are the column names converted to `camelCase`. It also contains various static events and methods.\r\n\r\nStatic Methods:\r\n\r\n- [`{TABLE}.Iter()`](#static-method-tableiter) iterates all subscribed rows in the client cache.\r\n- [`{TABLE}.FilterBy{COLUMN}(value)`](#static-method-tablefilterbycolumn) filters subscribed rows in the client cache by a column value.\r\n- [`{TABLE}.Count()`](#static-method-tablecount) counts the number of subscribed rows in the client cache.\r\n\r\nStatic Events:\r\n\r\n- [`{TABLE}.OnInsert`](#static-event-tableoninsert) is called when a row is inserted into the client cache.\r\n- [`{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete) is called when a row is about to be removed from the client cache.\r\n- If the table has a primary key attribute, [`{TABLE}.OnUpdate`](#static-event-tableonupdate) is called when a row is updated.\r\n- [`{TABLE}.OnDelete`](#static-event-tableondelete) is called while a row is being removed from the client cache. You should almost always use [`{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete) instead.\r\n\r\nNote that it is not possible to directly insert into the database from the client SDK! All insertion validation should be performed inside serverside modules for security reasons. You can instead [invoke reducers](#observe-and-invoke-reducers), which run code inside the database that can insert rows for you.\r\n\r\n#### Static Method `{TABLE}.Iter`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    public static System.Collections.Generic.IEnumerable<TABLE> Iter();\r\n}\r\n\r\n}\r\n```\r\n\r\nIterate over all the subscribed rows in the table. This method is only available after [`SpacetimeDBClient.onSubscriptionApplied`](#event-spacetimedbclientonsubscriptionapplied) has occurred.\r\n\r\nWhen iterating over rows and filtering for those containing a particular column, [`TableType::filter`](#method-filter) will be more efficient, so prefer it when possible.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\nSpacetimeDBClient.instance.onConnect += (string authToken, Identity identity) => {\r\n    SpacetimeDBClient.instance.Subscribe(new List<string> { \"SELECT * FROM User\" });\r\n};\r\nSpacetimeDBClient.instance.onSubscriptionApplied += () => {\r\n    // Will print a line for each `User` row in the database.\r\n    foreach (var user in User.Iter()) {\r\n        Console.WriteLine($\"User: {user.Name}\");\r\n    }\r\n};\r\nSpacetimeDBClient.instance.connect(/* ... */);\r\n```\r\n\r\n#### Static Method `{TABLE}.FilterBy{COLUMN}`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    // If the column has no #[unique] or #[primarykey] constraint\r\n    public static System.Collections.Generic.IEnumerable<TABLE> FilterBySender(COLUMNTYPE value);\r\n\r\n    // If the column has a #[unique] or #[primarykey] constraint\r\n    public static TABLE? FilterBySender(COLUMNTYPE value);\r\n}\r\n\r\n}\r\n```\r\n\r\nFor each column of a table, `spacetime generate` generates a static method on the [table class](#class-table) to filter or seek subscribed rows where that column matches a requested value. These methods are named `filterBy{COLUMN}`, where `{COLUMN}` is the column name converted to `PascalCase`.\r\n\r\nThe method's return type depends on the column's attributes:\r\n\r\n- For unique columns, including those annotated `#[unique]` and `#[primarykey]`, the `filterBy{COLUMN}` method returns a `{TABLE}?`, where `{TABLE}` is the [table class](#class-table).\r\n- For non-unique columns, the `filter_by` method returns an `IEnumerator<{TABLE}>`.\r\n\r\n#### Static Method `{TABLE}.Count`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    public static int Count();\r\n}\r\n\r\n}\r\n```\r\n\r\nReturn the number of subscribed rows in the table, or 0 if there is no active connection.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\nSpacetimeDBClient.instance.onConnect += (string authToken, Identity identity) => {\r\n    SpacetimeDBClient.instance.Subscribe(new List<string> { \"SELECT * FROM User\" });\r\n};\r\nSpacetimeDBClient.instance.onSubscriptionApplied += () => {\r\n    Console.WriteLine($\"There are {User.Count()} users in the database.\");\r\n};\r\nSpacetimeDBClient.instance.connect(/* ... */);\r\n```\r\n\r\n#### Static Event `{TABLE}.OnInsert`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    public delegate void InsertEventHandler(\r\n        TABLE insertedValue,\r\n        ReducerEvent? dbEvent\r\n    );\r\n    public static event InsertEventHandler OnInsert;\r\n}\r\n\r\n}\r\n```\r\n\r\nRegister a delegate for when a subscribed row is newly inserted into the database.\r\n\r\nThe delegate takes two arguments:\r\n\r\n- A [`{TABLE}`](#class-table) instance with the data of the inserted row\r\n- A [`ReducerEvent?`], which contains the data of the reducer that inserted the row, or `null` if the row is being inserted while initializing a subscription.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\n/* initialize, subscribe to table User... */\r\n\r\nUser.OnInsert += (User user, ReducerEvent? reducerEvent) => {\r\n    if (reducerEvent == null) {\r\n        Console.WriteLine($\"New user '{user.Name}' received during subscription update.\");\r\n    } else {\r\n        Console.WriteLine($\"New user '{user.Name}' inserted by reducer {reducerEvent.Reducer}.\");\r\n    }\r\n};\r\n```\r\n\r\n#### Static Event `{TABLE}.OnBeforeDelete`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    public delegate void DeleteEventHandler(\r\n        TABLE deletedValue,\r\n        ReducerEvent dbEvent\r\n    );\r\n    public static event DeleteEventHandler OnBeforeDelete;\r\n}\r\n\r\n}\r\n```\r\n\r\nRegister a delegate for when a subscribed row is about to be deleted from the database. If a reducer deletes many rows at once, this delegate will be invoked for each of those rows before any of them is deleted.\r\n\r\nThe delegate takes two arguments:\r\n\r\n- A [`{TABLE}`](#class-table) instance with the data of the deleted row\r\n- A [`ReducerEvent`](#class-reducerevent), which contains the data of the reducer that deleted the row.\r\n\r\nThis event should almost always be used instead of [`OnDelete`](#static-event-tableondelete). This is because often, many rows will be deleted at once, and `OnDelete` can be invoked in an arbitrary order on these rows. This means that data related to a row may already be missing when `OnDelete` is called. `OnBeforeDelete` does not have this problem.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\n/* initialize, subscribe to table User... */\r\n\r\nUser.OnBeforeDelete += (User user, ReducerEvent reducerEvent) => {\r\n    Console.WriteLine($\"User '{user.Name}' deleted by reducer {reducerEvent.Reducer}.\");\r\n};\r\n```\r\n\r\n#### Static Event `{TABLE}.OnDelete`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    public delegate void DeleteEventHandler(\r\n        TABLE deletedValue,\r\n        SpacetimeDB.ReducerEvent dbEvent\r\n    );\r\n    public static event DeleteEventHandler OnDelete;\r\n}\r\n\r\n}\r\n```\r\n\r\nRegister a delegate for when a subscribed row is being deleted from the database. If a reducer deletes many rows at once, this delegate will be invoked on those rows in arbitrary order, and data for some rows may already be missing when it is invoked. For this reason, prefer the event [`{TABLE}.OnBeforeDelete`](#static-event-tableonbeforedelete).\r\n\r\nThe delegate takes two arguments:\r\n\r\n- A [`{TABLE}`](#class-table) instance with the data of the deleted row\r\n- A [`ReducerEvent`](#class-reducerevent), which contains the data of the reducer that deleted the row.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\n/* initialize, subscribe to table User... */\r\n\r\nUser.OnBeforeDelete += (User user, ReducerEvent reducerEvent) => {\r\n    Console.WriteLine($\"User '{user.Name}' deleted by reducer {reducerEvent.Reducer}.\");\r\n};\r\n```\r\n\r\n#### Static Event `{TABLE}.OnUpdate`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass TABLE {\r\n    public delegate void UpdateEventHandler(\r\n        TABLE oldValue,\r\n        TABLE newValue,\r\n        ReducerEvent dbEvent\r\n    );\r\n    public static event UpdateEventHandler OnUpdate;\r\n}\r\n\r\n}\r\n```\r\n\r\nRegister a delegate for when a subscribed row is being updated. This event is only available if the row has a column with the `#[primary_key]` attribute.\r\n\r\nThe delegate takes three arguments:\r\n\r\n- A [`{TABLE}`](#class-table) instance with the old data of the updated row\r\n- A [`{TABLE}`](#class-table) instance with the new data of the updated row\r\n- A [`ReducerEvent`](#class-reducerevent), which contains the data of the reducer that updated the row.\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\n\r\n/* initialize, subscribe to table User... */\r\n\r\nUser.OnUpdate += (User oldUser, User newUser, ReducerEvent reducerEvent) => {\r\n    Debug.Assert(oldUser.UserId == newUser.UserId, \"Primary key never changes in an update\");\r\n\r\n    Console.WriteLine($\"User with ID {oldUser.UserId} had name changed \"+\r\n    $\"from '{oldUser.Name}' to '{newUser.Name}' by reducer {reducerEvent.Reducer}.\");\r\n};\r\n```\r\n\r\n## Observe and invoke reducers\r\n\r\n\"Reducer\" is SpacetimeDB's name for the stored procedures that run in modules inside the database. You can invoke reducers from a connected client SDK, and also receive information about which reducers are running.\r\n\r\n`spacetime generate` generates a class [`SpacetimeDB.Types.Reducer`](#class-reducer) that contains methods and events for each reducer defined in a module. To invoke a reducer, use the method [`Reducer.{REDUCER}`](#static-method-reducerreducer) generated for it. To receive a callback each time a reducer is invoked, use the static event [`Reducer.On{REDUCER}`](#static-event-reduceronreducer).\r\n\r\n### Class `Reducer`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\nclass Reducer {}\r\n\r\n}\r\n```\r\n\r\nThis class contains a static method and event for each reducer defined in a module.\r\n\r\n#### Static Method `Reducer.{REDUCER}`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\nclass Reducer {\r\n\r\n/* void {REDUCER_NAME}(...ARGS...) */\r\n\r\n}\r\n}\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates a static method which sends a request to the database to invoke that reducer. The generated function's name is the reducer's name converted to `PascalCase`.\r\n\r\nReducers don't run immediately! They run as soon as the request reaches the database. Don't assume data inserted by a reducer will be available immediately after you call this method.\r\n\r\nFor reducers which accept a `ReducerContext` as their first argument, the `ReducerContext` is not included in the generated function's argument list.\r\n\r\nFor example, if we define a reducer in Rust as follows:\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\npub fn set_name(\r\n    ctx: ReducerContext,\r\n    user_id: u64,\r\n    name: String\r\n) -> Result<(), Error>;\r\n```\r\n\r\nThe following C# static method will be generated:\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\nclass Reducer {\r\n\r\npublic static void SendMessage(UInt64 userId, string name);\r\n\r\n}\r\n}\r\n```\r\n\r\n#### Static Event `Reducer.On{REDUCER}`\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\nclass Reducer {\r\n\r\npublic delegate void /*{REDUCER}*/Handler(ReducerEvent reducerEvent, /* {ARGS...} */);\r\n\r\npublic static event /*{REDUCER}*/Handler On/*{REDUCER}*/Event;\r\n\r\n}\r\n}\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates an event to run each time the reducer is invoked. The generated functions are named `on{REDUCER}Event`, where `{REDUCER}` is the reducer's name converted to `PascalCase`.\r\n\r\nThe first argument to the event handler is an instance of [`SpacetimeDB.Types.ReducerEvent`](#class-reducerevent) describing the invocation -- its timestamp, arguments, and whether it succeeded or failed. The remaining arguments are the arguments passed to the reducer. Reducers cannot have return values, so no return value information is included.\r\n\r\nFor example, if we define a reducer in Rust as follows:\r\n\r\n```rust\r\n#[spacetimedb(reducer)]\r\npub fn set_name(\r\n    ctx: ReducerContext,\r\n    user_id: u64,\r\n    name: String\r\n) -> Result<(), Error>;\r\n```\r\n\r\nThe following C# static method will be generated:\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\nclass Reducer {\r\n\r\npublic delegate void SetNameHandler(\r\n    ReducerEvent reducerEvent,\r\n    UInt64 userId,\r\n    string name\r\n);\r\npublic static event SetNameHandler OnSetNameEvent;\r\n\r\n}\r\n}\r\n```\r\n\r\nWhich can be used as follows:\r\n\r\n```cs\r\n/* initialize, wait for onSubscriptionApplied... */\r\n\r\nReducer.SetNameHandler += (\r\n    ReducerEvent reducerEvent,\r\n    UInt64 userId,\r\n    string name\r\n) => {\r\n    if (reducerEvent.Status == ClientApi.Event.Types.Status.Committed) {\r\n        Console.WriteLine($\"User with id {userId} set name to {name}\");\r\n    } else if (reducerEvent.Status == ClientApi.Event.Types.Status.Failed) {\r\n        Console.WriteLine(\r\n            $\"User with id {userId} failed to set name to {name}:\"\r\n            + reducerEvent.ErrMessage\r\n        );\r\n    } else if (reducerEvent.Status == ClientApi.Event.Types.Status.OutOfEnergy) {\r\n        Console.WriteLine(\r\n            $\"User with id {userId} failed to set name to {name}:\"\r\n            + \"Invoker ran out of energy\"\r\n        );\r\n    }\r\n};\r\nReducer.SetName(USER_ID, NAME);\r\n```\r\n\r\n### Class `ReducerEvent`\r\n\r\n`spacetime generate` defines an class `ReducerEvent` containing an enum `ReducerType` with a variant for each reducer defined by a module. The variant's name will be the reducer's name converted to `PascalCase`.\r\n\r\nFor example, the example project shown in the Rust Module quickstart will generate the following (abridged) code.\r\n\r\n```cs\r\nnamespace SpacetimeDB.Types {\r\n\r\npublic enum ReducerType\r\n{\r\n    /* A member for each reducer in the module, with names converted to PascalCase */\r\n    None,\r\n    SendMessage,\r\n    SetName,\r\n}\r\npublic partial class SendMessageArgsStruct\r\n{\r\n    /* A member for each argument of the reducer SendMessage, with names converted to PascalCase. */\r\n    public string Text;\r\n}\r\npublic partial class SetNameArgsStruct\r\n{\r\n    /* A member for each argument of the reducer SetName, with names converted to PascalCase. */\r\n    public string Name;\r\n}\r\npublic partial class ReducerEvent : ReducerEventBase {\r\n    // Which reducer was invoked\r\n    public ReducerType Reducer { get; }\r\n    // If event.Reducer == ReducerType.SendMessage, the arguments\r\n    // sent to the SendMessage reducer. Otherwise, accesses will\r\n    // throw a runtime error.\r\n    public SendMessageArgsStruct SendMessageArgs { get; }\r\n    // If event.Reducer == ReducerType.SetName, the arguments\r\n    // passed to the SetName reducer. Otherwise, accesses will\r\n    // throw a runtime error.\r\n    public SetNameArgsStruct SetNameArgs { get; }\r\n\r\n    /* Additional information, present on any ReducerEvent */\r\n    // The name of the reducer.\r\n    public string ReducerName { get; }\r\n    // The timestamp of the reducer invocation inside the database.\r\n    public ulong Timestamp { get; }\r\n    // The identity of the client that invoked the reducer.\r\n    public SpacetimeDB.Identity Identity { get; }\r\n    // Whether the reducer succeeded, failed, or ran out of energy.\r\n    public ClientApi.Event.Types.Status Status { get; }\r\n    // If event.Status == Status.Failed, the error message returned from inside the module.\r\n    public string ErrMessage { get; }\r\n}\r\n\r\n}\r\n```\r\n\r\n#### Enum `Status`\r\n\r\n```cs\r\nnamespace ClientApi {\r\npublic sealed partial class Event {\r\npublic static partial class Types {\r\n\r\npublic enum Status {\r\n    Committed = 0,\r\n    Failed = 1,\r\n    OutOfEnergy = 2,\r\n}\r\n\r\n}\r\n}\r\n}\r\n```\r\n\r\nAn enum whose variants represent possible reducer completion statuses of a reducer invocation.\r\n\r\n##### Variant `Status.Committed`\r\n\r\nThe reducer finished successfully, and its row changes were committed to the database.\r\n\r\n##### Variant `Status.Failed`\r\n\r\nThe reducer failed, either by panicking or returning a `Err`.\r\n\r\n##### Variant `Status.OutOfEnergy`\r\n\r\nThe reducer was canceled because the module owner had insufficient energy to allow it to run to completion.\r\n\r\n## Identity management\r\n\r\n### Class `AuthToken`\r\n\r\nThe AuthToken helper class handles creating and saving SpacetimeDB identity tokens in the filesystem.\r\n\r\n#### Static Method `AuthToken.Init`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass AuthToken {\r\n    public static void Init(\r\n        string configFolder = \".spacetime_csharp_sdk\",\r\n        string configFile = \"settings.ini\",\r\n        string? configRoot = null\r\n    );\r\n}\r\n\r\n}\r\n```\r\n\r\nCreates a file `$\"{configRoot}/{configFolder}/{configFile}\"` to store tokens.\r\nIf no arguments are passed, the default is `\"%HOME%/.spacetime_csharp_sdk/settings.ini\"`.\r\n\r\n| Argument       | Type     | Meaning                                                                            |\r\n| -------------- | -------- | ---------------------------------------------------------------------------------- |\r\n| `configFolder` | `string` | The folder to store the config file in. Default is `\"spacetime_csharp_sdk\"`.       |\r\n| `configFile`   | `string` | The name of the config file. Default is `\"settings.ini\"`.                          |\r\n| `configRoot`   | `string` | The root folder to store the config file in. Default is the user's home directory. |\r\n\r\n#### Static Property `AuthToken.Token`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass AuthToken {\r\n    public static string? Token { get; }\r\n}\r\n\r\n}\r\n```\r\n\r\nThe auth token stored on the filesystem, if one exists.\r\n\r\n#### Static Method `AuthToken.SaveToken`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\nclass AuthToken {\r\n    public static void SaveToken(string token);\r\n}\r\n\r\n}\r\n```\r\n\r\nSave a token to the filesystem.\r\n\r\n### Class `Identity`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\npublic struct Identity : IEquatable<Identity>\r\n{\r\n    public byte[] Bytes { get; }\r\n    public static Identity From(byte[] bytes);\r\n    public bool Equals(Identity other);\r\n    public static bool operator ==(Identity a, Identity b);\r\n    public static bool operator !=(Identity a, Identity b);\r\n}\r\n\r\n}\r\n```\r\n\r\nA unique public identifier for a client connected to a database.\r\n\r\nColumns of type `Identity` inside a module will be represented in the C# SDK as properties of type `byte[]`. `Identity` is essentially just a wrapper around `byte[]`, and you can use the `Bytes` property to get a `byte[]` that can be used to filter tables and so on.\r\n\r\n## Customizing logging\r\n\r\nThe SpacetimeDB C# SDK performs internal logging. Instances of [`ISpacetimeDBLogger`](#interface-ispacetimedblogger) can be passed to [`SpacetimeDBClient.CreateInstance`](#static-method-spacetimedbclientcreateinstance) to customize how SDK logs are delivered to your application.\r\n\r\nThis is set up automatically for you if you use Unity-- adding a [`NetworkManager`](#class-networkmanager) component to your unity scene will automatically initialize the `SpacetimeDBClient` with a [`UnityDebugLogger`](#class-unitydebuglogger).\r\n\r\nOutside of unity, all you need to do is the following:\r\n\r\n```cs\r\nusing SpacetimeDB;\r\nusing SpacetimeDB.Types;\r\nSpacetimeDBClient.CreateInstance(new ConsoleLogger());\r\n```\r\n\r\n### Interface `ISpacetimeDBLogger`\r\n\r\n```cs\r\nnamespace SpacetimeDB\r\n{\r\n\r\npublic interface ISpacetimeDBLogger\r\n{\r\n    void Log(string message);\r\n    void LogError(string message);\r\n    void LogWarning(string message);\r\n    void LogException(Exception e);\r\n}\r\n\r\n}\r\n```\r\n\r\nThis interface provides methods that are invoked when the SpacetimeDB C# SDK needs to log at various log levels. You can create custom implementations if needed to integrate with existing logging solutions.\r\n\r\n### Class `ConsoleLogger`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\npublic class ConsoleLogger : ISpacetimeDBLogger {}\r\n\r\n}\r\n```\r\n\r\nAn `ISpacetimeDBLogger` implementation for regular .NET applications, using `Console.Write` when logs are received.\r\n\r\n### Class `UnityDebugLogger`\r\n\r\n```cs\r\nnamespace SpacetimeDB {\r\n\r\npublic class UnityDebugLogger : ISpacetimeDBLogger {}\r\n\r\n}\r\n```\r\n\r\nAn `ISpacetimeDBLogger` implementation for Unity, using the Unity `Debug.Log` api.\r\n",
              "editUrl": "SDK%20Reference.md",
              "jumpLinks": [
                {
                  "title": "The SpacetimeDB C# client SDK",
                  "route": "the-spacetimedb-c-client-sdk",
                  "depth": 1
                },
                {
                  "title": "Table of Contents",
                  "route": "table-of-contents",
                  "depth": 2
                },
                {
                  "title": "Install the SDK",
                  "route": "install-the-sdk",
                  "depth": 2
                },
                {
                  "title": "Using the `dotnet` CLI tool",
                  "route": "using-the-dotnet-cli-tool",
                  "depth": 3
                },
                {
                  "title": "Using Unity",
                  "route": "using-unity",
                  "depth": 3
                },
                {
                  "title": "Generate module bindings",
                  "route": "generate-module-bindings",
                  "depth": 2
                },
                {
                  "title": "Initialization",
                  "route": "initialization",
                  "depth": 2
                },
                {
                  "title": "Static Method `SpacetimeDBClient.CreateInstance`",
                  "route": "static-method-spacetimedbclient-createinstance-",
                  "depth": 3
                },
                {
                  "title": "Property `SpacetimeDBClient.instance`",
                  "route": "property-spacetimedbclient-instance-",
                  "depth": 3
                },
                {
                  "title": "Class `NetworkManager`",
                  "route": "class-networkmanager-",
                  "depth": 3
                },
                {
                  "title": "Method `SpacetimeDBClient.Connect`",
                  "route": "method-spacetimedbclient-connect-",
                  "depth": 3
                },
                {
                  "title": "Event `SpacetimeDBClient.onIdentityReceived`",
                  "route": "event-spacetimedbclient-onidentityreceived-",
                  "depth": 3
                },
                {
                  "title": "Event `SpacetimeDBClient.onConnect`",
                  "route": "event-spacetimedbclient-onconnect-",
                  "depth": 3
                },
                {
                  "title": "Subscribe to queries",
                  "route": "subscribe-to-queries",
                  "depth": 2
                },
                {
                  "title": "Method `SpacetimeDBClient.Subscribe`",
                  "route": "method-spacetimedbclient-subscribe-",
                  "depth": 3
                },
                {
                  "title": "Event `SpacetimeDBClient.onSubscriptionApplied`",
                  "route": "event-spacetimedbclient-onsubscriptionapplied-",
                  "depth": 3
                },
                {
                  "title": "View rows of subscribed tables",
                  "route": "view-rows-of-subscribed-tables",
                  "depth": 2
                },
                {
                  "title": "Class `{TABLE}`",
                  "route": "class-table-",
                  "depth": 3
                },
                {
                  "title": "Static Method `{TABLE}.Iter`",
                  "route": "static-method-table-iter-",
                  "depth": 4
                },
                {
                  "title": "Static Method `{TABLE}.FilterBy{COLUMN}`",
                  "route": "static-method-table-filterby-column-",
                  "depth": 4
                },
                {
                  "title": "Static Method `{TABLE}.Count`",
                  "route": "static-method-table-count-",
                  "depth": 4
                },
                {
                  "title": "Static Event `{TABLE}.OnInsert`",
                  "route": "static-event-table-oninsert-",
                  "depth": 4
                },
                {
                  "title": "Static Event `{TABLE}.OnBeforeDelete`",
                  "route": "static-event-table-onbeforedelete-",
                  "depth": 4
                },
                {
                  "title": "Static Event `{TABLE}.OnDelete`",
                  "route": "static-event-table-ondelete-",
                  "depth": 4
                },
                {
                  "title": "Static Event `{TABLE}.OnUpdate`",
                  "route": "static-event-table-onupdate-",
                  "depth": 4
                },
                {
                  "title": "Observe and invoke reducers",
                  "route": "observe-and-invoke-reducers",
                  "depth": 2
                },
                {
                  "title": "Class `Reducer`",
                  "route": "class-reducer-",
                  "depth": 3
                },
                {
                  "title": "Static Method `Reducer.{REDUCER}`",
                  "route": "static-method-reducer-reducer-",
                  "depth": 4
                },
                {
                  "title": "Static Event `Reducer.On{REDUCER}`",
                  "route": "static-event-reducer-on-reducer-",
                  "depth": 4
                },
                {
                  "title": "Class `ReducerEvent`",
                  "route": "class-reducerevent-",
                  "depth": 3
                },
                {
                  "title": "Enum `Status`",
                  "route": "enum-status-",
                  "depth": 4
                },
                {
                  "title": "Variant `Status.Committed`",
                  "route": "variant-status-committed-",
                  "depth": 5
                },
                {
                  "title": "Variant `Status.Failed`",
                  "route": "variant-status-failed-",
                  "depth": 5
                },
                {
                  "title": "Variant `Status.OutOfEnergy`",
                  "route": "variant-status-outofenergy-",
                  "depth": 5
                },
                {
                  "title": "Identity management",
                  "route": "identity-management",
                  "depth": 2
                },
                {
                  "title": "Class `AuthToken`",
                  "route": "class-authtoken-",
                  "depth": 3
                },
                {
                  "title": "Static Method `AuthToken.Init`",
                  "route": "static-method-authtoken-init-",
                  "depth": 4
                },
                {
                  "title": "Static Property `AuthToken.Token`",
                  "route": "static-property-authtoken-token-",
                  "depth": 4
                },
                {
                  "title": "Static Method `AuthToken.SaveToken`",
                  "route": "static-method-authtoken-savetoken-",
                  "depth": 4
                },
                {
                  "title": "Class `Identity`",
                  "route": "class-identity-",
                  "depth": 3
                },
                {
                  "title": "Customizing logging",
                  "route": "customizing-logging",
                  "depth": 2
                },
                {
                  "title": "Interface `ISpacetimeDBLogger`",
                  "route": "interface-ispacetimedblogger-",
                  "depth": 3
                },
                {
                  "title": "Class `ConsoleLogger`",
                  "route": "class-consolelogger-",
                  "depth": 3
                },
                {
                  "title": "Class `UnityDebugLogger`",
                  "route": "class-unitydebuglogger-",
                  "depth": 3
                }
              ],
              "pages": []
            }
          ]
        },
        {
          "title": "Welcome to Client SDK Languages# SpacetimeDB Client SDKs Overview",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# Welcome to Client SDK Languages# SpacetimeDB Client SDKs Overview\r\n\r\nThe SpacetimeDB Client SDKs provide a comprehensive interface to interact with the SpacetimeDB server engine from various programming languages. Currently, SDKs are available for\r\n\r\n- [Rust](/docs/client-languages/rust/rust-sdk-reference) - [(Quickstart)](/docs/client-languages/rust/rust-sdk-quickstart-guide)\r\n- [C#](/docs/client-languages/csharp/csharp-sdk-reference) - [(Quickstart)](/docs/client-languages/csharp/csharp-sdk-quickstart-guide)\r\n- [TypeScript](/docs/client-languages/typescript/typescript-sdk-reference) - [(Quickstart)](client-languages/typescript/typescript-sdk-quickstart-guide)\r\n- [Python](/docs/client-languages/python/python-sdk-reference) - [(Quickstart)](/docs/python/python-sdk-quickstart-guide)\r\n\r\n## Key Features\r\n\r\nThe SpacetimeDB Client SDKs offer the following key functionalities:\r\n\r\n### Connection Management\r\n\r\nThe SDKs handle the process of connecting and disconnecting from the SpacetimeDB server, simplifying this process for the client applications.\r\n\r\n### Authentication\r\n\r\nThe SDKs support authentication using an auth token, allowing clients to securely establish a session with the SpacetimeDB server.\r\n\r\n### Local Database View\r\n\r\nEach client can define a local view of the database via a subscription consisting of a set of queries. This local view is maintained by the server and populated into a local cache on the client side.\r\n\r\n### Reducer Calls\r\n\r\nThe SDKs allow clients to call transactional functions (reducers) on the server.\r\n\r\n### Callback Registrations\r\n\r\nThe SpacetimeDB Client SDKs offer powerful callback functionality that allow clients to monitor changes in their local database view. These callbacks come in two forms:\r\n\r\n#### Connection and Subscription Callbacks\r\n\r\nClients can also register callbacks that trigger when the connection to the server is established or lost, or when a subscription is updated. This allows clients to react to changes in the connection status.\r\n\r\n#### Row Update Callbacks\r\n\r\nClients can register callbacks that trigger when any row in their local cache is updated by the server. These callbacks contain information about the reducer that triggered the change. This feature enables clients to react to changes in data that they're interested in.\r\n\r\n#### Reducer Call Callbacks\r\n\r\nClients can also register callbacks that fire when a reducer call modifies something in the client's local view. This allows the client to know when a transactional function it has executed has had an effect on the data it cares about.\r\n\r\nAdditionally, when a client makes a reducer call that fails, the SDK triggers the registered reducer callback on the client that initiated the failed call with the error message that was returned from the server. This allows for appropriate error handling or user notifications.\r\n\r\n## Choosing a Language\r\n\r\nWhen selecting a language for your client application with SpacetimeDB, a variety of factors come into play. While the functionality of the SDKs remains consistent across different languages, the choice of language will often depend on the specific needs and context of your application. Here are a few considerations:\r\n\r\n### Team Expertise\r\n\r\nThe familiarity of your development team with a particular language can greatly influence your choice. You might want to choose a language that your team is most comfortable with to increase productivity and reduce development time.\r\n\r\n### Application Type\r\n\r\nDifferent languages are often better suited to different types of applications. For instance, if you are developing a web-based application, you might opt for TypeScript due to its seamless integration with web technologies. On the other hand, if you're developing a desktop application, you might choose C# or Python, depending on your requirements and platform. Python is also very useful for utility scripts and tools.\r\n\r\n### Performance\r\n\r\nThe performance characteristics of the different languages can also be a factor. If your application is performance-critical, you might opt for Rust, known for its speed and memory efficiency.\r\n\r\n### Platform Support\r\n\r\nThe platform you're targeting can also influence your choice. For instance, if you're developing a game or a 3D application using the Unity engine, you'll want to choose the C# SDK, as Unity uses C# as its primary scripting language.\r\n\r\n### Ecosystem and Libraries\r\n\r\nEach language has its own ecosystem of libraries and tools that can help in developing your application. If there's a library in a particular language that you want to use, it may influence your choice.\r\n\r\nRemember, the best language to use is the one that best fits your use case and the one you and your team are most comfortable with. It's worth noting that due to the consistent functionality across different SDKs, transitioning from one language to another should you need to in the future will primarily involve syntax changes rather than changes in the application's logic.\r\n\r\nYou may want to use multiple languages in your application. For instance, you might want to use C# in Unity for your game logic, TypeScript for a web-based administration panel, and Python for utility scripts. This is perfectly fine, as the SpacetimeDB server is completely client-agnostic.\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "Welcome to Client SDK Languages# SpacetimeDB Client SDKs Overview",
              "route": "welcome-to-client-sdk-languages-spacetimedb-client-sdks-overview",
              "depth": 1
            },
            {
              "title": "Key Features",
              "route": "key-features",
              "depth": 2
            },
            {
              "title": "Connection Management",
              "route": "connection-management",
              "depth": 3
            },
            {
              "title": "Authentication",
              "route": "authentication",
              "depth": 3
            },
            {
              "title": "Local Database View",
              "route": "local-database-view",
              "depth": 3
            },
            {
              "title": "Reducer Calls",
              "route": "reducer-calls",
              "depth": 3
            },
            {
              "title": "Callback Registrations",
              "route": "callback-registrations",
              "depth": 3
            },
            {
              "title": "Connection and Subscription Callbacks",
              "route": "connection-and-subscription-callbacks",
              "depth": 4
            },
            {
              "title": "Row Update Callbacks",
              "route": "row-update-callbacks",
              "depth": 4
            },
            {
              "title": "Reducer Call Callbacks",
              "route": "reducer-call-callbacks",
              "depth": 4
            },
            {
              "title": "Choosing a Language",
              "route": "choosing-a-language",
              "depth": 2
            },
            {
              "title": "Team Expertise",
              "route": "team-expertise",
              "depth": 3
            },
            {
              "title": "Application Type",
              "route": "application-type",
              "depth": 3
            },
            {
              "title": "Performance",
              "route": "performance",
              "depth": 3
            },
            {
              "title": "Platform Support",
              "route": "platform-support",
              "depth": 3
            },
            {
              "title": "Ecosystem and Libraries",
              "route": "ecosystem-and-libraries",
              "depth": 3
            }
          ],
          "pages": []
        },
        {
          "title": "Python",
          "identifier": "Python",
          "indexIdentifier": "index",
          "comingSoon": false,
          "hasPages": true,
          "editUrl": "Python/index.md",
          "jumpLinks": [],
          "pages": [
            {
              "title": "Python Client SDK Quick Start",
              "identifier": "index",
              "indexIdentifier": "index",
              "content": "# Python Client SDK Quick Start\r\n\r\nIn this guide, we'll show you how to get up and running with a simple SpacetimDB app with a client written in Python.\r\n\r\nWe'll implement a command-line client for the module created in our [Rust Module Quickstart](/docs/languages/rust/rust-module-quickstart-guide) or [C# Module Quickstart](/docs/languages/csharp/csharp-module-reference) guides. Make sure you follow one of these guides before you start on this one.\r\n\r\n## Install the SpacetimeDB SDK Python Package\r\n\r\n1. Run pip install\r\n\r\n```bash\r\npip install spacetimedb_sdk\r\n```\r\n\r\n## Project structure\r\n\r\nEnter the directory `quickstart-chat` you created in the Rust or C# Module Quickstart guides and create a `client` folder:\r\n\r\n```bash\r\ncd quickstart-chat\r\nmkdir client\r\n```\r\n\r\n## Create the Python main file\r\n\r\nCreate a file called `main.py` in the `client` and open it in your favorite editor. We prefer [VS Code](https://code.visualstudio.com/).\r\n\r\n## Add imports\r\n\r\nWe need to add several imports for this quickstart:\r\n\r\n- [`asyncio`](https://docs.python.org/3/library/asyncio.html) is required to run the async code in the SDK.\r\n- [`multiprocessing.Queue`](https://docs.python.org/3/library/multiprocessing.html) allows us to pass our input to the async code, which we will run in a separate thread.\r\n- [`threading`](https://docs.python.org/3/library/threading.html) allows us to spawn our async code in a separate thread so the main thread can run the input loop.\r\n\r\n- `spacetimedb_sdk.spacetimedb_async_client.SpacetimeDBAsyncClient` is the async wrapper around the SpacetimeDB client which we use to interact with our SpacetimeDB module.\r\n- `spacetimedb_sdk.local_config` is an optional helper module to load the auth token from local storage.\r\n\r\n```python\r\nimport asyncio\r\nfrom multiprocessing import Queue\r\nimport threading\r\n\r\nfrom spacetimedb_sdk.spacetimedb_async_client import SpacetimeDBAsyncClient\r\nimport spacetimedb_sdk.local_config as local_config\r\n```\r\n\r\n## Generate your module types\r\n\r\nThe `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.\r\n\r\nIn your `client` directory, run:\r\n\r\n```bash\r\nmkdir -p module_bindings\r\nspacetime generate --lang python --out-dir src/module_bindings --project_path ../server\r\n```\r\n\r\nTake a look inside `client/module_bindings`. The CLI should have generated five files:\r\n\r\n```\r\nmodule_bindings\r\n+-- message.py\r\n+-- send_message_reducer.py\r\n+-- set_name_reducer.py\r\n+-- user.py\r\n```\r\n\r\nNow we import these types by adding the following lines to `main.py`:\r\n\r\n```python\r\nimport module_bindings\r\nfrom module_bindings.user import User\r\nfrom module_bindings.message import Message\r\nimport module_bindings.send_message_reducer as send_message_reducer\r\nimport module_bindings.set_name_reducer as set_name_reducer\r\n```\r\n\r\n## Global variables\r\n\r\nNext we will add our global `input_queue` and `local_identity` variables which we will explain later when they are used.\r\n\r\n```python\r\ninput_queue = Queue()\r\nlocal_identity = None\r\n```\r\n\r\n## Define main function\r\n\r\nWe'll work outside-in, first defining our `main` function at a high level, then implementing each behavior it needs. We need `main` to do four things:\r\n\r\n1. Init the optional local config module. The first parameter is the directory name to be created in the user home directory.\r\n1. Create our async SpacetimeDB client.\r\n1. Register our callbacks.\r\n1. Start the async client in a thread.\r\n1. Run a loop to read user input and send it to a repeating event in the async client.\r\n1. When the user exits, stop the async client and exit the program.\r\n\r\n```python\r\nif __name__ == \"__main__\":\r\n    local_config.init(\".spacetimedb-python-quickstart\")\r\n\r\n    spacetime_client = SpacetimeDBAsyncClient(module_bindings)\r\n\r\n    register_callbacks(spacetime_client)\r\n\r\n    thread = threading.Thread(target=run_client, args=(spacetime_client,))\r\n    thread.start()\r\n\r\n    input_loop()\r\n\r\n    spacetime_client.force_close()\r\n    thread.join()\r\n```\r\n\r\n## Register callbacks\r\n\r\nWe need to handle several sorts of events:\r\n\r\n1. OnSubscriptionApplied is a special callback that is executed when the local client cache is populated. We will talk more about this later.\r\n2. When a new user joins or a user is updated, we'll print an appropriate message.\r\n3. When we receive a new message, we'll print it.\r\n4. If the server rejects our attempt to set our name, we'll print an error.\r\n5. If the server rejects a message we send, we'll print an error.\r\n6. We use the `schedule_event` function to register a callback to be executed after 100ms. This callback will check the input queue for any user input and execute the appropriate command.\r\n\r\nBecause python requires functions to be defined before they're used, the following code must be added to `main.py` before main block:\r\n\r\n```python\r\ndef register_callbacks(spacetime_client):\r\n    spacetime_client.client.register_on_subscription_applied(on_subscription_applied)\r\n\r\n    User.register_row_update(on_user_row_update)\r\n    Message.register_row_update(on_message_row_update)\r\n\r\n    set_name_reducer.register_on_set_name(on_set_name_reducer)\r\n    send_message_reducer.register_on_send_message(on_send_message_reducer)\r\n\r\n    spacetime_client.schedule_event(0.1, check_commands)\r\n```\r\n\r\n### Handling User row updates\r\n\r\nFor each table, we can register a row update callback to be run whenever a subscribed row is inserted, updated or deleted. We register these callbacks using the `register_row_update` methods that are generated automatically for each table by `spacetime generate`.\r\n\r\nThese callbacks can fire in two contexts:\r\n\r\n- After a reducer runs, when the client's cache is updated about changes to subscribed rows.\r\n- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.\r\n\r\nThis second case means that, even though the module only ever inserts online users, the client's `User::row_update` callbacks may be invoked with users who are offline. We'll only notify about online users.\r\n\r\nWe are also going to check for updates to the user row. This can happen for three reasons:\r\n\r\n1. They've set their name using the `set_name` reducer.\r\n2. They're an existing user re-connecting, so their `online` has been set to `true`.\r\n3. They've disconnected, so their `online` has been set to `false`.\r\n\r\nWe'll print an appropriate message in each of these cases.\r\n\r\n`row_update` callbacks take four arguments: the row operation (\"insert\", \"update\", or \"delete\"), the old row if it existed, the new or updated row, and a `ReducerEvent`. This will `None` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is an class that contains information about the reducer that triggered this row update event.\r\n\r\nWhenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define a function `user_name_or_identity` handle this.\r\n\r\nAdd these functions before the `register_callbacks` function:\r\n\r\n```python\r\ndef user_name_or_identity(user):\r\n    if user.name:\r\n        return user.name\r\n    else:\r\n        return (str(user.identity))[:8]\r\n\r\ndef on_user_row_update(row_op, user_old, user, reducer_event):\r\n    if row_op == \"insert\":\r\n        if user.online:\r\n            print(f\"User {user_name_or_identity(user)} connected.\")\r\n    elif row_op == \"update\":\r\n        if user_old.online and not user.online:\r\n            print(f\"User {user_name_or_identity(user)} disconnected.\")\r\n        elif not user_old.online and user.online:\r\n            print(f\"User {user_name_or_identity(user)} connected.\")\r\n\r\n        if user_old.name != user.name:\r\n            print(\r\n                f\"User {user_name_or_identity(user_old)} renamed to {user_name_or_identity(user)}.\"\r\n            )\r\n```\r\n\r\n### Print messages\r\n\r\nWhen we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `on_message_row_update` callback will check if its `reducer_event` argument is not `None`, and only print in that case.\r\n\r\nTo find the `User` based on the message's `sender` identity, we'll use `User::filter_by_identity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `filter_by_identity` accepts a `bytes`, rather than an `&Identity`. The `sender` identity stored in the message is also a `bytes`, not an `Identity`, so we can just pass it to the filter method.\r\n\r\nWe'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.\r\n\r\nAdd these functions before the `register_callbacks` function:\r\n\r\n```python\r\ndef on_message_row_update(row_op, message_old, message, reducer_event):\r\n    if reducer_event is not None and row_op == \"insert\":\r\n        print_message(message)\r\n\r\ndef print_message(message):\r\n    user = User.filter_by_identity(message.sender)\r\n    user_name = \"unknown\"\r\n    if user is not None:\r\n        user_name = user_name_or_identity(user)\r\n\r\n    print(f\"{user_name}: {message.text}\")\r\n```\r\n\r\n### Warn if our name was rejected\r\n\r\nWe can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `register_on_<reducer>` method, which is automatically implemented for each reducer by `spacetime generate`.\r\n\r\nEach reducer callback takes three fixed arguments:\r\n\r\n1. The `Identity` of the client who requested the reducer invocation.\r\n2. The `Status` of the reducer run, one of `committed`, `failed` or `outofenergy`.\r\n3. The `Message` returned by the reducer in error cases, or `None` if the reducer succeeded.\r\n\r\nIt also takes a variable number of arguments which match the calling arguments of the reducer.\r\n\r\nThese callbacks will be invoked in one of two cases:\r\n\r\n1. If the reducer was successful and altered any of our subscribed rows.\r\n2. If we requested an invocation which failed.\r\n\r\nNote that a status of `failed` or `outofenergy` implies that the caller identity is our own identity.\r\n\r\nWe already handle successful `set_name` invocations using our `User::on_update` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `on_set_name_reducer` as a callback which checks if the reducer failed, and if it did, prints an error message including the rejected name.\r\n\r\nWe'll test both that our identity matches the sender and that the status is `failed`, even though the latter implies the former, for demonstration purposes.\r\n\r\nAdd this function before the `register_callbacks` function:\r\n\r\n```python\r\ndef on_set_name_reducer(sender, status, message, name):\r\n    if sender == local_identity:\r\n        if status == \"failed\":\r\n            print(f\"Failed to set name: {message}\")\r\n```\r\n\r\n### Warn if our message was rejected\r\n\r\nWe handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.\r\n\r\nAdd this function before the `register_callbacks` function:\r\n\r\n```python\r\ndef on_send_message_reducer(sender, status, message, msg):\r\n    if sender == local_identity:\r\n        if status == \"failed\":\r\n            print(f\"Failed to send message: {message}\")\r\n```\r\n\r\n### OnSubscriptionApplied callback\r\n\r\nThis callback fires after the client cache is updated as a result in a change to the client subscription. This happens after connect and if after calling `subscribe` to modify the subscription.\r\n\r\nIn this case, we want to print all the existing messages when the subscription is applied. `print_messages_in_order` iterates over all the `Message`s we've received, sorts them, and then prints them. `Message.iter()` is generated for all table types, and returns an iterator over all the messages in the client's cache.\r\n\r\nAdd these functions before the `register_callbacks` function:\r\n\r\n```python\r\ndef print_messages_in_order():\r\n    all_messages = sorted(Message.iter(), key=lambda x: x.sent)\r\n    for entry in all_messages:\r\n        print(f\"{user_name_or_identity(User.filter_by_identity(entry.sender))}: {entry.text}\")\r\n\r\ndef on_subscription_applied():\r\n    print(f\"\\nSYSTEM: Connected.\")\r\n    print_messages_in_order()\r\n```\r\n\r\n### Check commands repeating event\r\n\r\nWe'll use a repeating event to check the user input queue every 100ms. If there's a command in the queue, we'll execute it. If not, we'll just keep waiting. Notice that at the end of the function we call `schedule_event` again to so the event will repeat.\r\n\r\nIf the command is to send a message, we'll call the `send_message` reducer. If the command is to set our name, we'll call the `set_name` reducer.\r\n\r\nAdd these functions before the `register_callbacks` function:\r\n\r\n```python\r\ndef check_commands():\r\n    global input_queue\r\n\r\n    if not input_queue.empty():\r\n        choice = input_queue.get()\r\n        if choice[0] == \"name\":\r\n            set_name_reducer.set_name(choice[1])\r\n        else:\r\n            send_message_reducer.send_message(choice[1])\r\n\r\n    spacetime_client.schedule_event(0.1, check_commands)\r\n```\r\n\r\n### OnConnect callback\r\n\r\nThis callback fires after the client connects to the server. We'll use it to save our credentials to a file so that we can re-authenticate as the same user next time we connect.\r\n\r\nThe `on_connect` callback takes two arguments:\r\n\r\n1. The `Auth Token` is the equivalent of your private key. This is the only way to authenticate with the SpacetimeDB module as this user.\r\n2. The `Identity` is the equivalent of your public key. This is used to uniquely identify this user and will be sent to other clients. We store this in a global variable so we can use it to identify that a given message or transaction was sent by us.\r\n\r\nTo store our auth token, we use the optional component `local_config`, which provides a simple interface for storing and retrieving a single `Identity` from a file. We'll use the `local_config::set_string` method to store the auth token. Other projects might want to associate this token with some other identifier such as an email address or Steam ID.\r\n\r\nThe `on_connect` callback is passed to the client connect function so it just needs to be defined before the `run_client` described next.\r\n\r\n```python\r\ndef on_connect(auth_token, identity):\r\n    global local_identity\r\n    local_identity = identity\r\n\r\n    local_config.set_string(\"auth_token\", auth_token)\r\n```\r\n\r\n## Async client thread\r\n\r\nWe are going to write a function that starts the async client, which will be executed on a separate thread.\r\n\r\n```python\r\ndef run_client(spacetime_client):\r\n    asyncio.run(\r\n        spacetime_client.run(\r\n            local_config.get_string(\"auth_token\"),\r\n            \"localhost:3000\",\r\n            \"chat\",\r\n            False,\r\n            on_connect,\r\n            [\"SELECT * FROM User\", \"SELECT * FROM Message\"],\r\n        )\r\n    )\r\n```\r\n\r\n## Input loop\r\n\r\nFinally, we need a function to be executed on the main loop which listens for user input and adds it to the queue.\r\n\r\n```python\r\ndef input_loop():\r\n    global input_queue\r\n\r\n    while True:\r\n        user_input = input()\r\n        if len(user_input) == 0:\r\n            return\r\n        elif user_input.startswith(\"/name \"):\r\n            input_queue.put((\"name\", user_input[6:]))\r\n        else:\r\n            input_queue.put((\"message\", user_input))\r\n```\r\n\r\n## Run the client\r\n\r\nMake sure your module from the Rust or C# module quickstart is published. If you used a different module name than `chat`, you will need to update the `connect` call in the `run_client` function.\r\n\r\nRun the client:\r\n\r\n```bash\r\npython main.py\r\n```\r\n\r\nIf you want to connect another client, you can use the --client command line option, which is built into the local_config module. This will create different settings file for the new client's auth token.\r\n\r\n```bash\r\npython main.py --client 2\r\n```\r\n\r\n## Next steps\r\n\r\nCongratulations! You've built a simple chat app with a Python client. You can now use this as a starting point for your own SpacetimeDB apps.\r\n\r\nFor a more complex example of the Spacetime Python SDK, check out our [AI Agent](https://github.com/clockworklabs/spacetime-mud/tree/main/ai-agent-python-client) for the [Spacetime Multi-User Dungeon](https://github.com/clockworklabs/spacetime-mud). The AI Agent uses the OpenAI API to create dynamic content on command.\r\n",
              "hasPages": false,
              "editUrl": "index.md",
              "jumpLinks": [
                {
                  "title": "Python Client SDK Quick Start",
                  "route": "python-client-sdk-quick-start",
                  "depth": 1
                },
                {
                  "title": "Install the SpacetimeDB SDK Python Package",
                  "route": "install-the-spacetimedb-sdk-python-package",
                  "depth": 2
                },
                {
                  "title": "Project structure",
                  "route": "project-structure",
                  "depth": 2
                },
                {
                  "title": "Create the Python main file",
                  "route": "create-the-python-main-file",
                  "depth": 2
                },
                {
                  "title": "Add imports",
                  "route": "add-imports",
                  "depth": 2
                },
                {
                  "title": "Generate your module types",
                  "route": "generate-your-module-types",
                  "depth": 2
                },
                {
                  "title": "Global variables",
                  "route": "global-variables",
                  "depth": 2
                },
                {
                  "title": "Define main function",
                  "route": "define-main-function",
                  "depth": 2
                },
                {
                  "title": "Register callbacks",
                  "route": "register-callbacks",
                  "depth": 2
                },
                {
                  "title": "Handling User row updates",
                  "route": "handling-user-row-updates",
                  "depth": 3
                },
                {
                  "title": "Print messages",
                  "route": "print-messages",
                  "depth": 3
                },
                {
                  "title": "Warn if our name was rejected",
                  "route": "warn-if-our-name-was-rejected",
                  "depth": 3
                },
                {
                  "title": "Warn if our message was rejected",
                  "route": "warn-if-our-message-was-rejected",
                  "depth": 3
                },
                {
                  "title": "OnSubscriptionApplied callback",
                  "route": "onsubscriptionapplied-callback",
                  "depth": 3
                },
                {
                  "title": "Check commands repeating event",
                  "route": "check-commands-repeating-event",
                  "depth": 3
                },
                {
                  "title": "OnConnect callback",
                  "route": "onconnect-callback",
                  "depth": 3
                },
                {
                  "title": "Async client thread",
                  "route": "async-client-thread",
                  "depth": 2
                },
                {
                  "title": "Input loop",
                  "route": "input-loop",
                  "depth": 2
                },
                {
                  "title": "Run the client",
                  "route": "run-the-client",
                  "depth": 2
                },
                {
                  "title": "Next steps",
                  "route": "next-steps",
                  "depth": 2
                }
              ],
              "pages": []
            },
            {
              "title": "The SpacetimeDB Python client SDK",
              "identifier": "SDK Reference",
              "indexIdentifier": "SDK Reference",
              "hasPages": false,
              "content": "# The SpacetimeDB Python client SDK\r\n\r\nThe SpacetimeDB client SDK for Python contains all the tools you need to build native clients for SpacetimeDB modules using Python.\r\n\r\n## Install the SDK\r\n\r\nUse pip to install the SDK:\r\n\r\n```bash\r\npip install spacetimedb-sdk\r\n```\r\n\r\n## Generate module bindings\r\n\r\nEach SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's directory and generate the Python interface files using the Spacetime CLI. From your project directory, run:\r\n\r\n```bash\r\nmkdir -p module_bindings\r\nspacetime generate --lang python \\\r\n    --out-dir module_bindings \\\r\n    --project-path PATH-TO-MODULE-DIRECTORY\r\n```\r\n\r\nReplace `PATH-TO-MODULE-DIRECTORY` with the path to your SpacetimeDB module.\r\n\r\nImport your bindings in your client's code:\r\n\r\n```python\r\nimport module_bindings\r\n```\r\n\r\n## Basic vs Async SpacetimeDB Client\r\n\r\nThis SDK provides two different client modules for interacting with your SpacetimeDB module.\r\n\r\nThe Basic client allows you to have control of the main loop of your application and you are responsible for regularly calling the client's `update` function. This is useful in settings like PyGame where you want to have full control of the main loop.\r\n\r\nThe Async client has a run function that you call after you set up all your callbacks and it will take over the main loop and handle updating the client for you. With the async client, you can have a regular \"tick\" function by using the `schedule_event` function.\r\n\r\n## Common Client Reference\r\n\r\nThe following functions and types are used in both the Basic and Async clients.\r\n\r\n### API at a glance\r\n\r\n| Definition                                                                                              | Description                                                                                  |\r\n| ------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |\r\n| Type [`Identity`](#type-identity)                                                                       | A unique public identifier for a client.                                                     |\r\n| Type [`ReducerEvent`](#type-reducerevent)                                                               | `class` containing information about the reducer that triggered a row update event.          |\r\n| Type [`module_bindings::{TABLE}`](#type-table)                                                          | Autogenerated `class` type for a table, holding one row.                                     |\r\n| Method [`module_bindings::{TABLE}::filter_by_{COLUMN}`](#method-filter_by_column)                       | Autogenerated method to iterate over or seek subscribed rows where a column matches a value. |\r\n| Method [`module_bindings::{TABLE}::iter`](#method-iter)                                                 | Autogenerated method to iterate over all subscribed rows.                                    |\r\n| Method [`module_bindings::{TABLE}::register_row_update`](#method-register_row_update)                   | Autogenerated method to register a callback that fires when a row changes.                   |\r\n| Function [`module_bindings::{REDUCER_NAME}::{REDUCER_NAME}`](#function-reducer)                         | Autogenerated function to invoke a reducer.                                                  |\r\n| Function [`module_bindings::{REDUCER_NAME}::register_on_{REDUCER_NAME}`](#function-register_on_reducer) | Autogenerated function to register a callback to run whenever the reducer is invoked.        |\r\n\r\n### Type `Identity`\r\n\r\n```python\r\nclass Identity:\r\n    @staticmethod\r\n    def from_string(string)\r\n\r\n    @staticmethod\r\n    def from_bytes(data)\r\n\r\n    def __str__(self)\r\n\r\n    def __eq__(self, other)\r\n```\r\n\r\n| Member        | Args       | Meaning                              |\r\n| ------------- | ---------- | ------------------------------------ |\r\n| `from_string` | `str`      | Create an Identity from a hex string |\r\n| `from_bytes`  | `bytes`    | Create an Identity from raw bytes    |\r\n| `__str__`     | `None`     | Convert the Identity to a hex string |\r\n| `__eq__`      | `Identity` | Compare two Identities for equality  |\r\n\r\nA unique public identifier for a client connected to a database.\r\n\r\n### Type `ReducerEvent`\r\n\r\n```python\r\nclass ReducerEvent:\r\n    def __init__(self, caller_identity, reducer_name, status, message, args):\r\n        self.caller_identity = caller_identity\r\n        self.reducer_name = reducer_name\r\n        self.status = status\r\n        self.message = message\r\n        self.args = args\r\n```\r\n\r\n| Member            | Args        | Meaning                                                                     |\r\n| ----------------- | ----------- | --------------------------------------------------------------------------- |\r\n| `caller_identity` | `Identity`  | The identity of the user who invoked the reducer                            |\r\n| `reducer_name`    | `str`       | The name of the reducer that was invoked                                    |\r\n| `status`          | `str`       | The status of the reducer invocation (\"committed\", \"failed\", \"outofenergy\") |\r\n| `message`         | `str`       | The message returned by the reducer if it fails                             |\r\n| `args`            | `List[str]` | The arguments passed to the reducer                                         |\r\n\r\nThis class contains the information about a reducer event to be passed to row update callbacks.\r\n\r\n### Type `{TABLE}`\r\n\r\n```python\r\nclass TABLE:\r\n\tis_table_class = True\r\n\r\n\tprimary_key = \"identity\"\r\n\r\n\t@classmethod\r\n\tdef register_row_update(cls, callback: Callable[[str,TABLE,TABLE,ReducerEvent], None])\r\n\r\n\t@classmethod\r\n\tdef iter(cls) -> Iterator[User]\r\n\r\n\t@classmethod\r\n\tdef filter_by_COLUMN_NAME(cls, COLUMN_VALUE) -> TABLE\r\n```\r\n\r\nThis class is autogenerated for each table in your module. It contains methods for filtering and iterating over subscribed rows.\r\n\r\n### Method `filter_by_{COLUMN}`\r\n\r\n```python\r\ndef filter_by_COLUMN(self, COLUMN_VALUE) -> TABLE\r\n```\r\n\r\n| Argument       | Type          | Meaning                |\r\n| -------------- | ------------- | ---------------------- |\r\n| `column_value` | `COLUMN_TYPE` | The value to filter by |\r\n\r\nFor each column of a table, `spacetime generate` generates a `classmethod` on the [table class](#type-table) to filter or seek subscribed rows where that column matches a requested value. These methods are named `filter_by_{COLUMN}`, where `{COLUMN}` is the column name converted to `snake_case`.\r\n\r\nThe method's return type depends on the column's attributes:\r\n\r\n- For unique columns, including those annotated `#[unique]` and `#[primarykey]`, the `filter_by` method returns a `{TABLE}` or None, where `{TABLE}` is the [table struct](#type-table).\r\n- For non-unique columns, the `filter_by` method returns an `Iterator` that can be used in a `for` loop.\r\n\r\n### Method `iter`\r\n\r\n```python\r\ndef iter(self) -> Iterator[TABLE]\r\n```\r\n\r\nIterate over all the subscribed rows in the table.\r\n\r\n### Method `register_row_update`\r\n\r\n```python\r\ndef register_row_update(self, callback: Callable[[str,TABLE,TABLE,ReducerEvent], None])\r\n```\r\n\r\n| Argument   | Type                                      | Meaning                                                                                          |\r\n| ---------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------ |\r\n| `callback` | `Callable[[str,TABLE,TABLE,ReducerEvent]` | Callback to be invoked when a row is updated (Args: row_op, old_value, new_value, reducer_event) |\r\n\r\nRegister a callback function to be executed when a row is updated. Callback arguments are:\r\n\r\n- `row_op`: The type of row update event. One of `\"insert\"`, `\"delete\"`, or `\"update\"`.\r\n- `old_value`: The previous value of the row, `None` if the row was inserted.\r\n- `new_value`: The new value of the row, `None` if the row was deleted.\r\n- `reducer_event`: The [`ReducerEvent`](#type-reducerevent) that caused the row update, or `None` if the row was updated as a result of a subscription change.\r\n\r\n### Function `{REDUCER_NAME}`\r\n\r\n```python\r\ndef {REDUCER_NAME}(arg1, arg2)\r\n```\r\n\r\nThis function is autogenerated for each reducer in your module. It is used to invoke the reducer. The arguments match the arguments defined in the reducer's `#[reducer]` attribute.\r\n\r\n### Function `register_on_{REDUCER_NAME}`\r\n\r\n```python\r\ndef register_on_{REDUCER_NAME}(callback: Callable[[Identity, str, str, ARG1_TYPE, ARG1_TYPE], None])\r\n```\r\n\r\n| Argument   | Type                                                         | Meaning                                                                                           |\r\n| ---------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |\r\n| `callback` | `Callable[[Identity, str, str, ARG1_TYPE, ARG1_TYPE], None]` | Callback to be invoked when the reducer is invoked (Args: caller_identity, status, message, args) |\r\n\r\nRegister a callback function to be executed when the reducer is invoked. Callback arguments are:\r\n\r\n- `caller_identity`: The identity of the user who invoked the reducer.\r\n- `status`: The status of the reducer invocation (\"committed\", \"failed\", \"outofenergy\").\r\n- `message`: The message returned by the reducer if it fails.\r\n- `args`: Variable number of arguments passed to the reducer.\r\n\r\n## Async Client Reference\r\n\r\n### API at a glance\r\n\r\n| Definition                                                                                                        | Description                                                                                              |\r\n| ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |\r\n| Function [`SpacetimeDBAsyncClient::run`](#function-run)                                                           | Run the client. This function will not return until the client is closed.                                |\r\n| Function [`SpacetimeDBAsyncClient::subscribe`](#function-subscribe)                                               | Subscribe to receive data and transaction updates for the provided queries.                              |\r\n| Function [`SpacetimeDBAsyncClient::register_on_subscription_applied`](#function-register_on_subscription_applied) | Register a callback when the local cache is updated as a result of a change to the subscription queries. |\r\n| Function [`SpacetimeDBAsyncClient::force_close`](#function-force_close)                                           | Signal the client to stop processing events and close the connection to the server.                      |\r\n| Function [`SpacetimeDBAsyncClient::schedule_event`](#function-schedule_event)                                     | Schedule an event to be fired after a delay                                                              |\r\n\r\n### Function `run`\r\n\r\n```python\r\nasync def run(\r\n        self,\r\n        auth_token,\r\n        host,\r\n        address_or_name,\r\n        ssl_enabled,\r\n        on_connect,\r\n        subscription_queries=[],\r\n    )\r\n```\r\n\r\nRun the client. This function will not return until the client is closed.\r\n\r\n| Argument               | Type                              | Meaning                                                        |\r\n| ---------------------- | --------------------------------- | -------------------------------------------------------------- |\r\n| `auth_token`           | `str`                             | Auth token to authenticate the user. (None if new user)        |\r\n| `host`                 | `str`                             | Hostname of SpacetimeDB server                                 |\r\n| `address_or_name`      | `&str`                            | Name or address of the module.                                 |\r\n| `ssl_enabled`          | `bool`                            | Whether to use SSL when connecting to the server.              |\r\n| `on_connect`           | `Callable[[str, Identity], None]` | Callback to be invoked when the client connects to the server. |\r\n| `subscription_queries` | `List[str]`                       | List of queries to subscribe to.                               |\r\n\r\nIf `auth_token` is not None, they will be passed to the new connection to identify and authenticate the user. Otherwise, a new Identity and auth token will be generated by the server. An optional [local_config](#local_config) module can be used to store the user's auth token to local storage.\r\n\r\nIf you are connecting to SpacetimeDB Cloud `testnet` the host should be `testnet.spacetimedb.com` and `ssl_enabled` should be `True`. If you are connecting to SpacetimeDB Standalone locally, the host should be `localhost:3000` and `ssl_enabled` should be `False`. For instructions on how to deploy to these environments, see the [Deployment Section](/docs/DeploymentOverview.md)\r\n\r\n```python\r\nasyncio.run(\r\n    spacetime_client.run(\r\n        AUTH_TOKEN,\r\n        \"localhost:3000\",\r\n        \"my-module-name\",\r\n        False,\r\n        on_connect,\r\n        [\"SELECT * FROM User\", \"SELECT * FROM Message\"],\r\n    )\r\n)\r\n```\r\n\r\n### Function `subscribe`\r\n\r\n```rust\r\ndef subscribe(self, queries: List[str])\r\n```\r\n\r\nSubscribe to a set of queries, to be notified when rows which match those queries are altered.\r\n\r\n| Argument  | Type        | Meaning                      |\r\n| --------- | ----------- | ---------------------------- |\r\n| `queries` | `List[str]` | SQL queries to subscribe to. |\r\n\r\nThe `queries` should be a slice of strings representing SQL queries.\r\n\r\nA new call to `subscribe` will remove all previous subscriptions and replace them with the new `queries`. If any rows matched the previous subscribed queries but do not match the new queries, those rows will be removed from the client cache. Row update events will be dispatched for any inserts and deletes that occur as a result of the new queries. For these events, the [`ReducerEvent`](#type-reducerevent) argument will be `None`.\r\n\r\nThis should be called before the async client is started with [`run`](#function-run).\r\n\r\n```python\r\nspacetime_client.subscribe([\"SELECT * FROM User;\", \"SELECT * FROM Message;\"])\r\n```\r\n\r\nSubscribe to a set of queries, to be notified when rows which match those queries are altered.\r\n\r\n### Function `register_on_subscription_applied`\r\n\r\n```python\r\ndef register_on_subscription_applied(self, callback)\r\n```\r\n\r\nRegister a callback function to be executed when the local cache is updated as a result of a change to the subscription queries.\r\n\r\n| Argument   | Type                 | Meaning                                                |\r\n| ---------- | -------------------- | ------------------------------------------------------ |\r\n| `callback` | `Callable[[], None]` | Callback to be invoked when subscriptions are applied. |\r\n\r\nThe callback will be invoked after a successful [`subscribe`](#function-subscribe) call when the initial set of matching rows becomes available.\r\n\r\n```python\r\nspacetime_client.register_on_subscription_applied(on_subscription_applied)\r\n```\r\n\r\n### Function `force_close`\r\n\r\n```python\r\ndef force_close(self)\r\n)\r\n```\r\n\r\nSignal the client to stop processing events and close the connection to the server.\r\n\r\n```python\r\nspacetime_client.force_close()\r\n```\r\n\r\n### Function `schedule_event`\r\n\r\n```python\r\ndef schedule_event(self, delay_secs, callback, *args)\r\n```\r\n\r\nSchedule an event to be fired after a delay\r\n\r\nTo create a repeating event, call schedule_event() again from within the callback function.\r\n\r\n| Argument     | Type                 | Meaning                                                        |\r\n| ------------ | -------------------- | -------------------------------------------------------------- |\r\n| `delay_secs` | `float`              | number of seconds to wait before firing the event              |\r\n| `callback`   | `Callable[[], None]` | Callback to be invoked when the event fires.                   |\r\n| `args`       | `*args`              | Variable number of arguments to pass to the callback function. |\r\n\r\n```python\r\ndef application_tick():\r\n    # ... do some work\r\n\r\n    spacetime_client.schedule_event(0.1, application_tick)\r\n\r\nspacetime_client.schedule_event(0.1, application_tick)\r\n```\r\n\r\n## Basic Client Reference\r\n\r\n### API at a glance\r\n\r\n| Definition                                                                                                       | Description                                                                                                                      |\r\n| ---------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |\r\n| Function [`SpacetimeDBClient::init`](#function-init)                                                             | Create a network manager instance.                                                                                               |\r\n| Function [`SpacetimeDBClient::subscribe`](#function-subscribe)                                                   | Subscribe to receive data and transaction updates for the provided queries.                                                      |\r\n| Function [`SpacetimeDBClient::register_on_event`](#function-register_on_event)                                   | Register a callback function to handle transaction update events.                                                                |\r\n| Function [`SpacetimeDBClient::unregister_on_event`](#function-unregister_on_event)                               | Unregister a callback function that was previously registered using `register_on_event`.                                         |\r\n| Function [`SpacetimeDBClient::register_on_subscription_applied`](#function-register_on_subscription_applied)     | Register a callback function to be executed when the local cache is updated as a result of a change to the subscription queries. |\r\n| Function [`SpacetimeDBClient::unregister_on_subscription_applied`](#function-unregister_on_subscription_applied) | Unregister a callback function from the subscription update event.                                                               |\r\n| Function [`SpacetimeDBClient::update`](#function-update)                                                         | Process all pending incoming messages from the SpacetimeDB module.                                                               |\r\n| Function [`SpacetimeDBClient::close`](#function-close)                                                           | Close the WebSocket connection.                                                                                                  |\r\n| Type [`TransactionUpdateMessage`](#type-transactionupdatemessage)                                                | Represents a transaction update message.                                                                                         |\r\n\r\n### Function `init`\r\n\r\n```python\r\n@classmethod\r\ndef init(\r\n    auth_token: str,\r\n    host: str,\r\n    address_or_name: str,\r\n    ssl_enabled: bool,\r\n    autogen_package: module,\r\n    on_connect: Callable[[], NoneType] = None,\r\n    on_disconnect: Callable[[str], NoneType] = None,\r\n    on_identity: Callable[[str, Identity], NoneType] = None,\r\n    on_error: Callable[[str], NoneType] = None\r\n)\r\n```\r\n\r\nCreate a network manager instance.\r\n\r\n| Argument          | Type                              | Meaning                                                                                                                         |\r\n| ----------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |\r\n| `auth_token`      | `str`                             | This is the token generated by SpacetimeDB that matches the user's identity. If None, token will be generated                   |\r\n| `host`            | `str`                             | Hostname:port for SpacetimeDB connection                                                                                        |\r\n| `address_or_name` | `str`                             | The name or address of the database to connect to                                                                               |\r\n| `ssl_enabled`     | `bool`                            | Whether to use SSL when connecting to the server.                                                                               |\r\n| `autogen_package` | `ModuleType`                      | Python package where SpacetimeDB module generated files are located.                                                            |\r\n| `on_connect`      | `Callable[[], None]`              | Optional callback called when a connection is made to the SpacetimeDB module.                                                   |\r\n| `on_disconnect`   | `Callable[[str], None]`           | Optional callback called when the Python client is disconnected from the SpacetimeDB module. The argument is the close message. |\r\n| `on_identity`     | `Callable[[str, Identity], None]` | Called when the user identity is recieved from SpacetimeDB. First argument is the auth token used to login in future sessions.  |\r\n| `on_error`        | `Callable[[str], None]`           | Optional callback called when the Python client connection encounters an error. The argument is the error message.              |\r\n\r\nThis function creates a new SpacetimeDBClient instance. It should be called before any other functions in the SpacetimeDBClient class. This init will call connect for you.\r\n\r\n```python\r\nSpacetimeDBClient.init(autogen, on_connect=self.on_connect)\r\n```\r\n\r\n### Function `subscribe`\r\n\r\n```python\r\ndef subscribe(queries: List[str])\r\n```\r\n\r\nSubscribe to receive data and transaction updates for the provided queries.\r\n\r\n| Argument  | Type        | Meaning                                                                                                  |\r\n| --------- | ----------- | -------------------------------------------------------------------------------------------------------- |\r\n| `queries` | `List[str]` | A list of queries to subscribe to. Each query is a string representing an sql formatted query statement. |\r\n\r\nThis function sends a subscription request to the SpacetimeDB module, indicating that the client wants to receive data and transaction updates related to the specified queries.\r\n\r\n```python\r\nqueries = [\"SELECT * FROM table1\", \"SELECT * FROM table2 WHERE col2 = 0\"]\r\nSpacetimeDBClient.instance.subscribe(queries)\r\n```\r\n\r\n### Function `register_on_event`\r\n\r\n```python\r\ndef register_on_event(callback: Callable[[TransactionUpdateMessage], NoneType])\r\n```\r\n\r\nRegister a callback function to handle transaction update events.\r\n\r\n| Argument   | Type                                         | Meaning                                                                                                                                                                                                                  |\r\n| ---------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `callback` | `Callable[[TransactionUpdateMessage], None]` | A callback function that takes a single argument of type `TransactionUpdateMessage`. This function will be invoked with a `TransactionUpdateMessage` instance containing information about the transaction update event. |\r\n\r\nThis function registers a callback function that will be called when a reducer modifies a table matching any of the subscribed queries or if a reducer called by this Python client encounters a failure.\r\n\r\n```python\r\ndef handle_event(transaction_update):\r\n    # Code to handle the transaction update event\r\n\r\nSpacetimeDBClient.instance.register_on_event(handle_event)\r\n```\r\n\r\n### Function `unregister_on_event`\r\n\r\n```python\r\ndef unregister_on_event(callback: Callable[[TransactionUpdateMessage], NoneType])\r\n```\r\n\r\nUnregister a callback function that was previously registered using `register_on_event`.\r\n\r\n| Argument   | Type                                         | Meaning                              |\r\n| ---------- | -------------------------------------------- | ------------------------------------ |\r\n| `callback` | `Callable[[TransactionUpdateMessage], None]` | The callback function to unregister. |\r\n\r\n```python\r\nSpacetimeDBClient.instance.unregister_on_event(handle_event)\r\n```\r\n\r\n### Function `register_on_subscription_applied`\r\n\r\n```python\r\ndef register_on_subscription_applied(callback: Callable[[], NoneType])\r\n```\r\n\r\nRegister a callback function to be executed when the local cache is updated as a result of a change to the subscription queries.\r\n\r\n| Argument   | Type                 | Meaning                                                                                                                                                      |\r\n| ---------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `callback` | `Callable[[], None]` | A callback function that will be invoked on each subscription update. The callback function should not accept any arguments and should not return any value. |\r\n\r\n```python\r\ndef subscription_callback():\r\n    # Code to be executed on each subscription update\r\n\r\nSpacetimeDBClient.instance.register_on_subscription_applied(subscription_callback)\r\n```\r\n\r\n### Function `unregister_on_subscription_applied`\r\n\r\n```python\r\ndef unregister_on_subscription_applied(callback: Callable[[], NoneType])\r\n```\r\n\r\nUnregister a callback function from the subscription update event.\r\n\r\n| Argument   | Type                 | Meaning                                                                                                  |\r\n| ---------- | -------------------- | -------------------------------------------------------------------------------------------------------- |\r\n| `callback` | `Callable[[], None]` | A callback function that was previously registered with the `register_on_subscription_applied` function. |\r\n\r\n```python\r\ndef subscription_callback():\r\n    # Code to be executed on each subscription update\r\n\r\nSpacetimeDBClient.instance.register_on_subscription_applied(subscription_callback)\r\n```\r\n\r\n### Function `update`\r\n\r\n```python\r\ndef update()\r\n```\r\n\r\nProcess all pending incoming messages from the SpacetimeDB module.\r\n\r\nThis function must be called on a regular interval in the main loop to process incoming messages.\r\n\r\n```python\r\nwhile True:\r\n    SpacetimeDBClient.instance.update() # Call the update function in a loop to process incoming messages\r\n    # Additional logic or code can be added here\r\n```\r\n\r\n### Function `close`\r\n\r\n```python\r\ndef close()\r\n```\r\n\r\nClose the WebSocket connection.\r\n\r\nThis function closes the WebSocket connection to the SpacetimeDB module.\r\n\r\n```python\r\nSpacetimeDBClient.instance.close()\r\n```\r\n\r\n### Type `TransactionUpdateMessage`\r\n\r\n```python\r\nclass TransactionUpdateMessage:\r\n    def __init__(\r\n        self,\r\n        caller_identity: Identity,\r\n        status: str,\r\n        message: str,\r\n        reducer_name: str,\r\n        args: Dict\r\n    )\r\n```\r\n\r\n| Member            | Args       | Meaning                                           |\r\n| ----------------- | ---------- | ------------------------------------------------- |\r\n| `caller_identity` | `Identity` | The identity of the caller.                       |\r\n| `status`          | `str`      | The status of the transaction.                    |\r\n| `message`         | `str`      | A message associated with the transaction update. |\r\n| `reducer_name`    | `str`      | The reducer used for the transaction.             |\r\n| `args`            | `Dict`     | Additional arguments for the transaction.         |\r\n\r\nRepresents a transaction update message. Used in on_event callbacks.\r\n\r\nFor more details, see [`register_on_event`](#function-register_on_event).\r\n",
              "editUrl": "SDK%20Reference.md",
              "jumpLinks": [
                {
                  "title": "The SpacetimeDB Python client SDK",
                  "route": "the-spacetimedb-python-client-sdk",
                  "depth": 1
                },
                {
                  "title": "Install the SDK",
                  "route": "install-the-sdk",
                  "depth": 2
                },
                {
                  "title": "Generate module bindings",
                  "route": "generate-module-bindings",
                  "depth": 2
                },
                {
                  "title": "Basic vs Async SpacetimeDB Client",
                  "route": "basic-vs-async-spacetimedb-client",
                  "depth": 2
                },
                {
                  "title": "Common Client Reference",
                  "route": "common-client-reference",
                  "depth": 2
                },
                {
                  "title": "API at a glance",
                  "route": "api-at-a-glance",
                  "depth": 3
                },
                {
                  "title": "Type `Identity`",
                  "route": "type-identity-",
                  "depth": 3
                },
                {
                  "title": "Type `ReducerEvent`",
                  "route": "type-reducerevent-",
                  "depth": 3
                },
                {
                  "title": "Type `{TABLE}`",
                  "route": "type-table-",
                  "depth": 3
                },
                {
                  "title": "Method `filter_by_{COLUMN}`",
                  "route": "method-filter_by_-column-",
                  "depth": 3
                },
                {
                  "title": "Method `iter`",
                  "route": "method-iter-",
                  "depth": 3
                },
                {
                  "title": "Method `register_row_update`",
                  "route": "method-register_row_update-",
                  "depth": 3
                },
                {
                  "title": "Function `{REDUCER_NAME}`",
                  "route": "function-reducer_name-",
                  "depth": 3
                },
                {
                  "title": "Function `register_on_{REDUCER_NAME}`",
                  "route": "function-register_on_-reducer_name-",
                  "depth": 3
                },
                {
                  "title": "Async Client Reference",
                  "route": "async-client-reference",
                  "depth": 2
                },
                {
                  "title": "API at a glance",
                  "route": "api-at-a-glance",
                  "depth": 3
                },
                {
                  "title": "Function `run`",
                  "route": "function-run-",
                  "depth": 3
                },
                {
                  "title": "Function `subscribe`",
                  "route": "function-subscribe-",
                  "depth": 3
                },
                {
                  "title": "Function `register_on_subscription_applied`",
                  "route": "function-register_on_subscription_applied-",
                  "depth": 3
                },
                {
                  "title": "Function `force_close`",
                  "route": "function-force_close-",
                  "depth": 3
                },
                {
                  "title": "Function `schedule_event`",
                  "route": "function-schedule_event-",
                  "depth": 3
                },
                {
                  "title": "Basic Client Reference",
                  "route": "basic-client-reference",
                  "depth": 2
                },
                {
                  "title": "API at a glance",
                  "route": "api-at-a-glance",
                  "depth": 3
                },
                {
                  "title": "Function `init`",
                  "route": "function-init-",
                  "depth": 3
                },
                {
                  "title": "Function `subscribe`",
                  "route": "function-subscribe-",
                  "depth": 3
                },
                {
                  "title": "Function `register_on_event`",
                  "route": "function-register_on_event-",
                  "depth": 3
                },
                {
                  "title": "Function `unregister_on_event`",
                  "route": "function-unregister_on_event-",
                  "depth": 3
                },
                {
                  "title": "Function `register_on_subscription_applied`",
                  "route": "function-register_on_subscription_applied-",
                  "depth": 3
                },
                {
                  "title": "Function `unregister_on_subscription_applied`",
                  "route": "function-unregister_on_subscription_applied-",
                  "depth": 3
                },
                {
                  "title": "Function `update`",
                  "route": "function-update-",
                  "depth": 3
                },
                {
                  "title": "Function `close`",
                  "route": "function-close-",
                  "depth": 3
                },
                {
                  "title": "Type `TransactionUpdateMessage`",
                  "route": "type-transactionupdatemessage-",
                  "depth": 3
                }
              ],
              "pages": []
            }
          ]
        },
        {
          "title": "Rust",
          "identifier": "Rust",
          "indexIdentifier": "index",
          "comingSoon": false,
          "hasPages": true,
          "editUrl": "Rust/index.md",
          "jumpLinks": [],
          "pages": [
            {
              "title": "Rust Client SDK Quick Start",
              "identifier": "index",
              "indexIdentifier": "index",
              "content": "# Rust Client SDK Quick Start\r\n\r\nIn this guide we'll show you how to get up and running with a simple SpacetimDB app with a client written in Rust.\r\n\r\nWe'll implement a command-line client for the module created in our Rust or C# Module Quickstart guides. Make sure you follow one of these guides before you start on this one.\r\n\r\n## Project structure\r\n\r\nEnter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/server-languages/rust/rust-module-quickstart-guide) or [C# Module Quickstart](/docs/server-languages/csharp/csharp-module-reference) guides:\r\n\r\n```bash\r\ncd quickstart-chat\r\n```\r\n\r\nWithin it, create a `client` crate, our client application, which users run locally:\r\n\r\n```bash\r\ncargo new client\r\n```\r\n\r\n## Depend on `spacetimedb-sdk` and `hex`\r\n\r\n`client/Cargo.toml` should be initialized without any dependencies. We'll need two:\r\n\r\n- [`spacetimedb-sdk`](https://crates.io/crates/spacetimedb-sdk), which defines client-side interfaces for interacting with a remote SpacetimeDB module.\r\n- [`hex`](https://crates.io/crates/hex), which we'll use to print unnamed users' identities as hexadecimal strings.\r\n\r\nBelow the `[dependencies]` line in `client/Cargo.toml`, add:\r\n\r\n```toml\r\nspacetimedb-sdk = \"0.6\"\r\nhex = \"0.4\"\r\n```\r\n\r\nMake sure you depend on the same version of `spacetimedb-sdk` as is reported by the SpacetimeDB CLI tool's `spacetime version`!\r\n\r\n## Clear `client/src/main.rs`\r\n\r\n`client/src/main.rs` should be initialized with a trivial \"Hello world\" program. Clear it out so we can write our chat client.\r\n\r\nIn your `quickstart-chat` directory, run:\r\n\r\n```bash\r\nrm client/src/main.rs\r\ntouch client/src/main.rs\r\n```\r\n\r\n## Generate your module types\r\n\r\nThe `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.\r\n\r\nIn your `quickstart-chat` directory, run:\r\n\r\n```bash\r\nmkdir -p client/src/module_bindings\r\nspacetime generate --lang rust --out-dir client/src/module_bindings --project-path server\r\n```\r\n\r\nTake a look inside `client/src/module_bindings`. The CLI should have generated five files:\r\n\r\n```\r\nmodule_bindings\r\n├── message.rs\r\n├── mod.rs\r\n├── send_message_reducer.rs\r\n├── set_name_reducer.rs\r\n└── user.rs\r\n```\r\n\r\nWe need to declare the module in our client crate, and we'll want to import its definitions.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\nmod module_bindings;\r\nuse module_bindings::*;\r\n```\r\n\r\n## Add more imports\r\n\r\nWe'll need a whole boatload of imports from `spacetimedb_sdk`, which we'll describe when we use them.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\nuse spacetimedb_sdk::{\r\n    disconnect,\r\n    identity::{load_credentials, once_on_connect, save_credentials, Credentials, Identity},\r\n    on_disconnect, on_subscription_applied,\r\n    reducer::Status,\r\n    subscribe,\r\n    table::{TableType, TableWithPrimaryKey},\r\n};\r\n```\r\n\r\n## Define main function\r\n\r\nWe'll work outside-in, first defining our `main` function at a high level, then implementing each behavior it needs. We need `main` to do five things:\r\n\r\n1. Register callbacks on any events we want to handle. These will print to standard output messages received from the database and updates about users' names and online statuses.\r\n2. Establish a connection to the database. This will involve authenticating with our credentials, if we're a returning user.\r\n3. Subscribe to receive updates on tables.\r\n4. Loop, processing user input from standard input. This will be how we enable users to set their names and send messages.\r\n5. Close our connection. This one is easy; we just call `spacetimedb_sdk::disconnect`.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\nfn main() {\r\n    register_callbacks();\r\n    connect_to_db();\r\n    subscribe_to_tables();\r\n    user_input_loop();\r\n}\r\n```\r\n\r\n## Register callbacks\r\n\r\nWe need to handle several sorts of events:\r\n\r\n1. When we connect and receive our credentials, we'll save them to a file so that the next time we connect, we can re-authenticate as the same user.\r\n2. When a new user joins, we'll print a message introducing them.\r\n3. When a user is updated, we'll print their new name, or declare their new online status.\r\n4. When we receive a new message, we'll print it.\r\n5. When we're informed of the backlog of past messages, we'll sort them and print them in order.\r\n6. If the server rejects our attempt to set our name, we'll print an error.\r\n7. If the server rejects a message we send, we'll print an error.\r\n8. When our connection ends, we'll print a note, then exit the process.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Register all the callbacks our app will use to respond to database events.\r\nfn register_callbacks() {\r\n    // When we receive our `Credentials`, save them to a file.\r\n    once_on_connect(on_connected);\r\n\r\n    // When a new user joins, print a notification.\r\n    User::on_insert(on_user_inserted);\r\n\r\n    // When a user's status changes, print a notification.\r\n    User::on_update(on_user_updated);\r\n\r\n    // When a new message is received, print it.\r\n    Message::on_insert(on_message_inserted);\r\n\r\n    // When we receive the message backlog, print it in timestamp order.\r\n    on_subscription_applied(on_sub_applied);\r\n\r\n    // When we fail to set our name, print a warning.\r\n    on_set_name(on_name_set);\r\n\r\n    // When we fail to send a message, print a warning.\r\n    on_send_message(on_message_sent);\r\n\r\n    // When our connection closes, inform the user and exit.\r\n    on_disconnect(on_disconnected);\r\n}\r\n```\r\n\r\n### Save credentials\r\n\r\nEach client has a `Credentials`, which consists of two parts:\r\n\r\n- An `Identity`, a unique public identifier. We're using these to identify `User` rows.\r\n- A `Token`, a private key which SpacetimeDB uses to authenticate the client.\r\n\r\n`Credentials` are generated by SpacetimeDB each time a new client connects, and sent to the client so they can be saved, in order to re-connect with the same identity. The Rust SDK provides a pair of functions, `save_credentials` and `load_credentials`, for storing these credentials in a file. We'll save our credentials into a file in the directory `~/.spacetime_chat`, which should be unintrusive. If saving our credentials fails, we'll print a message to standard error, but otherwise continue normally; even though the user won't be able to reconnect with the same identity, they can still chat normally.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `on_connect` callback: save our credentials to a file.\r\nfn on_connected(creds: &Credentials) {\r\n    if let Err(e) = save_credentials(CREDS_DIR, creds) {\r\n        eprintln!(\"Failed to save credentials: {:?}\", e);\r\n    }\r\n}\r\n\r\nconst CREDS_DIR: &str = \".spacetime_chat\";\r\n```\r\n\r\n### Notify about new users\r\n\r\nFor each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `on_insert` and `on_delete` methods of the trait `TableType`, which is automatically implemented for each table by `spacetime generate`.\r\n\r\nThese callbacks can fire in two contexts:\r\n\r\n- After a reducer runs, when the client's cache is updated about changes to subscribed rows.\r\n- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.\r\n\r\nThis second case means that, even though the module only ever inserts online users, the client's `User::on_insert` callbacks may be invoked with users who are offline. We'll only notify about online users.\r\n\r\n`on_insert` and `on_delete` callbacks take two arguments: the altered row, and an `Option<&ReducerEvent>`. This will be `Some` for rows altered by a reducer run, and `None` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is an enum autogenerated by `spacetime generate` with a variant for each reducer defined by the module. For now, we can ignore this argument.\r\n\r\nWhenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define functions `user_name_or_identity` and `identity_leading_hex` to handle this.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `User::on_insert` callback:\r\n/// if the user is online, print a notification.\r\nfn on_user_inserted(user: &User, _: Option<&ReducerEvent>) {\r\n    if user.online {\r\n        println!(\"User {} connected.\", user_name_or_identity(user));\r\n    }\r\n}\r\n\r\nfn user_name_or_identity(user: &User) -> String {\r\n    user.name\r\n        .clone()\r\n        .unwrap_or_else(|| identity_leading_hex(&user.identity))\r\n}\r\n\r\nfn identity_leading_hex(id: &Identity) -> String {\r\n    hex::encode(&id.bytes()[0..8])\r\n}\r\n```\r\n\r\n### Notify about updated users\r\n\r\nBecause we declared a `#[primarykey]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User::update_by_identity` calls. We register these callbacks using the `on_update` method of the trait `TableWithPrimaryKey`, which is automatically implemented by `spacetime generate` for any table with a `#[primarykey]` column.\r\n\r\n`on_update` callbacks take three arguments: the old row, the new row, and an `Option<&ReducerEvent>`.\r\n\r\nIn our module, users can be updated for three reasons:\r\n\r\n1. They've set their name using the `set_name` reducer.\r\n2. They're an existing user re-connecting, so their `online` has been set to `true`.\r\n3. They've disconnected, so their `online` has been set to `false`.\r\n\r\nWe'll print an appropriate message in each of these cases.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `User::on_update` callback:\r\n/// print a notification about name and status changes.\r\nfn on_user_updated(old: &User, new: &User, _: Option<&ReducerEvent>) {\r\n    if old.name != new.name {\r\n        println!(\r\n            \"User {} renamed to {}.\",\r\n            user_name_or_identity(old),\r\n            user_name_or_identity(new)\r\n        );\r\n    }\r\n    if old.online && !new.online {\r\n        println!(\"User {} disconnected.\", user_name_or_identity(new));\r\n    }\r\n    if !old.online && new.online {\r\n        println!(\"User {} connected.\", user_name_or_identity(new));\r\n    }\r\n}\r\n```\r\n\r\n### Print messages\r\n\r\nWhen we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `print_new_message` callback will check if its `reducer_event` argument is `Some`, and only print in that case.\r\n\r\nTo find the `User` based on the message's `sender` identity, we'll use `User::filter_by_identity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `filter_by_identity` accepts an owned `Identity`, rather than a reference. We can `clone` the identity held in `message.sender`.\r\n\r\nWe'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `Message::on_insert` callback: print new messages.\r\nfn on_message_inserted(message: &Message, reducer_event: Option<&ReducerEvent>) {\r\n    if reducer_event.is_some() {\r\n        print_message(message);\r\n    }\r\n}\r\n\r\nfn print_message(message: &Message) {\r\n    let sender = User::filter_by_identity(message.sender.clone())\r\n        .map(|u| user_name_or_identity(&u))\r\n        .unwrap_or_else(|| \"unknown\".to_string());\r\n    println!(\"{}: {}\", sender, message.text);\r\n}\r\n```\r\n\r\n### Print past messages in order\r\n\r\nMessages we receive live will come in order, but when we connect, we'll receive all the past messages at once. We can't just print these in the order we receive them; the logs would be all shuffled around, and would make no sense. Instead, when we receive the log of past messages, we'll sort them by their sent timestamps and print them in order.\r\n\r\nWe'll handle this in our function `print_messages_in_order`, which we registered as an `on_subscription_applied` callback. `print_messages_in_order` iterates over all the `Message`s we've received, sorts them, and then prints them. `Message::iter()` is defined on the trait `TableType`, and returns an iterator over all the messages in the client's cache. Rust iterators can't be sorted in-place, so we'll collect it to a `Vec`, then use the `sort_by_key` method to sort by timestamp.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `on_subscription_applied` callback:\r\n/// sort all past messages and print them in timestamp order.\r\nfn on_sub_applied() {\r\n    let mut messages = Message::iter().collect::<Vec<_>>();\r\n    messages.sort_by_key(|m| m.sent);\r\n    for message in messages {\r\n        print_message(&message);\r\n    }\r\n}\r\n```\r\n\r\n### Warn if our name was rejected\r\n\r\nWe can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `on_reducer` method of the `Reducer` trait, which is automatically implemented for each reducer by `spacetime generate`.\r\n\r\nEach reducer callback takes at least two arguments:\r\n\r\n1. The `Identity` of the client who requested the reducer invocation.\r\n2. The `Status` of the reducer run, one of `Committed`, `Failed` or `OutOfEnergy`. `Status::Failed` holds the error which caused the reducer to fail, as a `String`.\r\n\r\nIn addition, it takes a reference to each of the arguments passed to the reducer itself.\r\n\r\nThese callbacks will be invoked in one of two cases:\r\n\r\n1. If the reducer was successful and altered any of our subscribed rows.\r\n2. If we requested an invocation which failed.\r\n\r\nNote that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.\r\n\r\nWe already handle successful `set_name` invocations using our `User::on_update` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `warn_if_name_rejected` as a `SetNameArgs::on_reducer` callback which checks if the reducer failed, and if it did, prints a message including the rejected name and the error.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `on_set_name` callback: print a warning if the reducer failed.\r\nfn on_name_set(_sender: &Identity, status: &Status, name: &String) {\r\n    if let Status::Failed(err) = status {\r\n        eprintln!(\"Failed to change name to {:?}: {}\", name, err);\r\n    }\r\n}\r\n```\r\n\r\n### Warn if our message was rejected\r\n\r\nWe handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `on_send_message` callback: print a warning if the reducer failed.\r\nfn on_message_sent(_sender: &Identity, status: &Status, text: &String) {\r\n    if let Status::Failed(err) = status {\r\n        eprintln!(\"Failed to send message {:?}: {}\", text, err);\r\n    }\r\n}\r\n```\r\n\r\n### Exit on disconnect\r\n\r\nWe can register callbacks to run when our connection ends using `on_disconnect`. These callbacks will run either when the client disconnects by calling `disconnect`, or when the server closes our connection. More involved apps might attempt to reconnect in this case, or do some sort of client-side cleanup, but we'll just print a note to the user and then exit the process.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Our `on_disconnect` callback: print a note, then exit the process.\r\nfn on_disconnected() {\r\n    eprintln!(\"Disconnected!\");\r\n    std::process::exit(0)\r\n}\r\n```\r\n\r\n## Connect to the database\r\n\r\nNow that our callbacks are all set up, we can connect to the database. We'll store the URI of the SpacetimeDB instance and our module name in constants `SPACETIMEDB_URI` and `DB_NAME`. Replace `<module-name>` with the name you chose when publishing your module during the module quickstart.\r\n\r\n`connect` takes an `Option<Credentials>`, which is `None` for a new connection, or `Some` for a returning user. The Rust SDK defines `load_credentials`, the counterpart to the `save_credentials` we used in our `save_credentials_or_log_error`, to load `Credentials` from a file. `load_credentials` returns `Result<Option<Credentials>>`, with `Ok(None)` meaning the credentials haven't been saved yet, and an `Err` meaning reading from disk failed. We can `expect` to handle the `Result`, and pass the `Option<Credentials>` directly to `connect`.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// The URL of the SpacetimeDB instance hosting our chat module.\r\nconst SPACETIMEDB_URI: &str = \"http://localhost:3000\";\r\n\r\n/// The module name we chose when we published our module.\r\nconst DB_NAME: &str = \"<module-name>\";\r\n\r\n/// Load credentials from a file and connect to the database.\r\nfn connect_to_db() {\r\n    connect(\r\n        SPACETIMEDB_URI,\r\n        DB_NAME,\r\n        load_credentials(CREDS_DIR).expect(\"Error reading stored credentials\"),\r\n    )\r\n    .expect(\"Failed to connect\");\r\n}\r\n```\r\n\r\n## Subscribe to queries\r\n\r\nSpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the \"chunk\" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Register subscriptions for all rows of both tables.\r\nfn subscribe_to_tables() {\r\n    subscribe(&[\"SELECT * FROM User;\", \"SELECT * FROM Message;\"]).unwrap();\r\n}\r\n```\r\n\r\n## Handle user input\r\n\r\nA user should interact with our client by typing lines into their terminal. A line that starts with `/name ` will set the user's name to the rest of the line. Any other line will send a message.\r\n\r\n`spacetime generate` defined two functions for us, `set_name` and `send_message`, which send a message to the database to invoke the corresponding reducer. The first argument, the `ReducerContext`, is supplied by the server, but we pass all other arguments ourselves. In our case, that means that both `set_name` and `send_message` take one argument, a `String`.\r\n\r\nTo `client/src/main.rs`, add:\r\n\r\n```rust\r\n/// Read each line of standard input, and either set our name or send a message as appropriate.\r\nfn user_input_loop() {\r\n    for line in std::io::stdin().lines() {\r\n        let Ok(line) = line else {\r\n            panic!(\"Failed to read from stdin.\");\r\n        };\r\n        if let Some(name) = line.strip_prefix(\"/name \") {\r\n            set_name(name.to_string());\r\n        } else {\r\n            send_message(line);\r\n        }\r\n    }\r\n}\r\n```\r\n\r\n## Run it\r\n\r\nChange your directory to the client app, then compile and run it. From the `quickstart-chat` directory, run:\r\n\r\n```bash\r\ncd client\r\ncargo run\r\n```\r\n\r\nYou should see something like:\r\n\r\n```\r\nUser d9e25c51996dea2f connected.\r\n```\r\n\r\nNow try sending a message. Type `Hello, world!` and press enter. You should see something like:\r\n\r\n```\r\nd9e25c51996dea2f: Hello, world!\r\n```\r\n\r\nNext, set your name. Type `/name <my-name>`, replacing `<my-name>` with your name. You should see something like:\r\n\r\n```\r\nUser d9e25c51996dea2f renamed to <my-name>.\r\n```\r\n\r\nThen send another message. Type `Hello after naming myself.` and press enter. You should see:\r\n\r\n```\r\n<my-name>: Hello after naming myself.\r\n```\r\n\r\nNow, close the app by hitting control-c, and start it again with `cargo run`. You should see yourself connecting, and your past messages in order:\r\n\r\n```\r\nUser <my-name> connected.\r\n<my-name>: Hello, world!\r\n<my-name>: Hello after naming myself.\r\n```\r\n\r\n## What's next?\r\n\r\nYou can find the full code for this client [in the Rust SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/quickstart-chat).\r\n\r\nCheck out the [Rust SDK Reference](/docs/client-languages/rust/rust-sdk-reference) for a more comprehensive view of the SpacetimeDB Rust SDK.\r\n\r\nOur bare-bones terminal interface has some quirks. Incoming messages can appear while the user is typing and be spliced into the middle of user input, which is less than ideal. Also, the user's input is interspersed with the program's output, so messages the user sends will seem to appear twice. Why not try building a better interface using [Rustyline](https://crates.io/crates/rustyline), [Cursive](https://crates.io/crates/cursive), or even a full-fledged GUI? We went for the Cursive route, and you can check out what we came up with [in the Rust SDK's examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/cursive-chat).\r\n\r\nOnce our chat server runs for a while, messages will accumulate, and it will get frustrating to see the entire backlog each time you connect. Instead, you could refine your `Message` subscription query, subscribing only to messages newer than, say, half an hour before the user connected.\r\n\r\nYou could also add support for styling messages, perhaps by interpreting HTML tags in the messages and printing appropriate [ANSI escapes](https://en.wikipedia.org/wiki/ANSI_escape_code).\r\n\r\nOr, you could extend the module and the client together, perhaps:\r\n\r\n- Adding a `moderator: bool` flag to `User` and allowing moderators to time-out or ban naughty chatters.\r\n- Adding a message of the day which gets shown to users whenever they connect, or some rules which get shown only to new users.\r\n- Supporting separate rooms or channels which users can join or leave, and maybe even direct messages.\r\n- Allowing users to set their status, which could be displayed alongside their username.\r\n",
              "hasPages": false,
              "editUrl": "index.md",
              "jumpLinks": [
                {
                  "title": "Rust Client SDK Quick Start",
                  "route": "rust-client-sdk-quick-start",
                  "depth": 1
                },
                {
                  "title": "Project structure",
                  "route": "project-structure",
                  "depth": 2
                },
                {
                  "title": "Depend on `spacetimedb-sdk` and `hex`",
                  "route": "depend-on-spacetimedb-sdk-and-hex-",
                  "depth": 2
                },
                {
                  "title": "Clear `client/src/main.rs`",
                  "route": "clear-client-src-main-rs-",
                  "depth": 2
                },
                {
                  "title": "Generate your module types",
                  "route": "generate-your-module-types",
                  "depth": 2
                },
                {
                  "title": "Add more imports",
                  "route": "add-more-imports",
                  "depth": 2
                },
                {
                  "title": "Define main function",
                  "route": "define-main-function",
                  "depth": 2
                },
                {
                  "title": "Register callbacks",
                  "route": "register-callbacks",
                  "depth": 2
                },
                {
                  "title": "Save credentials",
                  "route": "save-credentials",
                  "depth": 3
                },
                {
                  "title": "Notify about new users",
                  "route": "notify-about-new-users",
                  "depth": 3
                },
                {
                  "title": "Notify about updated users",
                  "route": "notify-about-updated-users",
                  "depth": 3
                },
                {
                  "title": "Print messages",
                  "route": "print-messages",
                  "depth": 3
                },
                {
                  "title": "Print past messages in order",
                  "route": "print-past-messages-in-order",
                  "depth": 3
                },
                {
                  "title": "Warn if our name was rejected",
                  "route": "warn-if-our-name-was-rejected",
                  "depth": 3
                },
                {
                  "title": "Warn if our message was rejected",
                  "route": "warn-if-our-message-was-rejected",
                  "depth": 3
                },
                {
                  "title": "Exit on disconnect",
                  "route": "exit-on-disconnect",
                  "depth": 3
                },
                {
                  "title": "Connect to the database",
                  "route": "connect-to-the-database",
                  "depth": 2
                },
                {
                  "title": "Subscribe to queries",
                  "route": "subscribe-to-queries",
                  "depth": 2
                },
                {
                  "title": "Handle user input",
                  "route": "handle-user-input",
                  "depth": 2
                },
                {
                  "title": "Run it",
                  "route": "run-it",
                  "depth": 2
                },
                {
                  "title": "What's next?",
                  "route": "what-s-next-",
                  "depth": 2
                }
              ],
              "pages": []
            },
            {
              "title": "The SpacetimeDB Rust client SDK",
              "identifier": "SDK Reference",
              "indexIdentifier": "SDK Reference",
              "hasPages": false,
              "content": "# The SpacetimeDB Rust client SDK\r\n\r\nThe SpacetimeDB client SDK for Rust contains all the tools you need to build native clients for SpacetimeDB modules using Rust.\r\n\r\n## Install the SDK\r\n\r\nFirst, create a new project using `cargo new` and add the SpacetimeDB SDK to your dependencies:\r\n\r\n```bash\r\ncargo add spacetimedb\r\n```\r\n\r\n## Generate module bindings\r\n\r\nEach SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's `src` directory and generate the Rust interface files using the Spacetime CLI. From your project directory, run:\r\n\r\n```bash\r\nmkdir -p src/module_bindings\r\nspacetime generate --lang rust \\\r\n    --out-dir src/module_bindings \\\r\n    --project-path PATH-TO-MODULE-DIRECTORY\r\n```\r\n\r\nReplace `PATH-TO-MODULE-DIRECTORY` with the path to your SpacetimeDB module.\r\n\r\nDeclare a `mod` for the bindings in your client's `src/main.rs`:\r\n\r\n```rust\r\nmod module_bindings;\r\n```\r\n\r\n## API at a glance\r\n\r\n| Definition                                                                                             | Description                                                                                                                  |\r\n| ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |\r\n| Function [`module_bindings::connect`](#function-connect)                                               | Autogenerated function to connect to a database.                                                                             |\r\n| Function [`spacetimedb_sdk::disconnect`](#function-disconnect)                                         | Close the active connection.                                                                                                 |\r\n| Function [`spacetimedb_sdk::on_disconnect`](#function-on_disconnect)                                   | Register a `FnMut` callback to run when a connection ends.                                                                   |\r\n| Function [`spacetimedb_sdk::once_on_disconnect`](#function-once_on_disconnect)                         | Register a `FnOnce` callback to run the next time a connection ends.                                                         |\r\n| Function [`spacetimedb_sdk::remove_on_disconnect`](#function-remove_on_disconnect)                     | Cancel an `on_disconnect` or `once_on_disconnect` callback.                                                                  |\r\n| Function [`spacetimedb_sdk::subscribe`](#function-subscribe)                                           | Subscribe to queries with a `&[&str]`.                                                                                       |\r\n| Function [`spacetimedb_sdk::subscribe_owned`](#function-subscribe_owned)                               | Subscribe to queries with a `Vec<String>`.                                                                                   |\r\n| Function [`spacetimedb_sdk::on_subscription_applied`](#function-on_subscription_applied)               | Register a `FnMut` callback to run when a subscription's initial rows become available.                                      |\r\n| Function [`spacetimedb_sdk::once_on_subscription_applied`](#function-once_on_subscription_applied)     | Register a `FnOnce` callback to run the next time a subscription's initial rows become available.                            |\r\n| Function [`spacetimedb_sdk::remove_on_subscription_applied`](#function-remove_on_subscription_applied) | Cancel an `on_subscription_applied` or `once_on_subscription_applied` callback.                                              |\r\n| Type [`spacetimedb_sdk::identity::Identity`](#type-identity)                                           | A unique public identifier for a client.                                                                                     |\r\n| Type [`spacetimedb_sdk::identity::Token`](#type-token)                                                 | A private authentication token corresponding to an `Identity`.                                                               |\r\n| Type [`spacetimedb_sdk::identity::Credentials`](#type-credentials)                                     | An `Identity` paired with its `Token`.                                                                                       |\r\n| Function [`spacetimedb_sdk::identity::identity`](#function-identity)                                   | Return the current connection's `Identity`.                                                                                  |\r\n| Function [`spacetimedb_sdk::identity::token`](#function-token)                                         | Return the current connection's `Token`.                                                                                     |\r\n| Function [`spacetimedb_sdk::identity::credentials`](#function-credentials)                             | Return the current connection's [`Credentials`](#type-credentials).                                                          |\r\n| Function [`spacetimedb_sdk::identity::on_connect`](#function-on-connect)                               | Register a `FnMut` callback to run when the connection's [`Credentials`](#type-credentials) are verified with the database.  |\r\n| Function [`spacetimedb_sdk::identity::once_on_connect`](#function-once_on_connect)                     | Register a `FnOnce` callback to run when the connection's [`Credentials`](#type-credentials) are verified with the database. |\r\n| Function [`spacetimedb_sdk::identity::remove_on_connect`](#function-remove_on_connect)                 | Cancel an `on_connect` or `once_on_connect` callback.                                                                        |\r\n| Function [`spacetimedb_sdk::identity::load_credentials`](#function-load_credentials)                   | Load a saved [`Credentials`](#type-credentials) from a file.                                                                 |\r\n| Function [`spacetimedb_sdk::identity::save_credentials`](#function-save_credentials)                   | Save a [`Credentials`](#type-credentials) to a file.                                                                         |\r\n| Type [`module_bindings::{TABLE}`](#type-table)                                                         | Autogenerated `struct` type for a table, holding one row.                                                                    |\r\n| Method [`module_bindings::{TABLE}::filter_by_{COLUMN}`](#method-filter_by_column)                      | Autogenerated method to iterate over or seek subscribed rows where a column matches a value.                                 |\r\n| Trait [`spacetimedb_sdk::table::TableType`](#trait-tabletype)                                          | Automatically implemented for all tables defined by a module.                                                                |\r\n| Method [`spacetimedb_sdk::table::TableType::count`](#method-count)                                     | Count the number of subscribed rows in a table.                                                                              |\r\n| Method [`spacetimedb_sdk::table::TableType::iter`](#method-iter)                                       | Iterate over all subscribed rows.                                                                                            |\r\n| Method [`spacetimedb_sdk::table::TableType::filter`](#method-filter)                                   | Iterate over a subset of subscribed rows matching a predicate.                                                               |\r\n| Method [`spacetimedb_sdk::table::TableType::find`](#method-find)                                       | Return one subscribed row matching a predicate.                                                                              |\r\n| Method [`spacetimedb_sdk::table::TableType::on_insert`](#method-on_insert)                             | Register a `FnMut` callback to run whenever a new subscribed row is inserted.                                                |\r\n| Method [`spacetimedb_sdk::table::TableType::remove_on_insert`](#method-remove_on_insert)               | Cancel an `on_insert` callback.                                                                                              |\r\n| Method [`spacetimedb_sdk::table::TableType::on_delete`](#method-on_delete)                             | Register a `FnMut` callback to run whenever a subscribed row is deleted.                                                     |\r\n| Method [`spacetimedb_sdk::table::TableType::remove_on_delete`](#method-remove_on_delete)               | Cancel an `on_delete` callback.                                                                                              |\r\n| Trait [`spacetimedb_sdk::table::TableWithPrimaryKey`](#trait-tablewithprimarykey)                      | Automatically implemented for tables with a column designated `#[primarykey]`.                                               |\r\n| Method [`spacetimedb_sdk::table::TableWithPrimaryKey::on_update`](#method-on_update)                   | Register a `FnMut` callback to run whenever an existing subscribed row is updated.                                           |\r\n| Method [`spacetimedb_sdk::table::TableWithPrimaryKey::remove_on_update`](#method-remove_on_update)     | Cancel an `on_update` callback.                                                                                              |\r\n| Type [`module_bindings::ReducerEvent`](#type-reducerevent)                                             | Autogenerated enum with a variant for each reducer defined by the module.                                                    |\r\n| Type [`module_bindings::{REDUCER}Args`](#type-reducerargs)                                             | Autogenerated `struct` type for a reducer, holding its arguments.                                                            |\r\n| Function [`module_bindings::{REDUCER}`](#function-reducer)                                             | Autogenerated function to invoke a reducer.                                                                                  |\r\n| Function [`module_bindings::on_{REDUCER}`](#function-on_reducer)                                       | Autogenerated function to register a `FnMut` callback to run whenever the reducer is invoked.                                |\r\n| Function [`module_bindings::once_on_{REDUCER}`](#function-once_on_reducer)                             | Autogenerated function to register a `FnOnce` callback to run the next time the reducer is invoked.                          |\r\n| Function [`module_bindings::remove_on_{REDUCER}`](#function-remove_on_reducer)                         | Autogenerated function to cancel an `on_{REDUCER}` or `once_on_{REDUCER}` callback.                                          |\r\n| Type [`spacetimedb_sdk::reducer::Status`](#type-status)                                                | Enum representing reducer completion statuses.                                                                               |\r\n\r\n## Connect to a database\r\n\r\n### Function `connect`\r\n\r\n```rust\r\nmodule_bindings::connect(\r\n    spacetimedb_uri: impl TryInto<Uri>,\r\n    db_name: &str,\r\n    credentials: Option<Credentials>,\r\n) -> anyhow::Result<()>\r\n```\r\n\r\nConnect to a database named `db_name` accessible over the internet at the URI `spacetimedb_uri`.\r\n\r\n| Argument          | Type                  | Meaning                                                      |\r\n| ----------------- | --------------------- | ------------------------------------------------------------ |\r\n| `spacetimedb_uri` | `impl TryInto<Uri>`   | URI of the SpacetimeDB instance running the module.          |\r\n| `db_name`         | `&str`                | Name of the module.                                          |\r\n| `credentials`     | `Option<Credentials>` | [`Credentials`](#type-credentials) to authenticate the user. |\r\n\r\nIf `credentials` are supplied, they will be passed to the new connection to identify and authenticate the user. Otherwise, a set of [`Credentials`](#type-credentials) will be generated by the server.\r\n\r\n```rust\r\nconst MODULE_NAME: &str = \"my-module-name\";\r\n\r\n// Connect to a local DB with a fresh identity\r\nconnect(\"http://localhost:3000\", MODULE_NAME, None)\r\n    .expect(\"Connection failed\");\r\n\r\n// Connect to cloud with a fresh identity.\r\nconnect(\"https://testnet.spacetimedb.com\", MODULE_NAME, None)\r\n    .expect(\"Connection failed\");\r\n\r\n// Connect with a saved identity\r\nconst CREDENTIALS_DIR: &str = \".my-module\";\r\nconnect(\r\n    \"https://testnet.spacetimedb.com\",\r\n    MODULE_NAME,\r\n    load_credentials(CREDENTIALS_DIR)\r\n        .expect(\"Error while loading credentials\"),\r\n).expect(\"Connection failed\");\r\n```\r\n\r\n### Function `disconnect`\r\n\r\n```rust\r\nspacetimedb_sdk::disconnect()\r\n```\r\n\r\nGracefully close the current WebSocket connection.\r\n\r\nIf there is no active connection, this operation does nothing.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, MODULE_NAME, credentials)\r\n    .expect(\"Connection failed\");\r\n\r\nrun_app();\r\n\r\ndisconnect();\r\n```\r\n\r\n### Function `on_disconnect`\r\n\r\n```rust\r\nspacetimedb_sdk::on_disconnect(\r\n    callback: impl FnMut() + Send + 'static,\r\n) -> DisconnectCallbackId\r\n```\r\n\r\nRegister a callback to be invoked when a connection ends.\r\n\r\n| Argument   | Type                            | Meaning                                                |\r\n| ---------- | ------------------------------- | ------------------------------------------------------ |\r\n| `callback` | `impl FnMut() + Send + 'static` | Callback to be invoked when subscriptions are applied. |\r\n\r\nThe callback will be invoked after calling [`disconnect`](#function-disconnect), or when a connection is closed by the server.\r\n\r\nThe returned `DisconnectCallbackId` can be passed to [`remove_on_disconnect`](#function-remove_on_disconnect) to unregister the callback.\r\n\r\n```rust\r\non_disconnect(|| println!(\"Disconnected!\"));\r\n\r\nconnect(SPACETIMEDB_URI, MODULE_NAME, credentials)\r\n    .expect(\"Connection failed\");\r\n\r\ndisconnect();\r\n\r\n// Will print \"Disconnected!\"\r\n```\r\n\r\n### Function `once_on_disconnect`\r\n\r\n```rust\r\nspacetimedb_sdk::once_on_disconnect(\r\n    callback: impl FnOnce() + Send + 'static,\r\n) -> DisconnectCallbackId\r\n```\r\n\r\nRegister a callback to be invoked the next time a connection ends.\r\n\r\n| Argument   | Type                            | Meaning                                                |\r\n| ---------- | ------------------------------- | ------------------------------------------------------ |\r\n| `callback` | `impl FnMut() + Send + 'static` | Callback to be invoked when subscriptions are applied. |\r\n\r\nThe callback will be invoked after calling [`disconnect`](#function-disconnect), or when a connection is closed by the server.\r\n\r\nThe callback will be unregistered after running.\r\n\r\nThe returned `DisconnectCallbackId` can be passed to [`remove_on_disconnect`](#function-remove_on_disconnect) to unregister the callback.\r\n\r\n```rust\r\nonce_on_disconnect(|| println!(\"Disconnected!\"));\r\n\r\nconnect(SPACETIMEDB_URI, MODULE_NAME, credentials)\r\n    .expect(\"Connection failed\");\r\n\r\ndisconnect();\r\n\r\n// Will print \"Disconnected!\"\r\n\r\nconnect(SPACETIMEDB_URI, MODULE_NAME, credentials)\r\n    .expect(\"Connection failed\");\r\n\r\ndisconnect();\r\n\r\n// Nothing printed this time.\r\n```\r\n\r\n### Function `remove_on_disconnect`\r\n\r\n```rust\r\nspacetimedb_sdk::remove_on_disconnect(\r\n    id: DisconnectCallbackId,\r\n)\r\n```\r\n\r\nUnregister a previously-registered [`on_disconnect`](#function-on_disconnect) callback.\r\n\r\n| Argument | Type                   | Meaning                                    |\r\n| -------- | ---------------------- | ------------------------------------------ |\r\n| `id`     | `DisconnectCallbackId` | Identifier for the callback to be removed. |\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n```rust\r\nlet id = on_disconnect(|| unreachable!());\r\n\r\nremove_on_disconnect(id);\r\n\r\ndisconnect();\r\n\r\n// No `unreachable` panic.\r\n```\r\n\r\n## Subscribe to queries\r\n\r\n### Function `subscribe`\r\n\r\n```rust\r\nspacetimedb_sdk::subscribe(queries: &[&str]) -> anyhow::Result<()>\r\n```\r\n\r\nSubscribe to a set of queries, to be notified when rows which match those queries are altered.\r\n\r\n| Argument  | Type      | Meaning                      |\r\n| --------- | --------- | ---------------------------- |\r\n| `queries` | `&[&str]` | SQL queries to subscribe to. |\r\n\r\nThe `queries` should be a slice of strings representing SQL queries.\r\n\r\n`subscribe` will return an error if called before establishing a connection with the autogenerated [`connect`](#function-connect) function. In that case, the queries are not registered.\r\n\r\n`subscribe` does not return data directly. The SDK will generate types [`module_bindings::{TABLE}`](#type-table) corresponding to each of the tables in your module. These types implement the trait [`spacetimedb_sdk::table_type::TableType`](#trait-tabletype), which contains methods such as [`TableType::on_insert`](#method-on_insert). Use these methods to receive data from the queries you subscribe to.\r\n\r\nA new call to `subscribe` (or [`subscribe_owned`](#function-subscribe_owned)) will remove all previous subscriptions and replace them with the new `queries`. If any rows matched the previous subscribed queries but do not match the new queries, those rows will be removed from the client cache, and [`TableType::on_delete`](#method-on_delete) callbacks will be invoked for them.\r\n\r\n```rust\r\nsubscribe(&[\"SELECT * FROM User;\", \"SELECT * FROM Message;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n```\r\n\r\n### Function `subscribe_owned`\r\n\r\n```rust\r\nspacetimedb_sdk::subscribe_owned(queries: Vec<String>) -> anyhow::Result<()>\r\n```\r\n\r\nSubscribe to a set of queries, to be notified when rows which match those queries are altered.\r\n\r\n| Argument  | Type          | Meaning                      |\r\n| --------- | ------------- | ---------------------------- |\r\n| `queries` | `Vec<String>` | SQL queries to subscribe to. |\r\n\r\nThe `queries` should be a `Vec` of `String`s representing SQL queries.\r\n\r\nA new call to `subscribe_owned` (or [`subscribe`](#function-subscribe)) will remove all previous subscriptions and replace them with the new `queries`.\r\nIf any rows matched the previous subscribed queries but do not match the new queries, those rows will be removed from the client cache, and [`TableType::on_delete`](#method-on_delete) callbacks will be invoked for them.\r\n\r\n`subscribe_owned` will return an error if called before establishing a connection with the autogenerated [`connect`](#function-connect) function. In that case, the queries are not registered.\r\n\r\n```rust\r\nlet query = format!(\"SELECT * FROM User WHERE name = '{}';\", compute_my_name());\r\n\r\nsubscribe_owned(vec![query])\r\n    .expect(\"Called `subscribe_owned` before `connect`\");\r\n```\r\n\r\n### Function `on_subscription_applied`\r\n\r\n```rust\r\nspacetimedb_sdk::on_subscription_applied(\r\n    callback: impl FnMut() + Send + 'static,\r\n) -> SubscriptionCallbackId\r\n```\r\n\r\nRegister a callback to be invoked the first time a subscription's matching rows becoming available.\r\n\r\n| Argument   | Type                            | Meaning                                                |\r\n| ---------- | ------------------------------- | ------------------------------------------------------ |\r\n| `callback` | `impl FnMut() + Send + 'static` | Callback to be invoked when subscriptions are applied. |\r\n\r\nThe callback will be invoked after a successful [`subscribe`](#function-subscribe) or [`subscribe_owned`](#function-subscribe_owned) call when the initial set of matching rows becomes available.\r\n\r\nThe returned `SubscriptionCallbackId` can be passed to [`remove_on_subscription_applied`](#function-remove_on_subscription_applied) to unregister the callback.\r\n\r\n```rust\r\non_subscription_applied(|| println!(\"Subscription applied!\"));\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print \"Subscription applied!\"\r\n\r\nsubscribe(&[\"SELECT * FROM User;\", \"SELECT * FROM Message;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n\r\n// Will print again.\r\n```\r\n\r\n### Function `once_on_subscription_applied`\r\n\r\n```rust\r\nspacetimedb_sdk::once_on_subscription_applied(\r\n    callback: impl FnOnce() + Send + 'static,\r\n) -> SubscriptionCallbackId\r\n```\r\n\r\nRegister a callback to be invoked the next time a subscription's matching rows become available.\r\n\r\n| Argument   | Type                            | Meaning                                                |\r\n| ---------- | ------------------------------- | ------------------------------------------------------ |\r\n| `callback` | `impl FnMut() + Send + 'static` | Callback to be invoked when subscriptions are applied. |\r\n\r\nThe callback will be invoked after a successful [`subscribe`](#function-subscribe) or [`subscribe_owned`](#function-subscribe_owned) call when the initial set of matching rows becomes available.\r\n\r\nThe callback will be unregistered after running.\r\n\r\nThe returned `SubscriptionCallbackId` can be passed to [`remove_on_subscription_applied`](#function-remove_on_subscription_applied) to unregister the callback.\r\n\r\n```rust\r\nonce_on_subscription_applied(|| println!(\"Subscription applied!\"));\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print \"Subscription applied!\"\r\n\r\nsubscribe(&[\"SELECT * FROM User;\", \"SELECT * FROM Message;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n\r\n// Nothing printed this time.\r\n```\r\n\r\n### Function `remove_on_subscription_applied`\r\n\r\n```rust\r\nspacetimedb_sdk::remove_on_subscription_applied(\r\n    id: SubscriptionCallbackId,\r\n)\r\n```\r\n\r\nUnregister a previously-registered [`on_subscription_applied`](#function-on_subscription_applied) callback.\r\n\r\n| Argument | Type                     | Meaning                                    |\r\n| -------- | ------------------------ | ------------------------------------------ |\r\n| `id`     | `SubscriptionCallbackId` | Identifier for the callback to be removed. |\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n```rust\r\nlet id = on_subscription_applied(|| println!(\"Subscription applied!\"));\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print \"Subscription applied!\"\r\n\r\nremove_on_subscription_applied(id);\r\n\r\nsubscribe(&[\"SELECT * FROM User;\", \"SELECT * FROM Message;\"])\r\n    .expect(\"Called `subscribe` before `connect`\");\r\n\r\n// Nothing printed this time.\r\n```\r\n\r\n## Identify a client\r\n\r\n### Type `Identity`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::Identity\r\n```\r\n\r\nA unique public identifier for a client connected to a database.\r\n\r\n### Type `Token`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::Token\r\n```\r\n\r\nA private access token for a client connected to a database.\r\n\r\n### Type `Credentials`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::Credentials\r\n```\r\n\r\nCredentials, including a private access token, sufficient to authenticate a client connected to a database.\r\n\r\n| Field      | Type                         |\r\n| ---------- | ---------------------------- |\r\n| `identity` | [`Identity`](#type-identity) |\r\n| `token`    | [`Token`](#type-token)       |\r\n\r\n### Function `identity`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::identity() -> Result<Identity>\r\n```\r\n\r\nRead the current connection's public [`Identity`](#type-identity).\r\n\r\nReturns an error if:\r\n\r\n- [`connect`](#function-connect) has not yet been called.\r\n- We connected anonymously, and we have not yet received our credentials.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\nprintln!(\"My identity is {:?}\", identity());\r\n\r\n// Prints \"My identity is Ok(Identity { bytes: [...several u8s...] })\"\r\n```\r\n\r\n### Function `token`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::token() -> Result<Token>\r\n```\r\n\r\nRead the current connection's private [`Token`](#type-token).\r\n\r\nReturns an error if:\r\n\r\n- [`connect`](#function-connect) has not yet been called.\r\n- We connected anonymously, and we have not yet received our credentials.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\nprintln!(\"My token is {:?}\", token());\r\n\r\n// Prints \"My token is Ok(Token {string: \"...several Base64 digits...\" })\"\r\n```\r\n\r\n### Function `credentials`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::credentials() -> Result<Credentials>\r\n```\r\n\r\nRead the current connection's [`Credentials`](#type-credentials), including a public [`Identity`](#type-identity) and a private [`Token`](#type-token).\r\n\r\nReturns an error if:\r\n\r\n- [`connect`](#function-connect) has not yet been called.\r\n- We connected anonymously, and we have not yet received our credentials.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\nprintln!(\"My credentials are {:?}\", credentials());\r\n\r\n// Prints \"My credentials are Ok(Credentials {\r\n//        identity: Identity { bytes: [...several u8s...] },\r\n//        token: Token { string: \"...several Base64 digits...\"},\r\n// })\"\r\n```\r\n\r\n### Function `on_connect`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::on_connect(\r\n    callback: impl FnMut(&Credentials) + Send + 'static,\r\n) -> ConnectCallbackId\r\n```\r\n\r\nRegister a callback to be invoked upon authentication with the database.\r\n\r\n| Argument   | Type                                      | Meaning                                                |\r\n| ---------- | ----------------------------------------- | ------------------------------------------------------ |\r\n| `callback` | `impl FnMut(&Credentials) + Send + 'sync` | Callback to be invoked upon successful authentication. |\r\n\r\nThe callback will be invoked with the [`Credentials`](#type-credentials) provided by the database to identify this connection. If [`Credentials`](#type-credentials) were supplied to [`connect`](#function-connect), those passed to the callback will be equivalent to the ones used to connect. If the initial connection was anonymous, a new set of [`Credentials`](#type-credentials) will be generated by the database to identify this user.\r\n\r\nThe [`Credentials`](#type-credentials) passed to the callback can be saved and used to authenticate the same user in future connections.\r\n\r\nThe returned `ConnectCallbackId` can be passed to [`remove_on_connect`](#function-remove_on_connect) to unregister the callback.\r\n\r\n```rust\r\non_connect(\r\n    |creds| println!(\"Successfully connected! My credentials are: {:?}\", creds)\r\n);\r\n\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print \"Successfully connected! My credentials are: \"\r\n// followed by a printed representation of the client's `Credentials`.\r\n```\r\n\r\n### Function `once_on_connect`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::once_on_connect(\r\n    callback: impl FnOnce(&Credentials) + Send + 'static,\r\n) -> ConnectCallbackId\r\n```\r\n\r\nRegister a callback to be invoked once upon authentication with the database.\r\n\r\n| Argument   | Type                                       | Meaning                                                          |\r\n| ---------- | ------------------------------------------ | ---------------------------------------------------------------- |\r\n| `callback` | `impl FnOnce(&Credentials) + Send + 'sync` | Callback to be invoked once upon next successful authentication. |\r\n\r\nThe callback will be invoked with the [`Credentials`](#type-credentials) provided by the database to identify this connection. If [`Credentials`](#type-credentials) were supplied to [`connect`](#function-connect), those passed to the callback will be equivalent to the ones used to connect. If the initial connection was anonymous, a new set of [`Credentials`](#type-credentials) will be generated by the database to identify this user.\r\n\r\nThe [`Credentials`](#type-credentials) passed to the callback can be saved and used to authenticate the same user in future connections.\r\n\r\nThe callback will be unregistered after running.\r\n\r\nThe returned `ConnectCallbackId` can be passed to [`remove_on_connect`](#function-remove_on_connect) to unregister the callback.\r\n\r\n### Function `remove_on_connect`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::remove_on_connect(id: ConnectCallbackId)\r\n```\r\n\r\nUnregister a previously-registered [`on_connect`](#function-on_connect) or [`once_on_connect`](#function-once_on_connect) callback.\r\n\r\n| Argument | Type                | Meaning                                    |\r\n| -------- | ------------------- | ------------------------------------------ |\r\n| `id`     | `ConnectCallbackId` | Identifier for the callback to be removed. |\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n```rust\r\nlet id = on_connect(|_creds| unreachable!());\r\n\r\nremove_on_connect(id);\r\n\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// No `unreachable` panic.\r\n```\r\n\r\n### Function `load_credentials`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::load_credentials(\r\n    dirname: &str,\r\n) -> Result<Option<Credentials>>\r\n```\r\n\r\nLoad a saved [`Credentials`](#type-credentials) from a file within `~/dirname`, if one exists.\r\n\r\n| Argument  | Type   | Meaning                                               |\r\n| --------- | ------ | ----------------------------------------------------- |\r\n| `dirname` | `&str` | Name of a sub-directory in the user's home directory. |\r\n\r\n`dirname` is treated as a directory in the user's home directory. If it contains a file named `credentials`, that file is treated as a BSATN-encoded [`Credentials`](#type-credentials), deserialized and returned. These files are created by [`save_credentials`](#function-save_credentials) with the same `dirname` argument.\r\n\r\nReturns `Ok(None)` if the directory or the credentials file does not exist. Returns `Err` when IO or deserialization fails. The returned `Result` may be unwrapped, and the contained `Option` passed to [`connect`](#function-connect).\r\n\r\n```rust\r\nconst CREDENTIALS_DIR = \".my-module\";\r\n\r\nlet creds = load_credentials(CREDENTIALS_DIR)\r\n    .expect(\"Error while loading credentials\");\r\n\r\nconnect(SPACETIMEDB_URI, DB_NAME, creds)\r\n    .expect(\"Failed to connect\");\r\n```\r\n\r\n### Function `save_credentials`\r\n\r\n```rust\r\nspacetimedb_sdk::identity::save_credentials(\r\n    dirname: &str,\r\n    credentials: &Credentials,\r\n) -> Result<()>\r\n```\r\n\r\nStore a [`Credentials`](#type-credentials) to a file within `~/dirname`, to be later loaded with [`load_credentials`](#function-load_credentials).\r\n\r\n| Argument      | Type           | Meaning                                               |\r\n| ------------- | -------------- | ----------------------------------------------------- |\r\n| `dirname`     | `&str`         | Name of a sub-directory in the user's home directory. |\r\n| `credentials` | `&Credentials` | [`Credentials`](#type-credentials) to store.          |\r\n\r\n`dirname` is treated as a directory in the user's home directory. The directory is created if it does not already exists. A file within it named `credentials` is created or replaced, containing `creds` encoded as BSATN. The saved credentials can be retrieved by [`load_credentials`](#function-load_credentials) with the same `dirname` argument.\r\n\r\nReturns `Err` when IO or serialization fails.\r\n\r\n```rust\r\nconst CREDENTIALS_DIR = \".my-module\";\r\n\r\nlet creds = load_credentials(CREDENTIALS_DIRectory)\r\n    .expect(\"Error while loading credentials\");\r\n\r\non_connect(|creds| {\r\n    if let Err(e) = save_credentials(CREDENTIALS_DIR, creds) {\r\n        eprintln!(\"Error while saving credentials: {:?}\", e);\r\n    }\r\n});\r\n\r\nconnect(SPACETIMEDB_URI, DB_NAME, creds)\r\n    .expect(\"Failed to connect\");\r\n```\r\n\r\n## View subscribed rows of tables\r\n\r\n### Type `{TABLE}`\r\n\r\n```rust\r\nmodule_bindings::{TABLE}\r\n```\r\n\r\nFor each table defined by a module, `spacetime generate` generates a struct in the `module_bindings` mod whose name is that table's name converted to `PascalCase`. The generated struct has a field for each of the table's columns, whose names are the column names converted to `snake_case`.\r\n\r\n### Method `filter_by_{COLUMN}`\r\n\r\n```rust\r\nmodule_bindings::{TABLE}::filter_by_{COLUMN}(\r\n    value: {COLUMN_TYPE},\r\n) -> {FILTER_RESULT}<{TABLE}>\r\n```\r\n\r\nFor each column of a table, `spacetime generate` generates a static method on the [table struct](#type-table) to filter or seek subscribed rows where that column matches a requested value. These methods are named `filter_by_{COLUMN}`, where `{COLUMN}` is the column name converted to `snake_case`.\r\n\r\nThe method's return type depends on the column's attributes:\r\n\r\n- For unique columns, including those annotated `#[unique]` and `#[primarykey]`, the `filter_by` method returns an `Option<{TABLE}>`, where `{TABLE}` is the [table struct](#type-table).\r\n- For non-unique columns, the `filter_by` method returns an `impl Iterator<Item = {TABLE}>`.\r\n\r\n### Trait `TableType`\r\n\r\n```rust\r\nspacetimedb_sdk::table::TableType\r\n```\r\n\r\nEvery [generated table struct](#type-table) implements the trait `TableType`.\r\n\r\n#### Method `count`\r\n\r\n```rust\r\nTableType::count() -> usize\r\n```\r\n\r\nReturn the number of subscribed rows in the table, or 0 if there is no active connection.\r\n\r\nThis method acquires a global lock.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\non_subscription_applied(|| println!(\"There are {} users\", User::count()));\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will the number of `User` rows in the database.\r\n```\r\n\r\n#### Method `iter`\r\n\r\n```rust\r\nTableType::iter() -> impl Iterator<Item = Self>\r\n```\r\n\r\nIterate over all the subscribed rows in the table.\r\n\r\nThis method acquires a global lock, but the iterator does not hold it.\r\n\r\nThis method must heap-allocate enough memory to hold all of the rows being iterated over. [`TableType::filter`](#method-filter) allocates significantly less, so prefer it when possible.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\non_subscription_applied(|| for user in User::iter() {\r\n    println!(\"{:?}\", user);\r\n});\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print a line for each `User` row in the database.\r\n```\r\n\r\n#### Method `filter`\r\n\r\n```rust\r\nTableType::filter(\r\n    predicate: impl FnMut(&Self) -> bool,\r\n) -> impl Iterator<Item = Self>\r\n```\r\n\r\nIterate over the subscribed rows in the table for which `predicate` returns `true`.\r\n\r\n| Argument    | Type                        | Meaning                                                                         |\r\n| ----------- | --------------------------- | ------------------------------------------------------------------------------- |\r\n| `predicate` | `impl FnMut(&Self) -> bool` | Test which returns `true` if a row should be included in the filtered iterator. |\r\n\r\nThis method acquires a global lock, and the `predicate` runs while the lock is held. The returned iterator does not hold the lock.\r\n\r\nThe `predicate` is called eagerly for each subscribed row in the table, even if the returned iterator is never consumed.\r\n\r\nThis method must heap-allocate enough memory to hold all of the matching rows, but does not allocate space for subscribed rows which do not match the `predicate`.\r\n\r\nClient authors should prefer calling [tables' generated `filter_by_{COLUMN}` methods](#method-filter_by_column) when possible rather than calling `TableType::filter`.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\non_subscription_applied(|| {\r\n    for user in User::filter(|user| user.age >= 30\r\n                                    && user.country == Country::USA) {\r\n        println!(\"{:?}\", user);\r\n    }\r\n});\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print a line for each `User` row in the database\r\n// who is at least 30 years old and who lives in the United States.\r\n```\r\n\r\n#### Method `find`\r\n\r\n```rust\r\nTableType::find(\r\n    predicate: impl FnMut(&Self) -> bool,\r\n) -> Option<Self>\r\n```\r\n\r\nLocate a subscribed row for which `predicate` returns `true`, if one exists.\r\n\r\n| Argument    | Type                        | Meaning                                                |\r\n| ----------- | --------------------------- | ------------------------------------------------------ |\r\n| `predicate` | `impl FnMut(&Self) -> bool` | Test which returns `true` if a row should be returned. |\r\n\r\nThis method acquires a global lock.\r\n\r\nIf multiple subscribed rows match `predicate`, one is chosen arbitrarily. The choice may not be stable across different calls to `find` with the same `predicate`.\r\n\r\nClient authors should prefer calling [tables' generated `filter_by_{COLUMN}` methods](#method-filter_by_column) when possible rather than calling `TableType::find`.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\non_subscription_applied(|| {\r\n    if let Some(tyler) = User::find(|user| user.first_name == \"Tyler\"\r\n                                           && user.surname == \"Cloutier\") {\r\n        println!(\"Found Tyler: {:?}\", tyler);\r\n    } else {\r\n        println!(\"Tyler isn't registered :(\");\r\n    }\r\n});\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will tell us whether Tyler Cloutier is registered in the database.\r\n```\r\n\r\n#### Method `on_insert`\r\n\r\n```rust\r\nTableType::on_insert(\r\n    callback: impl FnMut(&Self, Option<&ReducerEvent>) + Send + 'static,\r\n) -> InsertCallbackId<Self>\r\n```\r\n\r\nRegister an `on_insert` callback for when a subscribed row is newly inserted into the database.\r\n\r\n| Argument   | Type                                                        | Meaning                                                |\r\n| ---------- | ----------------------------------------------------------- | ------------------------------------------------------ |\r\n| `callback` | `impl FnMut(&Self, Option<&ReducerEvent>) + Send + 'static` | Callback to run whenever a subscribed row is inserted. |\r\n\r\nThe callback takes two arguments:\r\n\r\n- `row: &Self`, the newly-inserted row value.\r\n- `reducer_event: Option<&ReducerEvent>`, the [`ReducerEvent`](#type-reducerevent) which caused this row to be inserted, or `None` if this row is being inserted while initializing a subscription.\r\n\r\nThe returned `InsertCallbackId` can be passed to [`remove_on_insert`](#method-remove_on_insert) to remove the callback.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nUser::on_insert(|user, reducer_event| {\r\n    if let Some(reducer_event) = reducer_event {\r\n        println!(\"New user inserted by reducer {:?}: {:?}\", reducer_event, user);\r\n    } else {\r\n        println!(\"New user received during subscription update: {:?}\", user);\r\n    }\r\n});\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print a note whenever a new `User` row is inserted.\r\n```\r\n\r\n#### Method `remove_on_insert`\r\n\r\n```rust\r\nTableType::remove_on_insert(id: InsertCallbackId<Self>)\r\n```\r\n\r\nUnregister a previously-registered [`on_insert`](#method-on_insert) callback.\r\n\r\n| Argument | Type                     | Meaning                                                                 |\r\n| -------- | ------------------------ | ----------------------------------------------------------------------- |\r\n| `id`     | `InsertCallbackId<Self>` | Identifier for the [`on_insert`](#method-on_insert) callback to remove. |\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nlet id = User::on_insert(|_, _| unreachable!());\r\n\r\nUser::remove_on_insert(id);\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// No `unreachable` panic.\r\n```\r\n\r\n#### Method `on_delete`\r\n\r\n```rust\r\nTableType::on_delete(\r\n    callback: impl FnMut(&Self, Option<&ReducerEvent>) + Send + 'static,\r\n) -> DeleteCallbackId<Self>\r\n```\r\n\r\nRegister an `on_delete` callback for when a subscribed row is removed from the database.\r\n\r\n| Argument   | Type                                                        | Meaning                                               |\r\n| ---------- | ----------------------------------------------------------- | ----------------------------------------------------- |\r\n| `callback` | `impl FnMut(&Self, Option<&ReducerEvent>) + Send + 'static` | Callback to run whenever a subscribed row is deleted. |\r\n\r\nThe callback takes two arguments:\r\n\r\n- `row: &Self`, the previously-present row which is no longer resident in the database.\r\n- `reducer_event: Option<&ReducerEvent>`, the [`ReducerEvent`](#type-reducerevent) which caused this row to be deleted, or `None` if this row was previously subscribed but no longer matches the new queries while initializing a subscription.\r\n\r\nThe returned `DeleteCallbackId` can be passed to [`remove_on_delete`](#method-remove_on_delete) to remove the callback.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nUser::on_delete(|user, reducer_event| {\r\n    if let Some(reducer_event) = reducer_event {\r\n        println!(\"User deleted by reducer {:?}: {:?}\", reducer_event, user);\r\n    } else {\r\n        println!(\"User no longer subscribed during subscription update: {:?}\", user);\r\n    }\r\n});\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\n// Invoke a reducer which will delete a `User` row.\r\ndelete_user_by_name(\"Tyler Cloutier\".to_string());\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// Will print a note whenever a `User` row is inserted,\r\n// including \"User deleted by reducer ReducerEvent::DeleteUserByName(\r\n//     DeleteUserByNameArgs { name: \"Tyler Cloutier\" }\r\n// ): User { first_name: \"Tyler\", surname: \"Cloutier\" }\"\r\n```\r\n\r\n#### Method `remove_on_delete`\r\n\r\n```rust\r\nTableType::remove_on_delete(id: DeleteCallbackId<Self>)\r\n```\r\n\r\nUnregister a previously-registered [`on_delete`](#method-on_delete) callback.\r\n\r\n| Argument | Type                     | Meaning                                                                 |\r\n| -------- | ------------------------ | ----------------------------------------------------------------------- |\r\n| `id`     | `DeleteCallbackId<Self>` | Identifier for the [`on_delete`](#method-on_delete) callback to remove. |\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nlet id = User::on_delete(|_, _| unreachable!());\r\n\r\nUser::remove_on_delete(id);\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\n// Invoke a reducer which will delete a `User` row.\r\ndelete_user_by_name(\"Tyler Cloutier\".to_string());\r\n\r\nsleep(Duration::from_secs(1));\r\n\r\n// No `unreachable` panic.\r\n```\r\n\r\n### Trait `TableWithPrimaryKey`\r\n\r\n```rust\r\nspacetimedb_sdk::table::TableWithPrimaryKey\r\n```\r\n\r\n[Generated table structs](#type-table) with a column designated `#[primarykey]` implement the trait `TableWithPrimaryKey`.\r\n\r\n#### Method `on_update`\r\n\r\n```rust\r\nTableWithPrimaryKey::on_update(\r\n    callback: impl FnMut(&Self, &Self, Option<&Self::ReducerEvent>) + Send + 'static,\r\n) -> UpdateCallbackId<Self>\r\n```\r\n\r\nRegister an `on_update` callback for when an existing row is modified.\r\n\r\n| Argument   | Type                                                               | Meaning                                               |\r\n| ---------- | ------------------------------------------------------------------ | ----------------------------------------------------- |\r\n| `callback` | `impl FnMut(&Self, &Self, Option<&ReducerEvent>) + Send + 'static` | Callback to run whenever a subscribed row is updated. |\r\n\r\nThe callback takes three arguments:\r\n\r\n- `old: &Self`, the previous row value which has been replaced in the database.\r\n- `new: &Self`, the updated row value which is now resident in the database.\r\n- `reducer_event: Option<&ReducerEvent>`, the [`ReducerEvent`](#type-reducerevent) which caused this row to be inserted.\r\n\r\nThe returned `UpdateCallbackId` can be passed to [`remove_on_update`](#method-remove_on_update) to remove the callback.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nUser::on_update(|old, new, reducer_event| {\r\n    println!(\"User updated by reducer {:?}: from {:?} to {:?}\", reducer_event, old, new);\r\n});\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\n// Prints a line whenever a `User` row is updated by primary key.\r\n```\r\n\r\n#### Method `remove_on_update`\r\n\r\n```rust\r\nTableWithPrimaryKey::remove_on_update(id: UpdateCallbackId<Self>)\r\n```\r\n\r\n| Argument | Type                     | Meaning                                                                 |\r\n| -------- | ------------------------ | ----------------------------------------------------------------------- |\r\n| `id`     | `UpdateCallbackId<Self>` | Identifier for the [`on_update`](#method-on_update) callback to remove. |\r\n\r\nUnregister a previously-registered [`on_update`](#method-on_update) callback.\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n```rust\r\nconnect(SPACETIMEDB_URI, DB_NAME, None)\r\n    .expect(\"Failed to connect\");\r\n\r\nlet id = User::on_update(|_, _, _| unreachable!);\r\n\r\nUser::remove_on_update(id);\r\n\r\nsubscribe(&[\"SELECT * FROM User;\"])\r\n    .unwrap();\r\n\r\n// No `unreachable` panic.\r\n```\r\n\r\n## Observe and request reducer invocations\r\n\r\n### Type `ReducerEvent`\r\n\r\n```rust\r\nmodule_bindings::ReducerEvent\r\n```\r\n\r\n`spacetime generate` defines an enum `ReducerEvent` with a variant for each reducer defined by a module. The variant's name will be the reducer's name converted to `PascalCase`, and the variant will hold an instance of [the autogenerated reducer arguments struct for that reducer](#type-reducerargs).\r\n\r\n[`on_insert`](#method-on_insert), [`on_delete`](#method-on_delete) and [`on_update`](#method-on_update) callbacks accept an `Option<&ReducerEvent>` which identifies the reducer which caused the row to be inserted, deleted or updated.\r\n\r\n### Type `{REDUCER}Args`\r\n\r\n```rust\r\nmodule_bindings::{REDUCER}Args\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates a struct whose name is that reducer's name converted to `PascalCase`, suffixed with `Args`. The generated struct has a field for each of the reducer's arguments, whose names are the argument names converted to `snake_case`.\r\n\r\nFor reducers which accept a `ReducerContext` as their first argument, the `ReducerContext` is not included in the arguments struct.\r\n\r\n### Function `{REDUCER}`\r\n\r\n```rust\r\nmodule_bindings::{REDUCER}({ARGS...})\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates a function which sends a request to the database to invoke that reducer. The generated function's name is the reducer's name converted to `snake_case`.\r\n\r\nFor reducers which accept a `ReducerContext` as their first argument, the `ReducerContext` is not included in the generated function's argument list.\r\n\r\n### Function `on_{REDUCER}`\r\n\r\n```rust\r\nmodule_bindings::on_{REDUCER}(\r\n    callback: impl FnMut(&Identity, Status, {&ARGS...}) + Send + 'static,\r\n) -> ReducerCallbackId<{REDUCER}Args>\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates a function which registers a `FnMut` callback to run each time the reducer is invoked. The generated functions are named `on_{REDUCER}`, where `{REDUCER}` is the reducer's name converted to `snake_case`.\r\n\r\n| Argument   | Type                                                          | Meaning                                          |\r\n| ---------- | ------------------------------------------------------------- | ------------------------------------------------ |\r\n| `callback` | `impl FnMut(&Identity, &Status, {&ARGS...}) + Send + 'static` | Callback to run whenever the reducer is invoked. |\r\n\r\nThe callback always accepts two arguments:\r\n\r\n- `caller: &Identity`, the [`Identity`](#type-identity) of the client which invoked the reducer.\r\n- `status: &Status`, the termination [`Status`](#type-status) of the reducer run.\r\n\r\nIn addition, the callback accepts a reference to each of the reducer's arguments.\r\n\r\nClients will only be notified of reducer runs if either of two criteria is met:\r\n\r\n- The reducer inserted, deleted or updated at least one row to which the client is subscribed.\r\n- The reducer invocation was requested by this client, and the run failed.\r\n\r\nThe `on_{REDUCER}` function returns a `ReducerCallbackId<{REDUCER}Args>`, where `{REDUCER}Args` is the [generated reducer arguments struct](#type-reducerargs). This `ReducerCallbackId` can be passed to the [generated `remove_on_{REDUCER}` function](#function-remove_on_reducer) to cancel the callback.\r\n\r\n### Function `once_on_{REDUCER}`\r\n\r\n```rust\r\nmodule_bindings::once_on_{REDUCER}(\r\n    callback: impl FnOnce(&Identity, &Status, {&ARGS...}) + Send + 'static,\r\n) -> ReducerCallbackId<{REDUCER}Args>\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates a function which registers a `FnOnce` callback to run the next time the reducer is invoked. The generated functions are named `once_on_{REDUCER}`, where `{REDUCER}` is the reducer's name converted to `snake_case`.\r\n\r\n| Argument   | Type                                                           | Meaning                                               |\r\n| ---------- | -------------------------------------------------------------- | ----------------------------------------------------- |\r\n| `callback` | `impl FnOnce(&Identity, &Status, {&ARGS...}) + Send + 'static` | Callback to run the next time the reducer is invoked. |\r\n\r\nThe callback accepts the same arguments as an [on-reducer callback](#function-on_reducer), but may be a `FnOnce` rather than a `FnMut`.\r\n\r\nThe callback will be invoked in the same circumstances as an on-reducer callback.\r\n\r\nThe `once_on_{REDUCER}` function returns a `ReducerCallbackId<{REDUCER}Args>`, where `{REDUCER}Args` is the [generated reducer arguments struct](#type-reducerargs). This `ReducerCallbackId` can be passed to the [generated `remove_on_{REDUCER}` function](#function-remove_on_reducer) to cancel the callback.\r\n\r\n### Function `remove_on_{REDUCER}`\r\n\r\n```rust\r\nmodule_bindings::remove_on_{REDUCER}(id: ReducerCallbackId<{REDUCER}Args>)\r\n```\r\n\r\nFor each reducer defined by a module, `spacetime generate` generates a function which unregisters a previously-registered [on-reducer](#function-on_reducer) or [once-on-reducer](#function-once_on_reducer) callback.\r\n\r\n| Argument | Type                     | Meaning                                                                                                                           |\r\n| -------- | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |\r\n| `id`     | `UpdateCallbackId<Self>` | Identifier for the [`on_{REDUCER}`](#function-on_reducer) or [`once_on_{REDUCER}`](#function-once_on_reducer) callback to remove. |\r\n\r\nIf `id` does not refer to a currently-registered callback, this operation does nothing.\r\n\r\n### Type `Status`\r\n\r\n```rust\r\nspacetimedb_sdk::reducer::Status\r\n```\r\n\r\nAn enum whose variants represent possible reducer completion statuses.\r\n\r\nA `Status` is passed as the second argument to [`on_{REDUCER}`](#function-on_reducer) and [`once_on_{REDUCER}`](#function-once_on_reducer) callbacks.\r\n\r\n#### Variant `Status::Committed`\r\n\r\nThe reducer finished successfully, and its row changes were committed to the database.\r\n\r\n#### Variant `Status::Failed(String)`\r\n\r\nThe reducer failed, either by panicking or returning an `Err`.\r\n\r\n| Field | Type     | Meaning                                             |\r\n| ----- | -------- | --------------------------------------------------- |\r\n| 0     | `String` | The error message which caused the reducer to fail. |\r\n\r\n#### Variant `Status::OutOfEnergy`\r\n\r\nThe reducer was canceled because the module owner had insufficient energy to allow it to run to completion.\r\n",
              "editUrl": "SDK%20Reference.md",
              "jumpLinks": [
                {
                  "title": "The SpacetimeDB Rust client SDK",
                  "route": "the-spacetimedb-rust-client-sdk",
                  "depth": 1
                },
                {
                  "title": "Install the SDK",
                  "route": "install-the-sdk",
                  "depth": 2
                },
                {
                  "title": "Generate module bindings",
                  "route": "generate-module-bindings",
                  "depth": 2
                },
                {
                  "title": "API at a glance",
                  "route": "api-at-a-glance",
                  "depth": 2
                },
                {
                  "title": "Connect to a database",
                  "route": "connect-to-a-database",
                  "depth": 2
                },
                {
                  "title": "Function `connect`",
                  "route": "function-connect-",
                  "depth": 3
                },
                {
                  "title": "Function `disconnect`",
                  "route": "function-disconnect-",
                  "depth": 3
                },
                {
                  "title": "Function `on_disconnect`",
                  "route": "function-on_disconnect-",
                  "depth": 3
                },
                {
                  "title": "Function `once_on_disconnect`",
                  "route": "function-once_on_disconnect-",
                  "depth": 3
                },
                {
                  "title": "Function `remove_on_disconnect`",
                  "route": "function-remove_on_disconnect-",
                  "depth": 3
                },
                {
                  "title": "Subscribe to queries",
                  "route": "subscribe-to-queries",
                  "depth": 2
                },
                {
                  "title": "Function `subscribe`",
                  "route": "function-subscribe-",
                  "depth": 3
                },
                {
                  "title": "Function `subscribe_owned`",
                  "route": "function-subscribe_owned-",
                  "depth": 3
                },
                {
                  "title": "Function `on_subscription_applied`",
                  "route": "function-on_subscription_applied-",
                  "depth": 3
                },
                {
                  "title": "Function `once_on_subscription_applied`",
                  "route": "function-once_on_subscription_applied-",
                  "depth": 3
                },
                {
                  "title": "Function `remove_on_subscription_applied`",
                  "route": "function-remove_on_subscription_applied-",
                  "depth": 3
                },
                {
                  "title": "Identify a client",
                  "route": "identify-a-client",
                  "depth": 2
                },
                {
                  "title": "Type `Identity`",
                  "route": "type-identity-",
                  "depth": 3
                },
                {
                  "title": "Type `Token`",
                  "route": "type-token-",
                  "depth": 3
                },
                {
                  "title": "Type `Credentials`",
                  "route": "type-credentials-",
                  "depth": 3
                },
                {
                  "title": "Function `identity`",
                  "route": "function-identity-",
                  "depth": 3
                },
                {
                  "title": "Function `token`",
                  "route": "function-token-",
                  "depth": 3
                },
                {
                  "title": "Function `credentials`",
                  "route": "function-credentials-",
                  "depth": 3
                },
                {
                  "title": "Function `on_connect`",
                  "route": "function-on_connect-",
                  "depth": 3
                },
                {
                  "title": "Function `once_on_connect`",
                  "route": "function-once_on_connect-",
                  "depth": 3
                },
                {
                  "title": "Function `remove_on_connect`",
                  "route": "function-remove_on_connect-",
                  "depth": 3
                },
                {
                  "title": "Function `load_credentials`",
                  "route": "function-load_credentials-",
                  "depth": 3
                },
                {
                  "title": "Function `save_credentials`",
                  "route": "function-save_credentials-",
                  "depth": 3
                },
                {
                  "title": "View subscribed rows of tables",
                  "route": "view-subscribed-rows-of-tables",
                  "depth": 2
                },
                {
                  "title": "Type `{TABLE}`",
                  "route": "type-table-",
                  "depth": 3
                },
                {
                  "title": "Method `filter_by_{COLUMN}`",
                  "route": "method-filter_by_-column-",
                  "depth": 3
                },
                {
                  "title": "Trait `TableType`",
                  "route": "trait-tabletype-",
                  "depth": 3
                },
                {
                  "title": "Method `count`",
                  "route": "method-count-",
                  "depth": 4
                },
                {
                  "title": "Method `iter`",
                  "route": "method-iter-",
                  "depth": 4
                },
                {
                  "title": "Method `filter`",
                  "route": "method-filter-",
                  "depth": 4
                },
                {
                  "title": "Method `find`",
                  "route": "method-find-",
                  "depth": 4
                },
                {
                  "title": "Method `on_insert`",
                  "route": "method-on_insert-",
                  "depth": 4
                },
                {
                  "title": "Method `remove_on_insert`",
                  "route": "method-remove_on_insert-",
                  "depth": 4
                },
                {
                  "title": "Method `on_delete`",
                  "route": "method-on_delete-",
                  "depth": 4
                },
                {
                  "title": "Method `remove_on_delete`",
                  "route": "method-remove_on_delete-",
                  "depth": 4
                },
                {
                  "title": "Trait `TableWithPrimaryKey`",
                  "route": "trait-tablewithprimarykey-",
                  "depth": 3
                },
                {
                  "title": "Method `on_update`",
                  "route": "method-on_update-",
                  "depth": 4
                },
                {
                  "title": "Method `remove_on_update`",
                  "route": "method-remove_on_update-",
                  "depth": 4
                },
                {
                  "title": "Observe and request reducer invocations",
                  "route": "observe-and-request-reducer-invocations",
                  "depth": 2
                },
                {
                  "title": "Type `ReducerEvent`",
                  "route": "type-reducerevent-",
                  "depth": 3
                },
                {
                  "title": "Type `{REDUCER}Args`",
                  "route": "type-reducer-args-",
                  "depth": 3
                },
                {
                  "title": "Function `{REDUCER}`",
                  "route": "function-reducer-",
                  "depth": 3
                },
                {
                  "title": "Function `on_{REDUCER}`",
                  "route": "function-on_-reducer-",
                  "depth": 3
                },
                {
                  "title": "Function `once_on_{REDUCER}`",
                  "route": "function-once_on_-reducer-",
                  "depth": 3
                },
                {
                  "title": "Function `remove_on_{REDUCER}`",
                  "route": "function-remove_on_-reducer-",
                  "depth": 3
                },
                {
                  "title": "Type `Status`",
                  "route": "type-status-",
                  "depth": 3
                },
                {
                  "title": "Variant `Status::Committed`",
                  "route": "variant-status-committed-",
                  "depth": 4
                },
                {
                  "title": "Variant `Status::Failed(String)`",
                  "route": "variant-status-failed-string-",
                  "depth": 4
                },
                {
                  "title": "Variant `Status::OutOfEnergy`",
                  "route": "variant-status-outofenergy-",
                  "depth": 4
                }
              ],
              "pages": []
            }
          ]
        },
        {
          "title": "Typescript",
          "identifier": "Typescript",
          "indexIdentifier": "index",
          "comingSoon": false,
          "hasPages": true,
          "editUrl": "Typescript/index.md",
          "jumpLinks": [],
          "pages": [
            {
              "title": "Typescript Client SDK Quick Start",
              "identifier": "index",
              "indexIdentifier": "index",
              "content": "# Typescript Client SDK Quick Start\r\n\r\nIn this guide we'll show you how to get up and running with a simple SpacetimDB app with a client written in Typescript.\r\n\r\nWe'll implement a basic single page web app for the module created in our Rust or C# Module Quickstart guides. **Make sure you follow one of these guides before you start on this one.**\r\n\r\n## Project structure\r\n\r\nEnter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/server-languages/rust/rust-module-quickstart-guide) or [C# Module Quickstart](/docs/server-languages/csharp/csharp-module-reference) guides:\r\n\r\n```bash\r\ncd quickstart-chat\r\n```\r\n\r\nWithin it, create a `client` react app:\r\n\r\n```bash\r\nnpx create-react-app client --template typescript\r\n```\r\n\r\nWe also need to install the `spacetime-client-sdk` package:\r\n\r\n```bash\r\ncd client\r\nnpm install @clockworklabs/spacetimedb-sdk\r\n```\r\n\r\n## Basic layout\r\n\r\nWe are going to start by creating a basic layout for our app. The page contains four sections:\r\n\r\n1. A profile section, where we can set our name.\r\n2. A message section, where we can see all the messages.\r\n3. A system section, where we can see system messages.\r\n4. A new message section, where we can send a new message.\r\n\r\nThe `onSubmitNewName` and `onMessageSubmit` callbacks will be called when the user clicks the submit button in the profile and new message sections, respectively. We'll hook these up later.\r\n\r\nReplace the entire contents of `client/src/App.tsx` with the following:\r\n\r\n```typescript\r\nimport React, { useEffect, useState } from \"react\";\r\nimport logo from \"./logo.svg\";\r\nimport \"./App.css\";\r\n\r\nexport type MessageType = {\r\n  name: string;\r\n  message: string;\r\n};\r\n\r\nfunction App() {\r\n  const [newName, setNewName] = useState(\"\");\r\n  const [settingName, setSettingName] = useState(false);\r\n  const [name, setName] = useState(\"\");\r\n  const [systemMessage, setSystemMessage] = useState(\"\");\r\n  const [messages, setMessages] = useState<MessageType[]>([]);\r\n\r\n  const [newMessage, setNewMessage] = useState(\"\");\r\n\r\n  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {\r\n    e.preventDefault();\r\n    setSettingName(false);\r\n    // Fill in app logic here\r\n  };\r\n\r\n  const onMessageSubmit = (e: React.FormEvent<HTMLFormElement>) => {\r\n    e.preventDefault();\r\n    // Fill in app logic here\r\n    setNewMessage(\"\");\r\n  };\r\n\r\n  return (\r\n    <div className=\"App\">\r\n      <div className=\"profile\">\r\n        <h1>Profile</h1>\r\n        {!settingName ? (\r\n          <>\r\n            <p>{name}</p>\r\n            <button\r\n              onClick={() => {\r\n                setSettingName(true);\r\n                setNewName(name);\r\n              }}\r\n            >\r\n              Edit Name\r\n            </button>\r\n          </>\r\n        ) : (\r\n          <form onSubmit={onSubmitNewName}>\r\n            <input\r\n              type=\"text\"\r\n              style={{ marginBottom: \"1rem\" }}\r\n              value={newName}\r\n              onChange={(e) => setNewName(e.target.value)}\r\n            />\r\n            <button type=\"submit\">Submit</button>\r\n          </form>\r\n        )}\r\n      </div>\r\n      <div className=\"message\">\r\n        <h1>Messages</h1>\r\n        {messages.length < 1 && <p>No messages</p>}\r\n        <div>\r\n          {messages.map((message, key) => (\r\n            <div key={key}>\r\n              <p>\r\n                <b>{message.name}</b>\r\n              </p>\r\n              <p>{message.message}</p>\r\n            </div>\r\n          ))}\r\n        </div>\r\n      </div>\r\n      <div className=\"system\" style={{ whiteSpace: \"pre-wrap\" }}>\r\n        <h1>System</h1>\r\n        <div>\r\n          <p>{systemMessage}</p>\r\n        </div>\r\n      </div>\r\n      <div className=\"new-message\">\r\n        <form\r\n          onSubmit={onMessageSubmit}\r\n          style={{\r\n            display: \"flex\",\r\n            flexDirection: \"column\",\r\n            width: \"50%\",\r\n            margin: \"0 auto\",\r\n          }}\r\n        >\r\n          <h3>New Message</h3>\r\n          <textarea\r\n            value={newMessage}\r\n            onChange={(e) => setNewMessage(e.target.value)}\r\n          ></textarea>\r\n          <button type=\"submit\">Send</button>\r\n        </form>\r\n      </div>\r\n    </div>\r\n  );\r\n}\r\n\r\nexport default App;\r\n```\r\n\r\nNow when you run `npm start`, you should see a basic chat app that does not yet send or receive messages.\r\n\r\n## Generate your module types\r\n\r\nThe `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.\r\n\r\nIn your `quickstart-chat` directory, run:\r\n\r\n```bash\r\nmkdir -p client/src/module_bindings\r\nspacetime generate --lang typescript --out-dir client/src/module_bindings --project_path server\r\n```\r\n\r\nTake a look inside `client/src/module_bindings`. The CLI should have generated four files:\r\n\r\n```\r\nmodule_bindings\r\n├── message.ts\r\n├── send_message_reducer.ts\r\n├── set_name_reducer.ts\r\n└── user.ts\r\n```\r\n\r\nWe need to import these types into our `client/src/App.tsx`. While we are at it, we will also import the SpacetimeDBClient class from our SDK.\r\n\r\n> There is a known issue where if you do not use every type in your file, it will not pull them into the published build. To fix this, we are using `console.log` to force them to get pulled in.\r\n\r\n```typescript\r\nimport { SpacetimeDBClient, Identity } from \"@clockworklabs/spacetimedb-sdk\";\r\n\r\nimport Message from \"./module_bindings/message\";\r\nimport User from \"./module_bindings/user\";\r\nimport SendMessageReducer from \"./module_bindings/send_message_reducer\";\r\nimport SetNameReducer from \"./module_bindings/set_name_reducer\";\r\nconsole.log(Message, User, SendMessageReducer, SetNameReducer);\r\n```\r\n\r\n## Create your SpacetimeDB client\r\n\r\nFirst, we need to create a SpacetimeDB client and connect to the module. Create your client at the top of the `App` function.\r\n\r\nWe are going to create a stateful variable to store our client's SpacetimeDB identity when we receive it. Also, we are using `localStorage` to retrieve your auth token if this client has connected before. We will explain these later.\r\n\r\nReplace `<module-name>` with the name you chose when publishing your module during the module quickstart. If you are using SpacetimeDB Cloud, the host will be `wss://spacetimedb.com/spacetimedb`.\r\n\r\nAdd this before the `App` function declaration:\r\n\r\n```typescript\r\nlet token = localStorage.getItem(\"auth_token\") || undefined;\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"chat\",\r\n  token\r\n);\r\n```\r\n\r\nInside the `App` function, add a few refs:\r\n\r\n```typescript\r\nlet local_identity = useRef<Identity | undefined>(undefined);\r\nlet initialized = useRef<boolean>(false);\r\nconst client = useRef<SpacetimeDBClient>(spacetimeDBClient);\r\n```\r\n\r\n## Register callbacks and connect\r\n\r\nWe need to handle several sorts of events:\r\n\r\n1. `onConnect`: When we connect and receive our credentials, we'll save them to browser local storage, so that the next time we connect, we can re-authenticate as the same user.\r\n2. `initialStateSync`: When we're informed of the backlog of past messages, we'll sort them and update the `message` section of the page.\r\n3. `Message.onInsert`: When we receive a new message, we'll update the `message` section of the page.\r\n4. `User.onInsert`: When a new user joins, we'll update the `system` section of the page with an appropiate message.\r\n5. `User.onUpdate`: When a user is updated, we'll add a message with their new name, or declare their new online status to the `system` section of the page.\r\n6. `SetNameReducer.on`: If the server rejects our attempt to set our name, we'll update the `system` section of the page with an appropriate error message.\r\n7. `SendMessageReducer.on`: If the server rejects a message we send, we'll update the `system` section of the page with an appropriate error message.\r\n\r\nWe will add callbacks for each of these items in the following sections. All of these callbacks will be registered inside the `App` function after the `useRef` declarations.\r\n\r\n### onConnect Callback\r\n\r\nOn connect SpacetimeDB will provide us with our client credentials.\r\n\r\nEach client has a credentials which consists of two parts:\r\n\r\n- An `Identity`, a unique public identifier. We're using these to identify `User` rows.\r\n- A `Token`, a private key which SpacetimeDB uses to authenticate the client.\r\n\r\nThese credentials are generated by SpacetimeDB each time a new client connects, and sent to the client so they can be saved, in order to re-connect with the same identity.\r\n\r\nWe want to store our local client identity in a stateful variable and also save our `token` to local storage for future connections.\r\n\r\nOnce we are connected, we can send our subscription to the SpacetimeDB module. SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the \"chunk\" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\nclient.current.onConnect((token, identity) => {\r\n  console.log(\"Connected to SpacetimeDB\");\r\n\r\n  local_identity.current = identity;\r\n\r\n  localStorage.setItem(\"auth_token\", token);\r\n\r\n  client.current.subscribe([\"SELECT * FROM User\", \"SELECT * FROM Message\"]);\r\n});\r\n```\r\n\r\n### initialStateSync callback\r\n\r\nThis callback fires when our local client cache of the database is populated. This is a good time to set the initial messages list.\r\n\r\nWe'll define a helper function, `setAllMessagesInOrder`, to supply the `MessageType` class for our React application. It will call the autogenerated `Message.all` function to get an array of `Message` rows, then sort them and convert them to `MessageType`.\r\n\r\nTo find the `User` based on the message's `sender` identity, we'll use `User::filterByIdentity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `filterByIdentity` accepts a `UInt8Array`, rather than an `Identity`. The `sender` identity stored in the message is also a `UInt8Array`, not an `Identity`, so we can just pass it to the filter method.\r\n\r\nWhenever we want to display a user name, if they have set a name, we'll use that. If they haven't set a name, we'll instead use the first 8 bytes of their identity, encoded as hexadecimal. We'll define the function `userNameOrIdentity` to handle this.\r\n\r\nWe also have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll display `unknown`.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\nfunction userNameOrIdentity(user: User): string {\r\n  console.log(`Name: ${user.name} `);\r\n  if (user.name !== null) {\r\n    return user.name || \"\";\r\n  } else {\r\n    var identityStr = new Identity(user.identity).toHexString();\r\n    console.log(`Name: ${identityStr} `);\r\n    return new Identity(user.identity).toHexString().substring(0, 8);\r\n  }\r\n}\r\n\r\nfunction setAllMessagesInOrder() {\r\n  let messages = Array.from(Message.all());\r\n  messages.sort((a, b) => (a.sent > b.sent ? 1 : a.sent < b.sent ? -1 : 0));\r\n\r\n  let messagesType: MessageType[] = messages.map((message) => {\r\n    let sender_identity = User.filterByIdentity(message.sender);\r\n    let display_name = sender_identity\r\n      ? userNameOrIdentity(sender_identity)\r\n      : \"unknown\";\r\n\r\n    return {\r\n      name: display_name,\r\n      message: message.text,\r\n    };\r\n  });\r\n\r\n  setMessages(messagesType);\r\n}\r\n\r\nclient.current.on(\"initialStateSync\", () => {\r\n  setAllMessagesInOrder();\r\n  var user = User.filterByIdentity(local_identity?.current?.toUint8Array()!);\r\n  setName(userNameOrIdentity(user!));\r\n});\r\n```\r\n\r\n### Message.onInsert callback - Update messages\r\n\r\nWhen we receive a new message, we'll update the messages section of the page. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. When the server is initializing our cache, we'll get a callback for each existing message, but we don't want to update the page for those. To that effect, our `onInsert` callback will check if its `ReducerEvent` argument is not `undefined`, and only update the `message` section in that case.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\nMessage.onInsert((message, reducerEvent) => {\r\n  if (reducerEvent !== undefined) {\r\n    setAllMessagesInOrder();\r\n  }\r\n});\r\n```\r\n\r\n### User.onInsert callback - Notify about new users\r\n\r\nFor each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `onInsert` and `onDelete` methods of the trait `TableType`, which is automatically implemented for each table by `spacetime generate`.\r\n\r\nThese callbacks can fire in two contexts:\r\n\r\n- After a reducer runs, when the client's cache is updated about changes to subscribed rows.\r\n- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.\r\n\r\nThis second case means that, even though the module only ever inserts online users, the client's `User.onInsert` callbacks may be invoked with users who are offline. We'll only notify about online users.\r\n\r\n`onInsert` and `onDelete` callbacks take two arguments: the altered row, and a `ReducerEvent | undefined`. This will be `undefined` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is a class containing information about the reducer that triggered this event. For now, we can ignore this argument.\r\n\r\nWe are going to add a helper function called `appendToSystemMessage` that will append a line to the `systemMessage` state. We will use this to update the `system` message when a new user joins.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\n// Helper function to append a line to the systemMessage state\r\nfunction appendToSystemMessage(line: String) {\r\n  setSystemMessage((prevMessage) => prevMessage + \"\\n\" + line);\r\n}\r\n\r\nUser.onInsert((user, reducerEvent) => {\r\n  if (user.online) {\r\n    appendToSystemMessage(`${userNameOrIdentity(user)} has connected.`);\r\n  }\r\n});\r\n```\r\n\r\n### User.onUpdate callback - Notify about updated users\r\n\r\nBecause we declared a `#[primarykey]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User::update_by_identity` calls. We register these callbacks using the `onUpdate` method which is automatically implemented by `spacetime generate` for any table with a `#[primarykey]` column.\r\n\r\n`onUpdate` callbacks take three arguments: the old row, the new row, and a `ReducerEvent`.\r\n\r\nIn our module, users can be updated for three reasons:\r\n\r\n1. They've set their name using the `set_name` reducer.\r\n2. They're an existing user re-connecting, so their `online` has been set to `true`.\r\n3. They've disconnected, so their `online` has been set to `false`.\r\n\r\nWe'll update the `system` message in each of these cases.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\nUser.onUpdate((oldUser, user, reducerEvent) => {\r\n  if (oldUser.online === false && user.online === true) {\r\n    appendToSystemMessage(`${userNameOrIdentity(user)} has connected.`);\r\n  } else if (oldUser.online === true && user.online === false) {\r\n    appendToSystemMessage(`${userNameOrIdentity(user)} has disconnected.`);\r\n  }\r\n\r\n  if (user.name !== oldUser.name) {\r\n    appendToSystemMessage(\r\n      `User ${userNameOrIdentity(oldUser)} renamed to ${userNameOrIdentity(\r\n        user\r\n      )}.`\r\n    );\r\n  }\r\n});\r\n```\r\n\r\n### SetNameReducer.on callback - Handle errors and update profile name\r\n\r\nWe can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `OnReducer` method which is automatically implemented for each reducer by `spacetime generate`.\r\n\r\nEach reducer callback takes two arguments:\r\n\r\n1. `ReducerEvent` that contains information about the reducer that triggered this event. It contains several fields. The ones we care about are:\r\n\r\n   - `callerIdentity`: The `Identity` of the client that called the reducer.\r\n   - `status`: The `Status` of the reducer run, one of `\"Committed\"`, `\"Failed\"` or `\"OutOfEnergy\"`.\r\n   - `message`: The error message, if any, that the reducer returned.\r\n\r\n2. `ReducerArgs` which is an array containing the arguments with which the reducer was invoked.\r\n\r\nThese callbacks will be invoked in one of two cases:\r\n\r\n1. If the reducer was successful and altered any of our subscribed rows.\r\n2. If we requested an invocation which failed.\r\n\r\nNote that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.\r\n\r\nWe already handle other users' `set_name` calls using our `User.onUpdate` callback, but we need some additional behavior for setting our own name. If our name was rejected, we'll update the `system` message. If our name was accepted, we'll update our name in the app.\r\n\r\nWe'll test both that our identity matches the sender and that the status is `Failed`, even though the latter implies the former, for demonstration purposes.\r\n\r\nIf the reducer status comes back as `committed`, we'll update the name in our app.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\nSetNameReducer.on((reducerEvent, reducerArgs) => {\r\n  if (\r\n    local_identity.current &&\r\n    reducerEvent.callerIdentity.isEqual(local_identity.current)\r\n  ) {\r\n    if (reducerEvent.status === \"failed\") {\r\n      appendToSystemMessage(`Error setting name: ${reducerEvent.message} `);\r\n    } else if (reducerEvent.status === \"committed\") {\r\n      setName(reducerArgs[0]);\r\n    }\r\n  }\r\n});\r\n```\r\n\r\n### SendMessageReducer.on callback - Handle errors\r\n\r\nWe handle warnings on rejected messages the same way as rejected names, though the types and the error message are different. We don't need to do anything for successful SendMessage reducer runs; our Message.onInsert callback already displays them.\r\n\r\nTo the body of `App`, add:\r\n\r\n```typescript\r\nSendMessageReducer.on((reducerEvent, reducerArgs) => {\r\n  if (\r\n    local_identity.current &&\r\n    reducerEvent.callerIdentity.isEqual(local_identity.current)\r\n  ) {\r\n    if (reducerEvent.status === \"failed\") {\r\n      appendToSystemMessage(`Error sending message: ${reducerEvent.message} `);\r\n    }\r\n  }\r\n});\r\n```\r\n\r\n## Update the UI button callbacks\r\n\r\nWe need to update the `onSubmitNewName` and `onMessageSubmit` callbacks to send the appropriate reducer to the module.\r\n\r\n`spacetime generate` defined two functions for us, `SetNameReducer.call` and `SendMessageReducer.call`, which send a message to the database to invoke the corresponding reducer. The first argument, the `ReducerContext`, is supplied by the server, but we pass all other arguments ourselves. In our case, that means that both `SetNameReducer.call` and `SendMessageReducer.call` take one argument, a `String`.\r\n\r\nAdd the following to the `onSubmitNewName` callback:\r\n\r\n```typescript\r\nSetNameReducer.call(newName);\r\n```\r\n\r\nAdd the following to the `onMessageSubmit` callback:\r\n\r\n```typescript\r\nSendMessageReducer.call(newMessage);\r\n```\r\n\r\n## Connecting to the module\r\n\r\nWe need to connect to the module when the app loads. We'll do this by adding a `useEffect` hook to the `App` function. This hook should only run once, when the component is mounted, but we are going to use an `initialized` boolean to ensure that it only runs once.\r\n\r\n```typescript\r\nuseEffect(() => {\r\n  if (!initialized.current) {\r\n    client.current.connect();\r\n    initialized.current = true;\r\n  }\r\n}, []);\r\n```\r\n\r\n## What's next?\r\n\r\nWhen you run `npm start` you should see a chat app that can send and receive messages. If you open it in multiple private browser windows, you should see that messages are synchronized between them.\r\n\r\nCongratulations! You've built a simple chat app with SpacetimeDB. You can find the full source code for this app [here](https://github.com/clockworklabs/spacetimedb-typescript-sdk/tree/main/examples/quickstart)\r\n\r\nFor a more advanced example of the SpacetimeDB TypeScript SDK, take a look at the [Spacetime MUD (multi-user dungeon)](https://github.com/clockworklabs/spacetime-mud/tree/main/react-client).\r\n\r\n## Troubleshooting\r\n\r\nIf you encounter the following error:\r\n\r\n```\r\nTS2802: Type 'IterableIterator<any>' can only be iterated through when using the '--downlevelIteration' flag or with a '--target' of 'es2015' or higher.\r\n```\r\n\r\nYou can fix it by changing your compiler target. Add the following to your `tsconfig.json` file:\r\n\r\n```json\r\n{\r\n  \"compilerOptions\": {\r\n    \"target\": \"es2015\"\r\n  }\r\n}\r\n```\r\n",
              "hasPages": false,
              "editUrl": "index.md",
              "jumpLinks": [
                {
                  "title": "Typescript Client SDK Quick Start",
                  "route": "typescript-client-sdk-quick-start",
                  "depth": 1
                },
                {
                  "title": "Project structure",
                  "route": "project-structure",
                  "depth": 2
                },
                {
                  "title": "Basic layout",
                  "route": "basic-layout",
                  "depth": 2
                },
                {
                  "title": "Generate your module types",
                  "route": "generate-your-module-types",
                  "depth": 2
                },
                {
                  "title": "Create your SpacetimeDB client",
                  "route": "create-your-spacetimedb-client",
                  "depth": 2
                },
                {
                  "title": "Register callbacks and connect",
                  "route": "register-callbacks-and-connect",
                  "depth": 2
                },
                {
                  "title": "onConnect Callback",
                  "route": "onconnect-callback",
                  "depth": 3
                },
                {
                  "title": "initialStateSync callback",
                  "route": "initialstatesync-callback",
                  "depth": 3
                },
                {
                  "title": "Message.onInsert callback - Update messages",
                  "route": "message-oninsert-callback-update-messages",
                  "depth": 3
                },
                {
                  "title": "User.onInsert callback - Notify about new users",
                  "route": "user-oninsert-callback-notify-about-new-users",
                  "depth": 3
                },
                {
                  "title": "User.onUpdate callback - Notify about updated users",
                  "route": "user-onupdate-callback-notify-about-updated-users",
                  "depth": 3
                },
                {
                  "title": "SetNameReducer.on callback - Handle errors and update profile name",
                  "route": "setnamereducer-on-callback-handle-errors-and-update-profile-name",
                  "depth": 3
                },
                {
                  "title": "SendMessageReducer.on callback - Handle errors",
                  "route": "sendmessagereducer-on-callback-handle-errors",
                  "depth": 3
                },
                {
                  "title": "Update the UI button callbacks",
                  "route": "update-the-ui-button-callbacks",
                  "depth": 2
                },
                {
                  "title": "Connecting to the module",
                  "route": "connecting-to-the-module",
                  "depth": 2
                },
                {
                  "title": "What's next?",
                  "route": "what-s-next-",
                  "depth": 2
                },
                {
                  "title": "Troubleshooting",
                  "route": "troubleshooting",
                  "depth": 2
                }
              ],
              "pages": []
            },
            {
              "title": "The SpacetimeDB Typescript client SDK",
              "identifier": "SDK Reference",
              "indexIdentifier": "SDK Reference",
              "hasPages": false,
              "content": "# The SpacetimeDB Typescript client SDK\r\n\r\nThe SpacetimeDB client SDK for TypeScript contains all the tools you need to build clients for SpacetimeDB modules using Typescript, either in the browser or with NodeJS.\r\n\r\n> You need a database created before use the client, so make sure to follow the Rust or C# Module Quickstart guides if need one.\r\n\r\n## Install the SDK\r\n\r\nFirst, create a new client project, and add the following to your `tsconfig.json` file:\r\n\r\n```json\r\n{\r\n  \"compilerOptions\": {\r\n    //You can use any target higher than this one\r\n    //https://www.typescriptlang.org/tsconfig#target\r\n    \"target\": \"es2015\"\r\n  }\r\n}\r\n```\r\n\r\nThen add the SpacetimeDB SDK to your dependencies:\r\n\r\n```bash\r\ncd client\r\nnpm install @clockworklabs/spacetimedb-sdk\r\n```\r\n\r\nYou should have this folder layout starting from the root of your project:\r\n\r\n```bash\r\nquickstart-chat\r\n├── client\r\n│   ├── node_modules\r\n│   ├── public\r\n│   └── src\r\n└── server\r\n    └── src\r\n```\r\n\r\n### Tip for utilities/scripts\r\n\r\nIf want to create a quick script to test your module bindings from the command line, you can use https://www.npmjs.com/package/tsx to execute TypeScript files.\r\n\r\nThen you create a `script.ts` file and add the imports, code and execute with:\r\n\r\n```bash\r\nnpx tsx src/script.ts\r\n```\r\n\r\n## Generate module bindings\r\n\r\nEach SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's `src` directory and generate the Typescript interface files using the Spacetime CLI. From your project directory, run:\r\n\r\n```bash\r\nmkdir -p client/src/module_bindings\r\nspacetime generate --lang typescript \\\r\n    --out-dir client/src/module_bindings \\\r\n    --project-path server\r\n```\r\n\r\nAnd now you will get the files for the `reducers` & `tables`:\r\n\r\n```bash\r\nquickstart-chat\r\n├── client\r\n│   ├── node_modules\r\n│   ├── public\r\n│   └── src\r\n|       └── module_bindings\r\n|           ├── add_reducer.ts\r\n|           ├── person.ts\r\n|           └── say_hello_reducer.ts\r\n└── server\r\n    └── src\r\n```\r\n\r\nImport the `module_bindings` in your client's _main_ file:\r\n\r\n```typescript\r\nimport { SpacetimeDBClient, Identity } from \"@clockworklabs/spacetimedb-sdk\";\r\n\r\nimport Person from \"./module_bindings/person\";\r\nimport AddReducer from \"./module_bindings/add_reducer\";\r\nimport SayHelloReducer from \"./module_bindings/say_hello_reducer\";\r\nconsole.log(Person, AddReducer, SayHelloReducer);\r\n```\r\n\r\n> There is a known issue where if you do not use every type in your file, it will not pull them into the published build. To fix this, we are using `console.log` to force them to get pulled in.\r\n\r\n## API at a glance\r\n\r\n### Classes\r\n\r\n| Class                                           | Description                                                      |\r\n| ----------------------------------------------- | ---------------------------------------------------------------- |\r\n| [`SpacetimeDBClient`](#class-spacetimedbclient) | The database client connection to a SpacetimeDB server.          |\r\n| [`Identity`](#class-identity)                   | The user's public identity.                                      |\r\n| [`{Table}`](#class-table)                       | `{Table}` is a placeholder for each of the generated tables.     |\r\n| [`{Reducer}`](#class-reducer)                   | `{Reducer}` is a placeholder for each of the generated reducers. |\r\n\r\n### Class `SpacetimeDBClient`\r\n\r\nThe database client connection to a SpacetimeDB server.\r\n\r\nDefined in [spacetimedb-sdk.spacetimedb](https://github.com/clockworklabs/spacetimedb-typescript-sdk/blob/main/src/spacetimedb.ts):\r\n\r\n| Constructors                                                      | Description                                                              |\r\n| ----------------------------------------------------------------- | ------------------------------------------------------------------------ |\r\n| [`SpacetimeDBClient.constructor`](#spacetimedbclient-constructor) | Creates a new `SpacetimeDBClient` database client.                       |\r\n| Properties                                                        |\r\n| [`SpacetimeDBClient.identity`](#spacetimedbclient-identity)       | The user's public identity.                                              |\r\n| [`SpacetimeDBClient.live`](#spacetimedbclient-live)               | Whether the client is connected.                                         |\r\n| [`SpacetimeDBClient.token`](#spacetimedbclient-token)             | The user's private authentication token.                                 |\r\n| Methods                                                           |                                                                          |\r\n| [`SpacetimeDBClient.connect`](#spacetimedbclient-connect)         | Connect to a SpacetimeDB module.                                         |\r\n| [`SpacetimeDBClient.disconnect`](#spacetimedbclient-disconnect)   | Close the current connection.                                            |\r\n| [`SpacetimeDBClient.subscribe`](#spacetimedbclient-subscribe)     | Subscribe to a set of queries.                                           |\r\n| Events                                                            |                                                                          |\r\n| [`SpacetimeDBClient.onConnect`](#spacetimedbclient-onconnect)     | Register a callback to be invoked upon authentication with the database. |\r\n| [`SpacetimeDBClient.onError`](#spacetimedbclient-onerror)         | Register a callback to be invoked upon a error.                          |\r\n\r\n## Constructors\r\n\r\n### `SpacetimeDBClient` constructor\r\n\r\nCreates a new `SpacetimeDBClient` database client and set the initial parameters.\r\n\r\n```ts\r\nnew SpacetimeDBClient(host: string, name_or_address: string, auth_token?: string, protocol?: \"binary\" | \"json\")\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name              | Type                   | Description                                                                                                                                       |\r\n| :---------------- | :--------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `host`            | `string`               | The host of the SpacetimeDB server.                                                                                                               |\r\n| `name_or_address` | `string`               | The name or address of the SpacetimeDB module.                                                                                                    |\r\n| `auth_token?`     | `string`               | The credentials to use to connect to authenticate with SpacetimeDB.                                                                               |\r\n| `protocol?`       | `\"binary\"` \\| `\"json\"` | Define how encode the messages: `\"binary\"` \\| `\"json\"`. Binary is more efficient and compact, but JSON provides human-readable debug information. |\r\n\r\n#### Example\r\n\r\n```ts\r\nconst host = \"ws://localhost:3000\";\r\nconst name_or_address = \"database_name\";\r\nconst auth_token = undefined;\r\nconst protocol = \"binary\";\r\n\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  host,\r\n  name_or_address,\r\n  auth_token,\r\n  protocol\r\n);\r\n```\r\n\r\n## Properties\r\n\r\n### `SpacetimeDBClient` identity\r\n\r\nThe user's public [Identity](#class-identity).\r\n\r\n```\r\nidentity: Identity | undefined\r\n```\r\n\r\n---\r\n\r\n### `SpacetimeDBClient` live\r\n\r\nWhether the client is connected.\r\n\r\n```ts\r\nlive: boolean;\r\n```\r\n\r\n---\r\n\r\n### `SpacetimeDBClient` token\r\n\r\nThe user's private authentication token.\r\n\r\n```\r\ntoken: string | undefined\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name          | Type                                                   | Description                     |\r\n| :------------ | :----------------------------------------------------- | :------------------------------ |\r\n| `reducerName` | `string`                                               | The name of the reducer to call |\r\n| `serializer`  | [`Serializer`](../interfaces/serializer.Serializer.md) | -                               |\r\n\r\n---\r\n\r\n### `SpacetimeDBClient` connect\r\n\r\nConnect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.\r\n\r\n```ts\r\nconnect(host: string?, name_or_address: string?, auth_token: string?): Promise<void>\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name               | Type     | Description                                                                                                                                 |\r\n| :----------------- | :------- | :------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `host?`            | `string` | The hostname of the SpacetimeDB server. Defaults to the value passed to the [constructor](#spacetimedbclient-constructor).                  |\r\n| `name_or_address?` | `string` | The name or address of the SpacetimeDB module. Defaults to the value passed to the [constructor](#spacetimedbclient-constructor).           |\r\n| `auth_token?`      | `string` | The credentials to use to authenticate with SpacetimeDB. Defaults to the value passed to the [constructor](#spacetimedbclient-constructor). |\r\n\r\n#### Returns\r\n\r\n`Promise`<`void`\\>\r\n\r\n#### Example\r\n\r\n```ts\r\nconst host = \"ws://localhost:3000\";\r\nconst name_or_address = \"database_name\";\r\nconst auth_token = undefined;\r\n\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  host,\r\n  name_or_address,\r\n  auth_token\r\n);\r\n// Connect with the initial parameters\r\nspacetimeDBClient.connect();\r\n//Set the `auth_token`\r\nspacetimeDBClient.connect(undefined, undefined, NEW_TOKEN);\r\n```\r\n\r\n---\r\n\r\n### `SpacetimeDBClient` disconnect\r\n\r\nClose the current connection.\r\n\r\n```ts\r\ndisconnect(): void\r\n```\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\n\r\nspacetimeDBClient.disconnect();\r\n```\r\n\r\n---\r\n\r\n### `SpacetimeDBClient` subscribe\r\n\r\nSubscribe to a set of queries, to be notified when rows which match those queries are altered.\r\n\r\n> A new call to `subscribe` will remove all previous subscriptions and replace them with the new `queries`.\r\n> If any rows matched the previous subscribed queries but do not match the new queries,\r\n> those rows will be removed from the client cache, and [`{Table}.on_delete`](#table-ondelete) callbacks will be invoked for them.\r\n\r\n```ts\r\nsubscribe(queryOrQueries: string | string[]): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name             | Type                   | Description                      |\r\n| :--------------- | :--------------------- | :------------------------------- |\r\n| `queryOrQueries` | `string` \\| `string`[] | A `SQL` query or list of queries |\r\n\r\n#### Example\r\n\r\n```ts\r\nspacetimeDBClient.subscribe([\"SELECT * FROM User\", \"SELECT * FROM Message\"]);\r\n```\r\n\r\n## Events\r\n\r\n### `SpacetimeDBClient` onConnect\r\n\r\nRegister a callback to be invoked upon authentication with the database.\r\n\r\n```ts\r\nonConnect(callback: (token: string, identity: Identity) => void): void\r\n```\r\n\r\nThe callback will be invoked with the public [Identity](#class-identity) and private authentication token provided by the database to identify this connection. If credentials were supplied to [connect](#spacetimedbclient-connect), those passed to the callback will be equivalent to the ones used to connect. If the initial connection was anonymous, a new set of credentials will be generated by the database to identify this user.\r\n\r\nThe credentials passed to the callback can be saved and used to authenticate the same user in future connections.\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                     |\r\n| :--------- | :----------------------------------------------------------------------- |\r\n| `callback` | (`token`: `string`, `identity`: [`Identity`](#class-identity)) => `void` |\r\n\r\n#### Example\r\n\r\n```ts\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  console.log(\"Connected to SpacetimeDB\");\r\n  console.log(\"Token\", token);\r\n  console.log(\"Identity\", identity);\r\n});\r\n```\r\n\r\n---\r\n\r\n### `SpacetimeDBClient` onError\r\n\r\nRegister a callback to be invoked upon an error.\r\n\r\n```ts\r\nonError(callback: (...args: any[]) => void): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                           |\r\n| :--------- | :----------------------------- |\r\n| `callback` | (...`args`: `any`[]) => `void` |\r\n\r\n#### Example\r\n\r\n```ts\r\nspacetimeDBClient.onError((...args: any[]) => {\r\n  console.error(\"ERROR\", args);\r\n});\r\n```\r\n\r\n### Class `Identity`\r\n\r\nA unique public identifier for a client connected to a database.\r\n\r\nDefined in [spacetimedb-sdk.identity](https://github.com/clockworklabs/spacetimedb-typescript-sdk/blob/main/src/identity.ts):\r\n\r\n| Constructors                                    | Description                                  |\r\n| ----------------------------------------------- | -------------------------------------------- |\r\n| [`Identity.constructor`](#identity-constructor) | Creates a new `Identity`.                    |\r\n| Methods                                         |                                              |\r\n| [`Identity.isEqual`](#identity-isequal)         | Compare two identities for equality.         |\r\n| [`Identity.toHexString`](#identity-tohexstring) | Print the identity as a hexadecimal string.  |\r\n| Static methods                                  |                                              |\r\n| [`Identity.fromString`](#identity-fromstring)   | Parse an Identity from a hexadecimal string. |\r\n\r\n## Constructors\r\n\r\n### `Identity` constructor\r\n\r\n```ts\r\nnew Identity(data: Uint8Array)\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name   | Type         |\r\n| :----- | :----------- |\r\n| `data` | `Uint8Array` |\r\n\r\n## Methods\r\n\r\n### `Identity` isEqual\r\n\r\nCompare two identities for equality.\r\n\r\n```ts\r\nisEqual(other: Identity): boolean\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name    | Type                          |\r\n| :------ | :---------------------------- |\r\n| `other` | [`Identity`](#class-identity) |\r\n\r\n#### Returns\r\n\r\n`boolean`\r\n\r\n---\r\n\r\n### `Identity` toHexString\r\n\r\nPrint an `Identity` as a hexadecimal string.\r\n\r\n```ts\r\ntoHexString(): string\r\n```\r\n\r\n#### Returns\r\n\r\n`string`\r\n\r\n---\r\n\r\n### `Identity` fromString\r\n\r\nStatic method; parse an Identity from a hexadecimal string.\r\n\r\n```ts\r\nIdentity.fromString(str: string): Identity\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name  | Type     |\r\n| :---- | :------- |\r\n| `str` | `string` |\r\n\r\n#### Returns\r\n\r\n[`Identity`](#class-identity)\r\n\r\n### Class `{Table}`\r\n\r\nFor each table defined by a module, `spacetime generate` generates a `class` in the `module_bindings` folder whose name is that table's name converted to `PascalCase`.\r\n\r\nThe generated class has a field for each of the table's columns, whose names are the column names converted to `snake_case`.\r\n\r\n| Properties                                        | Description                                                                                                                       |\r\n| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |\r\n| [`Table.name`](#table-name)                       | The name of the class.                                                                                                            |\r\n| [`Table.tableName`](#table-tableName)             | The name of the table in the database.                                                                                            |\r\n| Methods                                           |                                                                                                                                   |\r\n| [`Table.isEqual`](#table-isequal)                 | Method to compare two identities.                                                                                                 |\r\n| [`Table.all`](#table-all)                         | Return all the subscribed rows in the table.                                                                                      |\r\n| [`Table.filterBy{COLUMN}`](#table-filterbycolumn) | Autogenerated; returned subscribed rows with a given value in a particular column. `{COLUMN}` is a placeholder for a column name. |\r\n| Events                                            |                                                                                                                                   |\r\n| [`Table.onInsert`](#table-oninsert)               | Register an `onInsert` callback for when a subscribed row is newly inserted into the database.                                    |\r\n| [`Table.removeOnInsert`](#table-removeoninsert)   | Unregister a previously-registered [`onInsert`](#table-oninsert) callback.                                                        |\r\n| [`Table.onUpdate`](#table-onupdate)               | Register an `onUpdate` callback for when an existing row is modified.                                                             |\r\n| [`Table.removeOnUpdate`](#table-removeonupdate)   | Unregister a previously-registered [`onUpdate`](#table-onupdate) callback.                                                        |\r\n| [`Table.onDelete`](#table-ondelete)               | Register an `onDelete` callback for when a subscribed row is removed from the database.                                           |\r\n| [`Table.removeOnDelete`](#table-removeondelete)   | Unregister a previously-registered [`onDelete`](#table-removeondelete) callback.                                                  |\r\n\r\n## Properties\r\n\r\n### {Table} name\r\n\r\n• **name**: `string`\r\n\r\nThe name of the `Class`.\r\n\r\n---\r\n\r\n### {Table} tableName\r\n\r\nThe name of the table in the database.\r\n\r\n▪ `Static` **tableName**: `string` = `\"Person\"`\r\n\r\n## Methods\r\n\r\n### {Table} all\r\n\r\nReturn all the subscribed rows in the table.\r\n\r\n```ts\r\n{Table}.all(): {Table}[]\r\n```\r\n\r\n#### Returns\r\n\r\n`{Table}[]`\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\n\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  spacetimeDBClient.subscribe([\"SELECT * FROM Person\"]);\r\n\r\n  setTimeout(() => {\r\n    console.log(Person.all()); // Prints all the `Person` rows in the database.\r\n  }, 5000);\r\n});\r\n```\r\n\r\n---\r\n\r\n### {Table} count\r\n\r\nReturn the number of subscribed rows in the table, or 0 if there is no active connection.\r\n\r\n```ts\r\n{Table}.count(): number\r\n```\r\n\r\n#### Returns\r\n\r\n`number`\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\n\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  spacetimeDBClient.subscribe([\"SELECT * FROM Person\"]);\r\n\r\n  setTimeout(() => {\r\n    console.log(Person.count());\r\n  }, 5000);\r\n});\r\n```\r\n\r\n---\r\n\r\n### {Table} filterBy{COLUMN}\r\n\r\nFor each column of a table, `spacetime generate` generates a static method on the `Class` to filter or seek subscribed rows where that column matches a requested value.\r\n\r\nThese methods are named `filterBy{COLUMN}`, where `{COLUMN}` is the column name converted to `camelCase`.\r\n\r\n```ts\r\n{Table}.filterBy{COLUMN}(value): {Table}[]\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name    | Type                        |\r\n| :------ | :-------------------------- |\r\n| `value` | The type of the `{COLUMN}`. |\r\n\r\n#### Returns\r\n\r\n`{Table}[]`\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\n\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  spacetimeDBClient.subscribe([\"SELECT * FROM Person\"]);\r\n\r\n  setTimeout(() => {\r\n    console.log(Person.filterByName(\"John\")); // prints all the `Person` rows named John.\r\n  }, 5000);\r\n});\r\n```\r\n\r\n---\r\n\r\n### {Table} fromValue\r\n\r\nDeserialize an `AlgebraicType` into this `{Table}`.\r\n\r\n```ts\r\n {Table}.fromValue(value: AlgebraicValue): {Table}\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name    | Type             |\r\n| :------ | :--------------- |\r\n| `value` | `AlgebraicValue` |\r\n\r\n#### Returns\r\n\r\n`{Table}`\r\n\r\n---\r\n\r\n### {Table} getAlgebraicType\r\n\r\nSerialize `this` into an `AlgebraicType`.\r\n\r\n#### Example\r\n\r\n```ts\r\n{Table}.getAlgebraicType(): AlgebraicType\r\n```\r\n\r\n#### Returns\r\n\r\n`AlgebraicType`\r\n\r\n---\r\n\r\n### {Table} onInsert\r\n\r\nRegister an `onInsert` callback for when a subscribed row is newly inserted into the database.\r\n\r\n```ts\r\n{Table}.onInsert(callback: (value: {Table}, reducerEvent: ReducerEvent | undefined) => void): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                          | Description                                            |\r\n| :--------- | :---------------------------------------------------------------------------- | :----------------------------------------------------- |\r\n| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \\| `ReducerEvent`) => `void` | Callback to run whenever a subscribed row is inserted. |\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  spacetimeDBClient.subscribe([\"SELECT * FROM Person\"]);\r\n});\r\n\r\nPerson.onInsert((person, reducerEvent) => {\r\n  if (reducerEvent) {\r\n    console.log(\"New person inserted by reducer\", reducerEvent, person);\r\n  } else {\r\n    console.log(\"New person received during subscription update\", person);\r\n  }\r\n});\r\n```\r\n\r\n---\r\n\r\n### {Table} removeOnInsert\r\n\r\nUnregister a previously-registered [`onInsert`](#table-oninsert) callback.\r\n\r\n```ts\r\n{Table}.removeOnInsert(callback: (value: Person, reducerEvent: ReducerEvent | undefined) => void): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                          |\r\n| :--------- | :---------------------------------------------------------------------------- |\r\n| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \\| `ReducerEvent`) => `void` |\r\n\r\n---\r\n\r\n### {Table} onUpdate\r\n\r\nRegister an `onUpdate` callback to run when an existing row is modified by primary key.\r\n\r\n```ts\r\n{Table}.onUpdate(callback: (oldValue: {Table}, newValue: {Table}, reducerEvent: ReducerEvent | undefined) => void): void\r\n```\r\n\r\n`onUpdate` callbacks are only meaningful for tables with a column declared as a primary key. Tables without primary keys will never fire `onUpdate` callbacks.\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                                                    | Description                                           |\r\n| :--------- | :------------------------------------------------------------------------------------------------------ | :---------------------------------------------------- |\r\n| `callback` | (`oldValue`: `{Table}`, `newValue`: `{Table}`, `reducerEvent`: `undefined` \\| `ReducerEvent`) => `void` | Callback to run whenever a subscribed row is updated. |\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  spacetimeDBClient.subscribe([\"SELECT * FROM Person\"]);\r\n});\r\n\r\nPerson.onUpdate((oldPerson, newPerson, reducerEvent) => {\r\n  console.log(\"Person updated by reducer\", reducerEvent, oldPerson, newPerson);\r\n});\r\n```\r\n\r\n---\r\n\r\n### {Table} removeOnUpdate\r\n\r\nUnregister a previously-registered [`onUpdate`](#table-onUpdate) callback.\r\n\r\n```ts\r\n{Table}.removeOnUpdate(callback: (oldValue: {Table}, newValue: {Table}, reducerEvent: ReducerEvent | undefined) => void): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                                                    |\r\n| :--------- | :------------------------------------------------------------------------------------------------------ |\r\n| `callback` | (`oldValue`: `{Table}`, `newValue`: `{Table}`, `reducerEvent`: `undefined` \\| `ReducerEvent`) => `void` |\r\n\r\n---\r\n\r\n### {Table} onDelete\r\n\r\nRegister an `onDelete` callback for when a subscribed row is removed from the database.\r\n\r\n```ts\r\n{Table}.onDelete(callback: (value: {Table}, reducerEvent: ReducerEvent | undefined) => void): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                          | Description                                           |\r\n| :--------- | :---------------------------------------------------------------------------- | :---------------------------------------------------- |\r\n| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \\| `ReducerEvent`) => `void` | Callback to run whenever a subscribed row is removed. |\r\n\r\n#### Example\r\n\r\n```ts\r\nvar spacetimeDBClient = new SpacetimeDBClient(\r\n  \"ws://localhost:3000\",\r\n  \"database_name\"\r\n);\r\nspacetimeDBClient.onConnect((token, identity) => {\r\n  spacetimeDBClient.subscribe([\"SELECT * FROM Person\"]);\r\n});\r\n\r\nPerson.onDelete((person, reducerEvent) => {\r\n  if (reducerEvent) {\r\n    console.log(\"Person deleted by reducer\", reducerEvent, person);\r\n  } else {\r\n    console.log(\r\n      \"Person no longer subscribed during subscription update\",\r\n      person\r\n    );\r\n  }\r\n});\r\n```\r\n\r\n---\r\n\r\n### {Table} removeOnDelete\r\n\r\nUnregister a previously-registered [`onDelete`](#table-onDelete) callback.\r\n\r\n```ts\r\n{Table}.removeOnDelete(callback: (value: {Table}, reducerEvent: ReducerEvent | undefined) => void): void\r\n```\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                                          |\r\n| :--------- | :---------------------------------------------------------------------------- |\r\n| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \\| `ReducerEvent`) => `void` |\r\n\r\n### Class `{Reducer}`\r\n\r\n`spacetime generate` defines an `{Reducer}` class in the `module_bindings` folder for each reducer defined by a module.\r\n\r\nThe class's name will be the reducer's name converted to `PascalCase`.\r\n\r\n| Static methods                  | Description                                                  |\r\n| ------------------------------- | ------------------------------------------------------------ |\r\n| [`Reducer.call`](#reducer-call) | Executes the reducer.                                        |\r\n| Events                          |                                                              |\r\n| [`Reducer.on`](#reducer-on)     | Register a callback to run each time the reducer is invoked. |\r\n\r\n## Static methods\r\n\r\n### {Reducer} call\r\n\r\nExecutes the reducer.\r\n\r\n```ts\r\n{Reducer}.call(): void\r\n```\r\n\r\n#### Example\r\n\r\n```ts\r\nSayHelloReducer.call();\r\n```\r\n\r\n## Events\r\n\r\n### {Reducer} on\r\n\r\nRegister a callback to run each time the reducer is invoked.\r\n\r\n```ts\r\n{Reducer}.on(callback: (reducerEvent: ReducerEvent, reducerArgs: any[]) => void): void\r\n```\r\n\r\nClients will only be notified of reducer runs if either of two criteria is met:\r\n\r\n- The reducer inserted, deleted or updated at least one row to which the client is subscribed.\r\n- The reducer invocation was requested by this client, and the run failed.\r\n\r\n#### Parameters\r\n\r\n| Name       | Type                                                        |\r\n| :--------- | :---------------------------------------------------------- |\r\n| `callback` | `(reducerEvent: ReducerEvent, reducerArgs: any[]) => void)` |\r\n\r\n#### Example\r\n\r\n```ts\r\nSayHelloReducer.on((reducerEvent, reducerArgs) => {\r\n  console.log(\"SayHelloReducer called\", reducerEvent, reducerArgs);\r\n});\r\n```\r\n",
              "editUrl": "SDK%20Reference.md",
              "jumpLinks": [
                {
                  "title": "The SpacetimeDB Typescript client SDK",
                  "route": "the-spacetimedb-typescript-client-sdk",
                  "depth": 1
                },
                {
                  "title": "Install the SDK",
                  "route": "install-the-sdk",
                  "depth": 2
                },
                {
                  "title": "Tip for utilities/scripts",
                  "route": "tip-for-utilities-scripts",
                  "depth": 3
                },
                {
                  "title": "Generate module bindings",
                  "route": "generate-module-bindings",
                  "depth": 2
                },
                {
                  "title": "API at a glance",
                  "route": "api-at-a-glance",
                  "depth": 2
                },
                {
                  "title": "Classes",
                  "route": "classes",
                  "depth": 3
                },
                {
                  "title": "Class `SpacetimeDBClient`",
                  "route": "class-spacetimedbclient-",
                  "depth": 3
                },
                {
                  "title": "Constructors",
                  "route": "constructors",
                  "depth": 2
                },
                {
                  "title": "`SpacetimeDBClient` constructor",
                  "route": "-spacetimedbclient-constructor",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "Properties",
                  "route": "properties",
                  "depth": 2
                },
                {
                  "title": "`SpacetimeDBClient` identity",
                  "route": "-spacetimedbclient-identity",
                  "depth": 3
                },
                {
                  "title": "`SpacetimeDBClient` live",
                  "route": "-spacetimedbclient-live",
                  "depth": 3
                },
                {
                  "title": "`SpacetimeDBClient` token",
                  "route": "-spacetimedbclient-token",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "`SpacetimeDBClient` connect",
                  "route": "-spacetimedbclient-connect",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "`SpacetimeDBClient` disconnect",
                  "route": "-spacetimedbclient-disconnect",
                  "depth": 3
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "`SpacetimeDBClient` subscribe",
                  "route": "-spacetimedbclient-subscribe",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "Events",
                  "route": "events",
                  "depth": 2
                },
                {
                  "title": "`SpacetimeDBClient` onConnect",
                  "route": "-spacetimedbclient-onconnect",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "`SpacetimeDBClient` onError",
                  "route": "-spacetimedbclient-onerror",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "Class `Identity`",
                  "route": "class-identity-",
                  "depth": 3
                },
                {
                  "title": "Constructors",
                  "route": "constructors",
                  "depth": 2
                },
                {
                  "title": "`Identity` constructor",
                  "route": "-identity-constructor",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Methods",
                  "route": "methods",
                  "depth": 2
                },
                {
                  "title": "`Identity` isEqual",
                  "route": "-identity-isequal",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "`Identity` toHexString",
                  "route": "-identity-tohexstring",
                  "depth": 3
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "`Identity` fromString",
                  "route": "-identity-fromstring",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "Class `{Table}`",
                  "route": "class-table-",
                  "depth": 3
                },
                {
                  "title": "Properties",
                  "route": "properties",
                  "depth": 2
                },
                {
                  "title": "{Table} name",
                  "route": "-table-name",
                  "depth": 3
                },
                {
                  "title": "{Table} tableName",
                  "route": "-table-tablename",
                  "depth": 3
                },
                {
                  "title": "Methods",
                  "route": "methods",
                  "depth": 2
                },
                {
                  "title": "{Table} all",
                  "route": "-table-all",
                  "depth": 3
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "{Table} count",
                  "route": "-table-count",
                  "depth": 3
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "{Table} filterBy{COLUMN}",
                  "route": "-table-filterby-column-",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "{Table} fromValue",
                  "route": "-table-fromvalue",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "{Table} getAlgebraicType",
                  "route": "-table-getalgebraictype",
                  "depth": 3
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "Returns",
                  "route": "returns",
                  "depth": 4
                },
                {
                  "title": "{Table} onInsert",
                  "route": "-table-oninsert",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "{Table} removeOnInsert",
                  "route": "-table-removeoninsert",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "{Table} onUpdate",
                  "route": "-table-onupdate",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "{Table} removeOnUpdate",
                  "route": "-table-removeonupdate",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "{Table} onDelete",
                  "route": "-table-ondelete",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "{Table} removeOnDelete",
                  "route": "-table-removeondelete",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Class `{Reducer}`",
                  "route": "class-reducer-",
                  "depth": 3
                },
                {
                  "title": "Static methods",
                  "route": "static-methods",
                  "depth": 2
                },
                {
                  "title": "{Reducer} call",
                  "route": "-reducer-call",
                  "depth": 3
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                },
                {
                  "title": "Events",
                  "route": "events",
                  "depth": 2
                },
                {
                  "title": "{Reducer} on",
                  "route": "-reducer-on",
                  "depth": 3
                },
                {
                  "title": "Parameters",
                  "route": "parameters",
                  "depth": 4
                },
                {
                  "title": "Example",
                  "route": "example",
                  "depth": 4
                }
              ],
              "pages": []
            }
          ]
        }
      ],
      "previousKey": {
        "title": "Server Module Languages",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "Module ABI Reference",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "Module ABI Reference",
      "identifier": "Module ABI Reference",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "Module%20ABI%20Reference/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "Module ABI Reference",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# Module ABI Reference\r\n\r\nThis document specifies the _low level details_ of module-host interactions (_\"Module ABI\"_). _**Most users**_ looking to interact with the host will want to use derived and higher level functionality like [`bindings`], `#[spacetimedb(table)]`, and `#[derive(SpacetimeType)]` rather than this low level ABI. For more on those, read the [Rust module quick start][module_quick_start] guide and the [Rust module reference][module_ref].\r\n\r\nThe Module ABI is defined in [`bindings_sys::raw`] and is used by modules to interact with their host and perform various operations like:\r\n\r\n- logging,\r\n- transporting data,\r\n- scheduling reducers,\r\n- altering tables,\r\n- inserting and deleting rows,\r\n- querying tables.\r\n\r\nIn the next few sections, we'll define the functions that make up the ABI and what these functions do.\r\n\r\n## General notes\r\n\r\nThe functions in this ABI all use the [`C` ABI on the `wasm32` platform][wasm_c_abi]. They are specified in a Rust `extern \"C\" { .. }` block. For those more familiar with the `C` notation, an [appendix][c_header] is provided with equivalent definitions as would occur in a `.h` file.\r\n\r\nMany functions in the ABI take in- or out-pointers, e.g. `*const u8` and `*mut u8`. The WASM host itself does not have undefined behavior. However, what WASM does not consider a memory access violation could be one according to some other language's abstract machine. For example, running the following on a WASM host would violate Rust's rules around writing across allocations:\r\n\r\n```rust\r\nfn main() {\r\n    let mut bytes = [0u8; 12];\r\n    let other_bytes = [0u8; 4];\r\n    unsafe { ffi_func_with_out_ptr_and_len(&mut bytes as *mut u8, 16); }\r\n    assert_eq!(other_bytes, [0u8; 4]);\r\n}\r\n```\r\n\r\nWhen we note in this reference that traps occur or errors are returned on memory access violations, we only mean those that WASM can directly detected, and not cases like the one above.\r\n\r\nShould memory access violations occur, such as a buffer overrun, undefined behavior will never result, as it does not exist in WASM. However, in many cases, an error code will result.\r\n\r\nSome functions will treat UTF-8 strings _lossily_. That is, if the slice identified by a `(ptr, len)` contains non-UTF-8 bytes, these bytes will be replaced with `�` in the read string.\r\n\r\nMost functions return a `u16` value. This is how these functions indicate an error where a `0` value means that there were no errors. Such functions will instead return any data they need to through out pointers.\r\n\r\n## Logging\r\n\r\n```rust\r\n/// The error log level.\r\nconst LOG_LEVEL_ERROR: u8 = 0;\r\n/// The warn log level.\r\nconst LOG_LEVEL_WARN: u8 = 1;\r\n/// The info log level.\r\nconst LOG_LEVEL_INFO: u8 = 2;\r\n/// The debug log level.\r\nconst LOG_LEVEL_DEBUG: u8 = 3;\r\n/// The trace log level.\r\nconst LOG_LEVEL_TRACE: u8 = 4;\r\n/// The panic log level.\r\n///\r\n/// A panic level is emitted just before\r\n/// a fatal error causes the WASM module to trap.\r\nconst LOG_LEVEL_PANIC: u8 = 101;\r\n\r\n/// Log at `level` a `text` message occuring in `filename:line_number`\r\n/// with `target` being the module path at the `log!` invocation site.\r\n///\r\n/// These various pointers are interpreted lossily as UTF-8 strings.\r\n/// The data pointed to are copied. Ownership does not transfer.\r\n///\r\n/// See https://docs.rs/log/latest/log/struct.Record.html#method.target\r\n/// for more info on `target`.\r\n///\r\n/// Calls to the function cannot fail\r\n/// irrespective of memory access violations.\r\n/// If they occur, no message is logged.\r\nfn _console_log(\r\n    // The level we're logging at.\r\n    // One of the `LOG_*` constants above.\r\n    level: u8,\r\n    // The module path, if any, associated with the message\r\n    // or to \"blame\" for the reason we're logging.\r\n    //\r\n    // This is a pointer to a buffer holding an UTF-8 encoded string.\r\n    // When the pointer is `NULL`, `target` is ignored.\r\n    target: *const u8,\r\n    // The length of the buffer pointed to by `text`.\r\n    // Unused when `target` is `NULL`.\r\n    target_len: usize,\r\n    // The file name, if any, associated with the message\r\n    // or to \"blame\" for the reason we're logging.\r\n    //\r\n    // This is a pointer to a buffer holding an UTF-8 encoded string.\r\n    // When the pointer is `NULL`, `filename` is ignored.\r\n    filename: *const u8,\r\n    // The length of the buffer pointed to by `text`.\r\n    // Unused when `filename` is `NULL`.\r\n    filename_len: usize,\r\n    // The line number associated with the message\r\n    // or to \"blame\" for the reason we're logging.\r\n    line_number: u32,\r\n    // A pointer to a buffer holding an UTF-8 encoded message to log.\r\n    text: *const u8,\r\n    // The length of the buffer pointed to by `text`.\r\n    text_len: usize,\r\n);\r\n```\r\n\r\n## Buffer handling\r\n\r\n```rust\r\n/// Returns the length of buffer `bufh` without\r\n/// transferring ownership of the data into the function.\r\n///\r\n/// The `bufh` must have previously been allocating using `_buffer_alloc`.\r\n///\r\n/// Traps if the buffer does not exist.\r\nfn _buffer_len(\r\n    // The buffer previously allocated using `_buffer_alloc`.\r\n    // Ownership of the buffer is not taken.\r\n    bufh: ManuallyDrop<Buffer>\r\n) -> usize;\r\n\r\n/// Consumes the buffer `bufh`,\r\n/// moving its contents to the WASM byte slice `(ptr, len)`.\r\n///\r\n/// Returns an error if the buffer does not exist\r\n/// or on any memory access violations associated with `(ptr, len)`.\r\nfn _buffer_consume(\r\n    // The buffer to consume and move into `(ptr, len)`.\r\n    // Ownership of the buffer and its contents are taken.\r\n    // That is, `bufh` won't be usable after this call.\r\n    bufh: Buffer,\r\n    // A WASM out pointer to write the contents of `bufh` to.\r\n    ptr: *mut u8,\r\n    // The size of the buffer pointed to by `ptr`.\r\n    // This size must match that of `bufh` or a trap will occur.\r\n    len: usize\r\n);\r\n\r\n/// Creates a buffer of size `data_len` in the host environment.\r\n///\r\n/// The contents of the byte slice lasting `data_len` bytes\r\n/// at the `data` WASM pointer are read\r\n/// and written into the newly initialized buffer.\r\n///\r\n/// Traps on any memory access violations.\r\nfn _buffer_alloc(data: *const u8, data_len: usize) -> Buffer;\r\n```\r\n\r\n## Reducer scheduling\r\n\r\n```rust\r\n/// Schedules a reducer to be called asynchronously at `time`.\r\n///\r\n/// The reducer is named as the valid UTF-8 slice `(name, name_len)`,\r\n/// and is passed the slice `(args, args_len)` as its argument.\r\n///\r\n/// A generated schedule id is assigned to the reducer.\r\n/// This id is written to the pointer `out`.\r\n///\r\n/// Errors on any memory access violations,\r\n/// if `(name, name_len)` does not point to valid UTF-8,\r\n/// or if the `time` delay exceeds `64^6 - 1` milliseconds from now.\r\nfn _schedule_reducer(\r\n    // A pointer to a buffer\r\n    // with a valid UTF-8 string of `name_len` many bytes.\r\n    name: *const u8,\r\n    // The number of bytes in the `name` buffer.\r\n    name_len: usize,\r\n    // A pointer to a byte buffer of `args_len` many bytes.\r\n    args: *const u8,\r\n    // The number of bytes in the `args` buffer.\r\n    args_len: usize,\r\n    // When to call the reducer.\r\n    time: u64,\r\n    // The schedule ID is written to this out pointer on a successful call.\r\n    out: *mut u64,\r\n);\r\n\r\n/// Unschedules a reducer\r\n/// using the same `id` generated as when it was scheduled.\r\n///\r\n/// This assumes that the reducer hasn't already been executed.\r\nfn _cancel_reducer(id: u64);\r\n```\r\n\r\n## Altering tables\r\n\r\n```rust\r\n/// Creates an index with the name `index_name` and type `index_type`,\r\n/// on a product of the given columns in `col_ids`\r\n/// in the table identified by `table_id`.\r\n///\r\n/// Here `index_name` points to a UTF-8 slice in WASM memory\r\n/// and `col_ids` points to a byte slice in WASM memory\r\n/// with each element being a column.\r\n///\r\n/// Currently only single-column-indices are supported\r\n/// and they may only be of the btree index type.\r\n/// In the former case, the function will panic,\r\n/// and in latter, an error is returned.\r\n///\r\n/// Returns an error on any memory access violations,\r\n/// if `(index_name, index_name_len)` is not valid UTF-8,\r\n/// or when a table with the provided `table_id` doesn't exist.\r\n///\r\n/// Traps if `index_type /= 0` or if `col_len /= 1`.\r\nfn _create_index(\r\n    // A pointer to a buffer holding an UTF-8 encoded index name.\r\n    index_name: *const u8,\r\n    // The length of the buffer pointed to by `index_name`.\r\n    index_name_len: usize,\r\n    // The ID of the table to create the index for.\r\n    table_id: u32,\r\n    // The type of the index.\r\n    // Must be `0` currently, that is, a btree-index.\r\n    index_type: u8,\r\n    // A pointer to a buffer holding a byte slice\r\n    // where each element is the position\r\n    // of a column to include in the index.\r\n    col_ids: *const u8,\r\n    // The length of the byte slice in `col_ids`. Must be `1`.\r\n    col_len: usize,\r\n) -> u16;\r\n```\r\n\r\n## Inserting and deleting rows\r\n\r\n```rust\r\n/// Inserts a row into the table identified by `table_id`,\r\n/// where the row is read from the byte slice `row_ptr` in WASM memory,\r\n/// lasting `row_len` bytes.\r\n///\r\n/// Errors if there were unique constraint violations,\r\n/// if there were any memory access violations in associated with `row`,\r\n/// if the `table_id` doesn't identify a table,\r\n/// or if `(row, row_len)` doesn't decode from BSATN to a `ProductValue`\r\n/// according to the `ProductType` that the table's schema specifies.\r\nfn _insert(\r\n    // The table to insert the row into.\r\n    // The interpretation of `(row, row_len)` depends on this ID\r\n    // as it's table schema determines how to decode the raw bytes.\r\n    table_id: u32,\r\n    // An in/out pointer to a byte buffer\r\n    // holding the BSATN-encoded `ProductValue` row data to insert.\r\n    //\r\n    // The pointer is written to with the inserted row re-encoded.\r\n    // This is due to auto-incrementing columns.\r\n    row: *mut u8,\r\n    // The length of the buffer pointed to by `row`.\r\n    row_len: usize\r\n) -> u16;\r\n\r\n/// Deletes all rows in the table identified by `table_id`\r\n/// where the column identified by `col_id` matches the byte string,\r\n/// in WASM memory, pointed to by `value`.\r\n///\r\n/// Matching is defined by decoding of `value` to an `AlgebraicValue`\r\n/// according to the column's schema and then `Ord for AlgebraicValue`.\r\n///\r\n/// The number of rows deleted is written to the WASM pointer `out`.\r\n///\r\n/// Errors if there were memory access violations\r\n/// associated with `value` or `out`,\r\n/// if no columns were deleted,\r\n/// or if the column wasn't found.\r\nfn _delete_by_col_eq(\r\n    // The table to delete rows from.\r\n    table_id: u32,\r\n    // The position of the column to match `(value, value_len)` against.\r\n    col_id: u32,\r\n    // A pointer to a byte buffer holding a BSATN-encoded `AlgebraicValue`\r\n    // of the `AlgebraicType` that the table's schema specifies\r\n    // for the column identified by `col_id`.\r\n    value: *const u8,\r\n    // The length of the buffer pointed to by `value`.\r\n    value_len: usize,\r\n    // An out pointer that the number of rows deleted is written to.\r\n    out: *mut u32\r\n) -> u16;\r\n```\r\n\r\n## Querying tables\r\n\r\n```rust\r\n/// Queries the `table_id` associated with the given (table) `name`\r\n/// where `name` points to a UTF-8 slice\r\n/// in WASM memory of `name_len` bytes.\r\n///\r\n/// The table id is written into the `out` pointer.\r\n///\r\n/// Errors on memory access violations associated with `name`\r\n/// or if the table does not exist.\r\nfn _get_table_id(\r\n    // A pointer to a buffer holding the name of the table\r\n    // as a valid UTF-8 encoded string.\r\n    name: *const u8,\r\n    // The length of the buffer pointed to by `name`.\r\n    name_len: usize,\r\n    // An out pointer to write the table ID to.\r\n    out: *mut u32\r\n) -> u16;\r\n\r\n/// Finds all rows in the table identified by `table_id`,\r\n/// where the row has a column, identified by `col_id`,\r\n/// with data matching the byte string,\r\n/// in WASM memory, pointed to at by `val`.\r\n///\r\n/// Matching is defined by decoding of `value`\r\n/// to an `AlgebraicValue` according to the column's schema\r\n/// and then `Ord for AlgebraicValue`.\r\n///\r\n/// The rows found are BSATN encoded and then concatenated.\r\n/// The resulting byte string from the concatenation\r\n/// is written to a fresh buffer\r\n/// with the buffer's identifier written to the WASM pointer `out`.\r\n///\r\n/// Errors if no table with `table_id` exists,\r\n/// if `col_id` does not identify a column of the table,\r\n/// if `(value, value_len)` cannot be decoded to an `AlgebraicValue`\r\n/// typed at the `AlgebraicType` of the column,\r\n/// or if memory access violations occurred associated with `value` or `out`.\r\nfn _iter_by_col_eq(\r\n    // Identifies the table to find rows in.\r\n    table_id: u32,\r\n    // The position of the column in the table\r\n    // to match `(value, value_len)` against.\r\n    col_id: u32,\r\n    // A pointer to a byte buffer holding a BSATN encoded\r\n    // value typed at the `AlgebraicType` of the column.\r\n    value: *const u8,\r\n    // The length of the buffer pointed to by `value`.\r\n    value_len: usize,\r\n    // An out pointer to which the new buffer's id is written to.\r\n    out: *mut Buffer\r\n) -> u16;\r\n\r\n/// Starts iteration on each row, as bytes,\r\n/// of a table identified by `table_id`.\r\n///\r\n/// The iterator is registered in the host environment\r\n/// under an assigned index which is written to the `out` pointer provided.\r\n///\r\n/// Errors if the table doesn't exist\r\n/// or if memory access violations occurred in association with `out`.\r\nfn _iter_start(\r\n    // The ID of the table to start row iteration on.\r\n    table_id: u32,\r\n    // An out pointer to which an identifier\r\n    // to the newly created buffer is written.\r\n    out: *mut BufferIter\r\n) -> u16;\r\n\r\n/// Like [`_iter_start`], starts iteration on each row,\r\n/// as bytes, of a table identified by `table_id`.\r\n///\r\n/// The rows are filtered through `filter`, which is read from WASM memory\r\n/// and is encoded in the embedded language defined by `spacetimedb_lib::filter::Expr`.\r\n///\r\n/// The iterator is registered in the host environment\r\n/// under an assigned index which is written to the `out` pointer provided.\r\n///\r\n/// Errors if `table_id` doesn't identify a table,\r\n/// if `(filter, filter_len)` doesn't decode to a filter expression,\r\n/// or if there were memory access violations\r\n/// in association with `filter` or `out`.\r\nfn _iter_start_filtered(\r\n    // The ID of the table to start row iteration on.\r\n    table_id: u32,\r\n    // A pointer to a buffer holding an encoded filter expression.\r\n    filter: *const u8,\r\n    // The length of the buffer pointed to by `filter`.\r\n    filter_len: usize,\r\n    // An out pointer to which an identifier\r\n    // to the newly created buffer is written.\r\n    out: *mut BufferIter\r\n) -> u16;\r\n\r\n/// Advances the registered iterator with the index given by `iter_key`.\r\n///\r\n/// On success, the next element (the row as bytes) is written to a buffer.\r\n/// The buffer's index is returned and written to the `out` pointer.\r\n/// If there are no elements left, an invalid buffer index is written to `out`.\r\n/// On failure however, the error is returned.\r\n///\r\n/// Errors if `iter` does not identify a registered `BufferIter`,\r\n/// or if there were memory access violations in association with `out`.\r\nfn _iter_next(\r\n    // An identifier for the iterator buffer to advance.\r\n    // Ownership of the buffer nor the identifier is moved into the function.\r\n    iter: ManuallyDrop<BufferIter>,\r\n    // An out pointer to write the newly created buffer's identifier to.\r\n    out: *mut Buffer\r\n) -> u16;\r\n\r\n/// Drops the entire registered iterator with the index given by `iter_key`.\r\n/// The iterator is effectively de-registered.\r\n///\r\n/// Returns an error if the iterator does not exist.\r\nfn _iter_drop(\r\n    // An identifier for the iterator buffer to unregister / drop.\r\n    iter: ManuallyDrop<BufferIter>\r\n) -> u16;\r\n```\r\n\r\n## Appendix, `bindings.h`\r\n\r\n```c\r\n#include <stdarg.h>\r\n#include <stdbool.h>\r\n#include <stddef.h>\r\n#include <stdint.h>\r\n#include <stdlib.h>\r\n\r\ntypedef uint32_t Buffer;\r\ntypedef uint32_t BufferIter;\r\n\r\nvoid _console_log(\r\n    uint8_t level,\r\n    const uint8_t *target,\r\n    size_t target_len,\r\n    const uint8_t *filename,\r\n    size_t filename_len,\r\n    uint32_t line_number,\r\n    const uint8_t *text,\r\n    size_t text_len\r\n);\r\n\r\n\r\nBuffer _buffer_alloc(\r\n    const uint8_t *data,\r\n    size_t data_len\r\n);\r\nvoid _buffer_consume(\r\n    Buffer bufh,\r\n    uint8_t *into,\r\n    size_t len\r\n);\r\nsize_t _buffer_len(Buffer bufh);\r\n\r\n\r\nvoid _schedule_reducer(\r\n    const uint8_t *name,\r\n    size_t name_len,\r\n    const uint8_t *args,\r\n    size_t args_len,\r\n    uint64_t time,\r\n    uint64_t *out\r\n);\r\nvoid _cancel_reducer(uint64_t id);\r\n\r\n\r\nuint16_t _create_index(\r\n    const uint8_t *index_name,\r\n    size_t index_name_len,\r\n    uint32_t table_id,\r\n    uint8_t index_type,\r\n    const uint8_t *col_ids,\r\n    size_t col_len\r\n);\r\n\r\n\r\nuint16_t _insert(\r\n    uint32_t table_id,\r\n    uint8_t *row,\r\n    size_t row_len\r\n);\r\nuint16_t _delete_by_col_eq(\r\n    uint32_t table_id,\r\n    uint32_t col_id,\r\n    const uint8_t *value,\r\n    size_t value_len,\r\n    uint32_t *out\r\n);\r\n\r\n\r\nuint16_t _get_table_id(\r\n    const uint8_t *name,\r\n    size_t name_len,\r\n    uint32_t *out\r\n);\r\nuint16_t _iter_by_col_eq(\r\n    uint32_t table_id,\r\n    uint32_t col_id,\r\n    const uint8_t *value,\r\n    size_t value_len,\r\n    Buffer *out\r\n);\r\nuint16_t _iter_drop(BufferIter iter);\r\nuint16_t _iter_next(BufferIter iter, Buffer *out);\r\nuint16_t _iter_start(uint32_t table_id, BufferIter *out);\r\nuint16_t _iter_start_filtered(\r\n    uint32_t table_id,\r\n    const uint8_t *filter,\r\n    size_t filter_len,\r\n    BufferIter *out\r\n);\r\n```\r\n\r\n[`bindings_sys::raw`]: https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/bindings-sys/src/lib.rs#L44-L215\r\n[`bindings`]: https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/bindings/src/lib.rs\r\n[module_ref]: /docs/languages/rust/rust-module-reference\r\n[module_quick_start]: /docs/languages/rust/rust-module-quick-start\r\n[wasm_c_abi]: https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md\r\n[c_header]: #appendix-bindingsh\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "Module ABI Reference",
              "route": "module-abi-reference",
              "depth": 1
            },
            {
              "title": "General notes",
              "route": "general-notes",
              "depth": 2
            },
            {
              "title": "Logging",
              "route": "logging",
              "depth": 2
            },
            {
              "title": "Buffer handling",
              "route": "buffer-handling",
              "depth": 2
            },
            {
              "title": "Reducer scheduling",
              "route": "reducer-scheduling",
              "depth": 2
            },
            {
              "title": "Altering tables",
              "route": "altering-tables",
              "depth": 2
            },
            {
              "title": "Inserting and deleting rows",
              "route": "inserting-and-deleting-rows",
              "depth": 2
            },
            {
              "title": "Querying tables",
              "route": "querying-tables",
              "depth": 2
            },
            {
              "title": "Appendix, `bindings.h`",
              "route": "appendix-bindings-h-",
              "depth": 2
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "Client SDK Languages",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "HTTP API Reference",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "HTTP API Reference",
      "identifier": "HTTP API Reference",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "HTTP%20API%20Reference/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "`/database` HTTP API",
          "identifier": "Databases",
          "indexIdentifier": "Databases",
          "hasPages": false,
          "content": "# `/database` HTTP API\r\n\r\nThe HTTP endpoints in `/database` allow clients to interact with Spacetime databases in a variety of ways, including retrieving information, creating and deleting databases, invoking reducers and evaluating SQL queries.\r\n\r\n## At a glance\r\n\r\n| Route                                                                                                               | Description                                                       |\r\n| ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |\r\n| [`/database/dns/:name GET`](#databasednsname-get)                                                                   | Look up a database's address by its name.                         |\r\n| [`/database/reverse_dns/:address GET`](#databasereverse_dnsaddress-get)                                             | Look up a database's name by its address.                         |\r\n| [`/database/set_name GET`](#databaseset_name-get)                                                                   | Set a database's name, given its address.                         |\r\n| [`/database/ping GET`](#databaseping-get)                                                                           | No-op. Used to determine whether a client can connect.            |\r\n| [`/database/register_tld GET`](#databaseregister_tld-get)                                                           | Register a top-level domain.                                      |\r\n| [`/database/request_recovery_code GET`](#databaserequest_recovery_code-get)                                         | Request a recovery code to the email associated with an identity. |\r\n| [`/database/confirm_recovery_code GET`](#databaseconfirm_recovery_code-get)                                         | Recover a login token from a recovery code.                       |\r\n| [`/database/publish POST`](#databasepublish-post)                                                                   | Publish a database given its module code.                         |\r\n| [`/database/delete/:address POST`](#databasedeleteaddress-post)                                                     | Delete a database.                                                |\r\n| [`/database/subscribe/:name_or_address GET`](#databasesubscribename_or_address-get)                                 | Begin a [WebSocket connection](/docs/websocket-api-reference).    |\r\n| [`/database/call/:name_or_address/:reducer POST`](#databasecallname_or_addressreducer-post)                         | Invoke a reducer in a database.                                   |\r\n| [`/database/schema/:name_or_address GET`](#databaseschemaname_or_address-get)                                       | Get the schema for a database.                                    |\r\n| [`/database/schema/:name_or_address/:entity_type/:entity GET`](#databaseschemaname_or_addressentity_typeentity-get) | Get a schema for a particular table or reducer.                   |\r\n| [`/database/info/:name_or_address GET`](#databaseinfoname_or_address-get)                                           | Get a JSON description of a database.                             |\r\n| [`/database/logs/:name_or_address GET`](#databaselogsname_or_address-get)                                           | Retrieve logs from a database.                                    |\r\n| [`/database/sql/:name_or_address POST`](#databasesqlname_or_address-post)                                           | Run a SQL query against a database.                               |\r\n\r\n## `/database/dns/:name GET`\r\n\r\nLook up a database's address by its name.\r\n\r\nAccessible through the CLI as `spacetime dns lookup <name>`.\r\n\r\n#### Parameters\r\n\r\n| Name    | Value                     |\r\n| ------- | ------------------------- |\r\n| `:name` | The name of the database. |\r\n\r\n#### Returns\r\n\r\nIf a database with that name exists, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"Success\": {\r\n    \"domain\": string,\r\n    \"address\": string\r\n} }\r\n```\r\n\r\nIf no database with that name exists, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"Failure\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\n## `/database/reverse_dns/:address GET`\r\n\r\nLook up a database's name by its address.\r\n\r\nAccessible through the CLI as `spacetime dns reverse-lookup <address>`.\r\n\r\n#### Parameters\r\n\r\n| Name       | Value                        |\r\n| ---------- | ---------------------------- |\r\n| `:address` | The address of the database. |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{ \"names\": array<string> }\r\n```\r\n\r\nwhere `<names>` is a JSON array of strings, each of which is a name which refers to the database.\r\n\r\n## `/database/set_name GET`\r\n\r\nSet the name associated with a database.\r\n\r\nAccessible through the CLI as `spacetime dns set-name <domain> <address>`.\r\n\r\n#### Query Parameters\r\n\r\n| Name           | Value                                                                     |\r\n| -------------- | ------------------------------------------------------------------------- |\r\n| `address`      | The address of the database to be named.                                  |\r\n| `domain`       | The name to register.                                                     |\r\n| `register_tld` | A boolean; whether to register the name as a TLD. Should usually be true. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Returns\r\n\r\nIf the name was successfully set, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"Success\": {\r\n    \"domain\": string,\r\n    \"address\": string\r\n} }\r\n```\r\n\r\nIf the top-level domain is not registered, and `register_tld` was not specified, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"TldNotRegistered\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\nIf the top-level domain is registered, but the identity provided in the `Authorization` header does not have permission to insert into it, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"PermissionDenied\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\n> Spacetime top-level domains are an upcoming feature, and are not fully implemented in SpacetimeDB 0.6. For now, database names should not contain slashes.\r\n\r\n## `/database/ping GET`\r\n\r\nDoes nothing and returns no data. Clients can send requests to this endpoint to determine whether they are able to connect to SpacetimeDB.\r\n\r\n## `/database/register_tld GET`\r\n\r\nRegister a new Spacetime top-level domain. A TLD is the part of a database name before the first `/`. For example, in the name `tyler/bitcraft`, the TLD is `tyler`. Each top-level domain is owned by at most one identity, and only the owner can publish databases with that TLD.\r\n\r\n> Spacetime top-level domains are an upcoming feature, and are not fully implemented in SpacetimeDB 0.6. For now, database names should not contain slashes.\r\n\r\nAccessible through the CLI as `spacetime dns register-tld <tld>`.\r\n\r\n#### Query Parameters\r\n\r\n| Name  | Value                                  |\r\n| ----- | -------------------------------------- |\r\n| `tld` | New top-level domain name to register. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Returns\r\n\r\nIf the domain is successfully registered, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"Success\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\nIf the domain is already registered to the caller, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"AlreadyRegistered\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\nIf the domain is already registered to another identity, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"Unauthorized\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\n## `/database/request_recovery_code GET`\r\n\r\nRequest a recovery code or link via email, in order to recover the token associated with an identity.\r\n\r\nAccessible through the CLI as `spacetime identity recover <email> <identity>`.\r\n\r\n#### Query Parameters\r\n\r\n| Name       | Value                                                                                                                                                                                                                                                                                                                 |\r\n| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `identity` | The identity whose token should be recovered.                                                                                                                                                                                                                                                                         |\r\n| `email`    | The email to send the recovery code or link to. This email must be associated with the identity, either during creation via [`/identity`](/docs/http-api-reference/identities#identity-post) or afterwards via [`/identity/:identity/set-email`](/docs/http-api-reference/identities#identityidentityset_email-post). |\r\n| `link`     | A boolean; whether to send a clickable link rather than a recovery code.                                                                                                                                                                                                                                              |\r\n\r\n## `/database/confirm_recovery_code GET`\r\n\r\nConfirm a recovery code received via email following a [`/database/request_recovery_code GET`](#-database-request_recovery_code-get) request, and retrieve the identity's token.\r\n\r\nAccessible through the CLI as `spacetime identity recover <email> <identity>`.\r\n\r\n#### Query Parameters\r\n\r\n| Name       | Value                                         |\r\n| ---------- | --------------------------------------------- |\r\n| `identity` | The identity whose token should be recovered. |\r\n| `email`    | The email which received the recovery code.   |\r\n| `code`     | The recovery code received via email.         |\r\n\r\nOn success, returns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"identity\": string,\r\n    \"token\": string\r\n}\r\n```\r\n\r\n## `/database/publish POST`\r\n\r\nPublish a database.\r\n\r\nAccessible through the CLI as `spacetime publish`.\r\n\r\n#### Query Parameters\r\n\r\n| Name              | Value                                                                                            |\r\n| ----------------- | ------------------------------------------------------------------------------------------------ |\r\n| `host_type`       | Optional; a SpacetimeDB module host type. Currently, only `\"wasmer\"` is supported.               |\r\n| `clear`           | A boolean; whether to clear any existing data when updating an existing database.                |\r\n| `name_or_address` | The name of the database to publish or update, or the address of an existing database to update. |\r\n| `register_tld`    | A boolean; whether to register the database's top-level domain.                                  |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Data\r\n\r\nA WebAssembly module in the [binary format](https://webassembly.github.io/spec/core/binary/index.html).\r\n\r\n#### Returns\r\n\r\nIf the database was successfully published, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"Success\": {\r\n    \"domain\": null | string,\r\n    \"address\": string,\r\n    \"op\": \"created\" | \"updated\"\r\n} }\r\n```\r\n\r\nIf the top-level domain for the requested name is not registered, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"TldNotRegistered\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\nIf the top-level domain for the requested name is registered, but the identity provided in the `Authorization` header does not have permission to insert into it, returns JSON in the form:\r\n\r\n```typescript\r\n{ \"PermissionDenied\": {\r\n    \"domain\": string\r\n} }\r\n```\r\n\r\n> Spacetime top-level domains are an upcoming feature, and are not fully implemented in SpacetimeDB 0.6. For now, database names should not contain slashes.\r\n\r\n## `/database/delete/:address POST`\r\n\r\nDelete a database.\r\n\r\nAccessible through the CLI as `spacetime delete <address>`.\r\n\r\n#### Parameters\r\n\r\n| Name       | Address                      |\r\n| ---------- | ---------------------------- |\r\n| `:address` | The address of the database. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n## `/database/subscribe/:name_or_address GET`\r\n\r\nBegin a [WebSocket connection](/docs/websocket-api-reference) with a database.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                        |\r\n| ------------------ | ---------------------------- |\r\n| `:name_or_address` | The address of the database. |\r\n\r\n#### Required Headers\r\n\r\nFor more information about WebSocket headers, see [RFC 6455](https://datatracker.ietf.org/doc/html/rfc6455).\r\n\r\n| Name                     | Value                                                                                                                                          |\r\n| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `Sec-WebSocket-Protocol` | [`v1.bin.spacetimedb`](/docs/websocket-api-reference#binary-protocol) or [`v1.text.spacetimedb`](/docs/websocket-api-reference#text-protocol). |\r\n| `Connection`             | `Updgrade`                                                                                                                                     |\r\n| `Upgrade`                | `websocket`                                                                                                                                    |\r\n| `Sec-WebSocket-Version`  | `13`                                                                                                                                           |\r\n| `Sec-WebSocket-Key`      | A 16-byte value, generated randomly by the client, encoded as Base64.                                                                          |\r\n\r\n#### Optional Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n## `/database/call/:name_or_address/:reducer POST`\r\n\r\nInvoke a reducer in a database.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                                |\r\n| ------------------ | ------------------------------------ |\r\n| `:name_or_address` | The name or address of the database. |\r\n| `:reducer`         | The name of the reducer.             |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Data\r\n\r\nA JSON array of arguments to the reducer.\r\n\r\n## `/database/schema/:name_or_address GET`\r\n\r\nGet a schema for a database.\r\n\r\nAccessible through the CLI as `spacetime describe <name_or_address>`.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                                |\r\n| ------------------ | ------------------------------------ |\r\n| `:name_or_address` | The name or address of the database. |\r\n\r\n#### Query Parameters\r\n\r\n| Name     | Value                                                       |\r\n| -------- | ----------------------------------------------------------- |\r\n| `expand` | A boolean; whether to include full schemas for each entity. |\r\n\r\n#### Returns\r\n\r\nReturns a JSON object with two properties, `\"entities\"` and `\"typespace\"`. For example, on the default module generated by `spacetime init` with `expand=true`, returns:\r\n\r\n```typescript\r\n{\r\n  \"entities\": {\r\n    \"Person\": {\r\n      \"arity\": 1,\r\n      \"schema\": {\r\n        \"elements\": [\r\n          {\r\n            \"algebraic_type\": {\r\n              \"Builtin\": {\r\n                \"String\": []\r\n              }\r\n            },\r\n            \"name\": {\r\n              \"some\": \"name\"\r\n            }\r\n          }\r\n        ]\r\n      },\r\n      \"type\": \"table\"\r\n    },\r\n    \"__init__\": {\r\n      \"arity\": 0,\r\n      \"schema\": {\r\n        \"elements\": [],\r\n        \"name\": \"__init__\"\r\n      },\r\n      \"type\": \"reducer\"\r\n    },\r\n    \"add\": {\r\n      \"arity\": 1,\r\n      \"schema\": {\r\n        \"elements\": [\r\n          {\r\n            \"algebraic_type\": {\r\n              \"Builtin\": {\r\n                \"String\": []\r\n              }\r\n            },\r\n            \"name\": {\r\n              \"some\": \"name\"\r\n            }\r\n          }\r\n        ],\r\n        \"name\": \"add\"\r\n      },\r\n      \"type\": \"reducer\"\r\n    },\r\n    \"say_hello\": {\r\n      \"arity\": 0,\r\n      \"schema\": {\r\n        \"elements\": [],\r\n        \"name\": \"say_hello\"\r\n      },\r\n      \"type\": \"reducer\"\r\n    }\r\n  },\r\n  \"typespace\": [\r\n    {\r\n      \"Product\": {\r\n        \"elements\": [\r\n          {\r\n            \"algebraic_type\": {\r\n              \"Builtin\": {\r\n                \"String\": []\r\n              }\r\n            },\r\n            \"name\": {\r\n              \"some\": \"name\"\r\n            }\r\n          }\r\n        ]\r\n      }\r\n    }\r\n  ]\r\n}\r\n```\r\n\r\nThe `\"entities\"` will be an object whose keys are table and reducer names, and whose values are objects of the form:\r\n\r\n```typescript\r\n{\r\n    \"arity\": number,\r\n    \"type\": \"table\" | \"reducer\",\r\n    \"schema\"?: ProductType\r\n}\r\n```\r\n\r\n| Entity field | Value                                                                                                                                                                                            |\r\n| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `arity`      | For tables, the number of colums; for reducers, the number of arguments.                                                                                                                         |\r\n| `type`       | For tables, `\"table\"`; for reducers, `\"reducer\"`.                                                                                                                                                |\r\n| `schema`     | A [JSON-encoded `ProductType`](/docs/satn-reference/satn-reference-json-format); for tables, the table schema; for reducers, the argument schema. Only present if `expand` is supplied and true. |\r\n\r\nThe `\"typespace\"` will be a JSON array of [`AlgebraicType`s](/docs/satn-reference/satn-reference-json-format) referenced by the module. This can be used to resolve `Ref` types within the schema; the type `{ \"Ref\": n }` refers to `response[\"typespace\"][n]`.\r\n\r\n## `/database/schema/:name_or_address/:entity_type/:entity GET`\r\n\r\nGet a schema for a particular table or reducer in a database.\r\n\r\nAccessible through the CLI as `spacetime describe <name_or_address> <entity_type> <entity_name>`.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                                                            |\r\n| ------------------ | ---------------------------------------------------------------- |\r\n| `:name_or_address` | The name or address of the database.                             |\r\n| `:entity_type`     | `reducer` to describe a reducer, or `table` to describe a table. |\r\n| `:entity`          | The name of the reducer or table.                                |\r\n\r\n#### Query Parameters\r\n\r\n| Name     | Value                                                         |\r\n| -------- | ------------------------------------------------------------- |\r\n| `expand` | A boolean; whether to include the full schema for the entity. |\r\n\r\n#### Returns\r\n\r\nReturns a single entity in the same format as in the `\"entities\"` returned by [the `/database/schema/:name_or_address GET` endpoint](#databaseschemaname_or_address-get):\r\n\r\n```typescript\r\n{\r\n    \"arity\": number,\r\n    \"type\": \"table\" | \"reducer\",\r\n    \"schema\"?: ProductType,\r\n}\r\n```\r\n\r\n| Field    | Value                                                                                                                                                                                            |\r\n| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `arity`  | For tables, the number of colums; for reducers, the number of arguments.                                                                                                                         |\r\n| `type`   | For tables, `\"table\"`; for reducers, `\"reducer\"`.                                                                                                                                                |\r\n| `schema` | A [JSON-encoded `ProductType`](/docs/satn-reference/satn-reference-json-format); for tables, the table schema; for reducers, the argument schema. Only present if `expand` is supplied and true. |\r\n\r\n## `/database/info/:name_or_address GET`\r\n\r\nGet a database's address, owner identity, host type, number of replicas and a hash of its WASM module.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                                |\r\n| ------------------ | ------------------------------------ |\r\n| `:name_or_address` | The name or address of the database. |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"address\": string,\r\n    \"identity\": string,\r\n    \"host_type\": \"wasmer\",\r\n    \"num_replicas\": number,\r\n    \"program_bytes_address\": string\r\n}\r\n```\r\n\r\n| Field                     | Type   | Meaning                                                     |\r\n| ------------------------- | ------ | ----------------------------------------------------------- |\r\n| `\"address\"`               | String | The address of the database.                                |\r\n| `\"identity\"`              | String | The Spacetime identity of the database's owner.             |\r\n| `\"host_type\"`             | String | The module host type; currently always `\"wasmer\"`.          |\r\n| `\"num_replicas\"`          | Number | The number of replicas of the database. Currently always 1. |\r\n| `\"program_bytes_address\"` | String | Hash of the WASM module for the database.                   |\r\n\r\n## `/database/logs/:name_or_address GET`\r\n\r\nRetrieve logs from a database.\r\n\r\nAccessible through the CLI as `spacetime logs <name_or_address>`.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                                |\r\n| ------------------ | ------------------------------------ |\r\n| `:name_or_address` | The name or address of the database. |\r\n\r\n#### Query Parameters\r\n\r\n| Name        | Value                                                           |\r\n| ----------- | --------------------------------------------------------------- |\r\n| `num_lines` | Number of most-recent log lines to retrieve.                    |\r\n| `follow`    | A boolean; whether to continue receiving new logs via a stream. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Returns\r\n\r\nText, or streaming text if `follow` is supplied, containing log lines.\r\n\r\n## `/database/sql/:name_or_address POST`\r\n\r\nRun a SQL query against a database.\r\n\r\nAccessible through the CLI as `spacetime sql <name_or_address> <query>`.\r\n\r\n#### Parameters\r\n\r\n| Name               | Value                                         |\r\n| ------------------ | --------------------------------------------- |\r\n| `:name_or_address` | The name or address of the database to query. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Data\r\n\r\nSQL queries, separated by `;`.\r\n\r\n#### Returns\r\n\r\nReturns a JSON array of statement results, each of which takes the form:\r\n\r\n```typescript\r\n{\r\n    \"schema\": ProductType,\r\n    \"rows\": array\r\n}\r\n```\r\n\r\nThe `schema` will be a [JSON-encoded `ProductType`](/docs/satn-reference/satn-reference-json-format) describing the type of the returned rows.\r\n\r\nThe `rows` will be an array of [JSON-encoded `ProductValue`s](/docs/satn-reference/satn-reference-json-format), each of which conforms to the `schema`.\r\n",
          "editUrl": "Databases.md",
          "jumpLinks": [
            {
              "title": "`/database` HTTP API",
              "route": "-database-http-api",
              "depth": 1
            },
            {
              "title": "At a glance",
              "route": "at-a-glance",
              "depth": 2
            },
            {
              "title": "`/database/dns/:name GET`",
              "route": "-database-dns-name-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/reverse_dns/:address GET`",
              "route": "-database-reverse_dns-address-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/set_name GET`",
              "route": "-database-set_name-get-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/ping GET`",
              "route": "-database-ping-get-",
              "depth": 2
            },
            {
              "title": "`/database/register_tld GET`",
              "route": "-database-register_tld-get-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/request_recovery_code GET`",
              "route": "-database-request_recovery_code-get-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "`/database/confirm_recovery_code GET`",
              "route": "-database-confirm_recovery_code-get-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "`/database/publish POST`",
              "route": "-database-publish-post-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Data",
              "route": "data",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/delete/:address POST`",
              "route": "-database-delete-address-post-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "`/database/subscribe/:name_or_address GET`",
              "route": "-database-subscribe-name_or_address-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Optional Headers",
              "route": "optional-headers",
              "depth": 4
            },
            {
              "title": "`/database/call/:name_or_address/:reducer POST`",
              "route": "-database-call-name_or_address-reducer-post-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Data",
              "route": "data",
              "depth": 4
            },
            {
              "title": "`/database/schema/:name_or_address GET`",
              "route": "-database-schema-name_or_address-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/schema/:name_or_address/:entity_type/:entity GET`",
              "route": "-database-schema-name_or_address-entity_type-entity-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/info/:name_or_address GET`",
              "route": "-database-info-name_or_address-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/logs/:name_or_address GET`",
              "route": "-database-logs-name_or_address-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/database/sql/:name_or_address POST`",
              "route": "-database-sql-name_or_address-post-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Data",
              "route": "data",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            }
          ],
          "pages": []
        },
        {
          "title": "`/energy` HTTP API",
          "identifier": "Energy",
          "indexIdentifier": "Energy",
          "hasPages": false,
          "content": "# `/energy` HTTP API\r\n\r\nThe HTTP endpoints in `/energy` allow clients to query identities' energy balances. Spacetime databases expend energy from their owners' balances while executing reducers.\r\n\r\n## At a glance\r\n\r\n| Route                                            | Description                                               |\r\n| ------------------------------------------------ | --------------------------------------------------------- |\r\n| [`/energy/:identity GET`](#energyidentity-get)   | Get the remaining energy balance for the user `identity`. |\r\n| [`/energy/:identity POST`](#energyidentity-post) | Set the energy balance for the user `identity`.           |\r\n\r\n## `/energy/:identity GET`\r\n\r\nGet the energy balance of an identity.\r\n\r\nAccessible through the CLI as `spacetime energy status <identity>`.\r\n\r\n#### Parameters\r\n\r\n| Name        | Value                   |\r\n| ----------- | ----------------------- |\r\n| `:identity` | The Spacetime identity. |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"balance\": string\r\n}\r\n```\r\n\r\n| Field     | Value                                                                                                                                                          |\r\n| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `balance` | The identity's energy balance, as a decimal integer. Note that energy balances may be negative, and will frequently be too large to store in a 64-bit integer. |\r\n\r\n## `/energy/:identity POST`\r\n\r\nSet the energy balance for an identity.\r\n\r\nNote that in the SpacetimeDB 0.6 Testnet, this endpoint always returns code 401, `UNAUTHORIZED`. Testnet energy balances cannot be refilled.\r\n\r\nAccessible through the CLI as `spacetime energy set-balance <balance> <identity>`.\r\n\r\n#### Parameters\r\n\r\n| Name        | Value                   |\r\n| ----------- | ----------------------- |\r\n| `:identity` | The Spacetime identity. |\r\n\r\n#### Query Parameters\r\n\r\n| Name      | Value                                      |\r\n| --------- | ------------------------------------------ |\r\n| `balance` | A decimal integer; the new balance to set. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"balance\": number\r\n}\r\n```\r\n\r\n| Field     | Value                                                                                                                                                              |\r\n| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |\r\n| `balance` | The identity's new energy balance, as a decimal integer. Note that energy balances may be negative, and will frequently be too large to store in a 64-bit integer. |\r\n",
          "editUrl": "Energy.md",
          "jumpLinks": [
            {
              "title": "`/energy` HTTP API",
              "route": "-energy-http-api",
              "depth": 1
            },
            {
              "title": "At a glance",
              "route": "at-a-glance",
              "depth": 2
            },
            {
              "title": "`/energy/:identity GET`",
              "route": "-energy-identity-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/energy/:identity POST`",
              "route": "-energy-identity-post-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            }
          ],
          "pages": []
        },
        {
          "title": "`/identity` HTTP API",
          "identifier": "Identities",
          "indexIdentifier": "Identities",
          "hasPages": false,
          "content": "# `/identity` HTTP API\r\n\r\nThe HTTP endpoints in `/identity` allow clients to generate and manage Spacetime public identities and private tokens.\r\n\r\n## At a glance\r\n\r\n| Route                                                                   | Description                                                        |\r\n| ----------------------------------------------------------------------- | ------------------------------------------------------------------ |\r\n| [`/identity GET`](#identity-get)                                        | Look up an identity by email.                                      |\r\n| [`/identity POST`](#identity-post)                                      | Generate a new identity and token.                                 |\r\n| [`/identity/websocket_token POST`](#identitywebsocket_token-post)       | Generate a short-lived access token for use in untrusted contexts. |\r\n| [`/identity/:identity/set-email POST`](#identityidentityset-email-post) | Set the email for an identity.                                     |\r\n| [`/identity/:identity/databases GET`](#identityidentitydatabases-get)   | List databases owned by an identity.                               |\r\n| [`/identity/:identity/verify GET`](#identityidentityverify-get)         | Verify an identity and token.                                      |\r\n\r\n## `/identity GET`\r\n\r\nLook up Spacetime identities associated with an email.\r\n\r\nAccessible through the CLI as `spacetime identity find <email>`.\r\n\r\n#### Query Parameters\r\n\r\n| Name    | Value                           |\r\n| ------- | ------------------------------- |\r\n| `email` | An email address to search for. |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"identities\": [\r\n        {\r\n            \"identity\": string,\r\n            \"email\": string\r\n        }\r\n    ]\r\n}\r\n```\r\n\r\nThe `identities` value is an array of zero or more objects, each of which has an `identity` and an `email`. Each `email` will be the same as the email passed as a query parameter.\r\n\r\n## `/identity POST`\r\n\r\nCreate a new identity.\r\n\r\nAccessible through the CLI as `spacetime identity new`.\r\n\r\n#### Query Parameters\r\n\r\n| Name    | Value                                                                                                                   |\r\n| ------- | ----------------------------------------------------------------------------------------------------------------------- |\r\n| `email` | An email address to associate with the new identity. If unsupplied, the new identity will not have an associated email. |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"identity\": string,\r\n    \"token\": string\r\n}\r\n```\r\n\r\n## `/identity/websocket_token POST`\r\n\r\nGenerate a short-lived access token which can be used in untrusted contexts, e.g. embedded in URLs.\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"token\": string\r\n}\r\n```\r\n\r\nThe `token` value is a short-lived [JSON Web Token](https://datatracker.ietf.org/doc/html/rfc7519).\r\n\r\n## `/identity/:identity/set-email POST`\r\n\r\nAssociate an email with a Spacetime identity.\r\n\r\nAccessible through the CLI as `spacetime identity set-email <identity> <email>`.\r\n\r\n#### Parameters\r\n\r\n| Name        | Value                                     |\r\n| ----------- | ----------------------------------------- |\r\n| `:identity` | The identity to associate with the email. |\r\n\r\n#### Query Parameters\r\n\r\n| Name    | Value             |\r\n| ------- | ----------------- |\r\n| `email` | An email address. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n## `/identity/:identity/databases GET`\r\n\r\nList all databases owned by an identity.\r\n\r\n#### Parameters\r\n\r\n| Name        | Value                 |\r\n| ----------- | --------------------- |\r\n| `:identity` | A Spacetime identity. |\r\n\r\n#### Returns\r\n\r\nReturns JSON in the form:\r\n\r\n```typescript\r\n{\r\n    \"addresses\": array<string>\r\n}\r\n```\r\n\r\nThe `addresses` value is an array of zero or more strings, each of which is the address of a database owned by the identity passed as a parameter.\r\n\r\n## `/identity/:identity/verify GET`\r\n\r\nVerify the validity of an identity/token pair.\r\n\r\n#### Parameters\r\n\r\n| Name        | Value                   |\r\n| ----------- | ----------------------- |\r\n| `:identity` | The identity to verify. |\r\n\r\n#### Required Headers\r\n\r\n| Name            | Value                                                                                       |\r\n| --------------- | ------------------------------------------------------------------------------------------- |\r\n| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http-api-reference/authorization). |\r\n\r\n#### Returns\r\n\r\nReturns no data.\r\n\r\nIf the token is valid and matches the identity, returns `204 No Content`.\r\n\r\nIf the token is valid but does not match the identity, returns `400 Bad Request`.\r\n\r\nIf the token is invalid, or no `Authorization` header is included in the request, returns `401 Unauthorized`.\r\n",
          "editUrl": "Identities.md",
          "jumpLinks": [
            {
              "title": "`/identity` HTTP API",
              "route": "-identity-http-api",
              "depth": 1
            },
            {
              "title": "At a glance",
              "route": "at-a-glance",
              "depth": 2
            },
            {
              "title": "`/identity GET`",
              "route": "-identity-get-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/identity POST`",
              "route": "-identity-post-",
              "depth": 2
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/identity/websocket_token POST`",
              "route": "-identity-websocket_token-post-",
              "depth": 2
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/identity/:identity/set-email POST`",
              "route": "-identity-identity-set-email-post-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Query Parameters",
              "route": "query-parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "`/identity/:identity/databases GET`",
              "route": "-identity-identity-databases-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            },
            {
              "title": "`/identity/:identity/verify GET`",
              "route": "-identity-identity-verify-get-",
              "depth": 2
            },
            {
              "title": "Parameters",
              "route": "parameters",
              "depth": 4
            },
            {
              "title": "Required Headers",
              "route": "required-headers",
              "depth": 4
            },
            {
              "title": "Returns",
              "route": "returns",
              "depth": 4
            }
          ],
          "pages": []
        },
        {
          "title": "SpacetimeDB HTTP Authorization",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# SpacetimeDB HTTP Authorization\r\n\r\nRather than a password, each Spacetime identity is associated with a private token. These tokens are generated by SpacetimeDB when the corresponding identity is created, and cannot be changed.\r\n\r\n> Do not share your SpacetimeDB token with anyone, ever.\r\n\r\n### Generating identities and tokens\r\n\r\nClients can request a new identity and token via [the `/identity POST` HTTP endpoint](/docs/http-api-reference/identities#identity-post).\r\n\r\nAlternately, a new identity and token will be generated during an anonymous connection via the [WebSocket API](/docs/websocket-api-reference), and passed to the client as [an `IdentityToken` message](/docs/websocket-api-reference#identitytoken).\r\n\r\n### Encoding `Authorization` headers\r\n\r\nMany SpacetimeDB HTTP endpoints either require or optionally accept a token in the `Authorization` header. SpacetimeDB authorization headers use `Basic` authorization with the username `token` and the token as the password. Because Spacetime tokens are not passwords, and SpacetimeDB Cloud uses TLS, usual security concerns about HTTP `Basic` authorization do not apply.\r\n\r\nTo construct an appropriate `Authorization` header value for a `token`:\r\n\r\n1. Prepend the string `token:`.\r\n2. Base64-encode.\r\n3. Prepend the string `Basic `.\r\n\r\n#### Python\r\n\r\n```python\r\ndef auth_header_value(token):\r\n    username_and_password = f\"token:{token}\".encode(\"utf-8\")\r\n    base64_encoded = base64.b64encode(username_and_password).decode(\"utf-8\")\r\n    return f\"Basic {base64_encoded}\"\r\n```\r\n\r\n#### Rust\r\n\r\n```rust\r\nfn auth_header_value(token: &str) -> String {\r\n    let username_and_password = format!(\"token:{}\", token);\r\n    let base64_encoded = base64::prelude::BASE64_STANDARD.encode(username_and_password);\r\n    format!(\"Basic {}\", encoded)\r\n}\r\n```\r\n\r\n#### C#\r\n\r\n```csharp\r\npublic string AuthHeaderValue(string token)\r\n{\r\n    var username_and_password = Encoding.UTF8.GetBytes($\"token:{auth}\");\r\n    var base64_encoded = Convert.ToBase64String(username_and_password);\r\n    return \"Basic \" + base64_encoded;\r\n}\r\n```\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "SpacetimeDB HTTP Authorization",
              "route": "spacetimedb-http-authorization",
              "depth": 1
            },
            {
              "title": "Generating identities and tokens",
              "route": "generating-identities-and-tokens",
              "depth": 3
            },
            {
              "title": "Encoding `Authorization` headers",
              "route": "encoding-authorization-headers",
              "depth": 3
            },
            {
              "title": "Python",
              "route": "python",
              "depth": 4
            },
            {
              "title": "Rust",
              "route": "rust",
              "depth": 4
            },
            {
              "title": "C#",
              "route": "c-",
              "depth": 4
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "Module ABI Reference",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "SATN Reference",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "SATN Reference",
      "identifier": "SATN Reference",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "SATN%20Reference/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "SATN Binary Format (BSATN)",
          "identifier": "Binary Format",
          "indexIdentifier": "Binary Format",
          "hasPages": false,
          "content": "# SATN Binary Format (BSATN)\r\n\r\nThe Spacetime Algebraic Type Notation binary (BSATN) format defines\r\nhow Spacetime `AlgebraicValue`s and friends are encoded as byte strings.\r\n\r\nAlgebraic values and product values are BSATN-encoded for e.g.,\r\nmodule-host communication and for storing row data in the database.\r\n\r\n## Notes on notation\r\n\r\nIn this reference, we give a formal definition of the format.\r\nTo do this, we use inductive definitions, and define the following notation:\r\n\r\n- `bsatn(x)` denotes a function converting some value `x` to a list of bytes.\r\n- `a: B` means that `a` is of type `B`.\r\n- `Foo(x)` denotes extracting `x` out of some variant or type `Foo`.\r\n- `a ++ b` denotes concatenating two byte lists `a` and `b`.\r\n- `bsatn(A) = bsatn(B) | ... | bsatn(Z)` where `B` to `Z` are variants of `A`\r\n  means that `bsatn(A)` is defined as e.g.,\r\n  `bsatn(B)`, `bsatn(C)`, .., `bsatn(Z)` depending on what variant of `A` it was.\r\n- `[]` denotes the empty list of bytes.\r\n\r\n## Values\r\n\r\n### At a glance\r\n\r\n| Type             | Description                                                      |\r\n| ---------------- | ---------------------------------------------------------------- |\r\n| `AlgebraicValue` | A value whose type may be any [`AlgebraicType`](#algebraictype). |\r\n| `SumValue`       | A value whose type is a [`SumType`](#sumtype).                   |\r\n| `ProductValue`   | A value whose type is a [`ProductType`](#producttype).           |\r\n| `BuiltinValue`   | A value whose type is a [`BuiltinType`](#builtintype).           |\r\n\r\n### `AlgebraicValue`\r\n\r\nThe BSATN encoding of an `AlgebraicValue` defers to the encoding of each variant:\r\n\r\n```fsharp\r\nbsatn(AlgebraicValue) = bsatn(SumValue) | bsatn(ProductValue) | bsatn(BuiltinValue)\r\n```\r\n\r\n### `SumValue`\r\n\r\nAn instance of a [`SumType`](#sumtype).\r\n`SumValue`s are binary-encoded as `bsatn(tag) ++ bsatn(variant_data)`\r\nwhere `tag: u8` is an index into the [`SumType.variants`](#sumtype)\r\narray of the value's [`SumType`](#sumtype),\r\nand where `variant_data` is the data of the variant.\r\nFor variants holding no data, i.e., of some zero sized type,\r\n`bsatn(variant_data) = []`.\r\n\r\n### `ProductValue`\r\n\r\nAn instance of a [`ProductType`](#producttype).\r\n`ProductValue`s are binary encoded as:\r\n\r\n```fsharp\r\nbsatn(elems) = bsatn(elem_0) ++ .. ++ bsatn(elem_n)\r\n```\r\n\r\nField names are not encoded.\r\n\r\n### `BuiltinValue`\r\n\r\nAn instance of a [`BuiltinType`](#builtintype).\r\nThe BSATN encoding of `BuiltinValue`s defers to the encoding of each variant:\r\n\r\n```fsharp\r\nbsatn(BuiltinValue)\r\n    = bsatn(Bool)\r\n    | bsatn(U8) | bsatn(U16) | bsatn(U32) | bsatn(U64) | bsatn(U128)\r\n    | bsatn(I8) | bsatn(I16) | bsatn(I32) | bsatn(I64) | bsatn(I128)\r\n    | bsatn(F32) | bsatn(F64)\r\n    | bsatn(String)\r\n    | bsatn(Array)\r\n    | bsatn(Map)\r\n\r\nbsatn(Bool(b)) = bsatn(b as u8)\r\nbsatn(U8(x)) = [x]\r\nbsatn(U16(x: u16)) = to_little_endian_bytes(x)\r\nbsatn(U32(x: u32)) = to_little_endian_bytes(x)\r\nbsatn(U64(x: u64)) = to_little_endian_bytes(x)\r\nbsatn(U128(x: u128)) = to_little_endian_bytes(x)\r\nbsatn(I8(x: i8)) = to_little_endian_bytes(x)\r\nbsatn(I16(x: i16)) = to_little_endian_bytes(x)\r\nbsatn(I32(x: i32)) = to_little_endian_bytes(x)\r\nbsatn(I64(x: i64)) = to_little_endian_bytes(x)\r\nbsatn(I128(x: i128)) = to_little_endian_bytes(x)\r\nbsatn(F32(x: f32)) = bsatn(f32_to_raw_bits(x)) // lossless conversion\r\nbsatn(F64(x: f64)) = bsatn(f64_to_raw_bits(x)) // lossless conversion\r\nbsatn(String(s)) = bsatn(len(s) as u32) ++ bsatn(bytes(s))\r\nbsatn(Array(a)) = bsatn(len(a) as u32)\r\n               ++ bsatn(normalize(a)_0) ++ .. ++ bsatn(normalize(a)_n)\r\nbsatn(Map(map)) = bsatn(len(m) as u32)\r\n               ++ bsatn(key(map_0)) ++ bsatn(value(map_0))\r\n               ..\r\n               ++ bsatn(key(map_n)) ++ bsatn(value(map_n))\r\n```\r\n\r\nWhere\r\n\r\n- `f32_to_raw_bits(x)` is the raw transmute of `x: f32` to `u32`\r\n- `f64_to_raw_bits(x)` is the raw transmute of `x: f64` to `u64`\r\n- `normalize(a)` for `a: ArrayValue` converts `a` to a list of `AlgebraicValue`s\r\n- `key(map_i)` extracts the key of the `i`th entry of `map`\r\n- `value(map_i)` extracts the value of the `i`th entry of `map`\r\n\r\n## Types\r\n\r\nAll SATS types are BSATN-encoded by converting them to an `AlgebraicValue`,\r\nthen BSATN-encoding that meta-value.\r\n\r\nSee [the SATN JSON Format](/docs/satn-reference-json-format)\r\nfor more details of the conversion to meta values.\r\nNote that these meta values are converted to BSATN and _not JSON_.\r\n",
          "editUrl": "Binary%20Format.md",
          "jumpLinks": [
            {
              "title": "SATN Binary Format (BSATN)",
              "route": "satn-binary-format-bsatn-",
              "depth": 1
            },
            {
              "title": "Notes on notation",
              "route": "notes-on-notation",
              "depth": 2
            },
            {
              "title": "Values",
              "route": "values",
              "depth": 2
            },
            {
              "title": "At a glance",
              "route": "at-a-glance",
              "depth": 3
            },
            {
              "title": "`AlgebraicValue`",
              "route": "-algebraicvalue-",
              "depth": 3
            },
            {
              "title": "`SumValue`",
              "route": "-sumvalue-",
              "depth": 3
            },
            {
              "title": "`ProductValue`",
              "route": "-productvalue-",
              "depth": 3
            },
            {
              "title": "`BuiltinValue`",
              "route": "-builtinvalue-",
              "depth": 3
            },
            {
              "title": "Types",
              "route": "types",
              "depth": 2
            }
          ],
          "pages": []
        },
        {
          "title": "SATN JSON Format",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# SATN JSON Format\r\n\r\nThe Spacetime Algebraic Type Notation JSON format defines how Spacetime `AlgebraicType`s and `AlgebraicValue`s are encoded as JSON. Algebraic types and values are JSON-encoded for transport via the [HTTP Databases API](/docs/http-api-reference/databases) and the [WebSocket text protocol](/docs/websocket-api-reference#text-protocol).\r\n\r\n## Values\r\n\r\n### At a glance\r\n\r\n| Type             | Description                                                      |\r\n| ---------------- | ---------------------------------------------------------------- |\r\n| `AlgebraicValue` | A value whose type may be any [`AlgebraicType`](#algebraictype). |\r\n| `SumValue`       | A value whose type is a [`SumType`](#sumtype).                   |\r\n| `ProductValue`   | A value whose type is a [`ProductType`](#producttype).           |\r\n| `BuiltinValue`   | A value whose type is a [`BuiltinType`](#builtintype).           |\r\n|                  |                                                                  |\r\n\r\n### `AlgebraicValue`\r\n\r\n```json\r\nSumValue | ProductValue | BuiltinValue\r\n```\r\n\r\n### `SumValue`\r\n\r\nAn instance of a [`SumType`](#sumtype). `SumValue`s are encoded as a JSON object with a single key, a non-negative integer tag which identifies the variant. The value associated with this key is the variant data. Variants which hold no data will have an empty array as their value.\r\n\r\nThe tag is an index into the [`SumType.variants`](#sumtype) array of the value's [`SumType`](#sumtype).\r\n\r\n```json\r\n{\r\n    \"<tag>\": AlgebraicValue\r\n}\r\n```\r\n\r\n### `ProductValue`\r\n\r\nAn instance of a [`ProductType`](#producttype). `ProductValue`s are encoded as JSON arrays. Each element of the `ProductValue` array is of the type of the corresponding index in the [`ProductType.elements`](#productype) array of the value's [`ProductType`](#producttype).\r\n\r\n```json\r\narray<AlgebraicValue>\r\n```\r\n\r\n### `BuiltinValue`\r\n\r\nAn instance of a [`BuiltinType`](#builtintype). `BuiltinValue`s are encoded as JSON values of corresponding types.\r\n\r\n```json\r\nboolean | number | string | array<AlgebraicValue> | map<AlgebraicValue, AlgebraicValue>\r\n```\r\n\r\n| [`BuiltinType`](#builtintype) | JSON type                             |\r\n| ----------------------------- | ------------------------------------- |\r\n| `Bool`                        | `boolean`                             |\r\n| Integer types                 | `number`                              |\r\n| Float types                   | `number`                              |\r\n| `String`                      | `string`                              |\r\n| Array types                   | `array<AlgebraicValue>`               |\r\n| Map types                     | `map<AlgebraicValue, AlgebraicValue>` |\r\n\r\nAll SATS integer types are encoded as JSON `number`s, so values of 64-bit and 128-bit integer types may lose precision when encoding values larger than 2⁵².\r\n\r\n## Types\r\n\r\nAll SATS types are JSON-encoded by converting them to an `AlgebraicValue`, then JSON-encoding that meta-value.\r\n\r\n### At a glance\r\n\r\n| Type                                    | Description                                                                          |\r\n| --------------------------------------- | ------------------------------------------------------------------------------------ |\r\n| [`AlgebraicType`](#algebraictype)       | Any SATS type.                                                                       |\r\n| [`SumType`](#sumtype)                   | Sum types, i.e. tagged unions.                                                       |\r\n| [`ProductType`](#productype)            | Product types, i.e. structures.                                                      |\r\n| [`BuiltinType`](#builtintype)           | Built-in and primitive types, including booleans, numbers, strings, arrays and maps. |\r\n| [`AlgebraicTypeRef`](#algebraictyperef) | An indirect reference to a type, used to implement recursive types.                  |\r\n\r\n#### `AlgebraicType`\r\n\r\n`AlgebraicType` is the most general meta-type in the Spacetime Algebraic Type System. Any SATS type can be represented as an `AlgebraicType`. `AlgebraicType` is encoded as a tagged union, with variants for [`SumType`](#sumtype), [`ProductType`](#producttype), [`BuiltinType`](#builtintype) and [`AlgebraicTypeRef`](#algebraictyperef).\r\n\r\n```json\r\n{ \"Sum\": SumType }\r\n| { \"Product\": ProductType }\r\n| { \"Builtin\": BuiltinType }\r\n| { \"Ref\": AlgebraicTypeRef }\r\n```\r\n\r\n#### `SumType`\r\n\r\nThe meta-type `SumType` represents sum types, also called tagged unions or Rust `enum`s. A sum type has some number of variants, each of which has an `AlgebraicType` of variant data, and an optional string discriminant. For each instance, exactly one variant will be active. The instance will contain only that variant's data.\r\n\r\nA `SumType` with zero variants is called an empty type or never type because it is impossible to construct an instance.\r\n\r\nInstances of `SumType`s are [`SumValue`s](#sumvalue), and store a tag which identifies the active variant.\r\n\r\n```json\r\n// SumType:\r\n{\r\n    \"variants\": array<SumTypeVariant>,\r\n}\r\n\r\n// SumTypeVariant:\r\n{\r\n    \"algebraic_type\": AlgebraicType,\r\n    \"name\": { \"some\": string } | { \"none\": [] }\r\n}\r\n```\r\n\r\n### `ProductType`\r\n\r\nThe meta-type `ProductType` represents product types, also called structs or tuples. A product type has some number of fields, each of which has an `AlgebraicType` of field data, and an optional string field name. Each instance will contain data for all of the product type's fields.\r\n\r\nA `ProductType` with zero fields is called a unit type because it has a single instance, the unit, which is empty.\r\n\r\nInstances of `ProductType`s are [`ProductValue`s](#productvalue), and store an array of field data.\r\n\r\n```json\r\n// ProductType:\r\n{\r\n    \"elements\": array<ProductTypeElement>,\r\n}\r\n\r\n// ProductTypeElement:\r\n{\r\n    \"algebraic_type\": AlgebraicType,\r\n    \"name\": { \"some\": string } | { \"none\": [] }\r\n}\r\n```\r\n\r\n### `BuiltinType`\r\n\r\nThe meta-type `BuiltinType` represents SATS primitive types: booleans, integers, floating-point numbers, strings, arrays and maps. `BuiltinType` is encoded as a tagged union, with a variant for each SATS primitive type.\r\n\r\nSATS integer types are identified by their signedness and width in bits. SATS supports the same set of integer types as Rust, i.e. 8, 16, 32, 64 and 128-bit signed and unsigned integers.\r\n\r\nSATS floating-point number types are identified by their width in bits. SATS supports 32 and 64-bit floats, which correspond to [IEEE 754](https://en.wikipedia.org/wiki/IEEE_754) single- and double-precision binary floats, respectively.\r\n\r\nSATS array and map types are homogeneous, meaning that each array has a single element type to which all its elements must conform, and each map has a key type and a value type to which all of its keys and values must conform.\r\n\r\n```json\r\n{ \"Bool\": [] }\r\n| { \"I8\": [] }\r\n| { \"U8\": [] }\r\n| { \"I16\": [] }\r\n| { \"U16\": [] }\r\n| { \"I32\": [] }\r\n| { \"U32\": [] }\r\n| { \"I64\": [] }\r\n| { \"U64\": [] }\r\n| { \"I128\": [] }\r\n| { \"U128\": [] }\r\n| { \"F32\": [] }\r\n| { \"F64\": [] }\r\n| { \"String\": [] }\r\n| { \"Array\": AlgebraicType }\r\n| { \"Map\": {\r\n      \"key_ty\": AlgebraicType,\r\n      \"ty\": AlgebraicType,\r\n  } }\r\n```\r\n\r\n### `AlgebraicTypeRef`\r\n\r\n`AlgebraicTypeRef`s are JSON-encoded as non-negative integers. These are indices into a typespace, like the one returned by the [`/database/schema/:name_or_address GET` HTTP endpoint](/docs/http-api-reference/databases#databaseschemaname_or_address-get).\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "SATN JSON Format",
              "route": "satn-json-format",
              "depth": 1
            },
            {
              "title": "Values",
              "route": "values",
              "depth": 2
            },
            {
              "title": "At a glance",
              "route": "at-a-glance",
              "depth": 3
            },
            {
              "title": "`AlgebraicValue`",
              "route": "-algebraicvalue-",
              "depth": 3
            },
            {
              "title": "`SumValue`",
              "route": "-sumvalue-",
              "depth": 3
            },
            {
              "title": "`ProductValue`",
              "route": "-productvalue-",
              "depth": 3
            },
            {
              "title": "`BuiltinValue`",
              "route": "-builtinvalue-",
              "depth": 3
            },
            {
              "title": "Types",
              "route": "types",
              "depth": 2
            },
            {
              "title": "At a glance",
              "route": "at-a-glance",
              "depth": 3
            },
            {
              "title": "`AlgebraicType`",
              "route": "-algebraictype-",
              "depth": 4
            },
            {
              "title": "`SumType`",
              "route": "-sumtype-",
              "depth": 4
            },
            {
              "title": "`ProductType`",
              "route": "-producttype-",
              "depth": 3
            },
            {
              "title": "`BuiltinType`",
              "route": "-builtintype-",
              "depth": 3
            },
            {
              "title": "`AlgebraicTypeRef`",
              "route": "-algebraictyperef-",
              "depth": 3
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "HTTP API Reference",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "SQL Reference",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "SQL Reference",
      "identifier": "SQL Reference",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "SQL%20Reference/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "SQL Support",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# SQL Support\r\n\r\nSpacetimeDB supports a subset of SQL as a query language. Developers can evaluate SQL queries against a Spacetime database via the `spacetime sql` command-line tool and the [`/database/sql/:name_or_address POST` HTTP endpoint](/docs/http-api-reference/databases#databasesqlname_or_address-post). Client developers also write SQL queries when subscribing to events in the [WebSocket API](/docs/websocket-api-reference#subscribe) or via an SDK `subscribe` function.\r\n\r\nSpacetimeDB aims to support much of the [SQL 2016 standard](https://www.iso.org/standard/63555.html), and in particular aims to be compatible with [PostgreSQL](https://www.postgresql.org/).\r\n\r\nSpacetimeDB 0.6 implements a relatively small subset of SQL. Future SpacetimeDB versions will implement additional SQL features.\r\n\r\n## Types\r\n\r\n| Type                                          | Description                            |\r\n| --------------------------------------------- | -------------------------------------- |\r\n| [Nullable types](#nullable-types)             | Types which may not hold a value.      |\r\n| [Logic types](#logic-types)                   | Booleans, i.e. `true` and `false`.     |\r\n| [Integer types](#integer-types)               | Numbers without fractional components. |\r\n| [Floating-point types](#floating-point-types) | Numbers with fractional components.    |\r\n| [Text types](#text-types)                     | UTF-8 encoded text.                    |\r\n\r\n### Definition statements\r\n\r\n| Statement                     | Description                          |\r\n| ----------------------------- | ------------------------------------ |\r\n| [CREATE TABLE](#create-table) | Create a new table.                  |\r\n| [DROP TABLE](#drop-table)     | Remove a table, discarding all rows. |\r\n\r\n### Query statements\r\n\r\n| Statement         | Description                                                                                  |\r\n| ----------------- | -------------------------------------------------------------------------------------------- |\r\n| [FROM](#from)     | A source of data, like a table or a value.                                                   |\r\n| [JOIN](#join)     | Combine several data sources.                                                                |\r\n| [SELECT](#select) | Select specific rows and columns from a data source, and optionally compute a derived value. |\r\n| [DELETE](#delete) | Delete specific rows from a table.                                                           |\r\n| [INSERT](#insert) | Insert rows into a table.                                                                    |\r\n| [UPDATE](#update) | Update specific rows in a table.                                                             |\r\n\r\n## Data types\r\n\r\nSpacetimeDB is built on the Spacetime Algebraic Type System, or SATS. SATS is a richer, more expressive type system than the one included in the SQL language.\r\n\r\nBecause SATS is a richer type system than SQL, some SATS types cannot cleanly correspond to SQL types. In particular, the SpacetimeDB SQL interface is unable to construct or compare instances of product and sum types. As such, SpacetimeDB SQL must largely restrict themselves to interacting with columns of builtin types.\r\n\r\nMost SATS builtin types map cleanly to SQL types.\r\n\r\n### Nullable types\r\n\r\nSpacetimeDB types, by default, do not permit `NULL` as a value. Nullable types are encoded in SATS using a sum type which corresponds to [Rust's `Option`](https://doc.rust-lang.org/stable/std/option/enum.Option.html). In SQL, such types can be written by adding the constraint `NULL`, like `INT NULL`.\r\n\r\n### Logic types\r\n\r\n| SQL       | SATS   | Example         |\r\n| --------- | ------ | --------------- |\r\n| `BOOLEAN` | `Bool` | `true`, `false` |\r\n\r\n### Numeric types\r\n\r\n#### Integer types\r\n\r\nAn integer is a number without a fractional component.\r\n\r\nAdding the `UNSIGNED` constraint to an integer type allows only positive values. This allows representing a larger positive range without increasing the width of the integer.\r\n\r\n| SQL                 | SATS  | Example | Min    | Max   |\r\n| ------------------- | ----- | ------- | ------ | ----- |\r\n| `TINYINT`           | `I8`  | 1       | -(2⁷)  | 2⁷-1  |\r\n| `TINYINT UNSIGNED`  | `U8`  | 1       | 0      | 2⁸-1  |\r\n| `SMALLINT`          | `I16` | 1       | -(2¹⁵) | 2¹⁵-1 |\r\n| `SMALLINT UNSIGNED` | `U16` | 1       | 0      | 2¹⁶-1 |\r\n| `INT`, `INTEGER`    | `I32` | 1       | -(2³¹) | 2³¹-1 |\r\n| `INT UNSIGNED`      | `U32` | 1       | 0      | 2³²-1 |\r\n| `BIGINT`            | `I64` | 1       | -(2⁶³) | 2⁶³-1 |\r\n| `BIGINT UNSIGNED`   | `U64` | 1       | 0      | 2⁶⁴-1 |\r\n\r\n#### Floating-point types\r\n\r\nSpacetimeDB supports single- and double-precision [binary IEEE-754 floats](https://en.wikipedia.org/wiki/IEEE_754).\r\n\r\n| SQL               | SATS  | Example | Min                      | Max                     |\r\n| ----------------- | ----- | ------- | ------------------------ | ----------------------- |\r\n| `REAL`            | `F32` | 1.0     | -3.40282347E+38          | 3.40282347E+38          |\r\n| `DOUBLE`, `FLOAT` | `F64` | 1.0     | -1.7976931348623157E+308 | 1.7976931348623157E+308 |\r\n\r\n### Text types\r\n\r\nSpacetimeDB supports a single string type, `String`. SpacetimeDB strings are UTF-8 encoded.\r\n\r\n| SQL                                             | SATS     | Example | Notes                |\r\n| ----------------------------------------------- | -------- | ------- | -------------------- |\r\n| `CHAR`, `VARCHAR`, `NVARCHAR`, `TEXT`, `STRING` | `String` | 'hello' | Always UTF-8 encoded |\r\n\r\n> SpacetimeDB SQL currently does not support length contraints like `CHAR(10)`.\r\n\r\n## Syntax\r\n\r\n### Comments\r\n\r\nSQL line comments begin with `--`.\r\n\r\n```sql\r\n-- This is a comment\r\n```\r\n\r\n### Expressions\r\n\r\nWe can express different, composable, values that are universally called `expressions`.\r\n\r\nAn expression is one of the following:\r\n\r\n#### Literals\r\n\r\n| Example   | Description |\r\n| --------- | ----------- |\r\n| `1`       | An integer. |\r\n| `1.0`     | A float.    |\r\n| `'hello'` | A string.   |\r\n| `true`    | A boolean.  |\r\n\r\n#### Binary operators\r\n\r\n| Example | Description         |\r\n| ------- | ------------------- |\r\n| `1 > 2` | Integer comparison. |\r\n| `1 + 2` | Integer addition.   |\r\n\r\n#### Logical expressions\r\n\r\nAny expression which returns a boolean, i.e. `true` or `false`, is a logical expression.\r\n\r\n| Example          | Description                                                  |\r\n| ---------------- | ------------------------------------------------------------ |\r\n| `1 > 2`          | Integer comparison.                                          |\r\n| `1 + 2 == 3`     | Equality comparison between a constant and a computed value. |\r\n| `true AND false` | Boolean and.                                                 |\r\n| `true OR false`  | Boolean or.                                                  |\r\n| `NOT true`       | Boolean inverse.                                             |\r\n\r\n#### Function calls\r\n\r\n| Example         | Description                                        |\r\n| --------------- | -------------------------------------------------- |\r\n| `lower('JOHN')` | Apply the function `lower` to the string `'JOHN'`. |\r\n\r\n#### Table identifiers\r\n\r\n| Example       | Description               |\r\n| ------------- | ------------------------- |\r\n| `inventory`   | Refers to a table.        |\r\n| `\"inventory\"` | Refers to the same table. |\r\n\r\n#### Column references\r\n\r\n| Example                    | Description                                             |\r\n| -------------------------- | ------------------------------------------------------- |\r\n| `inventory_id`             | Refers to a column.                                     |\r\n| `\"inventory_id\"`           | Refers to the same column.                              |\r\n| `\"inventory.inventory_id\"` | Refers to the same column, explicitly naming its table. |\r\n\r\n#### Wildcards\r\n\r\nSpecial \"star\" expressions which select all the columns of a table.\r\n\r\n| Example       | Description                                             |\r\n| ------------- | ------------------------------------------------------- |\r\n| `*`           | Refers to all columns of a table identified by context. |\r\n| `inventory.*` | Refers to all columns of the `inventory` table.         |\r\n\r\n#### Parenthesized expressions\r\n\r\nSub-expressions can be enclosed in parentheses for grouping and to override operator precedence.\r\n\r\n| Example       | Description             |\r\n| ------------- | ----------------------- |\r\n| `1 + (2 / 3)` | One plus a fraction.    |\r\n| `(1 + 2) / 3` | A sum divided by three. |\r\n\r\n### `CREATE TABLE`\r\n\r\nA `CREATE TABLE` statement creates a new, initially empty table in the database.\r\n\r\nThe syntax of the `CREATE TABLE` statement is:\r\n\r\n> **CREATE TABLE** _table_name_ (_column_name_ _data_type_, ...);\r\n\r\n![create-table](/images/syntax/create_table.svg)\r\n\r\n#### Examples\r\n\r\nCreate a table `inventory` with two columns, an integer `inventory_id` and a string `name`:\r\n\r\n```sql\r\nCREATE TABLE inventory (inventory_id INTEGER, name TEXT);\r\n```\r\n\r\nCreate a table `player` with two integer columns, an `entity_id` and an `inventory_id`:\r\n\r\n```sql\r\nCREATE TABLE player (entity_id INTEGER, inventory_id INTEGER);\r\n```\r\n\r\nCreate a table `location` with three columns, an integer `entity_id` and floats `x` and `z`:\r\n\r\n```sql\r\nCREATE TABLE location (entity_id INTEGER, x REAL, z REAL);\r\n```\r\n\r\n### `DROP TABLE`\r\n\r\nA `DROP TABLE` statement removes a table from the database, deleting all its associated rows, indexes, constraints and sequences.\r\n\r\nTo empty a table of rows without destroying the table, use [`DELETE`](#delete).\r\n\r\nThe syntax of the `DROP TABLE` statement is:\r\n\r\n> **DROP TABLE** _table_name_;\r\n\r\n![drop-table](/images/syntax/drop_table.svg)\r\n\r\nExamples:\r\n\r\n```sql\r\nDROP TABLE inventory;\r\n```\r\n\r\n## Queries\r\n\r\n### `FROM`\r\n\r\nA `FROM` clause derives a data source from a table name.\r\n\r\nThe syntax of the `FROM` clause is:\r\n\r\n> **FROM** _table_name_ _join_clause_?;\r\n\r\n![from](/images/syntax/from.svg)\r\n\r\n#### Examples\r\n\r\nSelect all rows from the `inventory` table:\r\n\r\n```sql\r\nSELECT * FROM inventory;\r\n```\r\n\r\n### `JOIN`\r\n\r\nA `JOIN` clause combines two data sources into a new data source.\r\n\r\nCurrently, SpacetimeDB SQL supports only inner joins, which return rows from two data sources where the values of two columns match.\r\n\r\nThe syntax of the `JOIN` clause is:\r\n\r\n> **JOIN** _table_name_ **ON** _expr_ = _expr_;\r\n\r\n![join](/images/syntax/join.svg)\r\n\r\n### Examples\r\n\r\nSelect all players rows who have a corresponding location:\r\n\r\n```sql\r\nSELECT player.* FROM player\r\n JOIN location\r\n ON location.entity_id = player.entity_id;\r\n```\r\n\r\nSelect all inventories which have a corresponding player, and where that player has a corresponding location:\r\n\r\n```sql\r\nSELECT inventory.* FROM inventory\r\n JOIN player\r\n ON inventory.inventory_id = player.inventory_id\r\n JOIN location\r\n ON player.entity_id = location.entity_id;\r\n```\r\n\r\n### `SELECT`\r\n\r\nA `SELECT` statement returns values of particular columns from a data source, optionally filtering the data source to include only rows which satisfy a `WHERE` predicate.\r\n\r\nThe syntax of the `SELECT` command is:\r\n\r\n> **SELECT** _column_expr_ > **FROM** _from_expr_\r\n> {**WHERE** _expr_}?\r\n\r\n![sql-select](/images/syntax/select.svg)\r\n\r\n#### Examples\r\n\r\nSelect all columns of all rows from the `inventory` table:\r\n\r\n```sql\r\nSELECT * FROM inventory;\r\nSELECT inventory.* FROM inventory;\r\n```\r\n\r\nSelect only the `inventory_id` column of all rows from the `inventory` table:\r\n\r\n```sql\r\nSELECT inventory_id FROM inventory;\r\nSELECT inventory.inventory_id FROM inventory;\r\n```\r\n\r\nAn optional `WHERE` clause can be added to filter the data source using a [logical expression](#logical-expressions). The `SELECT` will return only the rows from the data source for which the expression returns `true`.\r\n\r\n#### Examples\r\n\r\nSelect all columns of all rows from the `inventory` table, with a filter that is always true:\r\n\r\n```sql\r\nSELECT * FROM inventory WHERE 1 = 1;\r\n```\r\n\r\nSelect all columns of all rows from the `inventory` table with the `inventory_id` 1:\r\n\r\n```sql\r\nSELECT * FROM inventory WHERE inventory_id = 1;\r\n```\r\n\r\nSelect only the `name` column of all rows from the `inventory` table with the `inventory_id` 1:\r\n\r\n```sql\r\nSELECT name FROM inventory WHERE inventory_id = 1;\r\n```\r\n\r\nSelect all columns of all rows from the `inventory` table where the `inventory_id` is 2 or greater:\r\n\r\n```sql\r\nSELECT * FROM inventory WHERE inventory_id > 1;\r\n```\r\n\r\n### `INSERT`\r\n\r\nAn `INSERT INTO` statement inserts new rows into a table.\r\n\r\nOne can insert one or more rows specified by value expressions.\r\n\r\nThe syntax of the `INSERT INTO` statement is:\r\n\r\n> **INSERT INTO** _table_name_ (_column_name_, ...) **VALUES** (_expr_, ...), ...;\r\n\r\n![sql-insert](/images/syntax/insert.svg)\r\n\r\n#### Examples\r\n\r\nInsert a single row:\r\n\r\n```sql\r\nINSERT INTO inventory (inventory_id, name) VALUES (1, 'health1');\r\n```\r\n\r\nInsert two rows:\r\n\r\n```sql\r\nINSERT INTO inventory (inventory_id, name) VALUES (1, 'health1'), (2, 'health2');\r\n```\r\n\r\n### UPDATE\r\n\r\nAn `UPDATE` statement changes the values of a set of specified columns in all rows of a table, optionally filtering the table to update only rows which satisfy a `WHERE` predicate.\r\n\r\nColumns not explicitly modified with the `SET` clause retain their previous values.\r\n\r\nIf the `WHERE` clause is absent, the effect is to update all rows in the table.\r\n\r\nThe syntax of the `UPDATE` statement is\r\n\r\n> **UPDATE** _table_name_ **SET** > _column_name_ = _expr_, ...\r\n> {_WHERE expr_}?;\r\n\r\n![sql-update](/images/syntax/update.svg)\r\n\r\n#### Examples\r\n\r\nSet the `name` column of all rows from the `inventory` table with the `inventory_id` 1 to `'new name'`:\r\n\r\n```sql\r\nUPDATE inventory\r\n  SET name = 'new name'\r\n  WHERE inventory_id = 1;\r\n```\r\n\r\n### DELETE\r\n\r\nA `DELETE` statement deletes rows that satisfy the `WHERE` clause from the specified table.\r\n\r\nIf the `WHERE` clause is absent, the effect is to delete all rows in the table. In that case, the result is a valid empty table.\r\n\r\nThe syntax of the `DELETE` statement is\r\n\r\n> **DELETE** _table_name_\r\n> {**WHERE** _expr_}?;\r\n\r\n![sql-delete](/images/syntax/delete.svg)\r\n\r\n#### Examples\r\n\r\nDelete all the rows from the `inventory` table with the `inventory_id` 1:\r\n\r\n```sql\r\nDELETE FROM inventory WHERE inventory_id = 1;\r\n```\r\n\r\nDelete all rows from the `inventory` table, leaving it empty:\r\n\r\n```sql\r\nDELETE FROM inventory;\r\n```\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "SQL Support",
              "route": "sql-support",
              "depth": 1
            },
            {
              "title": "Types",
              "route": "types",
              "depth": 2
            },
            {
              "title": "Definition statements",
              "route": "definition-statements",
              "depth": 3
            },
            {
              "title": "Query statements",
              "route": "query-statements",
              "depth": 3
            },
            {
              "title": "Data types",
              "route": "data-types",
              "depth": 2
            },
            {
              "title": "Nullable types",
              "route": "nullable-types",
              "depth": 3
            },
            {
              "title": "Logic types",
              "route": "logic-types",
              "depth": 3
            },
            {
              "title": "Numeric types",
              "route": "numeric-types",
              "depth": 3
            },
            {
              "title": "Integer types",
              "route": "integer-types",
              "depth": 4
            },
            {
              "title": "Floating-point types",
              "route": "floating-point-types",
              "depth": 4
            },
            {
              "title": "Text types",
              "route": "text-types",
              "depth": 3
            },
            {
              "title": "Syntax",
              "route": "syntax",
              "depth": 2
            },
            {
              "title": "Comments",
              "route": "comments",
              "depth": 3
            },
            {
              "title": "Expressions",
              "route": "expressions",
              "depth": 3
            },
            {
              "title": "Literals",
              "route": "literals",
              "depth": 4
            },
            {
              "title": "Binary operators",
              "route": "binary-operators",
              "depth": 4
            },
            {
              "title": "Logical expressions",
              "route": "logical-expressions",
              "depth": 4
            },
            {
              "title": "Function calls",
              "route": "function-calls",
              "depth": 4
            },
            {
              "title": "Table identifiers",
              "route": "table-identifiers",
              "depth": 4
            },
            {
              "title": "Column references",
              "route": "column-references",
              "depth": 4
            },
            {
              "title": "Wildcards",
              "route": "wildcards",
              "depth": 4
            },
            {
              "title": "Parenthesized expressions",
              "route": "parenthesized-expressions",
              "depth": 4
            },
            {
              "title": "`CREATE TABLE`",
              "route": "-create-table-",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            },
            {
              "title": "`DROP TABLE`",
              "route": "-drop-table-",
              "depth": 3
            },
            {
              "title": "Queries",
              "route": "queries",
              "depth": 2
            },
            {
              "title": "`FROM`",
              "route": "-from-",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            },
            {
              "title": "`JOIN`",
              "route": "-join-",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 3
            },
            {
              "title": "`SELECT`",
              "route": "-select-",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            },
            {
              "title": "`INSERT`",
              "route": "-insert-",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            },
            {
              "title": "UPDATE",
              "route": "update",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            },
            {
              "title": "DELETE",
              "route": "delete",
              "depth": 3
            },
            {
              "title": "Examples",
              "route": "examples",
              "depth": 4
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "SATN Reference",
        "route": "index",
        "depth": 1
      },
      "nextKey": {
        "title": "WebSocket API Reference",
        "route": "index",
        "depth": 1
      }
    },
    {
      "title": "WebSocket API Reference",
      "identifier": "WebSocket API Reference",
      "indexIdentifier": "index",
      "comingSoon": false,
      "hasPages": true,
      "editUrl": "WebSocket%20API%20Reference/index.md",
      "jumpLinks": [],
      "pages": [
        {
          "title": "The SpacetimeDB WebSocket API",
          "identifier": "index",
          "indexIdentifier": "index",
          "content": "# The SpacetimeDB WebSocket API\r\n\r\nAs an extension of the [HTTP API](/doc/http-api-reference), SpacetimeDB offers a WebSocket API. Clients can subscribe to a database via a WebSocket connection to receive streaming updates as the database changes, and send requests to invoke reducers. Messages received from the server over a WebSocket will follow the same total ordering of transactions as are committed to the database.\r\n\r\nThe SpacetimeDB SDKs comminicate with their corresponding database using the WebSocket API.\r\n\r\n## Connecting\r\n\r\nTo initiate a WebSocket connection, send a `GET` request to the [`/database/subscribe/:name_or_address` endpoint](/docs/http-api-reference/databases#databasesubscribename_or_address-get) with headers appropriate to upgrade to a WebSocket connection as per [RFC 6455](https://datatracker.ietf.org/doc/html/rfc6455).\r\n\r\nTo re-connect with an existing identity, include its token in a [SpacetimeDB Authorization header](/docs/http-api-reference/authorization). Otherwise, a new identity and token will be generated for the client.\r\n\r\n## Protocols\r\n\r\nClients connecting via WebSocket can choose between two protocols, [`v1.bin.spacetimedb`](#binary-protocol) and [`v1.text.spacetimedb`](#text-protocol). Clients should include one of these protocols in the `Sec-WebSocket-Protocol` header of their request.\r\n\r\n| `Sec-WebSocket-Protocol` header value | Selected protocol          |\r\n| ------------------------------------- | -------------------------- |\r\n| `v1.bin.spacetimedb`                  | [Binary](#binary-protocol) |\r\n| `v1.text.spacetimedb`                 | [Text](#text-protocol)     |\r\n\r\n### Binary Protocol\r\n\r\nThe SpacetimeDB binary WebSocket protocol, `v1.bin.spacetimedb`, encodes messages using [ProtoBuf 3](https://protobuf.dev), and reducer and row data using [BSATN](/docs/satn-reference/satn-reference-binary-format).\r\n\r\nThe binary protocol's messages are defined in [`client_api.proto`](https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/client-api-messages/protobuf/client_api.proto).\r\n\r\n### Text Protocol\r\n\r\nThe SpacetimeDB text WebSocket protocol, `v1.text.spacetimedb`, encodes messages, reducer and row data as JSON. Reducer arguments and table rows are JSON-encoded according to the [SATN JSON format](/docs/satn-reference/satn-reference-json-format).\r\n\r\n## Messages\r\n\r\n### Client to server\r\n\r\n| Message                         | Description                                                                 |\r\n| ------------------------------- | --------------------------------------------------------------------------- |\r\n| [`FunctionCall`](#functioncall) | Invoke a reducer.                                                           |\r\n| [`Subscribe`](#subscribe)       | Register queries to receive streaming updates for a subset of the database. |\r\n\r\n#### `FunctionCall`\r\n\r\nClients send a `FunctionCall` message to request that the database run a reducer. The message includes the reducer's name and a SATS `ProductValue` of arguments.\r\n\r\n##### Binary: ProtoBuf definition\r\n\r\n```protobuf\r\nmessage FunctionCall {\r\n    string reducer = 1;\r\n    bytes argBytes = 2;\r\n}\r\n```\r\n\r\n| Field      | Value                                                    |\r\n| ---------- | -------------------------------------------------------- |\r\n| `reducer`  | The name of the reducer to invoke.                       |\r\n| `argBytes` | The reducer arguments encoded as a BSATN `ProductValue`. |\r\n\r\n##### Text: JSON encoding\r\n\r\n```typescript\r\n{\r\n    \"call\": {\r\n        \"fn\": string,\r\n        \"args\": array,\r\n    }\r\n}\r\n```\r\n\r\n| Field  | Value                                          |\r\n| ------ | ---------------------------------------------- |\r\n| `fn`   | The name of the reducer to invoke.             |\r\n| `args` | The reducer arguments encoded as a JSON array. |\r\n\r\n#### `Subscribe`\r\n\r\nClients send a `Subscribe` message to register SQL queries in order to receive streaming updates.\r\n\r\nThe client will only receive [`TransactionUpdate`s](#transactionupdate) for rows to which it is subscribed, and for reducer runs which alter at least one subscribed row. As a special exception, the client is always notified when a reducer run it requests via a [`FunctionCall` message](#functioncall) fails.\r\n\r\nSpacetimeDB responds to each `Subscribe` message with a [`SubscriptionUpdate` message](#subscriptionupdate) containing all matching rows at the time the subscription is applied.\r\n\r\nEach `Subscribe` message establishes a new set of subscriptions, replacing all previous subscriptions. Clients which want to add a query to an existing subscription must send a `Subscribe` message containing all the previous queries in addition to the new query. In this case, the returned [`SubscriptionUpdate`](#subscriptionupdate) will contain all previously-subscribed rows in addition to the newly-subscribed rows.\r\n\r\nEach query must be a SQL `SELECT * FROM` statement on a single table with an optional `WHERE` clause. See the [SQL Reference](/docs/sql-reference) for the subset of SQL supported by SpacetimeDB.\r\n\r\n##### Binary: ProtoBuf definition\r\n\r\n```protobuf\r\nmessage Subscribe {\r\n    repeated string query_strings = 1;\r\n}\r\n```\r\n\r\n| Field           | Value                                                             |\r\n| --------------- | ----------------------------------------------------------------- |\r\n| `query_strings` | A sequence of strings, each of which contains a single SQL query. |\r\n\r\n##### Text: JSON encoding\r\n\r\n```typescript\r\n{\r\n    \"subscribe\": {\r\n        \"query_strings\": array<string>\r\n    }\r\n}\r\n```\r\n\r\n| Field           | Value                                                           |\r\n| --------------- | --------------------------------------------------------------- |\r\n| `query_strings` | An array of strings, each of which contains a single SQL query. |\r\n\r\n### Server to client\r\n\r\n| Message                                     | Description                                                                |\r\n| ------------------------------------------- | -------------------------------------------------------------------------- |\r\n| [`IdentityToken`](#identitytoken)           | Sent once upon successful connection with the client's identity and token. |\r\n| [`SubscriptionUpdate`](#subscriptionupdate) | Initial message in response to a [`Subscribe` message](#subscribe).        |\r\n| [`TransactionUpdate`](#transactionupdate)   | Streaming update after a reducer runs containing altered rows.             |\r\n\r\n#### `IdentityToken`\r\n\r\nUpon establishing a WebSocket connection, the server will send an `IdentityToken` message containing the client's identity and token. If the client included a [SpacetimeDB Authorization header](/docs/http-api-reference/authorization) in their connection request, the `IdentityToken` message will contain the same token used to connect, and its corresponding identity. If the client connected anonymously, SpacetimeDB will generate a new identity and token for the client.\r\n\r\n##### Binary: ProtoBuf definition\r\n\r\n```protobuf\r\nmessage IdentityToken {\r\n    bytes identity = 1;\r\n    string token = 2;\r\n}\r\n```\r\n\r\n| Field      | Value                                   |\r\n| ---------- | --------------------------------------- |\r\n| `identity` | The client's public Spacetime identity. |\r\n| `token`    | The client's private access token.      |\r\n\r\n##### Text: JSON encoding\r\n\r\n```typescript\r\n{\r\n    \"IdentityToken\": {\r\n        \"identity\": array<number>,\r\n        \"token\": string\r\n    }\r\n}\r\n```\r\n\r\n| Field      | Value                                   |\r\n| ---------- | --------------------------------------- |\r\n| `identity` | The client's public Spacetime identity. |\r\n| `token`    | The client's private access token.      |\r\n\r\n#### `SubscriptionUpdate`\r\n\r\nIn response to a [`Subscribe` message](#subscribe), the database sends a `SubscriptionUpdate` containing all of the matching rows which are resident in the database at the time the `Subscribe` was received.\r\n\r\n##### Binary: ProtoBuf definition\r\n\r\n```protobuf\r\nmessage SubscriptionUpdate {\r\n    repeated TableUpdate tableUpdates = 1;\r\n}\r\n\r\nmessage TableUpdate {\r\n    uint32 tableId = 1;\r\n    string tableName = 2;\r\n    repeated TableRowOperation tableRowOperations = 3;\r\n}\r\n\r\nmessage TableRowOperation {\r\n    enum OperationType {\r\n        DELETE = 0;\r\n        INSERT = 1;\r\n    }\r\n    OperationType op = 1;\r\n    bytes row_pk = 2;\r\n    bytes row = 3;\r\n}\r\n```\r\n\r\nEach `SubscriptionUpdate` contains a `TableUpdate` for each table with subscribed rows. Each `TableUpdate` contains a `TableRowOperation` for each subscribed row. `SubscriptionUpdate`, `TableUpdate` and `TableRowOperation` are also used by the [`TransactionUpdate` message](#transactionupdate) to encode rows altered by a reducer, so `TableRowOperation` includes an `OperationType` which identifies the row alteration as either an insert or a delete. When a client receives a `SubscriptionUpdate` message in response to a [`Subscribe` message](#subscribe), all of the `TableRowOperation`s will have `op` of `INSERT`.\r\n\r\n| `TableUpdate` field  | Value                                                                                                         |\r\n| -------------------- | ------------------------------------------------------------------------------------------------------------- |\r\n| `tableId`            | An integer identifier for the table. A table's `tableId` is not stable, so clients should not depend on it.   |\r\n| `tableName`          | The string name of the table. Clients should use this field to identify the table, rather than the `tableId`. |\r\n| `tableRowOperations` | A `TableRowOperation` for each inserted or deleted row.                                                       |\r\n\r\n| `TableRowOperation` field | Value                                                                                                                                                                                                      |\r\n| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `op`                      | `INSERT` for inserted rows during a [`TransactionUpdate`](#transactionupdate) or rows resident upon applying a subscription; `DELETE` for deleted rows during a [`TransactionUpdate`](#transactionupdate). |\r\n| `row_pk`                  | An opaque hash of the row computed by SpacetimeDB. Clients can use this hash to identify a previously `INSERT`ed row during a `DELETE`.                                                                    |\r\n| `row`                     | The altered row, encoded as a BSATN `ProductValue`.                                                                                                                                                        |\r\n\r\n##### Text: JSON encoding\r\n\r\n```typescript\r\n// SubscriptionUpdate:\r\n{\r\n    \"SubscriptionUpdate\": {\r\n        \"table_updates\": array<TableUpdate>\r\n    }\r\n}\r\n\r\n// TableUpdate:\r\n{\r\n    \"table_id\": number,\r\n    \"table_name\": string,\r\n    \"table_row_operations\": array<TableRowOperation>\r\n}\r\n\r\n// TableRowOperation:\r\n{\r\n    \"op\": \"insert\" | \"delete\",\r\n    \"row_pk\": string,\r\n    \"row\": array\r\n}\r\n```\r\n\r\nEach `SubscriptionUpdate` contains a `TableUpdate` for each table with subscribed rows. Each `TableUpdate` contains a `TableRowOperation` for each subscribed row. `SubscriptionUpdate`, `TableUpdate` and `TableRowOperation` are also used by the [`TransactionUpdate` message](#transactionupdate) to encode rows altered by a reducer, so `TableRowOperation` includes an `\"op\"` field which identifies the row alteration as either an insert or a delete. When a client receives a `SubscriptionUpdate` message in response to a [`Subscribe` message](#subscribe), all of the `TableRowOperation`s will have `\"op\"` of `\"insert\"`.\r\n\r\n| `TableUpdate` field    | Value                                                                                                          |\r\n| ---------------------- | -------------------------------------------------------------------------------------------------------------- |\r\n| `table_id`             | An integer identifier for the table. A table's `table_id` is not stable, so clients should not depend on it.   |\r\n| `table_name`           | The string name of the table. Clients should use this field to identify the table, rather than the `table_id`. |\r\n| `table_row_operations` | A `TableRowOperation` for each inserted or deleted row.                                                        |\r\n\r\n| `TableRowOperation` field | Value                                                                                                                                                                                                          |\r\n| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `op`                      | `\"insert\"` for inserted rows during a [`TransactionUpdate`](#transactionupdate) or rows resident upon applying a subscription; `\"delete\"` for deleted rows during a [`TransactionUpdate`](#transactionupdate). |\r\n| `row_pk`                  | An opaque hash of the row computed by SpacetimeDB. Clients can use this hash to identify a previously inserted row during a delete.                                                                            |\r\n| `row`                     | The altered row, encoded as a JSON array.                                                                                                                                                                      |\r\n\r\n#### `TransactionUpdate`\r\n\r\nUpon a reducer run, a client will receive a `TransactionUpdate` containing information about the reducer which ran and the subscribed rows which it altered. Clients will only receive a `TransactionUpdate` for a reducer invocation if either of two criteria is met:\r\n\r\n1. The reducer ran successfully and altered at least one row to which the client subscribes.\r\n2. The reducer was invoked by the client, and either failed or was terminated due to insufficient energy.\r\n\r\nEach `TransactionUpdate` contains a [`SubscriptionUpdate`](#subscriptionupdate) with all rows altered by the reducer, including inserts and deletes; and an `Event` with information about the reducer itself, including a [`FunctionCall`](#functioncall) containing the reducer's name and arguments.\r\n\r\n##### Binary: ProtoBuf definition\r\n\r\n```protobuf\r\nmessage TransactionUpdate {\r\n    Event event = 1;\r\n    SubscriptionUpdate subscriptionUpdate = 2;\r\n}\r\n\r\nmessage Event {\r\n    enum Status {\r\n        committed = 0;\r\n        failed = 1;\r\n        out_of_energy = 2;\r\n    }\r\n    uint64 timestamp = 1;\r\n    bytes callerIdentity = 2;\r\n    FunctionCall functionCall = 3;\r\n    Status status = 4;\r\n    string message = 5;\r\n    int64 energy_quanta_used = 6;\r\n    uint64 host_execution_duration_micros = 7;\r\n}\r\n```\r\n\r\n| Field                | Value                                                                                                                       |\r\n| -------------------- | --------------------------------------------------------------------------------------------------------------------------- |\r\n| `event`              | An `Event` containing information about the reducer run.                                                                    |\r\n| `subscriptionUpdate` | A [`SubscriptionUpdate`](#subscriptionupdate) containing all the row insertions and deletions committed by the transaction. |\r\n\r\n| `Event` field                    | Value                                                                                                                                                                                                          |\r\n| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `timestamp`                      | The time when the reducer started, as microseconds since the Unix epoch.                                                                                                                                       |\r\n| `callerIdentity`                 | The identity of the client which requested the reducer invocation. For event-driven and scheduled reducers, this is the identity of the database owner.                                                        |\r\n| `functionCall`                   | A [`FunctionCall`](#functioncall) containing the name of the reducer and the arguments passed to it.                                                                                                           |\r\n| `status`                         | `committed` if the reducer ran successfully and its changes were committed to the database; `failed` if the reducer signaled an error; `out_of_energy` if the reducer was canceled due to insufficient energy. |\r\n| `message`                        | The error message with which the reducer failed if `status` is `failed`, or the empty string otherwise.                                                                                                        |\r\n| `energy_quanta_used`             | The amount of energy consumed by running the reducer.                                                                                                                                                          |\r\n| `host_execution_duration_micros` | The duration of the reducer's execution, in microseconds.                                                                                                                                                      |\r\n\r\n##### Text: JSON encoding\r\n\r\n```typescript\r\n// TransactionUpdate:\r\n{\r\n    \"TransactionUpdate\": {\r\n        \"event\": Event,\r\n        \"subscription_update\": SubscriptionUpdate\r\n    }\r\n}\r\n\r\n// Event:\r\n{\r\n    \"timestamp\": number,\r\n    \"status\": \"committed\" | \"failed\" | \"out_of_energy\",\r\n    \"caller_identity\": string,\r\n    \"function_call\": {\r\n        \"reducer\": string,\r\n        \"args\": array,\r\n    },\r\n    \"energy_quanta_used\": number,\r\n    \"message\": string\r\n}\r\n```\r\n\r\n| Field                 | Value                                                                                                                       |\r\n| --------------------- | --------------------------------------------------------------------------------------------------------------------------- |\r\n| `event`               | An `Event` containing information about the reducer run.                                                                    |\r\n| `subscription_update` | A [`SubscriptionUpdate`](#subscriptionupdate) containing all the row insertions and deletions committed by the transaction. |\r\n\r\n| `Event` field           | Value                                                                                                                                                                                                          |\r\n| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |\r\n| `timestamp`             | The time when the reducer started, as microseconds since the Unix epoch.                                                                                                                                       |\r\n| `status`                | `committed` if the reducer ran successfully and its changes were committed to the database; `failed` if the reducer signaled an error; `out_of_energy` if the reducer was canceled due to insufficient energy. |\r\n| `caller_identity`       | The identity of the client which requested the reducer invocation. For event-driven and scheduled reducers, this is the identity of the database owner.                                                        |\r\n| `function_call.reducer` | The name of the reducer.                                                                                                                                                                                       |\r\n| `function_call.args`    | The reducer arguments encoded as a JSON array.                                                                                                                                                                 |\r\n| `energy_quanta_used`    | The amount of energy consumed by running the reducer.                                                                                                                                                          |\r\n| `message`               | The error message with which the reducer failed if `status` is `failed`, or the empty string otherwise.                                                                                                        |\r\n",
          "hasPages": false,
          "editUrl": "index.md",
          "jumpLinks": [
            {
              "title": "The SpacetimeDB WebSocket API",
              "route": "the-spacetimedb-websocket-api",
              "depth": 1
            },
            {
              "title": "Connecting",
              "route": "connecting",
              "depth": 2
            },
            {
              "title": "Protocols",
              "route": "protocols",
              "depth": 2
            },
            {
              "title": "Binary Protocol",
              "route": "binary-protocol",
              "depth": 3
            },
            {
              "title": "Text Protocol",
              "route": "text-protocol",
              "depth": 3
            },
            {
              "title": "Messages",
              "route": "messages",
              "depth": 2
            },
            {
              "title": "Client to server",
              "route": "client-to-server",
              "depth": 3
            },
            {
              "title": "`FunctionCall`",
              "route": "-functioncall-",
              "depth": 4
            },
            {
              "title": "Binary: ProtoBuf definition",
              "route": "binary-protobuf-definition",
              "depth": 5
            },
            {
              "title": "Text: JSON encoding",
              "route": "text-json-encoding",
              "depth": 5
            },
            {
              "title": "`Subscribe`",
              "route": "-subscribe-",
              "depth": 4
            },
            {
              "title": "Binary: ProtoBuf definition",
              "route": "binary-protobuf-definition",
              "depth": 5
            },
            {
              "title": "Text: JSON encoding",
              "route": "text-json-encoding",
              "depth": 5
            },
            {
              "title": "Server to client",
              "route": "server-to-client",
              "depth": 3
            },
            {
              "title": "`IdentityToken`",
              "route": "-identitytoken-",
              "depth": 4
            },
            {
              "title": "Binary: ProtoBuf definition",
              "route": "binary-protobuf-definition",
              "depth": 5
            },
            {
              "title": "Text: JSON encoding",
              "route": "text-json-encoding",
              "depth": 5
            },
            {
              "title": "`SubscriptionUpdate`",
              "route": "-subscriptionupdate-",
              "depth": 4
            },
            {
              "title": "Binary: ProtoBuf definition",
              "route": "binary-protobuf-definition",
              "depth": 5
            },
            {
              "title": "Text: JSON encoding",
              "route": "text-json-encoding",
              "depth": 5
            },
            {
              "title": "`TransactionUpdate`",
              "route": "-transactionupdate-",
              "depth": 4
            },
            {
              "title": "Binary: ProtoBuf definition",
              "route": "binary-protobuf-definition",
              "depth": 5
            },
            {
              "title": "Text: JSON encoding",
              "route": "text-json-encoding",
              "depth": 5
            }
          ],
          "pages": []
        }
      ],
      "previousKey": {
        "title": "SQL Reference",
        "route": "index",
        "depth": 1
      },
      "nextKey": null
    }
  ],
  "rootEditURL": "https://github.com/clockworklabs/spacetime-docs/edit/master/docs/"
};