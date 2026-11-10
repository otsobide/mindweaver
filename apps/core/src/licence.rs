//! The licence a gathered artifact says it is under, as its source wrote it
//! (#99): what an audit of the shelf asks first, kept by the ledger beside
//! where each artifact came from.
//!
//! Read and never judged. Which licences a workspace may take is somebody's
//! policy, not core's, and an SPDX name matched out of a file's words would
//! be a guess written down as a fact: a licence is kept as its author wrote
//! it, with where it was read, and one nothing says is `unknown`, said as
//! that rather than left out. A repository's own licence, the one a forge
//! shows beside it, is a question for its API, which a gather never asks
//! (out of #99's scope).

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::artifact::Artifact;

/// How much of a licence file is read for the line that names it: more
/// than any licence's opening line needs, and a bound on what a source
/// can make a gather read and the ledger keep.
const READ: u64 = 4096;

/// The longest the line read off a licence file is kept: enough for any
/// licence's name, and the rest is in the file, which the shelf keeps.
const KEPT: usize = 200;

/// Where a gathered artifact says what it is licensed under, and what it
/// says.
///
/// Tagged by `from`, beside what that place said, so a reader tells a
/// licence its author declared from a line read off a file:
/// `{"from": "manifest", "text": "MIT"}`, `{"from": "file", "file":
/// "LICENSE.txt", "text": "Apache License"}` or `{"from": "unknown"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum Licence {
    /// The `license` its manifest declares, as written: a licence's name,
    /// or words pointing at a file it bundles, as the Agent Skills format
    /// has it.
    Manifest { text: String },
    /// A licence file in its folder, beside its manifest, which declares
    /// none: the file's name, and its first line that holds anything, as
    /// written, when there is one to read ([`KEPT`] characters of it at
    /// most).
    File { file: String, text: Option<String> },
    /// Neither: nothing it holds says.
    Unknown,
}

impl Licence {
    /// The licence `artifact`, put on the shelf at `folder`, says it is
    /// under: its manifest's `license`, else a licence file directly in
    /// `folder`, else none. A rule declares none and has no folder of its
    /// own: its `folder` is the one it was gathered from, whose licence
    /// file is every rule's there, and no rule itself (#111).
    ///
    /// A licence file is one whose name is `LICENSE` or `LICENCE`, in any
    /// case, alone or followed by an extension or a `-` and more
    /// (`LICENSE.txt`, `LICENCE.md`, `LICENSE-MIT`), the first of them by
    /// name when there are several. `COPYING` and a licence further down
    /// the folder are not looked for: a skill's own folder is where the
    /// format puts what it bundles. A `license` written empty declares
    /// nothing, and a file that cannot be read is named without its line.
    pub fn of(artifact: &Artifact, folder: &Path) -> Licence {
        let declared = artifact
            .manifest()
            .and_then(|manifest| manifest.license.as_deref())
            .filter(|text| !text.trim().is_empty());
        if let Some(text) = declared {
            return Licence::Manifest {
                text: text.to_owned(),
            };
        }
        match licence_file(folder) {
            Some(file) => Licence::File {
                text: first_line(&folder.join(&file)),
                file,
            },
            None => Licence::Unknown,
        }
    }

    /// The word for where it was read, as `from` and the ledger hold it.
    pub const fn slug(&self) -> &'static str {
        match self {
            Licence::Manifest { .. } => "manifest",
            Licence::File { .. } => "file",
            Licence::Unknown => "unknown",
        }
    }

    /// What a listing shows of it: what it says, the file it is in when
    /// the file said nothing that could be read, or `unknown`.
    pub fn said(&self) -> &str {
        match self {
            Licence::Manifest { text }
            | Licence::File {
                text: Some(text), ..
            } => text,
            Licence::File { file, text: None } => file,
            Licence::Unknown => "unknown",
        }
    }
}

/// The name of the first licence file directly in `folder`, by name.
fn licence_file(folder: &Path) -> Option<String> {
    let mut found: Vec<String> = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_licence_file(name))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Whether a file called `name` is a licence file ([`Licence::of`]).
pub(crate) fn is_licence_file(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE"].iter().any(|stem| {
        upper
            .strip_prefix(stem)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '-']))
    })
}

/// The first line of the file at `path` that holds anything, without the
/// space around it, as written: `None` when the first [`READ`] bytes hold
/// none, or it is not text.
fn first_line(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(READ)
        .read_to_end(&mut bytes)
        .ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let line = bytes
        .split(|byte| *byte == b'\n')
        .map(|line| std::str::from_utf8(line).map(str::trim))
        .find(|line| line.as_ref().map_or(true, |line| !line.is_empty()))?
        .ok()?;
    Some(line.chars().take(KEPT).collect())
}

#[cfg(test)]
#[path = "licence_test.rs"]
mod tests;
