"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.toPascalCase = exports._tableProxy = void 0;
// Helper function for creating a proxy for a table class
function _tableProxy(t, client) {
    return new Proxy(t, {
        get: (target, prop) => {
            if (typeof target[prop] === "function") {
                return (...args) => {
                    const originalDb = t.db;
                    t.db = client.db;
                    const result = t[prop](...args);
                    t.db = originalDb;
                    return result;
                };
            }
            else {
                return t[prop];
            }
        },
    });
}
exports._tableProxy = _tableProxy;
function toPascalCase(s) {
    const str = s.replace(/([-_][a-z])/gi, ($1) => {
        return $1.toUpperCase().replace("-", "").replace("_", "");
    });
    return str.charAt(0).toUpperCase() + str.slice(1);
}
exports.toPascalCase = toPascalCase;
