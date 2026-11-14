# ckbup

Installer for official releases of CKB development tools.

> **Early development.** ckbup installs `ckb`, `ckb-cli`, `ckb-debugger`, `fnn`
> and `ckb-light-client`. `moleculec` and the `list`, `use`, `update` and
> `remove` commands are planned.

ckbup downloads prebuilt binaries from each project's GitHub Releases, checks
them against the published SHA-256 checksums, and keeps every installed version
under one directory.

## Build

Requires Rust 1.88 or newer.

```sh
cargo build --release
```

The binary is written to `target/release/ckbup` (`ckbup.exe` on Windows).

## Usage

```sh
ckbup install ckb                          # latest stable release
ckbup install fnn@v0.9.1                   # a specific release tag (the leading "v" is optional)
ckbup install ckb-light-client --portable  # prefer the portable build
```

Each install:

1. resolves the release with a single GitHub API request;
2. picks the archive for your OS and CPU from that release's asset list;
3. downloads it and verifies its SHA-256 against the asset digest reported by
   GitHub and, for ckb-debugger, against the project's own `-sha256.txt` file;
4. extracts it and keeps the whole release package (binaries, config templates
   and docs) in `~/.ckbup/toolchains/<tool>/<version>/`;
5. on the first install of a tool, copies its binaries into `~/.ckbup/bin/` and
   records it as the active version.

A checksum mismatch stops the install and deletes the download.

If `~/.ckbup/bin` is not on your `PATH`, ckbup prints the command to add it for
your shell. It never edits shell profiles itself.

## Supported tools

| Tool | Repository | Put on `PATH` |
|---|---|---|
| ckb | nervosnetwork/ckb | `ckb` |
| ckb-cli | nervosnetwork/ckb-cli | `ckb-cli` |
| ckb-debugger | nervosnetwork/ckb-standalone-debugger | `ckb-debugger` |
| fnn | nervosnetwork/fiber | `fnn`, `fnn-cli` |
| ckb-light-client | nervosnetwork/ckb-light-client | `ckb-light-client` |

The ckb package also contains a copy of `ckb-cli`. ckbup keeps it inside the
package and never puts it on `PATH`; install the `ckb-cli` tool for that.

### Platforms

Builds published by the latest stable releases (checked November 2026):

| Release | Linux x86_64 | Linux aarch64 | macOS x86_64 | macOS aarch64 | Windows x86_64 |
|---|---|---|---|---|---|
| ckb v0.210.0 | standard, portable | standard ¹ | standard, portable | standard, portable | standard |
| ckb-cli v2.0.0 | yes | yes | yes | yes | yes |
| ckb-debugger v1.1.1 | yes | yes | none | yes | yes |
| fnn v0.10.1 | standard, portable | portable | portable | portable | standard |
| ckb-light-client v0.5.4 | standard, portable | none ² | standard, portable | none ² | standard |

1. ckb's Linux aarch64 archive has no `-portable` suffix, but upstream builds it
   as a portable build.
2. The prerelease v0.5.5-rc1 adds Linux and macOS aarch64 builds:
   `ckbup install ckb-light-client@v0.5.5-rc1`.

ckbup decides from the asset list of the release you ask for, so older
releases may cover fewer hosts. No release of any tool has a Windows arm64
binary; ckbup reports that before it contacts GitHub.

### Standard and portable builds

Portable builds compile RocksDB with its `portable` feature. The ckb release
notes suggest them when a binary "complains about illegal instructions".

- ckb, ckb-cli and ckb-debugger: ckbup installs the standard build, or the
  portable one where only that exists.
- fnn and ckb-light-client: ckbup installs the portable build where one exists,
  because it runs on older Linux systems (see below), and the standard one
  otherwise.
- `--portable` puts the portable build first for any tool. If the release has no
  portable build for your host, ckbup says so and installs the standard one.

ckbup never installs the CentOS builds that ckb and ckb-cli also publish;
upstream has announced that they will end.

### Minimum glibc on Linux

Read from the binaries of these releases:

| Build | Minimum glibc |
|---|---|
| ckb v0.210.0, x86_64 (standard and portable) | 2.30 |
| ckb v0.210.0, aarch64 | 2.34 |
| fnn v0.9.1, x86_64 standard | 2.39 |
| fnn v0.9.1, x86_64 and aarch64 portable | 2.34 |
| ckb-light-client v0.5.4, x86_64 standard | 2.38 |
| ckb-light-client v0.5.4, x86_64 portable | 2.34 |

Every Linux build also needs `libstdc++.so.6` and `libgcc_s.so.1`.

### Windows runtime for ckb-light-client

`ckb-light-client.exe` (v0.5.4) imports the Microsoft C and C++ runtime DLLs
`vcruntime140.dll`, `vcruntime140_1.dll` and `msvcp140.dll`. If it does not
start because one of them is missing, install the
[Microsoft Visual C++ Redistributable for x64](https://aka.ms/vc14/vc_redist.x64.exe).
`ckb.exe`, `fnn.exe` and `fnn-cli.exe` do not import these DLLs.

## Releases without checksums

In these repositories, the assets of releases published before June 2025 carry
no SHA-256 digest on GitHub, and ckb, fnn and ckb-light-client publish no
checksum files. ckbup refuses to install such a release:

```text
$ ckbup install ckb-light-client@v0.4.1
error: GitHub lists no SHA-256 for ckb-light-client_v0.4.1-x86_64-windows.tar.gz, and ckb-light-client publishes no checksum file for v0.4.1; older releases often have none. Re-run with --allow-unverified to install it without verification
```

`--allow-unverified` installs it anyway and prints a warning. Use it only for a
release you have another reason to trust.

## Config templates and docs

Each install keeps the release's whole package and prints where its extras are:

| Tool | In `~/.ckbup/toolchains/<tool>/<version>/` |
|---|---|
| fnn | `config/mainnet/config.yml`, `config/testnet/config.yml` |
| ckb-light-client | `config/mainnet.toml`, `config/testnet.toml` |
| ckb | `docs/` (including `configure.md` and `quick-start.md`); on Linux and macOS also `init/` with systemd unit examples |

ckb ships no config file; `ckb init` creates one. Copy a template to your own
working directory before you edit it, because the version folder belongs to
ckbup.

## Configuration

| Variable | Effect |
|---|---|
| `CKBUP_HOME` | Directory for installed tools. Default: `~/.ckbup`. |
| `GITHUB_TOKEN` | Optional. Sent to the GitHub API to raise the unauthenticated limit of 60 requests per hour. |

## Layout

```text
~/.ckbup/
├── active.toml                         active version per tool
├── bin/                                copies of the active binaries
└── toolchains/<tool>/<version>/        every installed release package
```

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -- --ignored                 # real installs from GitHub into a temporary home
```

## Author

Steven Hert
