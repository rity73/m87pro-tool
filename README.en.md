# m87pro-tool

[한국어](README.md) | **English**

A small, unofficial configuration tool for the **Nodal Studio M87 Pro** keyboard on Windows.
It replaces the vendor's configuration app with two lightweight programs:

| Program | What it is |
|---|---|
| `m87.exe` | Command-line tool. Run it without arguments to open a full-screen terminal interface (mouse supported). English only. |
| `m87-gui.exe` | Window version with the same pages, in English or Korean (selectable in the program). |

Both programs can:

- turn on lighting effects and set colors, brightness and speed, or give each key its own color;
- remap keys on the normal and Fn layers (keys, media keys, mouse buttons, macros);
- create macros that the keyboard plays back;
- change keyboard settings such as the Windows-key lock and the sleep timer;
- show the battery level (2.4 GHz receiver);
- save, load, export and import complete setups as profiles;
- reset the keyboard to its factory state.

Settings and profiles are stored as plain files in `%APPDATA%\m87pro-tool`.

The programs are for the Nodal Studio M87 Pro only. Both show the license and this notice in
their About page (`m87`: the About button at the top; `m87-gui`: About in the left menu).

> This project is not affiliated with, endorsed by or supported by Nodal Studio or Sonix.
> Use it at your own risk.

## Supported hardware

| Connection | USB ID | Status |
|---|---|---|
| USB cable | `0C45:8006` (Sonix) | supported |
| 2.4 GHz receiver | `0C45:FEFE` | supported |
| Bluetooth | — | not supported (the keyboard cannot be configured over Bluetooth) |

## Download

Get `m87.exe` and `m87-gui.exe` from the [Releases](../../releases) page. They are single files
with no installer and no runtime dependencies (the C runtime is linked statically).

## Command-line examples

```
m87                      # open the full-screen interface
m87 ping                 # check that the keyboard answers
m87 battery              # battery level (2.4 GHz receiver)
m87 effects              # list lighting effects
m87 keys                 # list key names
m87 light --help         # lighting options
m87 remap --help         # key remapping options
m87 profile list         # saved profiles
m87 --verbose ...        # print every packet sent and received
```

Commands that write to the keyboard accept `--dry-run`, which prints the packets without sending them.
Run `m87 --help` or `m87 <command> --help` for the full list.

## Known limits

- **Bluetooth** connections cannot be configured. Use the USB cable or the 2.4 GHz receiver.
- **Fn+Backspace** on the keyboard turns all lighting off (and on again). While it is off, lighting
  changes from the tool have no visible effect.
- The **light bar above the arrow keys** is controlled only by the keyboard's own shortcuts. No
  command for it is known.
- Music-reactive and real-time lighting modes of the vendor app are not implemented.

## How it was made

The tool was written from scratch, for interoperability with hardware the author owns. The
configuration protocol was worked out from the vendor app and then checked on a real keyboard;
the notes are in [`docs/protocol.md`](docs/protocol.md). The key positions in
[`crates/m87-proto/src/layout.rs`](crates/m87-proto/src/layout.rs) were generated from the
vendor app's keyboard layout file. No vendor code or binaries are included.

## Building from source

Requires Rust 1.80 or later with the MSVC toolchain (`x86_64-pc-windows-msvc`).

```
cargo build --release
cargo test --workspace
```

The programs are written to `target\release\m87.exe` and `target\release\m87-gui.exe`.

Workspace layout:

| Crate | Purpose |
|---|---|
| `m87-proto` | Packet encoding for the configuration protocol (no I/O) |
| `m87-hid` | Device discovery and HID transports (direct Win32 calls) |
| `m87-core` | Saved state, profiles and complete send plans |
| `m87-app` | `m87.exe` (CLI and terminal interface) |
| `m87-gui` | `m87-gui.exe` (Win32 window interface) |

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org)

Only the release files built by this repository's GitHub Actions workflow from the tagged
source code are signed. Files built anywhere else are never signed with this certificate.

Team roles:

- Authors (committers): [rity73](https://github.com/rity73)
- Reviewers: [rity73](https://github.com/rity73)
- Approvers: [rity73](https://github.com/rity73)

Every signing request is approved manually by an approver.

### Privacy statement

This program will not transfer any information to other networked systems unless specifically
requested by the user or the person installing or operating it.

It talks only to the keyboard over USB, and it stores its settings and profiles only on your PC.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
