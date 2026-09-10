# wdi-installer

A small, self-contained Windows CLI that installs the [WinUSB](https://learn.microsoft.com/en-us/windows-hardware/drivers/usbcon/winusb-installation)
driver for a given USB device, identified by its vendor/product ID
(VID/PID), using [libwdi](https://github.com/pbatard/libwdi) via its Rust
binding, [`wdi`](https://crates.io/crates/wdi).

<!-- CI badge: fill in <owner>/<repo> once this is pushed to its own repository -->
<!-- [![Build](https://github.com/<owner>/wdi-installer/actions/workflows/build.yml/badge.svg)](https://github.com/<owner>/wdi-installer/actions/workflows/build.yml) -->

It exists to let a desktop application replace a "please install this
driver yourself with Zadig" step with a single native Windows UAC prompt:
the app detects that Windows has claimed a USB device with its own
generic driver (blocking `libusb`-based access to it), shells out to this
binary with the device's VID/PID, and the user only has to click "Yes"
once.

```
wdi-installer.exe --vid 0x054c --pid 0x0a1b [--log-file <path>]
```

| Flag         | Required | Description                                      |
| ------------ | -------- | ------------------------------------------------ |
| `--vid`      | yes      | USB vendor ID, hex (`0x...`) or decimal          |
| `--pid`      | yes      | USB product ID, hex (`0x...`) or decimal         |
| `--log-file` | no       | Also append progress/error messages to this file |

Exit code is `0` on success and `1` on any failure (device not found,
driver preparation/installation error). Progress and error messages go to
stdout (and to `--log-file`, if given) — this tool has no UI of its own.

## Elevation

This binary expects to already be running elevated (administrator) by
the time it starts — launch it with `ShellExecuteExW` and the `"runas"`
verb (or equivalent), not as a plain child process left to elevate
itself partway through. See the doc comment at the top of
[`src/main.rs`](src/main.rs) for the full reasoning: libwdi only writes a
valid signed driver catalog when the process is _already_ admin at the
`wdi_prepare_driver` step, not just later at `wdi_install_driver`.
Because of this, expect one native Windows UAC prompt each time this
runs — libwdi detects the pre-existing elevation and won't trigger a
second one.

## Building

Requires the **MSVC** toolchain (not MinGW): `libwdi-sys` compiles and
statically links libwdi's C sources, which need `cl.exe`. It also needs
the old WDK 8.0 redistributable (the `WDK_DIR` environment variable) for
`WdfCoInstaller*.dll`, which libwdi embeds into the driver package it
generates but which isn't part of a modern Windows SDK install. See
[`.github/workflows/build.yml`](.github/workflows/build.yml) for the
exact, tested steps (fetching and extracting that redistributable, then
a plain native build).

```
cargo build --release
```

## License

This crate's own code (everything in this repository except
`THIRD-PARTY-LICENSE-LGPL-3.0.txt`) is dual-licensed under your choice
of:

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

This matches the license of the `wdi` and `libwdi-sys` crates it depends
on. libwdi itself remains under LGPLv3 — see
[`THIRD-PARTY-LICENSE-LGPL-3.0.txt`](THIRD-PARTY-LICENSE-LGPL-3.0.txt)
and the [upstream libwdi project](https://github.com/pbatard/libwdi).
