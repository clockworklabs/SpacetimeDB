"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
function page(title, slug, path, props) {
    return { type: "page", path, slug, title, ...props };
}
function section(title) {
    return { type: "section", title };
}
const nav = {
    items: [
        section("Intro"),
        page("Overview", "index", "index.md"),
        page("Installation", "install", "install.md"),
        page("Getting Started", "getting-started", "getting-started.md"),
        section("Deploying"),
        page("Testnet", "deploying/testnet", "deploying/testnet.md"),
        section("Unity Tutorial"),
        page("Part 1 - Basic Multiplayer", "unity/part-1", "unity/part-1.md"),
        page("Part 2 - Resources And Scheduling", "unity/part-2", "unity/part-2.md"),
        page("Part 3 - BitCraft Mini", "unity/part-3", "unity/part-3.md"),
        section("Server Module Languages"),
        page("Server Modules", "modules", "modules/index.md"),
        page("Rust Module Quickstart", "modules/rust/quickstart", "modules/rust/quickstart.md"),
        page("Rust Module Reference", "modules/rust", "modules/rust/index.md"),
        page("C# Module Quickstart", "modules/c-sharp/quickstart", "modules/c-sharp/quickstart.md"),
        page("C# Module Reference", "modules/c-sharp", "modules/c-sharp/index.md"),
        section("Client SDK Languages"),
        page("Client SDKs", "sdks", "sdks/index.md"),
        page("Typescript SDK Quickstart", "sdks/typescript/quickstart", "sdks/typescript/quickstart.md"),
        page("Typescript SDK Reference", "sdks/typescript", "sdks/typescript/index.md"),
        page("Rust SDK Quickstart", "sdks/rust/quickstart", "sdks/rust/quickstart.md"),
        page("Rust SDK Reference", "sdks/rust", "sdks/rust/index.md"),
        page("Python SDK Quickstart", "sdks/python/quickstart", "sdks/python/quickstart.md"),
        page("Python SDK Reference", "sdks/python", "sdks/python/index.md"),
        page("C# SDK Quickstart", "sdks/c-sharp/quickstart", "sdks/c-sharp/quickstart.md"),
        page("C# SDK Reference", "sdks/c-sharp", "sdks/c-sharp/index.md"),
        section("WebAssembly ABI"),
        page("Module ABI Reference", "webassembly-abi", "webassembly-abi/index.md"),
        section("HTTP API"),
        page("HTTP", "http", "http/index.md"),
        page("`/identity`", "http/identity", "http/identity.md"),
        page("`/database`", "http/database", "http/database.md"),
        page("`/energy`", "http/energy", "http/energy.md"),
        section("WebSocket API Reference"),
        page("WebSocket", "ws", "ws/index.md"),
        section("Data Format"),
        page("SATN", "satn", "satn.md"),
        page("BSATN", "bsatn", "bsatn.md"),
        section("SQL"),
        page("SQL Reference", "sql", "sql/index.md"),
    ],
};
exports.default = nav;
