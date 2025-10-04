export type TableHandle<TableName extends string> = {
  /** Phantom table name */
  readonly tableName?: TableName;
};

export type TableNamesFromDb<Db> = Db extends object
  ? {
      [K in keyof Db]: Db[K] extends TableHandle<infer TableName>
        ? TableName
        : never;
    }[keyof Db]
  : never;
