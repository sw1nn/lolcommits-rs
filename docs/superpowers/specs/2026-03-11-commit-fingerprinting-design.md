# Commit Fingerprinting for Unknown Repo Resolution

## Problem

When `lolcommits_fixup` cannot find a commit SHA in any workspace repo (e.g., because the commit was rebased or force-pushed), the image is marked as `unknown`. Many of these commits could be matched to their correct repo using commit message characteristics.

## Solution

Build a profile of each repo's commit messages during the workspace scan, then use those profiles to suggest which repo an unresolved commit belongs to.

## Data Model

```rust
struct RepoProfile {
    repo_name: String,
    /// scope -> count (e.g., "server" -> 42)
    scopes: HashMap<String, usize>,
    /// commit_type -> count (e.g., "feat" -> 30)
    types: HashMap<String, usize>,
    /// significant tokens -> count from commit messages
    tokens: HashMap<String, usize>,
    /// full commit messages for exact matching
    messages: HashSet<String>,
    /// subject lines (first line) for exact matching
    subjects: HashSet<String>,
    /// total commits sampled
    commit_count: usize,
}
```

## Profile Building

During the existing `discover_repos()` walk, after finding each repo:

1. Walk all commits on all branches via `repo.revwalk()`
2. For each commit:
   - Store full message in `messages`
   - Store first line (subject) in `subjects`
   - Parse conventional commit format: extract scope into `scopes`, type into `types`
   - Tokenize subject line: split on whitespace/punctuation, lowercase, skip stopwords, count into `tokens`
3. Reuse existing `parse_commit_scope()` and `parse_commit_type()` from `git.rs`

### Stopword List

Hardcoded small set of common English words plus git-specific noise: "the", "a", "an", "and", "or", "to", "for", "in", "of", "with", "from", "merge", "branch", "commit", "update", "add", "remove", "change" (the verb — not the conventional commit type prefix which is extracted separately).

## Matching Algorithm

For each unresolved commit, matching happens in priority order.

### Tier 1: Exact Message Match

1. Check full commit message against every repo's `messages` set
2. If no hit, check subject line against every repo's `subjects` set
3. Exactly one repo matches -> assign (labelled "exact match")
4. Multiple repos match -> leave as `unknown`, report ambiguity

### Tier 2: Profile Scoring

Only runs if Tier 1 produces no result.

```
score = (scope_score * 5.0) + (token_score * 3.0) + (type_score * 1.0)
```

- **scope_score**: 1.0 if commit's scope exists in repo's `scopes` map, else 0.0
- **token_score**: fraction of commit's tokens found in repo's `tokens` map (`intersection / commit_token_count`)
- **type_score**: 1.0 if commit's type exists in repo's `types` map, else 0.0

Selection rules:
- Pick highest-scoring repo
- If top score < 1.0 (minimum threshold) -> leave as `unknown`
- If top two scores within 10% of each other -> treat as ambiguous, leave as `unknown`

## Integration with Fixup

### Updated Flow Per Image

1. Read metadata, extract SHA
2. Search repos for SHA -> found: fix repo name (unchanged)
3. Not found + whitelisted via `--keep-unresolved` -> keep as-is (unchanged)
4. Not found + not whitelisted -> **run matching against repo profiles**:
   - Exact match -> assign, report as `[guessed-exact]`
   - Profile match -> assign, report as `[guessed-profile]`
   - No match / ambiguous -> `unknown` (unchanged)

### New FixAction Variants

```rust
enum FixAction {
    Fix { ... },            // SHA found in repo
    GuessedExact { ... },   // exact message match
    GuessedProfile { ... }, // profile-based match with score
    KeysOnly,
    Skip,
}
```

### CLI Changes

New flags:

- `--no-guess` — disable fingerprinting, revert to current behaviour (all unresolved -> `unknown`)
- `--glob <PATTERN>` — filter image files by filename glob (e.g., `--glob 'unknown-*'`). Matches against filename only (not full path). Default: no filter (all `.png` files).

### Dry-run Output

```
[guessed-exact] old-repo-20260210-abc123.png
  repo: old-repo -> sw1nn-lolcommits-rs (exact message match)

[guessed-profile] worktrees-20260301-def456.png
  repo: worktrees -> my-other-project (scope: "server", score: 8.2)

[unresolved] mystery-20260315-ghi789.png
  (no match found, will be renamed to 'unknown')
```

## Design Decisions

- **All commits sampled** — repo histories are small enough; thoroughness over speed
- **Exact match before profiling** — rebased commits often have identical messages, this is a near-certainty signal
- **Ambiguous exact matches left as unknown** — safer than guessing between multiple repos
- **Profile-based tie-breaking not used for exact match ambiguity** — keep the two tiers independent for simplicity and predictability
- **Stopwords hardcoded** — small list, no need for external data
- **Glob filters on filename only** — all images live in a single flat directory
