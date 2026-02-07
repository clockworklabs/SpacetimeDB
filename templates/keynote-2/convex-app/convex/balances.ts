// convex/balances.ts
import { ShardedCounter } from '@convex-dev/sharded-counter';
import { components } from './_generated/api';

export const accountBalances = new ShardedCounter<string>(
  components.shardedCounter,
  {
    defaultShards: 10000,
  },
);

export const accountKey = (id: number) => `account:${id}`;
