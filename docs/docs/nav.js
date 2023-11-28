function page(title, slug, path, props) {
  return { type: "page", path, slug, title, ...props };
}

function section(title) {
  return { type: "section", title };
}

const nav = {
  items: [
    section("Intro"),
    page("Overview", "index", "Overview/index.md"),
    page("Getting Started", "getting-started", "Getting Started/index.md"),

    section("Deploying"),
    page("Testnet", "deploying/testnet", "Cloud Testnet/index.md"),

    section("Unity Tutorial"),
    page("Part 1 - Basic Multiplayer", "unity/part-1",  "Unity Tutorial/Part 1 - Basic Multiplayer.md"),
    page("Part 2 - Resources And Scheduling", "unity/part-2",  "Unity Tutorial/Part 2 - Resources And Scheduling.md"),
    page("Part 3 - BitCraft Mini", "unity/part-3", "Unity Tutorial/Part 3 - BitCraft Mini.md"),

    section("Server Module Languages"),
    page("Overview", "modules", "Server Module Languages/index.md"),
    page("Rust Quickstart", "modules/rust/quickstart",  "Server Module Languages/Rust/index.md"),
    page("Rust Reference", "modules/rust", "Server Module Languages/Rust/ModuleReference.md"),
    page("C# Quickstart", "modules/c-sharp/quickstart", "Server Module Languages/C#/index.md"),
    page("C# Reference", "modules/c-sharp", "Server Module Languages/C#/ModuleReference.md"),

    section("Client SDK Languages"),
    page("Overview", "sdks", "Client SDK Languages/index.md"),
    page("Typescript Quickstart", "sdks/typescript/quickstart", "Client SDK Languages/Typescript/index.md"),
    page("Typescript Reference", "sdks/typescript", "Client SDK Languages/Typescript/SDK Reference.md"),
    page("Rust Quickstart", "sdks/rust/quickstart", "Client SDK Languages/Rust/index.md"),
    page("Rust Reference", "sdks/rust", "Client SDK Languages/Rust/SDK Reference.md"),
    page("Python Quickstart", "sdks/python/quickstart", "Client SDK Languages/Python/index.md"),
    page("Python Reference", "sdks/python", "Client SDK Languages/Python/SDK Reference.md"),
    page("C# Quickstart", "sdks/c-sharp/quickstart", "Client SDK Languages/C#/index.md"),
    page("C# Reference", "sdks/c-sharp", "Client SDK Languages/C#/SDK Reference.md"),

    section("WebAssembly ABI"),
    page("Module ABI Reference", "webassembly-abi", "Module ABI Reference/index.md"),

    section("HTTP API"),
    page("HTTP", "http", "HTTP API Reference/index.md"),
    page("`/identity`", "http/identity", "HTTP API Reference/Identities.md"),
    page("`/database`", "http/database", "HTTP API Reference/Databases.md"),
    page("`/energy`", "http/energy", "HTTP API Reference/Energy.md"),

    section("WebSocket API Reference"),
    page("WebSocket", "ws", "WebSocket API Reference/index.md"),

    section("Data Format"),
    page("SATN", "satn", "SATN Reference/index.md"),
    page("BSATN", "bsatn", "SATN Reference/Binary Format.md"),

    section("SQL"),
    page("SQL Reference", "sql", "SQL Reference/index.md"),
  ],
};

export default nav;
