type Nav = {
  items: NavItem[];
};
type NavItem = NavPage | NavSection;
type NavPage = {
  type: 'page';
  path: string;
  slug: string;
  title: string;
  disabled?: boolean;
  href?: string;
};
type NavSection = {
  type: 'section';
  title: string;
};

function page(
  title: string,
  slug: string,
  path: string,
  props?: { disabled?: boolean; href?: string; description?: string }
): NavPage {
  return { type: 'page', path, slug, title, ...props };
}
function section(title: string): NavSection {
  return { type: 'section', title };
}

const nav: Nav = {
  items: [
    section('Intro'),
    page('Overview', 'index', 'index.md'), // TODO(BREAKING): For consistency & clarity, 'index' slug should be renamed 'intro'?
    page('Getting Started', 'getting-started', 'getting-started.md'),

    section('Deploying'),
    page('Maincloud', 'deploying/maincloud', 'deploying/maincloud.md'),
    page('Self-Hosting SpacetimeDB', 'deploying/spacetimedb-standalone', 'deploying/spacetimedb-standalone.md'),

    section('Unity Tutorial - Basic Multiplayer'),
    page('Overview', 'unity', 'unity/index.md'),
    page('1 - Setup', 'unity/part-1', 'unity/part-1.md'),
    page('2 - Connecting to SpacetimeDB', 'unity/part-2', 'unity/part-2.md'),
    page('3 - Gameplay', 'unity/part-3', 'unity/part-3.md'),
    page('4 - Moving and Colliding', 'unity/part-4', 'unity/part-4.md'),

    section('CLI Reference'),
    page('CLI Reference', 'cli-reference', 'cli-reference.md'),
    page(
      'SpacetimeDB Standalone Configuration',
      'cli-reference/standalone-config',
      'cli-reference/standalone-config.md'
    ),

    section('Server Module Languages'),
    page('Overview', 'modules', 'modules/index.md'),
    page(
      'Rust Quickstart',
      'modules/rust/quickstart',
      'modules/rust/quickstart.md'
    ),
    page('Rust Reference', 'modules/rust', 'modules/rust/index.md'),
    page(
      'C# Quickstart',
      'modules/c-sharp/quickstart',
      'modules/c-sharp/quickstart.md'
    ),
    page('C# Reference', 'modules/c-sharp', 'modules/c-sharp/index.md'),

    section('Client SDK Languages'),
    page('Overview', 'sdks', 'sdks/index.md'),
    page(
      'C# Quickstart',
      'sdks/c-sharp/quickstart',
      'sdks/c-sharp/quickstart.md'
    ),
    page('C# Reference', 'sdks/c-sharp', 'sdks/c-sharp/index.md'),
    page('Rust Quickstart', 'sdks/rust/quickstart', 'sdks/rust/quickstart.md'),
    page('Rust Reference', 'sdks/rust', 'sdks/rust/index.md'),
    page(
      'TypeScript Quickstart',
      'sdks/typescript/quickstart',
      'sdks/typescript/quickstart.md'
    ),
    page('TypeScript Reference', 'sdks/typescript', 'sdks/typescript/index.md'),

    section('SQL'),
    page('SQL Reference', 'sql', 'sql/index.md'),

    section('Subscriptions'),
    page('Subscription Reference', 'subscriptions', 'subscriptions/index.md'),
    page('Subscription Semantics', 'subscriptions/semantics', 'subscriptions/semantics.md'),

    section('Row Level Security'),
    page('Row Level Security', 'rls', 'rls/index.md'),

    section('How To'),
    page('Incremental Migrations', 'how-to/incremental-migrations', 'how-to/incremental-migrations.md'),
    page('Reject Client Connections', 'how-to/reject-client-connections', 'how-to/reject-client-connections.md'),

    section('HTTP API'),
    page('Authorization', 'http/authorization', 'http/authorization.md'),
    page('`/identity`', 'http/identity', 'http/identity.md'),
    page('`/database`', 'http/database', 'http/database.md'),

    section('Internals'),
    page('Module ABI Reference', 'webassembly-abi', 'webassembly-abi/index.md'),
    page('SATS-JSON Data Format', 'sats-json', 'sats-json.md'),
    page('BSATN Data Format', 'bsatn', 'bsatn.md'),

    section('Appendix'),
    page('Appendix', 'appendix', 'appendix.md'),
  ],
};

export default nav;
