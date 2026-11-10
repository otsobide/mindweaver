//! The TUI end to end: `mindweaver` and `mind` on their own, the screen a
//! person works in, driven one use case at a time in a terminal of its own.
//!
//! The screen's own suite (`tui_test`) drives its state with key events and
//! renders it into ratatui's test backend, without a terminal; what lies
//! between that and a person goes untested there: the event loops, raw mode
//! and the alternate screen, the keys as a terminal sends them, and the
//! transcript printed once the window is given back. Here each test is one
//! use case, in three parts, as in the CLI's end-to-end suite (`e2e_test`):
//!
//! - the scenario it needs, built from nothing for that test alone, with
//!   the CLI suite's own [`Scenario`];
//! - the binary started in a pseudo-terminal of its own from a folder of
//!   it ([`Terminal`]), and the keys a person presses, typed into it;
//! - what the screen draws, row by row where it is laid out and by what a
//!   row says where a path in a box would read otherwise as `/home`, the
//!   transcript the terminal shows once the screen has closed, and what it
//!   left on the disk.
//!
//! Every wait is for what is to be drawn, bounded, never a sleep, so a
//! slow machine is slower and no less certain. Unix alone, as a
//! pseudo-terminal is.

#![cfg(unix)]

// The CLI's suite runs every step this one does not.
#[allow(dead_code)]
#[path = "e2e/scenario.rs"]
mod scenario;

#[path = "tui_e2e/terminal.rs"]
mod terminal;

#[path = "tui_e2e/browsing.rs"]
mod browsing;

#[path = "tui_e2e/changing.rs"]
mod changing;

#[path = "tui_e2e/exporting.rs"]
mod exporting;

#[path = "tui_e2e/screens.rs"]
mod screens;

#[path = "tui_e2e/sources.rs"]
mod sources;

#[path = "tui_e2e/starting.rs"]
mod starting;

pub use scenario::Scenario;
pub use terminal::{Key, Terminal};

/// A skill's file as a person writes one.
pub fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
}

/// `~/Projects` linking `collapse` and `tanukeys`: `collapse` holding
/// `commit-style` from the library and `deploy`, its own, and `tanukeys`
/// nothing.
pub fn collapse_and_tanukeys() -> Scenario {
    let scenario = Scenario::projects(&["collapse", "tanukeys"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .given(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service to staging\"",
        );
    scenario
}
