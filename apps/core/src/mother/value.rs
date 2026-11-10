//! Mothers of the value objects in [`crate::value`], and of the texts that
//! are not one, for a test of a rule to be handed both sides of it.

use crate::hash;
use crate::mother::random;
use crate::mother::url::UrlMother;
use crate::value::{
    ArtifactName, ContentHash, InputName, Origin, Pin, ProjectId, Redacted, Revision, VariableName,
};

/// Makes [`ContentHash`]es, through the recipe or the parser, the only ways
/// one is made.
pub struct ContentHashMother;

impl ContentHashMother {
    /// The hash of bytes nobody chose.
    pub fn random() -> ContentHash {
        hash::bytes(&random::bytes(random::between(0, 256)))
    }

    /// The hash the recipe writes for `bytes`.
    pub fn of(bytes: &[u8]) -> ContentHash {
        hash::bytes(bytes)
    }

    /// The hash `digits` spells, which must be one.
    pub fn create(digits: &str) -> ContentHash {
        ContentHash::parse(digits).unwrap_or_else(|| panic!("{digits:?} is no hash"))
    }

    /// Digits that spell a hash, in lowercase, without making one.
    pub fn digits() -> String {
        random::hex(ContentHash::LEN)
    }

    /// The same hash, spelled with some of its letters in capitals, as one
    /// copied from elsewhere may be.
    pub fn respelled(hash: &ContentHash) -> String {
        hash.chars()
            .map(|c| {
                if random::chance() {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect()
    }

    /// Texts that are no hash, each beside why: what `parse` refuses.
    pub fn not_hashes() -> Vec<(&'static str, String)> {
        let digits = Self::digits();
        vec![
            ("empty", String::new()),
            ("one digit short", digits[1..].to_owned()),
            ("one digit over", format!("{digits}0")),
            ("twice as long", digits.repeat(2)),
            ("a letter past f", format!("g{}", &digits[1..])),
            (
                "a space inside",
                format!("{} {}", &digits[..31], &digits[32..]),
            ),
            ("a leading space", format!(" {}", &digits[1..])),
            ("a trailing newline", format!("{}\n", &digits[1..])),
            ("a 0x prefix", format!("0x{}", &digits[2..])),
            ("the line sha256sum prints", format!("{digits}  SKILL.md")),
            ("a sha256: prefix", format!("sha256:{}", &digits[7..])),
            ("a non-ASCII digit", format!("{}\u{e9}", &digits[2..])),
            ("full-width digits", "\u{ff10}".repeat(ContentHash::LEN / 3)),
            (
                "a hyphen inside",
                format!("{}-{}", &digits[..32], &digits[33..]),
            ),
        ]
    }
}

/// Makes [`ProjectId`]s, through the generator or the parser.
pub struct ProjectIdMother;

impl ProjectIdMother {
    /// A new id, as `mind init` gives a project.
    pub fn random() -> ProjectId {
        ProjectId::generate()
    }

    /// The id `digits` spells, which must be one.
    pub fn create(digits: &str) -> ProjectId {
        ProjectId::parse(digits).unwrap_or_else(|| panic!("{digits:?} is no project id"))
    }

    /// Digits that spell an id, without making one.
    pub fn digits() -> String {
        random::hex(ProjectId::LEN)
    }

    /// Texts that are no id, each beside why: what `parse` refuses. Capitals
    /// among them, unlike a hash's: an id has one spelling, the one an
    /// exclude group is keyed by.
    pub fn not_ids() -> Vec<(&'static str, String)> {
        let digits = Self::digits();
        vec![
            ("empty", String::new()),
            ("one digit short", digits[1..].to_owned()),
            ("one digit over", format!("{digits}0")),
            ("a hash's length", random::hex(ContentHash::LEN)),
            ("in capitals", format!("A{}", &digits[1..])),
            ("all in capitals", "ABCDEF0123456789".repeat(2)),
            ("a letter past f", format!("g{}", &digits[1..])),
            (
                "a UUID's hyphens",
                format!(
                    "{}-{}-{}-{}-{}",
                    &digits[..8],
                    &digits[8..12],
                    &digits[12..16],
                    &digits[16..20],
                    &digits[20..]
                ),
            ),
            ("a leading space", format!(" {}", &digits[1..])),
            ("a trailing newline", format!("{}\n", &digits[1..])),
            ("a non-ASCII letter", format!("{}\u{e9}", &digits[2..])),
        ]
    }
}

/// Makes [`Redacted`] texts, through the redaction, the only way one is made.
pub struct RedactedMother;

impl RedactedMother {
    /// Words with no URL in them.
    pub fn random() -> Redacted {
        Redacted::new(&random::text(40).replace("://", " "))
    }

    /// What `text` is once redacted.
    pub fn create(text: &str) -> Redacted {
        Redacted::new(text)
    }

    /// A URL that was typed with a credential in it, redacted.
    pub fn url() -> Redacted {
        Redacted::new(&UrlMother::credentialed().typed)
    }
}

/// Makes [`Origin`]s, spelled as `--from` takes one
/// ([`crate::install::Offer::origin`]).
pub struct OriginMother;

impl OriginMother {
    /// The workspace's own, a loaded workspace's or a source's.
    pub fn random() -> Origin {
        match random::between(0, 2) {
            0 => Self::workspace(),
            1 => Self::loaded(&format!("../{}", random::word())),
            _ => Self::source(&UrlMother::https(), Some(&random::word())),
        }
    }

    /// Exactly `text`.
    pub fn create(text: &str) -> Origin {
        Origin::from(text)
    }

    /// The workspace's own artifacts.
    pub fn workspace() -> Origin {
        Origin::from(crate::install::WORKSPACE_ORIGIN)
    }

    /// The own of the workspace loaded as `entry`, the entry as the marker
    /// spells it.
    pub fn loaded(entry: &str) -> Origin {
        crate::install::loaded_origin(entry)
    }

    /// A source's address, with its branch after a `#` when one is given, as
    /// [`crate::ledger::Source::origin`] spells it.
    pub fn source(url: &str, branch: Option<&str>) -> Origin {
        match branch {
            Some(branch) => Origin::from(format!("{url}#{branch}")),
            None => Origin::from(url),
        }
    }
}

/// Makes [`ArtifactName`]s.
pub struct ArtifactNameMother;

impl ArtifactNameMother {
    /// A name a skill or a rule may be written under.
    pub fn random() -> ArtifactName {
        ArtifactName::from(random::kebab())
    }

    /// Exactly `text`, whatever rule it breaks: a name read from the disk
    /// is held to none.
    pub fn create(text: &str) -> ArtifactName {
        ArtifactName::from(text)
    }

    /// A rule's route: names joined by slashes, as a rule filed in folders
    /// is asked for.
    pub fn route() -> ArtifactName {
        let segments: Vec<String> = (0..random::between(2, 3))
            .map(|_| random::kebab())
            .collect();
        ArtifactName::from(segments.join("/"))
    }

    /// Names no skill or rule may be written under, each beside why. An
    /// MCP server's are held to the keys an agent takes rather than to
    /// kebab-case, so some of these are a server's
    /// ([`McpMother::unwritable`](crate::mother::McpMother::unwritable)).
    pub fn unwritable() -> Vec<(&'static str, String)> {
        vec![
            ("empty", String::new()),
            ("a capital", String::from("Deploy")),
            ("a leading hyphen", String::from("-deploy")),
            ("a trailing hyphen", String::from("deploy-")),
            ("two hyphens in a row", String::from("de--ploy")),
            ("an underscore", String::from("de_ploy")),
            ("a space", String::from("de ploy")),
            ("a dot", String::from("de.ploy")),
            ("a parent", String::from("..")),
            ("a non-ASCII letter", String::from("d\u{e9}ploy")),
            (
                "a segment over the limit",
                "a".repeat(crate::skill::MAX_NAME_SEGMENT_LEN + 1),
            ),
            ("an empty segment", String::from("ops//deploy")),
        ]
    }
}

/// Makes [`Revision`]s, as a remote names a commit.
pub struct RevisionMother;

impl RevisionMother {
    /// A commit's full SHA-1, as git spells it.
    pub fn random() -> Revision {
        Revision::from(random::hex(40))
    }

    /// Exactly `text`.
    pub fn create(text: &str) -> Revision {
        Revision::from(text)
    }
}

/// Makes [`Pin`]s, through the parser, the one way one is made, and the
/// texts it refuses.
pub struct PinMother;

impl PinMother {
    /// A commit nobody chose, in full.
    pub fn random() -> Pin {
        Self::create(&random::hex(Pin::LEN))
    }

    /// The pin `digits` spells, which must be one.
    pub fn create(digits: &str) -> Pin {
        Pin::parse(digits).unwrap_or_else(|| panic!("{digits:?} is no pin"))
    }

    /// The pin a gather at `revision` takes, which must be a commit in full.
    pub fn of(revision: &Revision) -> Pin {
        Self::create(revision)
    }

    /// Texts that are no pin, each beside why: what `parse` refuses, and
    /// what a person types for a commit that is not one in full.
    pub fn not_pins() -> Vec<(&'static str, String)> {
        let digits = random::hex(Pin::LEN);
        vec![
            ("empty", String::new()),
            ("a commit shortened as git shows it", digits[..7].to_owned()),
            ("one digit short", digits[1..].to_owned()),
            ("one digit over", format!("{digits}0")),
            ("a SHA-256 commit", random::hex(64)),
            ("a letter past f", format!("g{}", &digits[1..])),
            ("a leading space", format!(" {}", &digits[1..])),
            ("a trailing newline", format!("{}\n", &digits[1..])),
            ("a branch", String::from("main")),
            ("a tag", String::from("v1.2.0")),
            ("HEAD", String::from("HEAD")),
            ("a full reference", String::from("refs/heads/main")),
            ("a 0x prefix", format!("0x{}", &digits[2..])),
            ("a non-ASCII digit", format!("{}\u{e9}", &digits[2..])),
        ]
    }
}

/// Makes [`InputName`]s, as a definition calls what somebody supplies.
pub struct InputNameMother;

impl InputNameMother {
    /// Words joined by underscores, as an input is usually named.
    pub fn random() -> InputName {
        InputName::from(format!("{}_{}", random::word(), random::word()))
    }

    /// Exactly `text`, whatever it holds: a definition on the disk is read
    /// whatever it calls an input.
    pub fn create(text: &str) -> InputName {
        InputName::from(text)
    }

    /// Texts no placeholder can be written of, each beside why: what
    /// [`InputName::is_writable`] refuses.
    pub fn unwritable() -> Vec<(&'static str, String)> {
        let word = random::word();
        vec![
            ("empty", String::new()),
            ("a closing brace", format!("{word}}}{word}")),
            ("an opening brace", format!("{{{word}")),
            ("a space", format!("{word} {word}")),
            ("a colon", format!("env:{word}")),
            ("a dot", format!("{word}.{word}")),
            ("a dollar", format!("${word}")),
            ("a letter past ASCII", format!("{word}\u{e9}")),
        ]
    }
}

/// Makes [`VariableName`]s, as a shell names an environment variable.
pub struct VariableNameMother;

impl VariableNameMother {
    /// Capitals joined by underscores, as a variable holding a secret is
    /// usually named.
    pub fn random() -> VariableName {
        VariableName::from(format!("{}_{}", random::word(), random::word()).to_ascii_uppercase())
    }

    /// Exactly `text`.
    pub fn create(text: &str) -> VariableName {
        VariableName::from(text)
    }

    /// Texts no agent can refer to as one variable, each beside why: what
    /// [`VariableName::is_portable`] refuses.
    pub fn unportable() -> Vec<(&'static str, String)> {
        let word = random::word().to_ascii_uppercase();
        vec![
            ("empty", String::new()),
            ("a leading digit", format!("9{word}")),
            ("a hyphen", format!("{word}-{word}")),
            ("a space", format!("{word} {word}")),
            ("a closing brace", format!("{word}}}")),
            ("a default after it", format!("{word}:-x")),
            ("a dollar", format!("${word}")),
            ("a dot", format!("{word}.{word}")),
            ("a letter past ASCII", format!("{word}\u{c9}")),
        ]
    }
}
