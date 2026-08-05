# CI / CD

CDViewer uses four GitHub Actions workflows under [`.github/workflows/`](../.github/workflows/).
This document explains what each one does, when it runs, and how to cut a
release.

| Workflow | File | Trigger | What it does |
|---|---|---|---|
| CI | [`ci.yml`](../.github/workflows/ci.yml) | `push` to `main`/`master`, every PR, `workflow_dispatch` | `cargo clippy -- -D warnings` + `cargo test` on `x86_64-pc-windows-msvc` |
| Release | [`release.yml`](../.github/workflows/release.yml) | Tags matching `v*` | Builds the `x86_64-pc-windows-msvc` binary and publishes a GitHub Release |
| Security | [`security.yml`](../.github/workflows/security.yml) | `push`, PR, daily cron, `workflow_dispatch` | Gitleaks (secret scanning) + Semgrep (SAST) |
| Docs | [`docs.yml`](../.github/workflows/docs.yml) | `push` to `main`/`master`, `workflow_dispatch` | Builds `cargo doc` and deploys to GitHub Pages |

## CI workflow

[`ci.yml`](../.github/workflows/ci.yml) is the day-to-day gate on every push
and PR. The matrix is:

| Target | Runner | Tests run? |
|---|---|---|
| `x86_64-pc-windows-msvc` | `windows-latest` | ✅ |

The single job:

1. Installs the stable toolchain via `dtolnay/rust-toolchain@stable` with
   the target and `clippy` added.
2. Caches `~/.cargo` and `target/` with `Swatinem/rust-cache@v2`, keyed by
   target (`${{ matrix.target }}`).
3. Runs `cargo clippy --all-targets --all-features --target <TARGET> -- -D warnings`.
   The `-D warnings` is a hard gate.
4. Runs `cargo test --all-features --target <TARGET>`.

The matrix is kept (with `fail-fast: false`) even though it has one entry,
so a target can be added back by appending a row.

### Why Windows x86-64 only

Windows x86-64 is the only target anyone is shipped: the viewer goes out on
study CDs burned for Windows desktops. The Linux, macOS and Windows-ARM legs
were removed because they burned CI minutes on platforms that never receive a
build. Dropping `aarch64-pc-windows-msvc` loses nothing — ARM Windows runs the
x86-64 binary under emulation.

Local Linux development still needs the eframe system packages; see the
`apt-get install` line in [`README.md`](../README.md). CI no longer installs
them because no CI job runs on Linux any more (except the docs build).

> **Integration tests auto-skip if `sample-data/` is absent**, so CI passes
> without the fixtures being checked in. Locally, drop your own anonymised
> samples into `sample-data/ct-data/` and `sample-data/mg-data/` if you want
> the tests to actually exercise the DICOM pipeline.

## Release workflow

[`release.yml`](../.github/workflows/release.yml) only fires on tags
matching `v*`. The build matrix:

| Target | Runner | Archive | Bin | Notes |
|---|---|---|---|---|
| `x86_64-pc-windows-msvc` | `windows-latest` | `.zip` | `dicom-viewer.exe` | The only shipped target |

Nothing else is built — see [Why Windows x86-64 only](#why-windows-x86-64-only)
above. The staging script still carries a `.tar.gz` branch, unused while the
matrix is Windows-only.

### What the release job does

1. Checks out the tagged commit.
2. Builds with `cargo build --release --locked --target <TARGET>`. The
   `--locked` flag is why we commit `Cargo.lock`. The MSVC C runtime is
   statically linked via
   [`.cargo/config.toml`](../.cargo/config.toml)
   (`rustflags = ["-C", "target-feature=+crt-static"]` for the
   `*-pc-windows-msvc` targets), so the shipped `.exe` runs without the
   Visual C++ Redistributable installed — it costs a few hundred KB and
   buys "double-click and it runs" on machines nobody can install onto.
3. Stages the artefact under `dist/release/dicom-viewer-<tag>-<target>/`,
   bundling:
   - the binary,
   - [`dist/README.txt`](../dist/README.txt),
   - [`dist/HELP.txt`](../dist/HELP.txt),
   - [`LICENSE`](../LICENSE),
   - [`CHANGELOG.md`](../CHANGELOG.md),
   - [`dist/autorun.inf`](../dist/autorun.inf) (Windows only).
4. Packs into a `.zip` with `7z` and uploads it as a workflow artefact
   (`if-no-files-found: error`).

### The release publication step

After the build job succeeds, the `release` job (on `ubuntu-latest`):

1. Downloads every artefact (`merge-multiple: true`).
2. Calls `softprops/action-gh-release@v2` with `generate_release_notes: true`
   and `fail_on_unmatched_files: true`.
3. Attaches the archive to a new GitHub Release for the tag.

GitHub's auto-generated notes come from PRs merged since the previous tag —
they complement, not replace, the curated entries in
[`CHANGELOG.md`](../CHANGELOG.md).

### How to cut a release

```bash
# 1. Update CHANGELOG.md: move items from [Unreleased] into a new
#    [X.Y.Z] - YYYY-MM-DD section. Add comparison links at the bottom.
# 2. Bump version in Cargo.toml. Run `cargo build` so Cargo.lock updates.
# 3. Commit.
git commit -am "Release vX.Y.Z"

# 4. Tag (annotated, signed if you have a GPG key).
git tag -a vX.Y.Z -m "vX.Y.Z"

# 5. Push the commit and the tag.
git push origin main
git push origin vX.Y.Z
```

Pushing the tag triggers `release.yml`. Watch it in the **Actions** tab.
The Windows build job runs first; the `release` job depends on it
succeeding.

If something goes wrong before the GitHub Release is created, you can
delete the tag with `git push --delete origin vX.Y.Z`, fix the issue, and
re-tag. If the Release was already created, delete it in the GitHub UI as
well before re-tagging.

> **Never force-push to a tag.** Delete and recreate instead.

## Docs workflow

[`docs.yml`](../.github/workflows/docs.yml) builds the rustdoc site for
the library crate and deploys it to GitHub Pages on every push to `main`.

The flow:

1. Install the eframe Linux system deps (the docs job is the only one that
   runs on Linux — same set as the `apt-get install` line in
   [`README.md`](../README.md)).
2. `cargo doc --no-deps --lib` with `RUSTDOCFLAGS="-D warnings"` — any
   broken intra-doc link fails the build.
3. Stage `target/doc/` as `site/`, drop a `.nojekyll` marker so
   Pages doesn't strip files beginning with `_`, and add a root
   `index.html` redirect to `dicom_viewer/index.html` so the Pages URL
   lands on the crate page.
4. `actions/configure-pages@v5` + `actions/upload-pages-artifact@v3` +
   `actions/deploy-pages@v4` publish to the `github-pages` environment.

### One-time setup

Before the first run, enable Pages in the repository settings:

1. **Settings → Pages → Build and deployment → Source → GitHub Actions.**
2. No branch needs to be configured; the workflow uploads its own
   artefact.

After the workflow's first successful deploy, the site is reachable at
`https://<owner>.github.io/<repo>/` (the workflow's "Deploy" job prints
the URL).

The build is gated by `-D warnings` so any unresolved `[item]` link fails
CI. If you need to ship a doc with an unresolved link temporarily, use
plain text or backticks instead of doc-link syntax.

## Security workflow

[`security.yml`](../.github/workflows/security.yml) runs:

- **Gitleaks** — secret scanning. Runs with `fetch-depth: 0` so commits that
  introduced a secret can be located by SHA, not just diffed against the
  previous push. Uploads SARIF findings to GitHub Advanced Security and
  comments on the PR.
- **Semgrep** — SAST via the official `semgrep/semgrep` container, running
  `semgrep ci`. Skipped on Dependabot PRs.

Both run on `push`, every PR, and a daily cron (`0 6 * * *`) so the latest
rulesets keep grading existing history clean.

False positives can be silenced in [`.semgrepignore`](../.semgrepignore).

> **If you find a vulnerability**, please don't open a public issue. Email
> the maintainer privately — see the contact in the project's GitHub
> profile.

## Pre-commit hooks (local)

[`.pre-commit-config.yaml`](../.pre-commit-config.yaml) mirrors the CI gates
on your machine:

- `cargo fmt` runs on every commit (cheap).
- `cargo clippy --all-targets --all-features -- -D warnings` and
  `cargo test` only fire on `git push` (`stages: [pre-push]`) to keep
  day-to-day commits snappy.

```bash
pip install pre-commit
pre-commit install                       # commit hook
pre-commit install --hook-type pre-push  # push hook
```

The top-level `exclude:` block keeps the whitespace fixers and large-file
check away from `Cargo.lock`, `sample-data/`, `target/`, and
`dist/cd-staging/`.

## Caching

`Swatinem/rust-cache@v2` caches `~/.cargo/registry`, `~/.cargo/git`, and
`target/` keyed by target and `Cargo.lock` (`${{ matrix.target }}` in CI,
`${{ matrix.target }}-release` in the release build, `docs` in the docs
build). A first run on Windows is slow (~15 min for a full release build).
Cached runs are typically 2–4 minutes for CI and 5–8 minutes for release
builds.

If you suspect a corrupted cache, bump the suffix in the `key:` field of
the relevant workflow (e.g. `${{ matrix.target }}-release` → `…-release-v2`).
