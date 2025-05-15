#!/bin/bash

# Keep in mind that this is also used from the private repo.

SUBDIR="${1:-.}"  # Default to '.' (the whole repo) if no argument given

# We need to figure out the root to make this work when called from a directory within the repo.
GIT_ROOT="$(git rev-parse --show-toplevel)"

# We have a comment in every generated file that has the version and git hash, so these would change with every commit.
# We ignore them to avoid having to regen files for every commit unrelated to code gen.

PATTERN='^// This was generated using spacetimedb cli version.*'
failed=0

git diff --exit-code --ignore-matching-lines="$PATTERN" -- "$SUBDIR"
