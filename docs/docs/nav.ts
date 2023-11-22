export type Nav = {
  items: NavItem[];
};
export type NavItem = NavPage | NavSection;
export type NavPage = {
  type: "page";
  path: string;
  title: string;
  disabled?: boolean;
  href?: string;
};
type NavSection = {
  type: "section";
  title: string;
};

function page(path: string, title: string, props?: { disabled?: boolean; href?: string; description?: string }): NavPage {
  return { type: "page", path: path, title, ...props };
}
function section(title: string): NavSection {
  return { type: "section", title };
}

export default {
  items: [
    section("Intro"),
    page("Overview/index.md", "Overview"),
    page("Getting Started/index.md", "Getting Started"),

    section("Deploying"),
    page("Cloud Testnet/index.md", "Testnet"),

    section("Unity Tutorial"),
    page("Unity Tutorial/Part 1 - Basic Multiplayer.md", "Part 1 - Basic Multiplayer"),
    page("Unity Tutorial/Part 2 - Resources And Scheduling.md", "Part 2 - Resources And Scheduling"),
    page("Unity Tutorial/Part 3 - BitCraft Mini.md", "Part 3 - BitCraft Mini"),

    section("Server Module Languages"),
    page("Server Module Languages/index.md", "Overview"),
    page("Server Module Languages/Rust/index.md", "Rust Quickstart"),
    page("Server Module Languages/Rust/ModuleReference.md", "Rust Reference"),
    page("Server Module Languages/C#/index.md", "C# Quickstart"),
    page("Server Module Languages/C#/ModuleReference.md", "C# Reference"),

    section("Client SDK Languages"),
    page("Client SDK Languages/index.md", "Overview"),
    page("Client SDK Languages/Typescript/index.md", "Typescript Quickstart"),
    page("Client SDK Languages/Typescript/SDK Reference.md", "Typescript Reference"),
    page("Client SDK Languages/Rust/index.md", "Rust Quickstart"),
    page("Client SDK Languages/Rust/SDK Reference.md", "Rust Reference"),
    page("Client SDK Languages/Python/index.md", "Python Quickstart"),
    page("Client SDK Languages/Python/SDK Reference.md", "Python Reference"),
    page("Client SDK Languages/C#/index.md", "C# Quickstart"),
    page("Client SDK Languages/C#/SDK Reference.md", "C# Reference"),

    section("WebAssembly ABI"),
    page("Module ABI Reference/index.md", "Module ABI Reference"),

    section("HTTP API"),
    page("HTTP API Reference/index.md", "HTTP"),
    page("HTTP API Reference/Identities.md", "`/identity`"),
    page("HTTP API Reference/Databases.md", "`/database`"),
    page("HTTP API Reference/Energy.md", "`/energy`"),

    section("WebSocket API Reference"),
    page("WebSocket API Reference/index.md", "WebSocket"),

    section("Data Format"),
    page("SATN Reference/index.md", "SATN"),
    page("SATN Reference/Binary Format.md", "BSATN"),

    section("SQL"),
    page("SQL Reference/index.md", "SQL Reference"),
  ],
} satisfies Nav;
