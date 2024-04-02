import { SpacetimeDBClient } from "./spacetimedb";
export type ReducerClass = {
    new (...args: any[]): Reducer;
    reducerName: string;
};
export declare class Reducer {
    static reducerName: string;
    call(..._args: any[]): void;
    on(..._args: any[]): void;
    protected client: SpacetimeDBClient;
    static with<T extends typeof Reducer>(client: SpacetimeDBClient): InstanceType<T>;
    protected static reducer?: any;
    protected static getReducer<T extends typeof Reducer>(): InstanceType<T>;
    constructor(client: SpacetimeDBClient);
}
