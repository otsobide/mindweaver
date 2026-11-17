//! Gives `mind` and `mindweaver` on Windows the main thread's stack they
//! have on Linux and macOS, 8 MiB, rather than the 1 MiB Windows reserves
//! for an executable's main thread.
//!
//! A debug build needs more than 1 MiB to parse a command line and start on
//! it (measured on Linux with `ulimit -s`: 1 MiB overflows, 1.5 MiB does
//! not), so every binary the tests spawn on Windows overflowed before it
//! printed a word. A link argument rather than running `main`'s body on a
//! thread of its own: the TUI, the panel's Ctrl-C and the progress line
//! keep the thread they start on everywhere, and only the two binaries are
//! linked with it (`-bins`), never a test harness or a library. Set for
//! every build, release included, since how much stack a command takes is
//! not a debug build's alone to grow.

/// What Linux and macOS give a program's main thread.
const STACK: u32 = 8 * 1024 * 1024;

fn main() {
    // Run again only when this file changes, rather than on every change to
    // the crate's sources, which is what cargo does for a script that names
    // nothing.
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{STACK}"),
        Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{STACK}"),
        _ => {}
    }
}
