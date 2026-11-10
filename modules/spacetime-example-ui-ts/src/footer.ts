export function mountFooter(parent: HTMLElement): HTMLElement {
  const footer = document.createElement('footer');
  footer.className = 'example-footer';

  const label = document.createElement('span');
  label.textContent = 'Built on';

  const link = document.createElement('a');
  link.href = 'https://spacetimedb.com';
  link.target = '_blank';
  link.rel = 'noopener noreferrer';
  const wordmark = document.createElement('img');
  wordmark.src = '/assets/brand.svg';
  wordmark.alt = 'SpacetimeDB';
  link.append(wordmark);

  footer.append(label, link);
  parent.append(footer);
  return footer;
}
