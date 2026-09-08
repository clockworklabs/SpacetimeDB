import assert from 'node:assert/strict';
import test from 'node:test';
import { transcriptMessages } from '../../dashboard/dashboard-transcript.js';

test('transcript normalizes Claude and Codex text/tools, redacts credentials, and ignores metadata', () => {
  const rows = [
    { type: 'assistant', message: { role: 'assistant', content: [{ type: 'text', text: '<script>hello</script>' },
      { type: 'tool_use', name: 'Bash', input: { command: 'echo hello' } }] } },
    { type: 'user', message: { role: 'user', content: [{ type: 'tool_result', content: 'Authorization: Bearer secret-example-token' }] } },
    { type: 'response_item', payload: { role: 'assistant', content: [{ type: 'output_text', text: 'Codex reply' }] } },
    { type: 'response_item', payload: { type: 'function_call', name: 'exec_command', arguments: '{"cmd":"ls"}' } },
    { type: 'item.completed', item: { type: 'command_execution', command: 'ls', aggregated_output: 'file.ts' } },
    { type: 'system', accessToken: 'never-show-metadata' },
  ];
  const result = transcriptMessages(rows.map(row => JSON.stringify(row)).join('\n') + '\n{broken');
  assert.equal(result.messages.length, 6);
  assert.equal(result.messages.filter(message => message.tool).length, 4);
  assert.equal(result.skipped, 1);
  assert.doesNotMatch(JSON.stringify(result), /secret-example-token|never-show-metadata/);
  assert.equal(result.messages[0]?.text, '<script>hello</script>'); // Escaped by the view, never interpreted as markup.
});
