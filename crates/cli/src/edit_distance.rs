/*
Some parts copyright, The Rust project developers.
See https://github.com/rust-lang/rust/blob/8882507bc7dbad0cc0548204eb8777e51ac92332/COPYRIGHT
for the parts where MIT / Apache-2.0 applies.

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
*/

//! Edit distances.
//!
//! The [edit distance] is a metric for measuring the difference between two strings.
//!
//! [edit distance]: https://en.wikipedia.org/wiki/Edit_distance

// The current implementation is the restricted Damerau-Levenshtein algorithm. It is restricted
// because it does not permit modifying characters that have already been transposed. The specific
// algorithm should not matter to the caller of the methods, which is why it is not noted in the
// documentation.

use std::{cmp, mem};

/// Finds the [edit distance] between two strings.
///
/// Returns `None` if the distance exceeds the limit.
///
/// [edit distance]: https://en.wikipedia.org/wiki/Edit_distance
pub fn edit_distance(a: &str, b: &str, limit: usize) -> Option<usize> {
    let mut a = &a.chars().collect::<Vec<_>>()[..];
    let mut b = &b.chars().collect::<Vec<_>>()[..];

    // Ensure that `b` is the shorter string, minimizing memory use.
    if a.len() < b.len() {
        mem::swap(&mut a, &mut b);
    }

    let min_dist = a.len() - b.len();
    // If we know the limit will be exceeded, we can return early.
    if min_dist > limit {
        return None;
    }

    // Strip common prefix.
    while let Some(((b_char, b_rest), (a_char, a_rest))) = b.split_first().zip(a.split_first()) {
        if a_char != b_char {
            break;
        }
        a = a_rest;
        b = b_rest;
    }
    // Strip common suffix.
    while let Some(((b_char, b_rest), (a_char, a_rest))) = b.split_last().zip(a.split_last()) {
        if a_char != b_char {
            break;
        }
        a = a_rest;
        b = b_rest;
    }

    // If either string is empty, the distance is the length of the other.
    // We know that `b` is the shorter string, so we don't need to check `a`.
    if b.is_empty() {
        return Some(min_dist);
    }

    let mut prev_prev = vec![usize::MAX; b.len() + 1];
    let mut prev = (0..=b.len()).collect::<Vec<_>>();
    let mut current = vec![0; b.len() + 1];

    // row by row
    for i in 1..=a.len() {
        current[0] = i;
        let a_idx = i - 1;

        // column by column
        for j in 1..=b.len() {
            let b_idx = j - 1;

            // There is no cost to substitute a character with itself.
            let substitution_cost = if a[a_idx] == b[b_idx] { 0 } else { 1 };

            current[j] = cmp::min(
                // deletion
                prev[j] + 1,
                cmp::min(
                    // insertion
                    current[j - 1] + 1,
                    // substitution
                    prev[j - 1] + substitution_cost,
                ),
            );

            if (i > 1) && (j > 1) && (a[a_idx] == b[b_idx - 1]) && (a[a_idx - 1] == b[b_idx]) {
                // transposition
                current[j] = cmp::min(current[j], prev_prev[j - 2] + 1);
            }
        }

        // Rotate the buffers, reusing the memory.
        [prev_prev, prev, current] = [prev, current, prev_prev];
    }

    // `prev` because we already rotated the buffers.
    let distance = prev[b.len()];
    (distance <= limit).then_some(distance)
}

/// Finds the best match for a given word in the given iterator.
///
/// As a loose rule to avoid the obviously incorrect suggestions, it takes
/// an optional limit for the maximum allowable edit distance, which defaults
/// to one-third of the given word.
///
/// We use case insensitive comparison to improve accuracy on an edge case with a lower(upper)case
/// letters mismatch.
pub fn find_best_match_for_name<'c>(candidates: &[&'c str], lookup: &str, dist: Option<usize>) -> Option<&'c str> {
    let lookup_uppercase = lookup.to_uppercase();

    // Priority of matches:
    // 1. Exact case insensitive match
    // 2. Edit distance match
    // 3. Sorted word match
    if let Some(c) = candidates.iter().find(|c| c.to_uppercase() == lookup_uppercase) {
        return Some(*c);
    }

    let mut dist = dist.unwrap_or_else(|| cmp::max(lookup.len(), 3) / 3);
    let mut best = None;
    // store the candidates with the same distance, only for `use_substring_score` current.
    for c in candidates {
        match edit_distance(lookup, c, dist) {
            Some(0) => return Some(*c),
            Some(d) => {
                dist = d - 1;
                best = Some(*c);
            }
            None => {}
        }
    }

    if best.is_some() {
        return best;
    }

    find_match_by_sorted_words(candidates, lookup)
}

fn find_match_by_sorted_words<'c>(iter_names: &[&'c str], lookup: &str) -> Option<&'c str> {
    iter_names.iter().fold(None, |result, candidate| {
        if sort_by_words(candidate) == sort_by_words(lookup) {
            Some(*candidate)
        } else {
            result
        }
    })
}

fn sort_by_words(name: &str) -> String {
    let mut split_words: Vec<&str> = name.split('_').collect();
    // We are sorting primitive &strs and can use unstable sort here.
    split_words.sort_unstable();
    split_words.join("_")
}
