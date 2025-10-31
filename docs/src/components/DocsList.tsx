import { useAllDocsData } from '@docusaurus/plugin-content-docs/lib/client/index.js';

export default function DocList() {
  const allDocsData = useAllDocsData();
  // `default` is the ID of the docs plugin instance
  const docs = allDocsData['default'].versions[0].docs;

  const docIds = docs.map((doc) => doc.id);

  return (
    <div>
      <h2>All Doc IDs</h2>
      <ul>
        {docIds.map((id) => (
          <li key={id}>{id}</li>
        ))}
      </ul>
    </div>
  );
}