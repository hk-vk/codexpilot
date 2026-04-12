# Codexpilot Prerelease Checklist

This runbook is the safest path to the first public `codexpilot` npm prerelease.

## Goal

Ship a prerelease that verifies:
- fork-owned GitHub release artifacts are produced correctly
- npm staging uses `codexpilot` package naming
- packaged native binaries match the expected target platform
- npm publishing works for the fork

## Current repo status

Already fixed in the repo:
- npm package naming is `codexpilot`
- split npm packaging flow exists (meta package + platform-tagged variants)
- staging scripts are fork-aware instead of hardcoded to `openai/codex`
- packaging now rejects a glibc/dynamic binary placed in a musl vendor slot
- release assets now use `codexpilot-npm-*` naming

Known remaining external checks:
- GitHub Actions release workflow must run successfully in the fork
- npm publishing must be allowed for package name `codexpilot`
- one prerelease publish must complete successfully before update-nudge behavior can be validated in the real world

## Recommended first release type

Use an alpha prerelease first, for example:
- `0.1.0-alpha.1`

This is safer than a stable `0.1.0` because:
- npm dist-tag can remain `alpha`
- the update prompt should not push most users to a prerelease unless explicitly installed that way
- artifact and package naming can be validated with lower risk

## Preconditions

Before tagging:
1. Ensure the working tree is clean except for intentional local-only files.
2. Confirm the fork remote is correct:
   - `git remote -v`
3. Confirm GitHub auth is active:
   - `gh auth status`
4. Confirm npm auth is active:
   - `npm whoami`
5. Confirm `codexpilot` is still unclaimed or owned by the intended npm account.

## Release workflow path

The release workflow is tag-driven.

Expected tag format:
- `rust-v0.1.0-alpha.1`

Recommended sequence:
1. Update version metadata in the repo if needed.
2. Commit all release-related changes.
3. Create the prerelease tag:
   ```bash
   git tag -a rust-v0.1.0-alpha.1 -m "Release 0.1.0-alpha.1"
   ```
4. Push the tag:
   ```bash
   git push origin rust-v0.1.0-alpha.1
   ```
5. Wait for [rust-release.yml](../../.github/workflows/rust-release.yml) to finish in the fork.

## Artifact verification

After the workflow finishes:
1. Download the release artifacts.
2. Verify the npm tarball asset names use `codexpilot-npm-*`.
3. Verify staged package names:
   - meta package name is `codexpilot`
   - Linux payload version is like `0.1.0-alpha.1-linux-x64`
4. Verify native binary kind matches target.

Important check for Linux musl payloads:
- the packaged binary must not be a glibc dynamic ELF in a musl target directory
- staging now fails loudly if that happens

## NPM publish checks

Before real publish, perform a dry run on the staged package(s) where possible.

For the meta package, expected result looks like:
- package name: `codexpilot`
- tiny launcher package
- optional dependencies point at platform-tagged `codexpilot` variants

For platform packages, expected result looks like:
- package name: `codexpilot`
- version includes platform suffix, e.g. `0.1.0-alpha.1-linux-x64`
- only the target-specific native payload is included

## Safe publish order

If the staged artifacts are correct:
1. Publish platform packages first using prerelease tags.
2. Publish the meta package last.
3. Verify npm install flow on a clean machine or isolated temp environment:
   ```bash
   npm install -g codexpilot@alpha
   codexpilot --version
   ```

## Post-publish checks

After publish:
1. Verify `npm view codexpilot version` returns the published version.
2. Verify install command works for the target platform.
3. Verify the app starts and reports the expected version.
4. Verify the update-check endpoint is live for `codexpilot`:
   - `https://registry.npmjs.org/codexpilot/latest`

## Update prompt caveat

The update nudge cannot be fully validated in the real world until:
1. one version of `codexpilot` is published
2. a newer version is published later
3. an older installed build checks the registry and sees the newer version

So the first prerelease validates publish/install.
The second prerelease validates the real npm-backed update prompt.

## Stop conditions

Do not publish if any of these happen:
- release workflow artifacts are missing for one or more required targets
- package names are not `codexpilot`
- Linux musl package validation detects a glibc/dynamic binary in the musl slot
- npm publish tries to target the wrong package name or dist-tag
- npm ownership / publisher setup is not correct

## Minimal success definition

The first prerelease is a success if:
- GitHub release artifacts are produced from the fork
- staged tarballs use `codexpilot` naming
- npm publish succeeds for the prerelease tag
- `npm install -g codexpilot@alpha` works on at least one target platform
