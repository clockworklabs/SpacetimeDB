import Link from 'next/link';
import { ReactNode } from 'react';

export type Item = {
  href: string;
  label: string;
  icon?: ReactNode;
  invertIcon?: boolean;
  description?: string;
};

export function CardLink({
  className,
  item,
}: {
  className?: string;
  item: Item;
}) {
  const icon = item.icon;
  return (
    <Link
      href={item.href}
      className={`card ${item.invertIcon ? 'card--invert-icon' : ''} ${className ?? ''}`.trim()}
    >
      {icon}
      <div className="card--body">
        <h6 className="text--truncate" title={item.label}>
          {item.label}
        </h6>
        {item.description ? (
          <p className="text--truncate text-body-3" title={item.description}>
            {item.description}
          </p>
        ) : null}
      </div>
    </Link>
  );
}
