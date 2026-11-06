import assert from 'node:assert/strict';
import { escapeHtml, renderMarkdown } from '../public/markdown.js';

assert.equal(
  escapeHtml(`<script data-x="'">&</script>`),
  '&lt;script data-x=&quot;&#39;&quot;&gt;&amp;&lt;/script&gt;'
);
assert.equal(renderMarkdown(''), '');
assert.equal(
  renderMarkdown('**bold** and *italic* and `code`'),
  '<p><strong>bold</strong> and <em>italic</em> and <code>code</code></p>'
);
assert.equal(
  renderMarkdown('[docs](https://example.com/path)'),
  '<p><a href="https://example.com/path" target="_blank" rel="noopener noreferrer">docs</a></p>'
);
assert.equal(
  renderMarkdown('[unsafe](javascript:alert(1))'),
  '<p>[unsafe](javascript:alert(1))</p>'
);
assert.equal(
  renderMarkdown('<img src=x onerror=alert(1)>'),
  '<p>&lt;img src=x onerror=alert(1)&gt;</p>'
);
assert.equal(
  renderMarkdown('```html\n<strong>not HTML</strong>\n```'),
  '<pre><code>&lt;strong&gt;not HTML&lt;/strong&gt;\n</code></pre>'
);
assert.equal(
  renderMarkdown('before\n```text\ninside\n```\nafter'),
  '<p>before</p><pre><code>inside\n</code></pre><p>after</p>'
);
assert.equal(renderMarkdown(' BLOCK0 '), '<p> BLOCK0 </p>');

console.log('agents markdown tests passed');
