export type ReducerHandle<ReducerName extends string> = {
  /** Phantom reducer name */
  readonly reducerName?: ReducerName;
};

export type ReducerNamesFromReducers<R> = R extends object 
  ? {
      [K in keyof R]: R[K] extends ReducerHandle<infer ReducerName>
        ? ReducerName 
        : never;
    }[keyof R]
  : never;
