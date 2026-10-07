import assert from 'node:assert/strict';
import { messageHtml } from '../spacetimedb/src/message';

const html = messageHtml(
  'Hello <team> & friends\nNext line\n\nSecond paragraph'
);

assert.match(html, /BlinkMacSystemFont/);
assert.match(html, /max-width:560px/);
assert.match(html, /Hello &lt;team&gt; &amp; friends<br>Next line/);
assert.match(html, /Second paragraph/);
assert.equal((html.match(/<p /g) ?? []).length, 2);
assert.doesNotMatch(html, /<team>/);

console.log('resend message tests passed');
