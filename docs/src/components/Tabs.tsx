'use client';

import React, { Children, isValidElement, ReactNode } from 'react';
import {
  Tabs as FumaTabs,
  Tab as FumaTab,
} from 'fumadocs-ui/components/tabs';

const LABEL_MAP: Record<string, string> = {
  typescript: 'TypeScript',
  csharp: 'C#',
  rust: 'Rust',
  python: 'Python',
  sql: 'SQL',
  bash: 'Bash',
  toml: 'TOML',
  json: 'JSON',
  fsharp: 'F#',
  cpp: 'C++',
};

function displayLabel(value: string, label?: string): string {
  if (label) return label;
  return LABEL_MAP[value] ?? value;
}

interface TabItemProps {
  value: string;
  label?: string;
  children: ReactNode;
}

export function TabItem({ value, label, children }: TabItemProps) {
  return (
    <FumaTab value={displayLabel(value, label)}>
      {children}
    </FumaTab>
  );
}

interface TabsProps {
  groupId?: string;
  children: ReactNode;
  [key: string]: unknown;
}

export function Tabs({ groupId, children, ...rest }: TabsProps) {
  const items: string[] = [];
  Children.forEach(children, (child) => {
    if (isValidElement<TabItemProps>(child)) {
      items.push(displayLabel(child.props.value, child.props.label));
    }
  });

  return (
    <FumaTabs groupId={groupId} items={items} {...rest}>
      {children}
    </FumaTabs>
  );
}
