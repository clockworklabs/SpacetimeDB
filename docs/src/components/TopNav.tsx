'use client';

import { useSearchContext } from 'fumadocs-ui/contexts/search';

const navLinks = [
  { text: 'Install', href: 'https://spacetimedb.com/install' },
  { text: 'Pricing', href: 'https://spacetimedb.com/pricing' },
  { text: 'Blog', href: 'https://spacetimedb.com/blog' },
  { text: 'Community', href: 'https://spacetimedb.com/community' },
  { text: 'Spacerace', href: 'https://spacetimedb.com/spacerace' },
  { text: 'Login', href: 'https://spacetimedb.com/login' },
];

export function TopNav() {
  return (
    <nav className="topnav">
      <a href="https://spacetimedb.com" className="topnav-logo">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src="https://spacetimedb.com/images/brand.svg"
          alt="SpacetimeDB Logo"
          height={24}
        />
      </a>
      <SearchButton />
      <div className="topnav-spacer" />
      <div className="topnav-links">
        {navLinks.map((link) => (
          <a
            key={link.href}
            href={link.href}
            className="topnav-link"
            target="_blank"
            rel="noopener noreferrer"
          >
            {link.text}
          </a>
        ))}
      </div>
    </nav>
  );
}

function SearchButton() {
  const { setOpenSearch } = useSearchContext();
  return (
    <button
      type="button"
      onClick={() => setOpenSearch(true)}
      className="topnav-search"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <span>Search for anything...</span>
      <kbd>&#x2318;K</kbd>
    </button>
  );
}
