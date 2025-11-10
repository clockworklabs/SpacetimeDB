import React, { ReactNode } from "react";
import Link from "@docusaurus/Link";
import { useDocById } from "@docusaurus/plugin-content-docs/client";
import Heading from "@theme/Heading";
import clsx from "clsx";

export type Item = {
  href: string;
  label: string;
  icon?: ReactNode;
  invertIcon?: boolean;
  docId?: string;
  description?: string;
};

function DocDescription({ docId }: { docId: string }) {
  const doc = useDocById(docId);
  return (
    <p className={clsx("text--truncate", "text-body-3")} title={doc?.description}>
      {doc?.description ?? ""}
    </p>
  );
}

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
      className={clsx(
        "card",
        className,
        item.invertIcon && "card--invert-icon"
      )}
    >
      {icon}
      <div className={clsx("card--body")}>
        <Heading as="h6" className={clsx("text--truncate")} title={item.label}>
          {item.label}
        </Heading>
        {item.docId ? (
          <DocDescription docId={item.docId} />
        ) : item.description ? (
          <p className={clsx("text--truncate", "text-body-3")} title={item.description}>{item.description}</p>
        ) : null}
      </div>
    </Link>
  );
}