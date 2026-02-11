import { CardLink } from './CardLink';

export function InstallCardLink() {
  return (
    <div style={{ maxWidth: 400 }}>
      <CardLink
        item={{
          href: 'https://spacetimedb.com/install',
          label: 'Install the SpacetimeDB CLI tool',
          icon: (
            <img
              src="/docs/images/icons/cli-icon.svg"
              height={40}
              alt="CLI"
            />
          ),
        }}
      />
    </div>
  );
}
