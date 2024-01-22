# Proposal: Unity tutorial refactor

# Context, Motivation

This is part of the larger [[WIP] Proposals for docs rework](https://www.notion.so/WIP-Proposals-for-docs-rework-68a907c8f4ab4330acb74c7da6b1d1b8?pvs=21) 

This Proposal in particular is focused on the **Unity tutorial experience**. Within that, it is interested in making “high-leverage” changes to the Unity tutorial, to address active pain points (below).

# Constraints & Definition of “Done”

## Scope

This proposal is **not** concerned with meaningfully expanding the **“Spacetime coverage”** of the Unity tutorial**.**

Instead, it is focused on leveraging existing content easier to consume and maintain. Items in this proposal should:

- have meaningful, present impact (e.g. active bug, user-reported clarity issue)
- take at most a day
- not meaningfully increase the “surface area” of our public-facing content

## Pain points to address

- Different gamedevs have explicitly conflicting use cases, so it’s difficult to make one comprehensive tutorial
- Our Unity tutorial is doing too many jobs: introduce SpacetimeDB concepts, address setup/one-time errors, explain the Unity SDK, teach design principles..
- Unclear which parts are self-contained / can be tested and run without continuing
- Unclear which portions can/can’t be easily “copypasted” to other projects
- No clear “before+after” of the code for each section
- A variety of specific / low-level issues (below)

# Proposed Solution

- Address “quick-fix” issues (below)
- Create a new Docs section: A “How do I…?” section / “Code Demo Gallery”
    - self-contained demos for implementing specific game features/systems in Spacetime
- Move later / “advanced” parts of our Unity tutorial to be standalone demos
    - “Main” Unity tutorial’s goals are to guide through setup, basic patterns, a simple “base project”
    - Transition smoothly from “end” of Unity tutorial into “Code demo gallery” sections

## Brief rationale

Imagined impacts:

- For us:
    - It’s easier to maintain self-contained standalone examples, with separate branches etc.
    - Easier to automate testing for isolated “demo branches” than progressive steps of the tutorial
    - We don’t have to try so hard to connect one example to the next, and have “one omni-demo-case”
- For prospective users:
    - Able to self-answer questions about what SpacetimeDB can do, whether it works for their use case, etc.
- For new users:
    - More reliable and up-to-date content (due to easier maintenance)
    - Easier to several focused guides than one long, progressively more complex one.

# Detailed Design

## Easier-to-fork Unity project

- Quick fixes:
    - Downloaded project folder structure doesn’t quite match docs (maybe just capitalization?)
    - Missing scripts on `Camera` objects
    - Warning about two audio listeners
- Update to latest (public) Spacetime
    - Some (small) breaking API param changes
    - Update our downloadable packages for starter & completed projects
    - Update any tutorial docs for altered code
- Replace the meshes and textures with simpler “greybox” ones
- Simplify confusing sections, using some code from [zeke-demo-project](https://github.com/clockworklabs/spacetime-docs/pull/27/files?short_path=fc133e8#diff-fc133e8a9aa771a22d18a12927bddc468193008d5340b0a4063d411c54941ac1)
    - Decouple the tutorial project’s mining logic from animator components
    - Logic controlling camera movement from user input

## Tutorial docs errata

- Docs mention explicit lists `SELECT * FROM [TABLE]` statements, but project uses `SELECT * FROM *`
- Properly introduce these suddenly-used types:
    - `StdbVector2` definition missing
    - Switches from `EntityComponent` -> `SpawnableEntityComponent`, `MobileEntityComponent` without mentioning it
- Address [Tyler friend project feedback](https://discord.com/channels/931210784011321394/1166512616718471279/1196276328543031376)
    - It was not clear from the docs that we had to install a unity package, and it was not clear on the website where to get that
    - Connecting to the server with the Unity package was not obvious how to do
    - It was not clear that we even had to use the `NetworkManager`
    - Why do we call row insert for populating the client cache?

## Create docs section: “How do I…?” Code demo gallery

- Add this new section at the end of the Unity tutorial, for demoing individual game features and accompanying code
- Each demo should have its own GitHub PRs or branches, with a shared “base project” where the Unity tutorial leaves off (or a previous demo).
- Each page should make clear
    - Motivating feature or use case (e.g. inventory system, resource mining)
    - MVP code to add the feature, with appropriately generic names not tied to any specific broader game etc.
    - Explain the “key pieces” of the code
    - Include links to other demos or the tutorial for further reading
- Move advanced functionality from the current Unity tutorial, into self-contained “How do I…?” pages listed in the navbar:
    - Chat functionality
    - Inventory system, management
    - Player-environment interaction
    - Shop functionality
    - Resource spawning, mining
- Add a CTA button, something like “Let us know if we’re missing your use case!”

# Execution stages

I propose to approach things in this order:

1. Update project to latest SpacetimeDB (since this might affect the set of errata)
2. Address tutorial docs quick fixes / errata (pretty well-contained scope, high visibility/impact)
3. Simplify code using: https://github.com/clockworklabs/zeke-demo-project
4. Greybox out the meshes (I believe this depends on some animator code being simplified)
5. Copy (parts of) tutorial code into their own self-contained branches
6. Create Markdown pages for each one, create the new docs section

# Alternatives considered

- Maintaining one sequential tutorial that progressively demonstrates each topic / use case
    - I claim this is harder to do well, is more difficult to maintain and test
- Redo the tutorial “from scratch” with a game more focused on highlighting SpacetimeDB features:
    - This would be helpful for e.g. attractive demoability on a stream
    - Switching to a meaningfully different design would require a meaningful amount of work “figuring out details” again (even to a minimal design)
    - Currently the most important thing is that users are able to understand the basic concepts, and I believe the current tutorial largely achieves that

# Open questions

- Name for the new docs section?
    - “How do I…?”
    - “Code Demo Gallery”

# FAQ

None, yet.

# Future work

- Further work from the larger proposal: [[WIP] Proposals for docs rework](https://www.notion.so/WIP-Proposals-for-docs-rework-68a907c8f4ab4330acb74c7da6b1d1b8?pvs=21)
- Expanding the Code demo gallery to include common or requested design patterns / systems:
    - [Code gallery wishlist](https://www.notion.so/clockworklabs/bc0aa1426d6646999ac9a35636332e1d?v=511e0050a20542e18323b036162715bc&pvs=4)
