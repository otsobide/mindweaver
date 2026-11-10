//! The content hash of a copy: what the ledger keeps of what an install
//! wrote, so that a copy nobody has touched can later be told from one
//! somebody edited (D-04).
//!
//! Defined here and nowhere else. Every target of an installation is compared
//! with the one hash recorded for it, on whichever machine the comparison
//! runs, so two spellings of the recipe (or one that depended on the
//! platform) would call an untouched copy edited. Three recipes, one for a
//! folder ([`tree`]), one for a file written whole ([`bytes`]) and one for
//! an MCP server's entry in an agent's config file ([`fingerprint`]), each
//! for its own kind of target and never compared with another. A fourth,
//! [`request`], is no copy's: the key the HTTP cache keeps an answer under,
//! here so that SHA-256 is spelled in one place.
//!
//! What each makes is a [`ContentHash`], the value it is, rather than the
//! text it is written as: only these recipes, and a digest somebody hands
//! over that [`ContentHash::parse`] reads, make one.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::value::ContentHash;

/// The SHA-256 of everything under `root`, as 64 lowercase hex digits.
///
/// What is hashed is each folder and file under `root`, in the order of
/// their paths relative to it (names joined by `/`, compared as bytes), each
/// written as:
///
/// - a folder: `d`, its path, a NUL byte;
/// - a file: `f`, its path, a NUL byte, its length as eight big-endian bytes,
///   then its bytes.
///
/// `root`'s own name is not part of it, so the same copy filed in two agents'
/// folders hashes alike in both. Paths use `/` on every platform and are
/// sorted rather than taken in the order a directory lists them, which is the
/// filesystem's own and differs between ext4, APFS and NTFS. No name can
/// contain a NUL byte, and every file says how long it is, so where one entry
/// ends is never in doubt and two different trees never write the same bytes.
/// A name is its UTF-8 wherever it is valid Unicode, which every name on every
/// platform the CI runs is, and otherwise the bytes the platform holds
/// ([`std::ffi::OsStr::as_encoded_bytes`]), so two names never read as one.
///
/// Folders count, empty ones included, so two trees hash alike exactly when
/// [`crate::copy::same`] says they hold the same names and bytes. Permissions
/// and times do not: Windows has no executable bit to agree on, and a time
/// changes when nothing in the file has.
///
/// A link, or anything else that is neither a file nor a folder, is refused
/// rather than followed. Following it could read anything (a link to
/// `/dev/zero` never ends), and the text of a link is spelled differently on
/// Windows. Mindweaver never writes one into a copy, since [`crate::copy::tree`]
/// copies what a link points at, so one found there was put there afterwards.
pub fn tree(root: &Path) -> Result<ContentHash, io::Error> {
    let mut entries = Vec::new();
    walk(root, &[], &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let mut hasher = Sha256::new();
    for entry in entries {
        match entry.file {
            None => {
                hasher.update(b"d");
                hasher.update(&entry.path);
                hasher.update([0]);
            }
            Some(file) => {
                // Read whole, so the length written is the length of the bytes
                // hashed even if the file changes while it is being read.
                let bytes = fs::read(&file)?;
                hasher.update(b"f");
                hasher.update(&entry.path);
                hasher.update([0]);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(&bytes);
            }
        }
    }

    Ok(hex(hasher))
}

/// The SHA-256 of a file's contents, as 64 lowercase hex digits.
///
/// For a target that is one file Mindweaver writes whole, as a rule is in
/// Claude's rules folder ([`crate::claude_rules`]), where [`tree`] walks a
/// folder. The bytes alone, not the file's name, as a copy's own name is not
/// part of its tree's hash; and handed the bytes rather than a path, so what
/// is hashed is what was written or read, never a file that changed in
/// between.
pub fn bytes(contents: &[u8]) -> ContentHash {
    hex(Sha256::new_with_prefix(contents))
}

/// The fingerprint of an MCP server's entry: the SHA-256 of its canonical
/// form, as 64 lowercase hex digits.
///
/// What the ledger keeps of an entry Mindweaver wrote into an agent's config
/// file (D-10), as [`tree`] is what it keeps of a copy: an entry can carry
/// no mark of its own, so this is the one sign that it holds what was
/// written. Handed the canonical text rather than the entry, since that is
/// each renderer's to make (`Entry::canonical` in [`crate::claude_mcp`],
/// [`crate::codex_mcp`] and [`crate::opencode_mcp`]) and what
/// [`crate::config_entry::Value::canonical`] gives of one read back, so an
/// entry is fingerprinted alike as it is written and as a file holds it,
/// however the file lays it out. The text alone, not the key it is under nor
/// the file, as a copy's own name is not part of its tree's hash.
pub fn fingerprint(canonical: &str) -> ContentHash {
    hex(Sha256::new_with_prefix(canonical.as_bytes()))
}

/// The key an answer read over HTTP is kept under: the SHA-256 of what
/// tells the request from another, as 64 lowercase hex digits.
///
/// Handed the request's canonical text rather than the request, since what
/// tells one from another is the cache's to say ([`crate::http::Cache`]).
/// A hash rather than the URL itself, as the name of the file the answer is
/// kept in, so a file's name is one every filesystem takes whatever the URL
/// holds, and says nothing a URL said. Never compared with the other
/// recipes' hashes, which are of what was written, not of what was asked.
pub fn request(canonical: &str) -> ContentHash {
    hex(Sha256::new_with_prefix(canonical.as_bytes()))
}

/// Whether `text` is a hash as [`bytes`] and [`tree`] spell one: 64 hex
/// digits and nothing else, in either case, since a hash copied from
/// elsewhere may be written in capitals and means the same.
///
/// For a hash somebody hands over to be compared with one of these, as
/// `edit --if-unchanged` does: one that is no hash at all, such as the line
/// `sha256sum` prints with the file's name after its digits, is told apart
/// from the hash of something else, which no comparison could do.
pub fn is_digest(text: &str) -> bool {
    ContentHash::parse(text).is_some()
}

/// What `hasher` has taken in, as 64 lowercase hex digits.
fn hex(hasher: Sha256) -> ContentHash {
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        // Writing to a `String` cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    ContentHash::computed(hex)
}

/// One folder or file found under the root.
struct Entry {
    /// Its path relative to the root, `/` separated.
    path: Vec<u8>,
    /// Where to read it, for a file; `None` for a folder.
    file: Option<PathBuf>,
}

fn walk(dir: &Path, prefix: &[u8], entries: &mut Vec<Entry>) -> Result<(), io::Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let mut path = prefix.to_vec();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(entry.file_name().as_encoded_bytes());

        // The entry's own type, which a link does not hand on to what it
        // points at.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            entries.push(Entry {
                path: path.clone(),
                file: None,
            });
            walk(&entry.path(), &path, entries)?;
        } else if kind.is_file() {
            entries.push(Entry {
                path,
                file: Some(entry.path()),
            });
        } else {
            return Err(io::Error::other(format!(
                "{} is neither a file nor a folder, so it is not hashed",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "hash_test.rs"]
mod tests;
