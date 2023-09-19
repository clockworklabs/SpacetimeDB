export type DocConfig = {
  sections: DocSectionConfig[];
  rootEditURL: string;
};

export type DocSectionConfig = {
  title: string;
  identifier: string;
  indexIdentifier?: string;
  comingSoon: boolean;
  tag?: boolean;
  hasPages: boolean;
  editUrl: string;
  nextKey?: JumpLink;
  previousKey?: JumpLink;
  content?: string;
  pages?: DocSectionConfig[];
  jumpLinks: JumpLink[];
};

export type JumpLink = {
  title: string;
  route: string;
  depth: number;
};
