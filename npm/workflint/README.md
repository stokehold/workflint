# workflint

workflint is a linter for GitHub Actions workflows and action metadata files, written in Rust. It is a compatible rewrite of [actionlint](https://github.com/rhysd/actionlint): it accepts actionlint's flags and config file, prints the same output layout, and fixes actionlint's known bugs.

## Install

```sh
npm install --save-dev workflint
pnpm add --save-dev workflint
bun add --dev workflint
```

The package has binaries for Linux, macOS, and Windows, each on x64 and arm64. Each binary is in a package of its own, such as `@stokehold/workflint-linux-x64`, which `workflint` lists as an optional dependency. The package manager installs only the one for your machine. Nothing runs at install time, so the install also works with `--ignore-scripts` and with pnpm and Bun, which skip dependencies' install scripts by default.

An install with `--omit=optional` leaves the binary out. `workflint` then exits with code 3, names the missing package, and says how to reinstall it.

## Usage

```sh
npx workflint                  # lints .github/workflows/*.yml and *.yaml
npx workflint path/to/ci.yml   # lints the files given
npx workflint -- -color        # passes -color to workflint
pnpm exec workflint
bunx workflint
```

npx reads some single-dash flags as its own, so `npx workflint -version` prints npm's version. Put `--` before workflint's flags, as in `npx workflint -- -version`. `pnpm exec`, `bunx`, and `package.json` scripts pass them through unchanged.

Or from a script in `package.json`:

```json
{
  "scripts": {
    "lint:workflows": "workflint"
  }
}
```

workflint exits with 0 when it finds no problems, 1 when it finds some, 2 for an invalid command line, and 3 for a fatal error, as actionlint does. The [command line reference](https://github.com/stokehold/workflint/blob/main/docs/cli.md) lists every flag.

## How it works

`bin/workflint.js` picks the package for `process.platform` and `process.arch`, finds it with `require.resolve`, and runs its binary with the same arguments and standard streams. It exits with the binary's exit code, or dies of the same signal. It sets `WORKFLINT_INSTALLED_FROM=installed from npm` unless the variable is already set, so the second line of `workflint -version` says how workflint was installed.

Each platform package holds the binary of the matching [GitHub release](https://github.com/stokehold/workflint/releases) archive, byte for byte. The release workflow publishes the packages with npm trusted publishing and provenance, so npm shows which workflow run built each one.

## License

MIT. workflint is a derivative work of actionlint by rhysd, also MIT-licensed. See LICENSE and NOTICE.
