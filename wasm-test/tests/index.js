import assert from "assert";
import { add } from "../build/debug.js";
assert.strictEqual(add(1, 2), 3);
console.log("ok");
