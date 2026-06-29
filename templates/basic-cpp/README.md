Get a SpacetimeDB C++ app running in under 5 minutes.

## Prerequisites

- [SpacetimeDB CLI](https://spacetimedb.com/install) installed
- [Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html) 4.0.21+ installed
- CMake 3.20+ and a make/ninja backend
- C++20 toolchain (host) — build targets WASM via Emscripten

After installing the SDK, run the appropriate `emsdk_env` script (PowerShell or Bash) so `emcc` and the CMake toolchain file are available on `PATH`.

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Install Emscripten

Use the official SDK (see [Emscripten downloads](https://emscripten.org/docs/getting_started/downloads.html)) and activate the environment so `emcc` and the CMake toolchain file are on PATH. We recommend Emscripten 4.0.21+.

```bash
# From your emsdk directory (after downloading/cloning)
# Windows PowerShell
./emsdk install 4.0.21
./emsdk activate 4.0.21
./emsdk_env.ps1

# macOS/Linux
./emsdk install 4.0.21
./emsdk activate 4.0.21
source ./emsdk_env.sh
```



## Create your project

Use the CLI-managed workflow with `spacetime build`, which wraps CMake + `emcc` for you, starts the local server, builds/publishes your module, and generates client bindings.

```bash
spacetime dev --template basic-cpp
```


Need manual control? You can still drive CMake+emcc directly (see `spacetimedb/CMakeLists.txt`), but the recommended path is `spacetime build`/`spacetime dev`.





Server code lives in the `spacetimedb` folder; the template uses CMake and the SpacetimeDB C++ SDK.


```
my-spacetime-app/
├── spacetimedb/               # Your C++ module
│   ├── CMakeLists.txt
│   └── src/
│       └── lib.cpp            # Server-side logic
├── Cargo.toml
└── src/
    └── main.rs                # Rust client application
```



## Understand tables and reducers

The template includes a `Person` table and two reducers: `add` to insert, `say_hello` to iterate and log.

```cpp
#include "spacetimedb.h"
using namespace SpacetimeDB;

struct Person { std::string name; };
SPACETIMEDB_STRUCT(Person, name)
SPACETIMEDB_TABLE(Person, person, Public)

SPACETIMEDB_REDUCER(add, ReducerContext ctx, std::string name) {
    ctx.db[person].insert(Person{name});
    return Ok();
}

SPACETIMEDB_REDUCER(say_hello, ReducerContext ctx) {
    for (const auto& person : ctx.db[person]) {
        LOG_INFO("Hello, " + person.name + "!");
    }
    LOG_INFO("Hello, World!");
    return Ok();
}
```



## Test with the CLI

Open a new terminal and navigate to your project directory. Then call reducers and inspect data right from the CLI.

```bash
cd my-spacetime-app

# Insert a person
spacetime call add Alice

# Query the person table
spacetime sql "SELECT * FROM person"

# Call say_hello to greet everyone
spacetime call say_hello

# View the module logs
spacetime logs
```

## Notes

- To use a local SDK clone instead of the fetched archive, set `SPACETIMEDB_CPP_SDK_DIR` before running `spacetime dev`/`spacetime build`.
- The template builds to WebAssembly with exceptions disabled (`-fno-exceptions`).
- If `emcc` is not found, re-run the appropriate `emsdk_env` script to populate environment variables.
