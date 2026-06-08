# Architecture Review Seed Cases

These cases benchmark the repo-local architecture reviewer as a detection system.

`cases.toml` contains positive cases the reviewer should flag and negative controls it should not flag. The cases are advisory fixtures, not CI gates.

## How To Use

1. Start from a clean disposable branch or worktree.
2. Apply one fixture from `cases.toml`.
3. Run `make arch-report`.
4. Run the `ump-dash-architecture-review` skill against the changed file.
5. Record whether the reviewer found the expected category and invariant.
6. Revert the fixture before applying the next case.

## Scoring

Track:

- schema validity
- precision
- recall
- high-severity recall
- false positives on negative cases
- duplicate finding rate
- vague finding rate
- location accuracy

The reviewer should not become a CI gate until deterministic checks are stable and negative controls produce low noise.
