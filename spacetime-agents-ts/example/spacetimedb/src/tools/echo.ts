// Demo tool: echoes the given message back.

import { t } from 'spacetimedb/server';
import { agentTool } from '@spacetimedb/agents/kit';

export default agentTool(
  'echoes the given message back to the caller',
  t.object('EchoArgs', { message: t.string() }),
  (_ctx, args) => `echo: ${args.message}`
);
