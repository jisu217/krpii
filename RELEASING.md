# Releasing

A version published to crates.io can be yanked but never replaced. Follow the steps in order.

## One-time setup

Sign in to [crates.io](https://crates.io) with GitHub, verify your e-mail address, create an API
token with the `publish-new` and `publish-update` scopes, and store it with `cargo login`.

## Steps

1. **Bump the version** in the root `Cargo.toml`: `[workspace.package]` and the `krpii-core`
   entry in `[workspace.dependencies]`.
2. **Update `CHANGELOG.md`**: move `[Unreleased]` entries under a dated version heading and
   update the links at the bottom.
3. **Merge** the change through a pull request with CI passing.
4. **Dry-run the binaries** (optional): run the *Release* workflow manually from the Actions
   tab. It builds every platform without creating a release.
5. **Publish the crates** from an up-to-date `main`:

   ```console
   $ git switch main && git pull
   $ cargo publish --workspace --dry-run
   $ cargo publish --workspace
   ```

6. **Tag the release**, which builds the binaries and publishes the GitHub release:

   ```console
   $ git tag -a v0.1.0 -m "v0.1.0"
   $ git push origin v0.1.0
   ```

7. **Verify** that the release lists five archives with checksums and that
   `cargo install krpii` installs the new version.
