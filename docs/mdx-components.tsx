import defaultMdxComponents from 'fumadocs-ui/mdx';
import type { MDXComponents } from 'mdx/types';
import { Callout, CalloutContainer, CalloutTitle, CalloutDescription } from 'fumadocs-ui/components/callout';
import { Tabs, TabItem } from '@/components/Tabs';
import { CardLink } from '@/components/CardLink';
import { CardLinkGrid } from '@/components/CardLinkGrid';
import { StepByStep, Step, StepText, StepCode } from '@/components/Steps';
import { QuickstartLinks } from '@/components/QuickstartLinks';
import { Check } from '@/components/Check';
import { InstallCardLink } from '@/components/InstallCardLink';

export function getMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    ...defaultMdxComponents,
    Callout,
    CalloutContainer,
    CalloutTitle,
    CalloutDescription,
    Tabs,
    Tab: TabItem,
    TabItem,
    CardLink,
    CardLinkGrid,
    StepByStep,
    Step,
    StepText,
    StepCode,
    QuickstartLinks,
    Check,
    InstallCardLink,
    ...components,
  };
}
