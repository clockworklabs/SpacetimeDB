/// <reference lib="dom" />
/// <reference lib="dom.iterable" />
export {};
const search = document.querySelector<HTMLInputElement>('[data-guide-search]')!;
const others = document.querySelector<HTMLInputElement>('[data-guide-others]')!;
const checks = [...document.querySelectorAll<HTMLDetailsElement>('.guide-check')];
const url = new URL(location.href);
search.value = url.searchParams.get('q') ?? '';
others.checked = url.searchParams.get('scope') === 'all';
function filter(): void {
  let shown = 0;
  for (const check of checks) {
    check.hidden = (!others.checked && check.dataset.active !== 'true')
      || !(check.dataset.search ?? '').includes(search.value.trim().toLowerCase());
    if (!check.hidden) shown++;
  }
  document.querySelector('[data-guide-count]')!.textContent = `${shown} ${shown === 1 ? 'check' : 'checks'} shown`;
  document.querySelector<HTMLElement>('[data-guide-empty]')!.hidden = shown > 0;
  if (search.value) url.searchParams.set('q', search.value); else url.searchParams.delete('q');
  if (others.checked) url.searchParams.set('scope', 'all'); else url.searchParams.delete('scope');
  history.replaceState(null, '', url);
}
search.addEventListener('input', filter);
others.addEventListener('change', filter);
const toggle = document.querySelector<HTMLButtonElement>('[data-guide-toggle]')!;
toggle.addEventListener('click', () => {
  const expand = toggle.textContent === 'Expand visible';
  for (const check of checks) check.open = expand && !check.hidden;
  toggle.textContent = expand ? 'Collapse all' : 'Expand visible';
});
filter();
