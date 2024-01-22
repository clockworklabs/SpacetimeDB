# Docs proposal: Polish the existing content

# Context, Motivation

This is part of the larger group of [[WIP] Proposals for docs rework](https://www.notion.so/WIP-Proposals-for-docs-rework-68a907c8f4ab4330acb74c7da6b1d1b8?pvs=21), as part of an ongoing project to improve the quality of our docs.

## Goals

This specific proposal is meant to address the “low-hanging” subset of pain points.

This is mostly achieved by reorganizing and consolidating existing docs content, as well as a small collection of other fixes, etc.

# Constraints & Definition of “Done”

## Scope

Items in scope for this proposal should be clear and meaningful “go forth and implement” items, so they should:

- be impactful
- have a lot low probability of surprise
- each take at **most** half a day
- not have much room for contention once we align at this level
- not meaningfully increase our surface area of public-facing content

## Pain points to address

- Bugs
    - Nav sidebar becomes super long topbar on non-wide screens
- Overall structure
    - Many of our doc pages cover a ton of content
        - There’s no toplevel/sidebar navigation to specific sections
            - [Tyler note](https://www.notion.so/Docs-proposal-Polish-the-existing-content-596a8fb415b443348e18ee17aa830d28?d=8c6b1562858b47c4a765895d3a4bfd97&pvs=4#e3bf734016c14f8db7ea49728e0d26bc)
              > This can certainly be added back in. We had to remove it because the previous implementation was buggy/had issues.
- New users - Getting stuck in engagement/onboarding process
    - Bugs, errors
- Users can stop considering us because we don’t obviously support their use case
- Dev time spent answering similar/repeated questions
    - Fixing an error or environment-specific issue
    - How to implement xyz game feature with SpacetimeDB
    - Whether a functionality is supported/possible

# Proposed Solution

- Immediately fix the low-work, high-impact issues
- Create more easy-to-find sections for users to self-support (see below for details)
    - Related: [(Estimated) Final docs structure](https://www.notion.so/Estimated-Final-docs-structure-492249b605a1476cb7ec54921a786cbf?pvs=21)
- Supplement our docs with repeated answers we give in Discord, etc.

## Brief rationale

- These are impactful fixes that have low probability of surprise-scope
- Creating the proposed doc sections (below) from existing content:
    - is **impactful**: gives users a meaningfully easier-to-find place to solve common problems, and gives our team a place to document answers to common questions
    - is **good ROI on work**: by relying primarily on existing content, we keep the scope for language-bikeshedding low (both within the author, and in the review process)
    - is **low surprise**: by relying primarily on existing content, we minimize review overhead for “is this okay / the right way to say this publicly?”

# Detailed Design

## Small fixes

- Web
    - Fix issue: Nav sidebar becomes super long topbar on non-wide screens
    - Table of contents - highlighted section should update as you scroll
    - Scan for & fix broken links
- Unity tutorial content
    - Make our example commands use full-length param for `--clear-database` (it’s currently easy to miss if following along visually)
    - Make sure link is obvious to download complete project (Mike Cann post made some comment about no finished example projects)
    - [Tyler friend project feedback](https://discord.com/channels/931210784011321394/1166512616718471279/1196276328543031376) - Clarify instructions for installing Unity project
    - Fix stale/broken names:
        - References to `TutorialGameManager` but is called `BitcraftMiniGameManager` in the actual package
        - Changes from generic naming to bitcraft naming in pt 2
        - Update/fix references to `onIdentityReceived`
- Small non-web fixes
    - Make `spacetime generate` clear out old files in the destination directory
        - stale autogen files can lead to confusing errors

## Create MVP doc sections

- create 80/20s of these sections by copypasting things we’ve said in blogs, discords, etc.
    - Terminology
    - Upcoming features
    - Common errors
    - FAQs
- create 80/20 CLI reference section
    - by roughly just copypasting the CLI help text

## Split up large pages

- Split large pages into multiple easily-linkable subpages (e.g. reference pages, Unity tutorial)
- If necessary, update the sidebar code to support more-deeply-nested links

## Expand FAQ & future work sections

Work with experienced folks to expand Docs FAQ & Upcoming Features sections for [these topics](https://www.notion.so/clockworklabs/85a5414f7d63424b9e9eddc09a0ff838?v=3202bd60e5eb4a18b0d6a39d1fe67448&pvs=4).

# Execution stages

1. Ping people for new FAQ answers
2. Work on quick fixes
3. Create new doc sections from available content
4. Split up large pages
5. As available, incorporate expert answers from above

# Alternatives considered

- Lean harder on the AI chatbot?
    - Why not:
        - Currently, the chatbot is too slow to provide an enjoyable user experience
        - Unclear how discoverable it is
        - Much larger set of unknowns associated with improving the UX
    - Takeaway: The AI chatbot isn’t a replacement for restructuring the existing docs, but we should also consider exploring this path, mindful to its larger scope/unknowns

# Open questions

## How should we measure the success of our docs?

- We could ask users for feedback?
- Analytics on where users churn?
    - Do we have analytics on how users navigate our website? (in particular our docs?)

# FAQ

- none so far

# Future work

- Further work from the larger proposal: [[WIP] Proposals for docs rework](https://www.notion.so/WIP-Proposals-for-docs-rework-68a907c8f4ab4330acb74c7da6b1d1b8?pvs=21)
    - In particular: [[WIP] Proposal: Split up Unity tutorial](https://www.notion.so/WIP-Proposal-Split-up-Unity-tutorial-ae1968411f784018a7481d6d89ea3b5e?pvs=21)
    - And in regards to long-term API reference doc quality: [[WIP] Proposal: API doc auto-generation](https://www.notion.so/WIP-Proposal-API-doc-auto-generation-47ff8e748baf49d591e7e54fafc647f7?pvs=21)
- Process-level stuff
    - Incrementally expand the new doc sections as new questions/answers happen
    - Start giving people links to answers in the docs, in discord, etc.
