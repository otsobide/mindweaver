//! What a command produced, and the helpers every report is laid out with.
//!
//! Apart from the commands so that a report is built one way: a row is made
//! `printable` and padded by `table` whichever command or screen it comes
//! from, and a new command cannot quietly grow a second way of doing either.

use std::fmt::{self, Write as _};
use std::io;
use std::path::Path;
use std::process::ExitCode;

use mindweaver_core::{paths, ArtifactError, Kind, ResolveError, ValidationIssue, WorkspaceError};
use serde::Serialize;

use crate::CliError;

/// What a command produced.
///
/// The text is built rather than printed as the command runs, which is what
/// lets a test assert on exactly what a user sees.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The report itself.
    pub stdout: String,
    /// Problems worth reporting that did not stop the command.
    pub stderr: Vec<Warning>,
    /// Whether everything the command looked at was in order.
    pub ok: bool,
}

impl Outcome {
    /// A clean report with nothing to warn about.
    pub(crate) fn plain(stdout: String) -> Self {
        Self {
            stdout,
            stderr: Vec::new(),
            ok: true,
        }
    }

    /// Print the outcome and return the code the process should exit with.
    ///
    /// Written rather than printed, because `print!` panics when the pipe is
    /// closed and `mind list | head` closes it on purpose. A reader that has
    /// seen enough is not an error, so that case exits as if all was well.
    pub fn report(&self) -> ExitCode {
        use std::io::Write as _;

        if let Err(error) = io::stdout().write_all(self.stdout.as_bytes()) {
            return if error.kind() == io::ErrorKind::BrokenPipe {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        for line in &self.stderr {
            let _ = writeln!(io::stderr(), "{line}");
        }
        if self.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// One line a command says beside what it reports, on stderr: a warning, a
/// note, a hint, or an error about one part of what it did; and the code of
/// what it says, when what it says has one.
///
/// The text is what a terminal shows, word for word, `warning: ` and all.
/// The code rides beside it for the panel, which acts on a condition by its
/// code rather than by matching English that is reworded whenever it is
/// made clearer: the code core gives what went wrong ([`mindweaver_core::code`]),
/// or the CLI's own ([`CliError::code`]). A line with none says something no
/// code is given to: a hint, a note, or what the CLI makes of what it found
/// rather than something that went wrong. Kept beside the text rather than
/// in a list of its own beside the lines, which every line pushed would have
/// to keep in step with.
///
/// Serialized as it is, `{"text": ..., "code": ...}`, in what a run of the
/// panel's backend says (`backend::Run`): the line a terminal shows, and
/// the code beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Warning {
    pub text: String,
    pub code: Option<&'static str>,
}

impl Warning {
    /// A line saying what `code` names.
    pub(crate) fn coded(code: &'static str, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            code: Some(code),
        }
    }
}

/// A line no code is given to.
impl From<String> for Warning {
    fn from(text: String) -> Self {
        Self { text, code: None }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// By its text alone, which is what a test of what a terminal shows asks
/// about; one about the code asks for the code.
impl PartialEq<str> for Warning {
    fn eq(&self, text: &str) -> bool {
        self.text == text
    }
}

impl PartialEq<&str> for Warning {
    fn eq(&self, text: &&str) -> bool {
        self.text == *text
    }
}

impl PartialEq<String> for Warning {
    fn eq(&self, text: &String) -> bool {
        self.text == *text
    }
}

/// A command that could not run, and whatever it had already found out.
///
/// The warnings travel with the error rather than being dropped, because they
/// usually explain it: `no skill named \`broken\`` is baffling on its own and
/// obvious next to `broken/SKILL.md: the file does not start with a `---`
/// front matter fence`.
#[derive(Debug)]
pub struct Failure {
    /// Why the command stopped.
    ///
    /// Boxed to keep the error half of every `Result` small: a `CliError`
    /// carries paths and an io::Error, and the success path should not pay for
    /// that on every return.
    pub error: Box<CliError>,
    /// What it had already noticed before it stopped.
    pub warnings: Vec<Warning>,
}

impl Failure {
    /// Print the warnings and the error, and return the process exit code.
    pub fn report(&self) -> ExitCode {
        for line in &self.warnings {
            eprintln!("{line}");
        }
        eprintln!("error: {}", self.error);
        ExitCode::FAILURE
    }
}

impl From<CliError> for Failure {
    fn from(error: CliError) -> Self {
        Self {
            error: Box::new(error),
            warnings: Vec::new(),
        }
    }
}

impl From<WorkspaceError> for Failure {
    fn from(error: WorkspaceError) -> Self {
        CliError::from(error).into()
    }
}

impl From<ResolveError> for Failure {
    fn from(error: ResolveError) -> Self {
        CliError::from(error).into()
    }
}

impl From<ArtifactError> for Failure {
    fn from(error: ArtifactError) -> Self {
        CliError::from(error).into()
    }
}

/// The version of what every reading command's `--json` prints around what
/// it read ([`json`]).
///
/// It moves when a field of the envelope goes or changes what it holds, and
/// not when one is added, which a reader passes over. What a command puts in
/// `data` is held to the same rule: every name in it is a contract, which is
/// why core spells each kind, agent and state as a command line takes it.
///
/// 2 since `status` dropped two states for the five an installation stands
/// in (D-04): `installed` is `current`, `outdated` or `orphaned` now, and
/// `partial` is `missing`, so a script reading version 1 would meet a state
/// it never knew and miss two it did.
pub const ENVELOPE_VERSION: u32 = 2;

/// What every reading command's `--json` prints:
/// `{"version": 2, "data": ..., "warnings": [...], "coded_warnings": [...]}`.
///
/// One shape for every command, so a script reads each the same way, and the
/// warnings inside it beside the data rather than on stderr alone: failures
/// are collected, not raised, in JSON as in text. On stderr as well, as the
/// text says them, for whoever pipes the JSON into `jq` at a terminal.
///
/// `coded_warnings` says each of `warnings` again, in the same order, with
/// its code beside it ([`Warning::code`]), for a reader that acts on a
/// condition rather than on English that is reworded whenever it is made
/// clearer. A name of its own rather than objects in `warnings`, whose
/// strings a version 2 script reads: changing what a field holds moves the
/// version, and adding one does not ([`ENVELOPE_VERSION`]). A list of codes
/// beside the strings, a reader pairing them by place, was the other way,
/// rejected as a [`Warning`] keeps its code on the line rather than in a
/// list beside the lines.
// No bound on `T` here, where serde's derive puts its own on what it
// implements: ts-rs names a generic type with a stand-in for `T` that
// serializes nothing, which such a bound would refuse.
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub(crate) struct Envelope<'a, T> {
    version: u32,
    data: &'a T,
    warnings: Vec<&'a str>,
    coded_warnings: Vec<Coded<'a>>,
}

/// One warning in the envelope's `coded_warnings`: its code, none for a line
/// no code is given to, and what it says, as `warnings` says it.
#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "CodedWarning"))]
#[cfg_attr(
    feature = "schema",
    derive(schemars::JsonSchema),
    schemars(rename = "CodedWarning")
)]
struct Coded<'a> {
    code: Option<&'static str>,
    message: &'a str,
}

/// What every warning line starts with on stderr, which the envelope leaves
/// out: inside `warnings`, every line is one.
const WARNING: &str = "warning: ";

/// `data` in the envelope, with `warnings`, each a line as stderr shows it,
/// in its `warnings` without the prefix and in its `coded_warnings` with its
/// code, and on stderr too, as the text says them; failing when there is
/// any, as the text would.
///
/// On stderr as well as in the envelope, as #64 asked: the envelope is a
/// script's, which reads every warning there and never needs stderr, and
/// stderr is the terminal's, where somebody piping the JSON into `jq` would
/// otherwise see the answer and nothing of what could not be read. Left
/// off stderr, as #36 first had it, was the other way, rejected for that.
///
/// Pretty-printed, two spaces a level, since a person reads it as often as a
/// script does, and `jq` takes either.
pub(crate) fn json<T: Serialize>(data: &T, warnings: &[Warning]) -> Outcome {
    let said: Vec<&str> = warnings
        .iter()
        .map(|warning| warning.text.strip_prefix(WARNING).unwrap_or(&warning.text))
        .collect();
    let envelope = Envelope {
        version: ENVELOPE_VERSION,
        data,
        coded_warnings: warnings
            .iter()
            .zip(&said)
            .map(|(warning, message)| Coded {
                code: warning.code,
                message,
            })
            .collect(),
        warnings: said,
    };
    // A report is strings, numbers and lists of them, and maps keyed by
    // field names, and every path in one is serialized as text whatever
    // bytes it holds: nothing in it can fail to serialize.
    let mut stdout = serde_json::to_string_pretty(&envelope).expect("a report serializes");
    stdout.push('\n');
    Outcome {
        stdout,
        stderr: warnings.to_vec(),
        ok: warnings.is_empty(),
    }
}

/// `text`, what a reading command made to print, as JSON: `data` in the
/// envelope in place of its stdout, each of its `warning:` lines in the
/// envelope, and its stderr left as it is, those lines among the rest, a
/// hint or a note, as [`json`] leaves them for whoever reads the terminal.
///
/// Its exit status is the text's, kept rather than worked out again from
/// the warnings, since a report can fail with none: `validate` fails for an
/// artifact that does not pass. Built from the text's outcome, rather than
/// beside it, so that a warning a command adds on the way out (a workspace
/// it consulted that could not be read) reaches the envelope as it reaches
/// stderr.
pub(crate) fn as_json<T: Serialize>(data: &T, text: Outcome) -> Outcome {
    let warnings: Vec<Warning> = text
        .stderr
        .iter()
        .filter(|line| line.text.starts_with(WARNING))
        .cloned()
        .collect();
    let mut outcome = json(data, &warnings);
    outcome.stderr = text.stderr;
    outcome.ok = text.ok;
    outcome
}

/// `text`, or, when `--json` asked for `data`, [`as_json`] of it.
pub(crate) fn json_or_text<T: Serialize>(data: Option<T>, text: Outcome) -> Outcome {
    match data {
        Some(data) => as_json(&data, text),
        None => text,
    }
}

/// Rows padded into columns, the last one left ragged.
pub(crate) fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.first().map_or(0, Vec::len);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut text = String::new();
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            if column + 1 == columns {
                line.push_str(cell);
            } else {
                let padding = widths[column].saturating_sub(cell.chars().count());
                let _ = write!(line, "{cell}{}  ", " ".repeat(padding));
            }
        }
        // An artifact with no summary would otherwise leave the padding of the
        // column before it hanging off the end of the line.
        let _ = writeln!(text, "{}", line.trim_end());
    }
    text
}

/// Two spaces in front of every line, so a block reads as belonging to the
/// line above it.
pub(crate) fn indent(text: &str) -> String {
    text.lines().fold(String::new(), |mut out, line| {
        let _ = writeln!(out, "  {line}");
        out
    })
}

/// Text safe to put in a row.
///
/// A name or an opening line comes from a file somebody wrote, and a carriage
/// return in one would let a row overwrite the row above it on the terminal,
/// hiding an entry, or fabricating one that looks real.
pub(crate) fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// What is wrong with one artifact: `label: ok`, or the count after the
/// label and a line for each thing under it. Whether the artifact is invalid
/// is returned, so a report can count it and an exit status follow it.
///
/// Only a problem makes it invalid. A warning ([`ValidationIssue::is_warning`])
/// is listed, marked as one, and leaves the verdict at `ok`.
pub(crate) fn verdict(text: &mut String, label: &str, issues: &[ValidationIssue]) -> bool {
    let warnings = issues.iter().filter(|issue| issue.is_warning()).count();
    let problems = issues.len() - warnings;
    let mut counts = Vec::new();
    if problems == 0 {
        counts.push(String::from("ok"));
    } else {
        counts.push(plural(problems, "problem"));
    }
    if warnings > 0 {
        counts.push(plural(warnings, "warning"));
    }
    let _ = writeln!(text, "{label}: {}", counts.join(", "));
    // Printable, because an issue quotes the name or the key it is about.
    for issue in issues {
        let marked = if issue.is_warning() { "warning: " } else { "" };
        let _ = writeln!(text, "  - {marked}{}", printable(&issue.to_string()));
    }
    problems > 0
}

/// "1 skill", "2 skills", "1 MCP server": a count of a kind, in the words
/// core gives it for one and for more ([`Kind::noun`], [`Kind::plural`]).
pub(crate) fn counted_kind(count: usize, kind: Kind) -> String {
    let noun = if count == 1 {
        kind.noun()
    } else {
        kind.plural()
    };
    format!("{count} {noun}")
}

/// `seconds` since the Unix epoch, as the ledger stamps what it records, as
/// a day and a minute in UTC: `2026-11-10 14:03 UTC`.
///
/// UTC rather than the machine's own zone, which the CLI would have to read
/// the system's database for, and which would make one listing read
/// differently on two machines; worked out here, from the days since the
/// epoch on the proleptic Gregorian calendar (Howard Hinnant's
/// `civil_from_days`), rather than with a crate of dates for one line.
pub(crate) fn utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let into = seconds.rem_euclid(86_400);
    let (hour, minute) = (into / 3_600, into % 3_600 / 60);
    // Days from 0000-03-01, so a leap day falls at the end of a year.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let from_march = (5 * of_year + 2) / 153;
    let day = of_year - (153 * from_march + 2) / 5 + 1;
    let month = if from_march < 10 {
        from_march + 3
    } else {
        from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// "1 skill", "2 skills".
pub(crate) fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// A registry entry spelled the way the marker file spells it.
///
/// Not `Path::display`, which uses the platform separator: entries are stored
/// with forward slashes so a workspace registered on one platform resolves on
/// the other, and on Windows `display` would report `..\collapse` for a line
/// that reads `../collapse`. What a command says it wrote has to be what
/// someone opening the file will find.
pub(crate) fn as_stored(entry: &Path) -> String {
    paths::to_config_string(entry).unwrap_or_else(|| entry.display().to_string())
}

#[cfg(test)]
#[path = "render_test.rs"]
mod tests;
