"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
class OperationsMap {
    items = [];
    isEqual(a, b) {
        if (a && typeof a === "object" && "isEqual" in a) {
            return a.isEqual(b);
        }
        return a === b;
    }
    set(key, value) {
        const existingIndex = this.items.findIndex(({ key: k }) => this.isEqual(k, key));
        if (existingIndex > -1) {
            this.items[existingIndex].value = value;
        }
        else {
            this.items.push({ key, value });
        }
    }
    get(key) {
        const item = this.items.find(({ key: k }) => this.isEqual(k, key));
        return item ? item.value : undefined;
    }
    delete(key) {
        const existingIndex = this.items.findIndex(({ key: k }) => this.isEqual(k, key));
        if (existingIndex > -1) {
            this.items.splice(existingIndex, 1);
            return true;
        }
        return false;
    }
    has(key) {
        return this.items.some(({ key: k }) => this.isEqual(k, key));
    }
    values() {
        return this.items.map((i) => i.value);
    }
}
exports.default = OperationsMap;
