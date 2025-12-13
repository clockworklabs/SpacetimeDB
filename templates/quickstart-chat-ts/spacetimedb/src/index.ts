// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { schema, t, table, SenderError } from 'spacetimedb/server';

const User = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string().optional(),
    online: t.bool(),
  }
);

const Message = table(
  { name: 'message', public: true },
  { sender: t.identity(), sent: t.timestamp(), text: t.string() }
);

const spacetimedb = schema(User, Message);

function validateName(name: string) {
  if (!name) throw new SenderError('Names must not be empty');
}

spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  validateName(name);
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('Cannot set name for unknown user');
  console.info(`User ${ctx.sender} sets name to ${name}`);
  ctx.db.user.identity.update({ ...user, name });
});

function validateMessage(text: string) {
  if (!text) throw new SenderError('Messages must not be empty');
}

spacetimedb.reducer('send_message', { text: t.string() }, (ctx, { text }) => {
  // Things to consider:
  // - Rate-limit messages per-user.
  // - Reject messages from unnamed user.
  validateMessage(text);
  console.info(`User ${ctx.sender}: ${text}`);
  ctx.db.message.insert({
    sender: ctx.sender,
    text,
    sent: ctx.timestamp,
  });
});

// Called when the module is initially published
spacetimedb.init(_ctx => {});

spacetimedb.clientConnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    // If this is a returning user, i.e. we already have a `User` with this `Identity`,
    // set `online: true`, but leave `name` and `identity` unchanged.
    ctx.db.user.identity.update({ ...user, online: true });
  } else {
    // If this is a new user, create a `User` row for the `Identity`,
    // which is online, but hasn't set a name.
    ctx.db.user.insert({
      name: undefined,
      identity: ctx.sender,
      online: true,
    });
  }
});

spacetimedb.clientDisconnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, online: false });
  } else {
    // This branch should be unreachable,
    // as it doesn't make sense for a client to disconnect without connecting first.
    console.warn(
      `Disconnect event for unknown user with identity ${ctx.sender}`
    );
  }
});
