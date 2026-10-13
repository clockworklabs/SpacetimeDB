import { t } from 'spacetimedb/server';
import { agentTool } from '@spacetimedb/agents';
import type { Tx } from '../types';

export default agentTool(
  'returns the current server time as an ISO-8601 string',
  t.unit(),
  ctx => {
    // The Tx cast avoids a circular type reference between the tool and schema.
    const tx = ctx as Tx;
    const micros = tx.timestamp.microsSinceUnixEpoch as bigint;
    return new Date(Number(micros / 1000n)).toISOString();
  }
);
