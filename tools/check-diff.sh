#!/bin/bash

# Keep in mind that this is also used from the private repo.

SUBDIR="${1:-.}"  # Default to '.' (the whole repo) if no argument given

# We need to figure out the root to make this work when called from a directory within the repo.
GIT_ROOT="$(git rev-parse --show-toplevel)"

# We have a comment in every generated file that has the version and git hash, so these would change with every commit.
# We ignore them to avoid having to regen files for every commit unrelated to code gen.

PATTERN='^// This was generated using spacetimedb cli version.*'
failed=0

for file in $(git diff --name-only -- "$SUBDIR"); do
  echo "Checking file $file"
  full_file="$GIT_ROOT/$file"
  # Only check files that still exist in working dir
  [ -f "$full_file" ] || continue

  diff_out=$(diff -u --ignore-matching-lines="$PATTERN" \
    <(git show HEAD:"$file" 2>/dev/null || cat /dev/null) "$full_file")
  if [ $? -ne 0 ]; then
    echo "Difference found in $file:"
    echo "$diff_out"
    echo # blank line for clarity
    failed=1
  fi
done

exit $failed
