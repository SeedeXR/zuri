# Publishing the npm packages

`@seedexr/zuri` (`packages/web`) and `@seedexr/zuri-react` (`packages/react`) are published
together, at one version.

## A release

1. Set the same `version` in `packages/web/package.json` and `packages/react/package.json`.
2. Commit, and get it to `main` through the usual gate.
3. Tag that commit `v<version>` and push the tag: `.github/workflows/npm.yml` builds, tests,
   packs and publishes both, the web package first (the React one names it as a peer).
   (The same tag has `editor.yml` build the desktop editor's installers, which it keeps
   as artifacts and does not release, and `ci.yml` run the gate once more. The publish
   waits for neither: push the tag after `main` is green, not with it.)

Run the workflow by hand (Actions > npm > Run workflow) to rehearse: it does all of that
with `npm publish --dry-run` in place of the upload, for a version that is not out yet
(one that is out is passed over, in a rehearsal too). If a release gets one package out and fails on the other, run the
failed job again: what is out already is passed over. A version, once out, cannot be
put out again: to change it, publish the next one.

## Trusted publishing: set once, per package

The workflow has no token. npm takes GitHub's word that this workflow of this repository
is running (OIDC), and attests where the package was built. For that, each package needs a
trusted publisher, set at npmjs.com by an owner: package > Settings > Trusted Publisher >
GitHub Actions, with

| Field | Value |
|---|---|
| Organization or user | `SeedeXR` |
| Repository | `zuri` |
| Workflow filename | `npm.yml` |
| Environment name | (empty) |

and `npm publish` among the allowed actions. Every field is case-sensitive, and npm does
not check them when they are saved: a mistake shows at the first publish, which has to
happen within two days of saving or the setting lapses (it then has to be deleted and
made again). So save it when the next version is ready to tag, not before. A package
has to exist before it can be given one ("The package you're configuring must already
exist on the npm registry", npm's page on `npm trust`), so the first version of each is
published by hand (below).

Anyone who can push a `v*` tag to the repository publishes, from whatever commit the
tag is on, after this workflow's own build and tests and not the full gate: tag only
what is on `main`. To hold it tighter, protect `v*` tags with a ruleset, or give the
workflow a GitHub environment with reviewers and name it at npm.

Once a release has gone out this way: package > Settings > Publishing access > "Require
two-factor authentication and disallow tokens", and revoke the tokens that were made for
publishing. The workflow goes on working; a leaked token cannot publish.

Needs: npm 11.5.1 or later and Node 22.14 or later (the workflow installs them), a
GitHub-hosted runner, and `repository.url` in each `package.json` naming this repository.

Sources: <https://docs.npmjs.com/trusted-publishers> and npm's pages on `npm trust` and on
access tokens, read 2026-11-18.

## By hand

From a clean checkout of the commit, with a login that may publish to `@seedexr`:

```bash
pnpm install --frozen-lockfile
(cd packages/web && pnpm publish --no-git-checks)      # builds first (prepack)
(cd packages/react && pnpm publish --no-git-checks)
```

`pnpm publish`, not `npm publish`: pnpm writes the React package's `workspace:^` peer as
the version it stands for. The account asks for a one-time password on a publish: add
`--otp=<code>`. The one kind of token that is exempt is a granular access token made
with "Bypass 2FA" (npm has no other kind since November 2025, and says that publishing
with those ends in January 2027). Once tokens are disallowed for a package (above),
only a login with its one-time password can publish it by hand.
