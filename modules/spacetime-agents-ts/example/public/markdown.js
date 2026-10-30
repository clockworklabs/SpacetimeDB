export function escapeHtml(value) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function renderMarkdown(source) {
  if (!source) return '';

  const codeBlocks = [];
  let rendered = source.replace(/```(?:[\w-]*)\n([\s\S]*?)```/g, (_, code) => {
    const index = codeBlocks.length;
    codeBlocks.push(code);
    return `\n\n\uE000CODE_BLOCK_${index}\uE001\n\n`;
  });

  rendered = escapeHtml(rendered);
  rendered = rendered.replace(
    /`([^`\n]+)`/g,
    (_, code) => `<code>${code}</code>`
  );
  rendered = rendered.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  rendered = rendered.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>');
  rendered = rendered.replace(
    /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>'
  );
  rendered = rendered
    .split(/\n{2,}/)
    .filter(Boolean)
    .map(paragraph => {
      const codeBlockMatch = paragraph.match(/^\uE000CODE_BLOCK_(\d+)\uE001$/);
      if (codeBlockMatch) {
        const code = codeBlocks[Number(codeBlockMatch[1])];
        if (code !== undefined) {
          return `<pre><code>${escapeHtml(code)}</code></pre>`;
        }
      }
      return `<p>${paragraph.replace(/\n/g, '<br>')}</p>`;
    })
    .join('');
  return rendered;
}
