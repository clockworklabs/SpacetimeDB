import { schema, SenderError, table, t } from 'spacetimedb/server';

const STARTING_BALANCE_CENTS = 10_000n;
const MAX_NAME_LENGTH = 20;
const MAX_U64 = (1n << 64n) - 1n;

const directory = table(
  { name: 'directory', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    nameKey: t.string().unique(),
  }
);

const account = table(
  { name: 'account' },
  {
    identity: t.identity().primaryKey(),
    balanceCents: t.u64(),
  }
);

const changeDirection = t.enum('ChangeDirection', {
  Credit: t.unit(),
  Debit: t.unit(),
});

const accountChange = table(
  { name: 'account_change' },
  {
    id: t.u64().primaryKey().autoInc(),
    accountIdentity: t.identity().index('btree'),
    counterpartyIdentity: t.identity(),
    direction: changeDirection,
    amountCents: t.u64(),
    createdAt: t.timestamp(),
  }
);

const spacetimedb = schema({ directory, account, accountChange });
export default spacetimedb;

export const onConnect = spacetimedb.clientConnected(ctx => {
  if (!ctx.db.account.identity.find(ctx.sender)) {
    ctx.db.account.insert({
      identity: ctx.sender,
      balanceCents: STARTING_BALANCE_CENTS,
    });
  }
});

export const my_account = spacetimedb.view(
  { name: 'my_account', public: true },
  account.rowType.optional(),
  ctx => ctx.db.account.identity.find(ctx.sender) ?? undefined
);

export const my_account_changes = spacetimedb.view(
  { name: 'my_account_changes', public: true },
  t.array(accountChange.rowType),
  ctx => [...ctx.db.accountChange.accountIdentity.filter(ctx.sender)]
);

export const set_name = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (!ctx.db.account.identity.find(ctx.sender)) {
      throw new SenderError('Account is not ready yet');
    }

    const displayName = name.trim();
    if (displayName.length === 0 || displayName.length > MAX_NAME_LENGTH) {
      throw new SenderError('Names must be between 1 and 20 characters');
    }

    const nameKey = displayName.toLowerCase();
    const owner = ctx.db.directory.nameKey.find(nameKey);
    if (owner && !owner.identity.isEqual(ctx.sender)) {
      throw new SenderError('That name is already in use');
    }

    const existing = ctx.db.directory.identity.find(ctx.sender);
    if (existing) {
      ctx.db.directory.identity.update({
        identity: ctx.sender,
        name: displayName,
        nameKey,
      });
    } else {
      ctx.db.directory.insert({
        identity: ctx.sender,
        name: displayName,
        nameKey,
      });
    }
  }
);

export const transfer = spacetimedb.reducer(
  { recipient: t.identity(), amountCents: t.u64() },
  (ctx, { recipient: recipientIdentity, amountCents }) => {
    const sender = ctx.db.directory.identity.find(ctx.sender);
    if (!sender) {
      throw new SenderError('Choose a name before sending money');
    }
    if (recipientIdentity.isEqual(ctx.sender)) {
      throw new SenderError('You cannot send money to yourself');
    }
    if (amountCents === 0n) {
      throw new SenderError('Amount must be greater than zero');
    }
    if (!ctx.db.directory.identity.find(recipientIdentity)) {
      throw new SenderError('Recipient does not exist');
    }

    const fromAccount = ctx.db.account.identity.find(ctx.sender);
    const toAccount = ctx.db.account.identity.find(recipientIdentity);
    if (!fromAccount || !toAccount) {
      throw new SenderError('Account does not exist');
    }
    if (fromAccount.balanceCents < amountCents) {
      throw new SenderError('Insufficient funds');
    }
    if (toAccount.balanceCents > MAX_U64 - amountCents) {
      throw new SenderError('Recipient balance is too large');
    }

    ctx.db.account.identity.update({
      ...fromAccount,
      balanceCents: fromAccount.balanceCents - amountCents,
    });
    ctx.db.account.identity.update({
      ...toAccount,
      balanceCents: toAccount.balanceCents + amountCents,
    });
    ctx.db.accountChange.insert({
      id: 0n,
      accountIdentity: ctx.sender,
      counterpartyIdentity: recipientIdentity,
      direction: { tag: 'Debit' },
      amountCents,
      createdAt: ctx.timestamp,
    });
    ctx.db.accountChange.insert({
      id: 0n,
      accountIdentity: recipientIdentity,
      counterpartyIdentity: ctx.sender,
      direction: { tag: 'Credit' },
      amountCents,
      createdAt: ctx.timestamp,
    });
  }
);
