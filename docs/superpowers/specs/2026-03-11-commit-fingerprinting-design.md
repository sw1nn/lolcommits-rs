# Commit Fingerprinting for Unknown Repo Resolution

## Problem

When `lolcommits_fixup` cannot find a commit SHA in any workspace repo (e.g., because the commit was rebased or force-pushed), the image is marked as `unknown`. Many of these commits could be matched to their correct repo using commit message characteristics.

## Solution

Build a profile of each repo's commit messages during the workspace scan, then use those profiles to suggest which repo an unresolved commit belongs to.

## Data Model

```rust
struct RepoProfile {
    repo_name: String,
    /// scope -> count. Only non-empty scopes from conventional commits.
    scopes: HashMap<String, usize>,
    /// commit_type -> count. Only real conventional commit types (feat, fix, etc.),
    /// not the "commit" fallback from parse_commit_type().
    types: HashMap<String, usize>,
    /// significant tokens -> count from commit subject lines
    tokens: HashMap<String, usize>,
    /// full commit messages (subject + body) for exact matching
    messages: HashSet<String>,
    /// subject lines (first line only) for exact matching
    subjects: HashSet<String>,
    /// total commits sampled
    commit_count: usize,
}
```

`RepoProfile` is stored as a field on the existing `RepoInfo` struct in `lolcommits_fixup.rs`, built immediately after the repo is discovered.

## Profile Building

During the existing `discover_repos()` walk, after finding each repo:

1. Walk all commits via `repo.revwalk()`. Push all local branch heads (`refs/heads/*`) as starting points. libgit2's revwalk handles deduplication internally — each commit is visited exactly once regardless of how many branches reach it.
2. For each commit:
   - Store full message (trimmed) in `messages`
   - Store first line (subject) in `subjects`
   - Parse conventional commit format using existing `parse_commit_scope()` and `parse_commit_type()` from `git.rs`, with filtering:
     - **Type**: only store if the message contains a `:` (indicating conventional commit format). Skip the `"commit"` fallback value that `parse_commit_type()` returns for non-conventional messages.
     - **Scope**: only store if non-empty.
   - Tokenize subject line into `tokens`
3. Profile building code lives in the fixup binary alongside `RepoInfo`.

### Tokenization Rules

- Strip the conventional commit prefix (`type(scope): `) before tokenizing
- Split on ASCII whitespace and ASCII punctuation characters
- Lowercase all tokens
- Discard tokens shorter than 2 characters
- Discard tokens that are pure numbers (e.g., "42", "123") but keep alphanumeric tokens (e.g., "v2", "api3")
- Discard stopwords (see below)

### Stopword List

Hardcoded small set of common English words plus git-specific noise: "the", "an", "and", "or", "to", "for", "in", "of", "with", "from", "merge", "branch", "commit", "update", "add", "remove", "change", "use", "new", "set", "when", "not", "into", "this", "that", "be", "is", "it", "on", "at", "by".

## Matching Algorithm

For each unresolved commit, the commit message is read from the image's PNG metadata (`lolcommit:Message` field), since the commit SHA is not resolvable in any repo by definition.

Matching happens in priority order.

### Tier 1: Exact Message Match

1. Check the image's full message against every repo's `messages` set
2. If no hit, check the image's message against every repo's `subjects` set (the PNG metadata message is typically the subject line only, so this covers the common case)
3. Exactly one repo matches -> assign (labelled "exact match")
4. Multiple repos match -> leave as `unknown`, report ambiguity

### Tier 2: Profile Scoring

Only runs if Tier 1 produces no result.

```
score = (scope_score * 5.0) + (token_score * 3.0) + (type_score * 1.0)
```

- **scope_score**: 1.0 if commit's scope (non-empty) exists in repo's `scopes` map, else 0.0. If the commit has no scope, scope_score is 0.0 for all repos.
- **token_score**: fraction of commit's tokens found in repo's `tokens` map (`intersection_count / commit_token_count`). If the commit has zero tokens, token_score is 0.0.
- **type_score**: 1.0 if commit's type (real conventional commit type, not fallback) exists in repo's `types` map, else 0.0. If the commit is not a conventional commit, type_score is 0.0 for all repos.

Selection rules:
- Pick highest-scoring repo
- If top score < 3.0 (minimum threshold — requires at least a scope match or significant token overlap) -> leave as `unknown`
- If there is only one candidate repo, no ambiguity check is needed
- If there are two or more candidates: if `second_score >= top_score * 0.9` -> treat as ambiguous, leave as `unknown`

## Integration with Fixup

### Updated Flow Per Image

1. Read metadata, extract SHA and message
2. Search repos for SHA -> found: fix repo name (unchanged)
3. Not found + whitelisted via `--keep-unresolved` -> keep as-is (unchanged)
4. Not found + not whitelisted -> **run matching against repo profiles**:
   - Exact match -> assign, report as `[guessed-exact]`
   - Profile match -> assign, report as `[guessed-profile]`
   - No match / ambiguous -> `unknown` (unchanged)

### New FixAction Variants

```rust
enum FixAction {
    Fix {
        old_repo: String,
        new_repo: String,
        old_filename: String,
        new_filename: String,
    },
    GuessedExact {
        old_repo: String,
        new_repo: String,
        old_filename: String,
        new_filename: String,
    },
    GuessedProfile {
        old_repo: String,
        new_repo: String,
        old_filename: String,
        new_filename: String,
        score: f64,
    },
    KeysOnly,
    Skip,
}
```

### CLI Changes

New flags:

- `--no-guess` — disable fingerprinting, revert to current behaviour (all unresolved -> `unknown`)
- `--glob <PATTERN>` — filter image files by filename glob (e.g., `--glob 'unknown-*'`). Matches against filename only (not full path). Default: no filter (all `.png` files). Independent of `--no-guess` — applies to all modes.

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
- **Local branches only** — push `refs/heads/*` into revwalk; remote-tracking branches would duplicate commits
- **Exact match before profiling** — rebased commits often have identical messages, this is a near-certainty signal
- **Ambiguous exact matches left as unknown** — safer than guessing between multiple repos
- **Profile-based tie-breaking not used for exact match ambiguity** — keep the two tiers independent for simplicity and predictability
- **Non-conventional commits excluded from type/scope maps** — prevents the `parse_commit_type()` fallback value `"commit"` from polluting every repo's profile
- **Minimum score threshold of 3.0** — requires meaningful signal (scope match or substantial token overlap) rather than just a shared commit type
- **Commit message from PNG metadata** — unresolved commits by definition cannot be found in git, so we use the message stored in the image's iTXt chunks
- **Stopwords hardcoded** — small list, no need for external data
- **Glob filters on filename only** — all images live in a single flat directory
- **`--glob` independent of `--no-guess`** — file filtering is useful in all modes
