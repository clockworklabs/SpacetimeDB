'use client';
import { useCopyButton } from 'fumadocs-ui/utils/use-copy-button';
import { buttonVariants } from 'fumadocs-ui/components/ui/button';

function CopyIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
      <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

export function CopyPageButton({ content }: { content: string }) {
  const [checked, onClick] = useCopyButton(() => {
    return navigator.clipboard.writeText(content);
  });

  return (
    <button
      className="inline-flex items-center gap-1.5 rounded-md border border-fd-border bg-fd-secondary px-2 py-1 text-[0.7rem] leading-tight text-fd-muted-foreground transition-colors hover:bg-fd-accent hover:text-fd-accent-foreground [&_svg]:size-2.5"
      onClick={onClick}
    >
      {checked ? <CheckIcon /> : <CopyIcon />}
      {checked ? 'Copied!' : 'Copy Page'}
    </button>
  );
}
