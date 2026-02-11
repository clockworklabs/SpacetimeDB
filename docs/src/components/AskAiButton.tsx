'use client';

import Link from 'next/link';

export function AskAiButton() {
  return (
    <Link
      href="/ask-ai"
      className="fixed bottom-6 right-6 z-50 flex items-center gap-2 rounded-full bg-fd-card border border-fd-border px-4 py-2.5 text-sm font-medium text-fd-foreground shadow-lg transition-colors hover:bg-fd-accent"
    >
      Ask AI{' '}
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="currentColor"
        className="text-[#4cf490]"
      >
        <path d="M12 3l1.912 5.813a2 2 0 0 0 1.275 1.275L21 12l-5.813 1.912a2 2 0 0 0-1.275 1.275L12 21l-1.912-5.813a2 2 0 0 0-1.275-1.275L3 12l5.813-1.912a2 2 0 0 0 1.275-1.275L12 3z" />
      </svg>
    </Link>
  );
}
