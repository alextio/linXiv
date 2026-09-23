# Contributing to linXiv

Thanks for helping out. This page covers how changes get from your fork into a release. For building and running the app, see [Setup](README.md#setup) in the README.

## One-time setup

Turn on the repo's git hooks. They check commit messages and, when you touch Rust, run `cargo fmt --check`:

```sh
git config core.hooksPath .githooks
```

## Branches

linXiv follows [GitLab Flow](https://about.gitlab.com/topics/version-control/what-is-gitlab-flow/) with release branches:

- **`main`** is the only long-lived branch. Branch from it and open your PR against it.
- **Name your branch `<type>/<short-slug>`**, using the same types as commit messages: `feat/`, `fix/`, `refactor/`, `perf/`, `docs/`, `test/`, `chore/`, `build/`, `ci/`, `style/`, `revert/`. For example: `fix/pdf-preview-flash`, `feat/zotero-export`.
- **Releases are tags** on `main`: `v0.6.0`, or `v0.6.0-beta.1` / `-rc.1` / `-alpha.1` for a pre-release. Pushing a tag builds the installers, and a hyphen marks the release as a pre-release. Never reuse a pre-release's version for a stable release (after `v0.6.0-beta.1`, the stable release can't be `v0.6.0`): RPM would sort the beta above it.
- **Release branches follow the tags.** Tagging `v0.6.0` on `main` creates `release/0.6`. Each later `v0.6.x` tag on `main` opens a PR that brings `release/0.6` up to that tag as one squashed commit; if earlier syncs were skipped, that one commit covers the whole gap. Once `main` moves on to 0.7, `release/0.6` stops syncing and stays as the 0.6 line.
- **Patching an old line:** open a PR against `release/0.6` (cherry-pick with `git cherry-pick -x` if the fix is already on `main`), then tag the patch on that branch (`v0.6.6`). A patch is a normal release, but it won't be marked as latest once a newer line has shipped.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/): `type(scope): imperative summary`, at most 72 characters, no trailing period. A body is optional: one or two sentences of why, at most 5 lines and 150 characters. Longer explanations belong in the PR description or docs. The full rules are in [`.githooks/commit-lint.sh`](.githooks/commit-lint.sh).

## Pull requests

- Fill in the PR template, especially **Network calls** and **Debt or workarounds**.
- Keep PRs small and focused; split unrelated changes.
- CI checks your branch name, every commit message, the Rust tests and `cargo fmt`, the frontend type check and tests, and CodeQL. All must pass, and a maintainer must approve the PR.

## License

linXiv is licensed under [Apache-2.0](LICENSE). By submitting a contribution you agree it is licensed under the same terms.
