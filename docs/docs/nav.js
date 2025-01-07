"use strict";
var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};
Object.defineProperty(exports, "__esModule", { value: true });
function page(title, slug, path, props) {
    return __assign({ type: 'page', path: path, slug: slug, title: title }, props);
}
function section(title) {
    return { type: 'section', title: title };
}
var nav = {
    items: [
        section('Intro'),
        page('Overview', 'index', 'index.md'), // TODO(BREAKING): For consistency & clarity, 'index' slug should be renamed 'intro'?
        page('Getting Started', 'getting-started', 'getting-started.md'),
        section('Deploying'),
        page('Testnet', 'deploying/testnet', 'deploying/testnet.md'),
        section('Migration Guides'),
        page('v0.12', 'migration/v0.12', 'migration/v0.12.md'),
        section('Unity Tutorial - Basic Multiplayer'),
        page('Overview', 'unity-tutorial', 'unity/index.md'),
        page('1 - Setup', 'unity/part-1', 'unity/part-1.md'),
        page('2a - Server (Rust)', 'unity/part-2a-rust', 'unity/part-2a-rust.md'),
        page('2b - Server (C#)', 'unity/part-2b-c-sharp', 'unity/part-2b-c-sharp.md'),
        page('3 - Client', 'unity/part-3', 'unity/part-3.md'),
        section('Unity Tutorial - Advanced'),
        page('4 - Resources And Scheduling', 'unity/part-4', 'unity/part-4.md'),
        page('5 - BitCraft Mini', 'unity/part-5', 'unity/part-5.md'),
        section('Server Module Languages'),
        page('Overview', 'modules', 'modules/index.md'),
        page('Rust Quickstart', 'modules/rust/quickstart', 'modules/rust/quickstart.md'),
        page('Rust Reference', 'modules/rust', 'modules/rust/index.md'),
        page('C# Quickstart', 'modules/c-sharp/quickstart', 'modules/c-sharp/quickstart.md'),
        page('C# Reference', 'modules/c-sharp', 'modules/c-sharp/index.md'),
        section('Client SDK Languages'),
        page('Overview', 'sdks', 'sdks/index.md'),
        page('Typescript Quickstart', 'sdks/typescript/quickstart', 'sdks/typescript/quickstart.md'),
        page('Typescript Reference', 'sdks/typescript', 'sdks/typescript/index.md'),
        page('Rust Quickstart', 'sdks/rust/quickstart', 'sdks/rust/quickstart.md'),
        page('Rust Reference', 'sdks/rust', 'sdks/rust/index.md'),
        page('C# Quickstart', 'sdks/c-sharp/quickstart', 'sdks/c-sharp/quickstart.md'),
        page('C# Reference', 'sdks/c-sharp', 'sdks/c-sharp/index.md'),
        section('How To'),
        page('Incremental Migrations', 'how-to/incremental-migrations', 'how-to/incremental-migrations.md'),
        section('WebAssembly ABI'),
        page('Module ABI Reference', 'webassembly-abi', 'webassembly-abi/index.md'),
        section('HTTP API'),
        page('HTTP', 'http', 'http/index.md'),
        page('`/identity`', 'http/identity', 'http/identity.md'),
        page('`/database`', 'http/database', 'http/database.md'),
        page('`/energy`', 'http/energy', 'http/energy.md'),
        section('WebSocket API Reference'),
        page('WebSocket', 'ws', 'ws/index.md'),
        section('Data Format'),
        page('SATN', 'satn', 'satn.md'),
        page('BSATN', 'bsatn', 'bsatn.md'),
        section('SQL'),
        page('SQL Reference', 'sql', 'sql/index.md'),
    ],
};
exports.default = nav;
