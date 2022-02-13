# SpacetimeDB
- Wasm

### Lightning
- A programming language (or extension to Rust) that allows you to declare tables
- You can annotate them with indexes

### Postgres
- Has Tables, MVCC, and Tuples

### CockroachDB
- Is a distributed replicated transactional KV store
- Doesn't delete data, just leaves tombstones for MVCC
- Keys are of the form "<table>/<index>/<key>/<columnName>"
- Lexigraphically sorts keys and partitions into 64 mb chunks
- Repartitions if a chunk fills up

### SkyLab
- Basically semi-distributed Git with janky Jupyter notebook smart contracts

### IPFS
- A giant unstructured, peer-to-peer, object storage with git tags basically

### Git vs Ethereum
Git:
- Object DB: a mapping from content hash to content
- Ref DB: mapping from string to commit hash
- Structure objects: Trees, Commits, Blobs
- Path: Each commit's tree structure forms a trie which maps a "path" to a blob, that path is basically a "key" in map.
- Forms a map from "path" -> "value"

Ethereum:
- Merkle Patricia Tree: Basically the same as Git structure objects, except weird. Forms a map from "key" -> "value"
    - Leaf (blob + metadata)
    - Extensions (simple tree + blob)
    - Branch (tree + blob)
- Strangely Ethereum mixes values in with the structure of the data, which to me seems like it would just cause data to be duplicated unnecessarily
- The point theoretically is so that you can prove that a given key value pair exists in the state by only sending the relevant nodes

Git and Ethereum's Merkle Patricia Tree are basical