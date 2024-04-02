export default class OperationsMap<K, V> {
    private items;
    private isEqual;
    set(key: K, value: V): void;
    get(key: K): V | undefined;
    delete(key: K): boolean;
    has(key: K): boolean;
    values(): Array<V>;
}
