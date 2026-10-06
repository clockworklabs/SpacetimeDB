// The strict app from ../app.ts, resolved the way Node resolves the package
// (NodeNext). Generated bindings import their own files without
// extensions, which NodeNext rejects, so this declares its module inline.
import {
  DbConnectionBuilder,
  DbConnectionImpl,
  procedures,
  reducerSchema,
  reducers,
  schema,
  t,
  table,
  type DbConnectionConfig,
} from 'spacetimedb';

const tablesSchema = schema({
  player: table(
    {
      name: 'player',
      constraints: [
        { name: 'player_id_key', constraint: 'unique', columns: ['id'] },
      ],
    },
    t.row({ id: t.u32().primaryKey(), name: t.string() })
  ),
});
const reducersSchema = reducers(
  reducerSchema('create_player', { name: t.string() })
);
const REMOTE_MODULE = {
  versionInfo: { cliVersion: '2.11.0' as const },
  tables: tablesSchema.schemaType.tables,
  reducers: reducersSchema.reducersType.reducers,
  ...procedures(),
};

class DbConnection extends DbConnectionImpl<typeof REMOTE_MODULE> {}

const conn = new DbConnectionBuilder<DbConnection>(
  REMOTE_MODULE,
  (config: DbConnectionConfig<typeof REMOTE_MODULE>) => new DbConnection(config)
)
  .withUri('ws://localhost:3000')
  .withDatabaseName('test')
  .build();

export const created: Promise<void> = conn.reducers.createPlayer({
  name: 'player',
});
export const players = [...conn.db.player.iter()];
