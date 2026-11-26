# ckbup

Installer for official releases of CKB development tools.

> **Early development.** ckbup installs, lists, switches, updates and removes
> `ckb`, `ckb-cli`, `ckb-debugger`, `fnn` and `ckb-light-client`. `moleculec` is
> planned.

ckbup downloads prebuilt binaries from each project's GitHub Releases, checks
them against the published SHA-256 checksums, and keeps every installed version
under one directory.

## Build

Requires Rust 1.89 or newer.

```sh
cargo build --release
```

The binary is written to `target/release/ckbup` (`ckbup.exe` on Windows).

## Usage

```sh
ckbup install ckb                          # latest stable release
ckbup install fnn@v0.9.1                   # a specific release tag (the leading "v" is optional)
ckbup install ckb-light-client --portable  # prefer the portable build
ckbup install ckb --keep-archives          # also keep the verified archive in the cache
ckbup list                                 # installed versions of every tool
ckbup list fnn                             # installed versions of one tool, with their folders
ckbup list --available                     # newest stable releases on GitHub
ckbup use fnn@v0.9.1                       # make an installed version active
ckbup update                               # move every tool that isn't held to its latest stable release
ckbup update fnn                           # move one tool, even a held one
ckbup remove fnn@v0.10.1                   # delete an installed version
```

Each install:

1. resolves the release with a single GitHub API request. Without a version,
   that is the newest stable release by version number: not a draft, not
   marked as a prerelease, and no `-` suffix (such as `-rc1`) in the tag.
   GitHub's own "latest release" follows the order of publication, so a fix
   for an older line or an unmarked release candidate could take its place;
   ckbup does not use it;
2. picks the archive for your OS and CPU from that release's asset list;
3. takes a cached copy of the archive when one matches the published checksums,
   otherwise downloads it, resuming a download that stopped part-way, and
   verifies its SHA-256 against the asset digest reported by GitHub and, for
   ckb-debugger, against the project's own `-sha256.txt` file;
4. extracts it and keeps the whole release package (binaries, config templates
   and docs) in `~/.ckbup/toolchains/<tool>/<version>/`. On Linux and macOS, a
   file the archive marks executable gets mode `0755` and every other file
   `0644`, so the copy of `ckb-cli` inside the ckb package stays executable;
5. on the first install of a tool, copies its binaries into `~/.ckbup/bin/` and
   records it as the active version. When you named the version, the tool is
   also held there (see [Update](#update)).

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
it in `active.toml`. It also holds the tool at that version, so that
`ckbup update` leaves it alone (see [Update](#update)). It works offline. A
version that is not installed is an error, and nothing changes:

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
`~/.ckbup/bin/` and clears its active version and its hold.

`remove` never deletes an archive kept in the cache; it says where it is (see
[Cache](#cache)).

### Update

`ckbup update` moves every installed tool to its latest stable release and
makes that release active. It downloads only when that release is not
installed yet. It costs one GitHub API request per tool it checks, five at most.

```text
$ ckbup update
ckb v0.210.0 is up to date (latest stable)
fnn v0.10.0 is held; move it to the latest stable release with: ckbup update fnn
```

A tool is **held** when you chose its version by name: with
`ckbup use fnn@v0.10.0`, or with `ckbup install fnn@v0.10.0` when that install
made the version active. `ckbup update` skips a held tool without any request,
because moving to a new version can need care: the ckb v0.106.0 and v0.108.0
release notes say that version "requires database migration", and fnn v0.8.0's
notes ask you to run `fnn-migrate` for its data format changes. `ckbup list`
shows a held version as `(active, held)`.

`ckbup update <tool>` moves that tool even when it is held, and releases the
hold, so later runs of `ckbup update` move it too. `ckbup install <tool>`
without a version releases a hold the same way.

The version that was active before stays installed:

```text
$ ckbup update fnn
Downloading fnn_v0.10.1-x86_64-windows.tar.gz
Verified SHA-256 against GitHub asset digest
Installed fnn v0.10.1 to C:\Users\me\.ckbup\toolchains\fnn\v0.10.1
Config templates: C:\Users\me\.ckbup\toolchains\fnn\v0.10.1\config
fnn v0.10.1 is active: C:\Users\me\.ckbup\bin\fnn.exe, C:\Users\me\.ckbup\bin\fnn-cli.exe
fnn now follows the latest stable release
fnn v0.10.0 stays installed. Switch back with: ckbup use fnn@v0.10.0. Remove it with: ckbup remove fnn@v0.10.0
```

`--remove-old` removes that version instead, as `ckbup remove` would.
`--portable`, `--allow-unverified` and `--keep-archives` work as they do for
`install`. When the active version was installed from a portable build, the
update installs the portable build too, if the new release has one.

`update` never moves a tool to an older release: an active version that is
newer than the latest stable release, such as a release candidate, is left as
it is. Naming a tool that is not installed is an error; install it with
`ckbup install <tool>`.

A failure for one tool is reported as `error: <tool>: ...`, and the other
tools are still updated; the exit code is then 1. Reaching the GitHub API rate
limit stops the whole run.

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

## Downloads and the cache

### Interrupted downloads

A download that stops part-way, because the connection dropped or ckbup was
stopped, stays in `~/.ckbup/downloads/`, and the next install or update of
the same archive asks GitHub only for the rest:

```text
$ ckbup install ckb@v0.210.0
Resuming ckb_v0.210.0_x86_64-pc-windows-msvc.zip at 6843056 of 27414343 bytes
Verified SHA-256 against GitHub asset digest
```

ckbup first checks that the bytes it has belong to the same file on GitHub.
If the file changed, it downloads it again from the start. The SHA-256 is
always checked on the complete file.

Two ckbup runs never write the same download. The second one stops at once:

```text
error: another ckbup is downloading ckb_v0.210.0_x86_64-pc-windows-msvc.zip (C:\Users\me\.ckbup\downloads\ckb_v0.210.0_x86_64-pc-windows-msvc.zip.lock); run the command again when it finishes
```

Right after a ckbup run was killed, the next one may report this once more,
until the system releases the lock of the killed process.

`install` and `update` delete download leftovers that are more than 7 days
old.

### Cache

`--keep-archives` on `install` and `update`, or `CKBUP_KEEP_ARCHIVES=1`, keeps
each verified archive in `~/.ckbup/cache/<tool>/<version>/`, so installing that
version again needs no download. Whatever the switch says, ckbup uses a cached
archive only after it matches the checksums published for the release, and
downloads it again if it does not. An archive without a published checksum is
never kept.

```text
$ ckbup install ckb-debugger@v1.1.2
Using cached C:\Users\me\.ckbup\cache\ckb-debugger\v1.1.2\ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz
Verified SHA-256 against GitHub asset digest
Verified SHA-256 against ckb-debugger_v1.1.2_x86_64-pc-windows-msvc-sha256.txt
```

`ckbup remove` never deletes cached archives. It says where they are:

```text
The archive stays in the cache: C:\Users\me\.ckbup\cache\ckb-debugger\v1.1.2 (3.0 MB). Delete it by hand to free the space.
```

To clear the whole cache, delete its folder:

```sh
Remove-Item -Recurse -Force $HOME\.ckbup\cache   # PowerShell
rm -rf ~/.ckbup/cache                            # Linux and macOS
```

With `CKBUP_HOME` set, the cache is `$CKBUP_HOME/cache` (`$env:CKBUP_HOME\cache`
in PowerShell).

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
| `CKBUP_KEEP_ARCHIVES` | `1` keeps verified archives in the cache, as `--keep-archives` does. Unset, empty or `0`: off. Any other value is an error. |
| `GITHUB_TOKEN` | Optional. Sent to the GitHub API to raise the unauthenticated limit of 60 requests per hour. Release downloads do not count against that limit. |

## Layout

```text
~/.ckbup/
├── active.toml                         active version per tool, and the held tools
├── bin/                                copies of the active binaries
├── cache/<tool>/<version>/             archives kept with --keep-archives
├── downloads/                          downloads in progress, or interrupted
└── toolchains/<tool>/<version>/        every installed release package
```

Each version folder that ckbup installs also holds `.ckbup.toml`, which
records the archive it came from and whether that was a portable build. ckbup
never copies it to `bin/`.

ckbup also leaves short-lived entries there:

- `toolchains/<tool>/.<version>.partial` while an install moves a package in;
- `toolchains/<tool>/.<version>.removing` (or `.removing.1` and so on) for a
  removed version whose folder could not be deleted yet;
- `bin/<name>.old` (or `<name>.old.1` and so on) for a replaced binary that
  was still running;
- `downloads/<archive>.part`, `.part.toml` and `.lock` while a download runs.
  After it stopped part-way, the `.part` and `.part.toml` stay for the next
  run, and so does the `.lock` when ckbup was killed;
- `downloads/.tmp*`, the work folder of an install, which stays behind only
  when ckbup was killed.

`list` ignores them. ckbup deletes the `.removing` folders and `.old` copies
the next time it installs, switches, updates or removes that tool, and the
next install of that version replaces a `.partial` folder. `install` and
`update` delete download leftovers that are more than 7 days old.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -- --ignored                 # GitHub requests and real installs into a temporary home
```

## Author

Steven Hert
