// Publish this fixture to a disposable database, then call test_rollback and
// test_replaced_run. Only provider responses are substituted; transactions are real.
import {
  schema,
  t,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as agents from '../../../src/submodule';
import { agentTool, defineAgent } from '../../../src/agent';
import { client } from '../../../src/submodule/client';

const spacetimedb = schema({ agents });
export default spacetimedb;
type Tx = TransactionCtx<InferSchema<typeof spacetimedb>>;

function check(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

const agent = client<Tx>({
  submodule: tx => tx.as.agents,
  agents: {
    test: defineAgent({
      defaultModel: 'test',
      defaultRetries: 0,
      tools: {
        fail: agentTool('Write, then fail', t.unit(), raw => {
          const tx = raw as Tx;
          tx.as.agents.db.apiKey.insert({
            provider: 'must-rollback',
            key: 'test',
            updatedAt: tx.timestamp,
          });
          throw new Error('tool failed');
        }),
      },
    }),
  },
});

function start(tx: Tx): bigint {
  const db = tx.as.agents.db;
  if (!db.apiKey.provider.find('openrouter')) {
    db.apiKey.insert({
      provider: 'openrouter',
      key: 'test',
      updatedAt: tx.timestamp,
    });
  }
  return agent.startThread(tx, { owner: 'test', agentName: 'test' });
}

function response(tool: boolean) {
  return {
    status: 200,
    text: () =>
      JSON.stringify({
        choices: [
          {
            message: tool
              ? {
                  content: null,
                  tool_calls: [
                    {
                      id: 'call',
                      type: 'function',
                      function: { name: 'fail', arguments: '{}' },
                    },
                  ],
                }
              : { content: 'old response' },
            finish_reason: tool ? 'tool_calls' : 'stop',
          },
        ],
        usage: { prompt_tokens: 1, completion_tokens: 1 },
      }),
  };
}

export const testRollback = spacetimedb.procedure(t.unit(), ctx => {
  const threadId = ctx.withTx(start);
  let failed = false;
  try {
    agent.sendMessage(
      { withTx: fn => ctx.withTx(fn), http: { fetch: () => response(true) } },
      {
        owner: 'test',
        threadId,
        content: 'test',
      }
    );
  } catch (error) {
    check(String(error).includes('tool failed'), 'unexpected run failure');
    failed = true;
  }
  ctx.withTx(tx => {
    const db = tx.as.agents.db;
    check(failed, 'tool failure did not reach the caller');
    check(
      !db.apiKey.provider.find('must-rollback'),
      'failed tool committed its write'
    );
    check(!db.threadLock.threadId.find(threadId), 'failed run leaked its lock');
    check(
      ![...db.message.threadId.filter(threadId)].some(row => row.toolCallsJson),
      'failed turn left unmatched tool calls'
    );
    agent.deleteThread(tx, { owner: 'test', threadId });
  });
  return {};
});

export const testReplacedRun = spacetimedb.procedure(t.unit(), ctx => {
  const threadId = ctx.withTx(start);
  let replaced = false;
  try {
    agent.sendMessage(
      {
        withTx: fn => ctx.withTx(fn),
        http: {
          fetch: () => {
            ctx.withTx(tx => {
              const db = tx.as.agents.db;
              const lock = db.threadLock.threadId.find(threadId)!;
              db.threadLock.threadId.delete(threadId);
              // Reuse the owner and timestamp: neither identifies a particular run.
              db.threadLock.insert({
                ...lock,
                runId: tx.newUuidV4().toString(),
              });
              replaced = true;
            });
            return response(false);
          },
        },
      },
      { owner: 'test', threadId, content: 'test' }
    );
  } catch {
    // A superseded run must stop without changing the replacement's state.
  }
  ctx.withTx(tx => {
    const db = tx.as.agents.db;
    check(replaced, 'test did not reach the provider');
    check(
      db.threadLock.threadId.find(threadId),
      'old run deleted the replacement lock'
    );
    check(
      [...db.message.threadId.filter(threadId)].every(
        row => row.role === 'user'
      ),
      'old run committed its response'
    );
    db.threadLock.threadId.delete(threadId);
    agent.deleteThread(tx, { owner: 'test', threadId });
  });
  return {};
});
