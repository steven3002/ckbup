# ckbup

Installer for official releases of CKB development tools.

> **Early development.** ckbup installs, lists, switches and removes `ckb`,
> `ckb-cli`, `ckb-debugger`, `fnn` and `ckb-light-client`. `moleculec` and the
> `update` command are planned.

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
ckbup list                                 # installed versions of every tool
ckbup list fnn                             # installed versions of one tool, with their folders
ckbup list --available                     # newest stable releases on GitHub
ckbup use fnn@v0.9.1                       # make an installed version active
ckbup remove fnn@v0.10.1                   # delete an installed version
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

Installing another version of a tool later keeps the active one and prints the
`ckbup use` command that switches to the new one.

A checksum mismatch stops the install and deletes the download.

If `~/.ckbup/bin` is not on your `PATH`, ckbup prints the command to add it for
your shell. It never edits shell profiles itself.

## Managing installed versions

### List

`ckbup list` shows every supported tool with its installed versions, newest
first, and marks the active one:

```text
$ ckbup list
ckb               not installed
ckb-cli           not installed
ckb-debugger      not installed
fnn               v0.10.1, v0.9.1 (active)
ckb-light-client  not installed
```

`ckbup list <tool>` shows each installed version of one tool with its folder.
`list` only reads; it never changes anything.

`ckbup list --available` shows the newest 10 stable releases of each tool on
GitHub, with their dates, which ones are installed or active, and what your OS
and CPU lack for them. Drafts, releases marked as prereleases and tags with a
`-` suffix (such as `-rc1`) are left out. It costs one GitHub API request per
tool, five for all tools. Output from November 2026:

```text
$ ckbup list --available ckb-light-client
ckb-light-client (nervosnetwork/ckb-light-client), newest stable releases for windows/x86_64:
  v0.5.4  2026-01-02  active
  v0.5.3  2025-09-09
  v0.5.2  2025-08-28
  v0.5.1  2025-07-01
  v0.5.0  2025-06-19
  v0.4.1  2024-11-13  no checksum (needs --allow-unverified)
  v0.4.0  2024-11-05  no checksum (needs --allow-unverified)
  v0.3.8  2024-08-12  no checksum (needs --allow-unverified)
  v0.3.7  2024-04-12  no checksum (needs --allow-unverified)
  v0.3.6  2024-02-08  no checksum (needs --allow-unverified)
Older releases: https://github.com/nervosnetwork/ckb-light-client/releases
```

A release that has no archive for your platform shows
`no archive for <os>/<arch>`.

### Switch

`ckbup use <tool>@<version>` makes an installed version active: it puts that
version's binaries in `~/.ckbup/bin/` in place of the previous ones and records
it in `active.toml`. It works offline. A version that is not installed is an
error, and nothing changes:

```text
$ ckbup use fnn@v0.9.0
error: fnn v0.9.0 is not installed. Installed: v0.10.1, v0.9.1. Install it with: ckbup install fnn@v0.9.0
```

### Remove

`ckbup remove <tool>@<version>` deletes an installed version. The active
version cannot be removed while other versions are installed; switch first:

```text
$ ckbup remove fnn@v0.9.1
error: fnn v0.9.1 is the active version. Switch first, for example: ckbup use fnn@v0.10.1
```

Removing the only installed version of a tool also takes its binaries out of
`~/.ckbup/bin/` and clears its active version.

### On Windows

Windows does not let a program overwrite or delete an executable while it
runs, but it can rename it. So:

- before ckbup replaces or removes a binary in `~/.ckbup/bin/`, it renames it
  to `<name>.old` (or `<name>.old.1` and so on). A renamed copy that is still
  running stays until the next time ckbup installs, switches or removes that
  tool;
- `remove` first renames the version folder to a hidden
  `.<version>.removing`. If a binary from it is still running, the version is
  already gone from `ckbup list`, and the folder is deleted on a later run;
- a version folder that a terminal or another program uses as its working
  directory cannot be removed until it is closed. `remove` says so and changes
  nothing.

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
Switching or removing ckb never touches the `ckb-cli` tool's binary.

### Platforms

Builds published by the latest stable releases (checked November 2026):

| Release | Linux x86_64 | Linux aarch64 | macOS x86_64 | macOS aarch64 | Windows x86_64 |
|---|---|---|---|---|---|
| ckb v0.210.0 | standard, portable | standard ¹ | standard, portable | standard, portable | standard |
| ckb-cli v2.0.0 | yes | yes | yes | yes | yes |
| ckb-debugger v1.1.2 | yes | yes | none | yes | yes |
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

ckbup also leaves short-lived entries there:

- `toolchains/<tool>/.<version>.partial` while an install moves a package in;
- `toolchains/<tool>/.<version>.removing` (or `.removing.1` and so on) for a
  removed version whose folder could not be deleted yet;
- `bin/<name>.old` (or `<name>.old.1` and so on) for a replaced binary that
  was still running.

`list` ignores them. ckbup deletes the last two the next time it installs,
switches or removes that tool, and the next install of that version replaces a
`.partial` folder.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -- --ignored                 # GitHub requests and real installs into a temporary home
```

## Author

Steven Hert
