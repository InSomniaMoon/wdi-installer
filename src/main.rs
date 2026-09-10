//! Standalone CLI: installs the WinUSB driver for a given USB device
//! (VID/PID) via libwdi.
//!
//! This binary is meant to already be running elevated (administrator)
//! by the time `main` starts — launch it with `ShellExecuteExW` and the
//! `"runas"` verb (not a plain child process left to elevate itself
//! partway through). This matters because of a real, observed failure
//! mode: libwdi's `wdi_prepare_driver` (`libwdi.c`, around line 1521)
//! only writes a valid signed driver catalog (`.cat`) file when
//! `IsUserAnAdmin()` already returns true *at that point*. If this
//! process starts unelevated and only lets libwdi re-elevate itself for
//! the later `wdi_install_driver` step (its default behavior when the
//! caller isn't already admin, see `libwdi.c` around line 1829),
//! `wdi_prepare_driver` still runs without admin rights just before that
//! — producing no valid catalog file. In practice this let Windows
//! silently keep preferring its own built-in driver over the "installed"
//! WinUSB one shortly after (observed as the stock driver reasserting
//! itself right after an apparently successful install). Pre-elevating
//! this whole process avoids that. It doesn't cause a double UAC prompt:
//! libwdi detects when the process is already elevated and falls back to
//! a plain `CreateProcess` for its own sub-installer step instead of
//! triggering another prompt (`libwdi.c`, around lines 1829 and 1866).
//!
//! Usage: wdi-installer.exe --vid 0x054c --pid 0x0a1b [--log-file <path>]

use std::env;
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use wdi::{
    create_list, install_driver, prepare_driver, CreateListOptions, DriverType,
    InstallDriverOptions, PrepareDriverOptions,
};

struct Args {
    vid: u16,
    pid: u16,
    log_file: Option<PathBuf>,
}

fn parse_vid_or_pid(s: &str) -> Option<u16> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => s.parse::<u16>().ok(),
    }
}

fn parse_args() -> Result<Args, String> {
    let mut vid = None;
    let mut pid = None;
    let mut log_file = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vid" => {
                let v = args.next().ok_or("--vid requires a value")?;
                vid = Some(parse_vid_or_pid(&v).ok_or_else(|| format!("invalid VID: {v}"))?);
            }
            "--pid" => {
                let v = args.next().ok_or("--pid requires a value")?;
                pid = Some(parse_vid_or_pid(&v).ok_or_else(|| format!("invalid PID: {v}"))?);
            }
            "--log-file" => {
                let v = args.next().ok_or("--log-file requires a value")?;
                log_file = Some(PathBuf::from(v));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        vid: vid.ok_or("--vid is required")?,
        pid: pid.ok_or("--pid is required")?,
        log_file,
    })
}

// The MSVC/UCRT C runtime doesn't expose `stdout` as a plain global
// symbol (unlike Unix, where `libc::stdout()` exists): on Windows, the C
// macro `stdout` expands to a call to this internal function
// (`__acrt_iob_func(1)`, 1 = stdout, 2 = stderr) — not wrapped by the
// `libc` crate for this target, hence this direct binding.
#[cfg(windows)]
extern "C" {
    fn __acrt_iob_func(index: u32) -> *mut libc::FILE;
}

/// Redirects the C runtime's own `stdout` (via `freopen` — not just
/// Rust's `stdout`; the two are independent on Windows, see the module
/// doc comment) to `path`, truncating it first. Needed to capture
/// libwdi's internal logging (printed on the C side via
/// `printf`/`fprintf(stdout, ...)`), which would otherwise be invisible
/// from the Rust side once this process is elevated with no console
/// visibly attached.
fn redirect_c_stdout_to_file(path: &std::path::Path) {
    let Some(path_str) = path.to_str() else {
        return;
    };
    let Ok(c_path) = CString::new(path_str) else {
        return;
    };
    let Ok(mode) = CString::new("w") else {
        return;
    };
    unsafe {
        let stdout = __acrt_iob_func(1);
        libc::freopen(c_path.as_ptr(), mode.as_ptr(), stdout);
    }
}

/// Writes a line to Rust's `stdout` (useful for manual use from a
/// terminal, without `--log-file`) and, if given, also appends it to the
/// log file — the `freopen` above only redirects the C runtime's
/// `stdout`, not Rust's own (two distinct streams), so our own messages
/// need this second path to end up in the same file as libwdi's internal
/// logging.
fn log_line(log_file: &Option<PathBuf>, line: &str) {
    println!("{line}");
    if let Some(path) = log_file {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Usage: wdi-installer.exe --vid <0xHEX|DEC> --pid <0xHEX|DEC> [--log-file <path>]");
            eprintln!("Error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(path) = &args.log_file {
        redirect_c_stdout_to_file(path);
    }

    log_line(
        &args.log_file,
        &format!(
            "Looking for device VID={:#06x} PID={:#06x}...",
            args.vid, args.pid
        ),
    );

    let devices = match create_list(CreateListOptions {
        list_all: true,
        ..Default::default()
    }) {
        Ok(d) => d,
        Err(e) => {
            log_line(
                &args.log_file,
                &format!("USB enumeration failed (wdi_create_list): {e}"),
            );
            return ExitCode::FAILURE;
        }
    };

    let Some(mut device) = devices
        .into_iter()
        .find(|d| d.vid == args.vid && d.pid == args.pid)
    else {
        log_line(
            &args.log_file,
            &format!(
                "No USB device found with VID={:#06x} PID={:#06x} (is it plugged in?).",
                args.vid, args.pid
            ),
        );
        return ExitCode::FAILURE;
    };

    let tmp_dir = env::temp_dir().join("wdi-installer");
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        log_line(
            &args.log_file,
            &format!(
                "Could not create temporary directory {}: {e}",
                tmp_dir.display()
            ),
        );
        return ExitCode::FAILURE;
    }
    let tmp_dir_str = tmp_dir.to_string_lossy().into_owned();
    let inf_name = "device.inf";

    let mut prepare_opts = PrepareDriverOptions::default().driver_type(DriverType::WinUsb);
    if let Err(e) = prepare_driver(&mut device, &tmp_dir_str, inf_name, &mut prepare_opts) {
        log_line(
            &args.log_file,
            &format!("Driver preparation failed (wdi_prepare_driver): {e}"),
        );
        return ExitCode::FAILURE;
    }

    log_line(&args.log_file, "Installing WinUSB driver...");
    let mut install_opts = InstallDriverOptions::default();
    if let Err(e) = install_driver(&mut device, &tmp_dir_str, inf_name, &mut install_opts) {
        log_line(
            &args.log_file,
            &format!("Driver installation failed (wdi_install_driver): {e}"),
        );
        return ExitCode::FAILURE;
    }

    log_line(&args.log_file, "WinUSB driver installed successfully.");
    ExitCode::SUCCESS
}
