import { schema, table, t, SenderError } from 'spacetimedb/server';

const deploymentNote = table(
  { name: 'deployment_note', public: true },
  {
    id: t.u32().primaryKey(),
    region: t.string(),
    mode: t.string(),
    banner: t.string(),
  }
);

const deploymentInfo = t.object('DeploymentInfo', {
  region: t.string(),
  mode: t.string(),
});

const spacetimedb = schema(
  { deploymentNote },
  {
    env: {
      REGION: t.string(),
      MODE: t.enum('Mode', ['development', 'production']),
      BANNER: t.string().optional(),
    },
  }
);
export default spacetimedb;

export const record_deployment = spacetimedb.reducer(
  { id: t.u32() },
  (ctx, { id }) => {
    if (ctx.db.deploymentNote.id.find(id)) throw new SenderError('note exists');
    ctx.db.deploymentNote.insert({
      id,
      region: ctx.env.REGION,
      mode: ctx.env.MODE,
      banner: ctx.env.BANNER ?? '(none)',
    });
  }
);

export const require_development = spacetimedb.reducer(ctx => {
  if (ctx.env.MODE === 'production') throw new SenderError('production locked');
});

export const current_deployment = spacetimedb.anonymousView(
  { name: 'current_deployment', public: true },
  t.array(deploymentInfo),
  ctx => [{ region: ctx.env.REGION, mode: ctx.env.MODE }]
);
