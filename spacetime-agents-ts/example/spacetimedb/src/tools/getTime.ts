// Demo tool: returns the current server time. ctx cast to Tx avoids a
// circular type reference between the kit and the schema-derived Tx.

import { t } from 'spacetimedb/server';
import { agentTool } from '@spacetimedb/agents/kit';
import type { Tx } from '../types';

export default agentTool(
  'returns the current server time as an ISO-8601 string',
  t.unit(),
  ctx => {
    const tx = ctx as Tx;
    const micros = tx.timestamp.microsSinceUnixEpoch as bigint;
    return new Date(Number(micros / 1000n)).toISOString();
  }
);
