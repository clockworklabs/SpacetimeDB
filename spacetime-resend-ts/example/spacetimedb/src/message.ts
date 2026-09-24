function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

export function messageHtml(message: string): string {
  const paragraphs = message
    .split(/\n{2,}/)
    .map(block => escapeHtml(block).replace(/\n/g, '<br>'))
    .map(block => `<p style="margin:0 0 16px;line-height:1.6;">${block}</p>`)
    .join('');
  return [
    "<div style=\"font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;",
    'font-size:15px;color:#1f2933;max-width:560px;margin:0 auto;padding:8px;">',
    paragraphs,
    '</div>',
  ].join('');
}
