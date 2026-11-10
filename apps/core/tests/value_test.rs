//! The value objects every record, report and view is made of, held to
//! their rules through what the crate makes public, as a caller reaches them.
//!
//! Each rule is tried both ways, on what it takes and on what it refuses,
//! and on values a mother draws afresh every run as well as the cases named
//! here, so that a change to how one is made or read, anywhere it is reused,
//! fails here before it reaches a record, a line printed or a contract. What
//! every value shares, reading as the text it holds, is one set of laws
//! ([`reads_as_its_text`]) every type is held to alike.

use std::borrow::{Borrow, Cow};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::fmt;
use std::hash::{BuildHasher, Hash, RandomState};
use std::ops::Deref;
use std::path::Path;

use mindweaver_core::hash;
use mindweaver_core::install::{is_folder_name, is_installable_name};
use mindweaver_core::mother::random;
use mindweaver_core::mother::{
    ArtifactNameMother, ContentHashMother, InputNameMother, OriginMother, PinMother,
    ProjectIdMother, RedactedMother, RevisionMother, SourceMother, UpstreamNameMother, UrlMother,
    VariableNameMother,
};
use mindweaver_core::url::carries_credentials;
use mindweaver_core::value::{
    ArtifactName, ContentHash, InputName, Origin, Pin, ProjectId, Redacted, Revision, UpstreamName,
    VariableName,
};
use mindweaver_core::Kind;
use serde::Serialize;
use serde_json::{json, Value};

/// How many values a mother draws for a rule that should hold of every one.
const DRAWS: usize = 200;

/// What every value holding text is, whatever its rule: read as `text`
/// through every way the crate offers, equal to it either way round, hashed
/// and ordered as it is, so a value can stand in a map keyed by text and a
/// sorted listing sorts as it did when the field was a `String`.
fn reads_as_its_text<T>(value: &T, text: &str)
where
    T: Deref<Target = str>
        + AsRef<str>
        + AsRef<OsStr>
        + AsRef<Path>
        + Borrow<str>
        + fmt::Display
        + fmt::Debug
        + Clone
        + Eq
        + Ord
        + Hash
        + PartialEq<str>
        + PartialEq<String>
        + for<'a> PartialEq<&'a str>
        + for<'a> PartialEq<Cow<'a, str>>
        + Into<String>,
    str: PartialEq<T>,
    String: PartialEq<T>,
    for<'a> &'a str: PartialEq<T>,
{
    let context = format!("{text:?}");
    same(value, text, &format!("{context}: deref"));
    same(
        AsRef::<str>::as_ref(value),
        text,
        &format!("{context}: as str"),
    );
    assert_eq!(
        AsRef::<OsStr>::as_ref(value),
        OsStr::new(text),
        "{context}: as an OsStr"
    );
    assert_eq!(
        AsRef::<Path>::as_ref(value),
        Path::new(text),
        "{context}: as a Path"
    );
    same(
        Borrow::<str>::borrow(value),
        text,
        &format!("{context}: borrowed"),
    );
    same(&format!("{value}"), text, &format!("{context}: displayed"));
    same(
        &format!("{value:?}"),
        &format!("{text:?}"),
        &format!("{context}: debugged"),
    );

    assert!(*value == *text, "{context}: == str");
    assert!(*value == text, "{context}: == &str");
    let owned = text.to_owned();
    assert!(*value == owned, "{context}: == String");
    assert!(*value == Cow::Borrowed(text), "{context}: == Cow");
    assert!(*text == *value, "{context}: str ==");
    assert!(text == *value, "{context}: &str ==");
    assert!(owned == *value, "{context}: String ==");

    let copy = value.clone();
    assert_eq!(copy, *value, "{context}: a clone");
    let hasher = RandomState::new();
    assert_eq!(
        hasher.hash_one(value),
        hasher.hash_one(text),
        "{context}: hashed as its text, as Borrow<str> requires"
    );
    let set = HashSet::from([value.clone()]);
    assert!(set.contains(text), "{context}: found by its text in a set");
    let map = BTreeMap::from([(value.clone(), ())]);
    assert!(
        map.contains_key(text),
        "{context}: found by its text in a map"
    );

    let given: String = copy.into();
    same(&given, text, &format!("{context}: given up as a String"));
}

/// That `read` is `text`, out of the generic function above, where the
/// bounds that let a value be compared with text either way round leave
/// two texts with more than one comparison to choose.
fn same(read: &str, text: &str, what: &str) {
    assert_eq!(read, text, "{what}");
}

/// That two values order as their texts do.
fn orders_as_its_text<T: Ord + Deref<Target = str> + fmt::Debug>(a: &T, b: &T) {
    assert_eq!(a.cmp(b), (**a).cmp(&**b), "{a:?} against {b:?}");
}

/// What serde writes of `value`.
fn written<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

// ---------------------------------------------------------------------------
// ContentHash: made by the recipe or the parser, 64 lowercase digits.
// ---------------------------------------------------------------------------

#[test]
fn a_hash_reads_as_the_digits_it_holds() {
    for _ in 0..DRAWS {
        let hash = ContentHashMother::random();
        let digits = hash.as_str().to_owned();

        reads_as_its_text(&hash, &digits);
        assert_eq!(hash.clone().into_string(), digits);
    }
}

#[test]
fn every_hash_the_recipe_makes_is_64_lowercase_hex_digits() {
    for _ in 0..DRAWS {
        let hash = ContentHashMother::random();

        assert_eq!(hash.len(), ContentHash::LEN, "{hash}");
        assert!(
            hash.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{hash}"
        );
    }
}

#[test]
fn a_hash_is_64_digits_since_it_is_a_sha256() {
    assert_eq!(ContentHash::LEN, 64);
    // The SHA-256 of nothing, as every implementation of it writes it: the
    // recipe for bytes is SHA-256 and nothing around it.
    assert_eq!(
        ContentHashMother::of(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn the_recipe_gives_one_hash_for_one_content_and_another_for_any_other() {
    for _ in 0..DRAWS {
        let bytes = random::bytes(random::between(0, 64));
        let mut other = bytes.clone();
        other.push(random::number() as u8);

        assert_eq!(hash::bytes(&bytes), hash::bytes(&bytes), "{bytes:?}");
        assert_ne!(hash::bytes(&bytes), hash::bytes(&other), "{bytes:?}");
    }
}

#[test]
fn a_hash_the_recipe_wrote_parses_back_to_itself() {
    for _ in 0..DRAWS {
        let hash = ContentHashMother::random();

        assert_eq!(ContentHash::parse(&hash), Some(hash.clone()), "{hash}");
    }
}

#[test]
fn a_hash_spelled_with_capitals_is_the_same_hash_kept_in_lowercase() {
    for _ in 0..DRAWS {
        let hash = ContentHashMother::random();
        let spelled = ContentHashMother::respelled(&hash);

        let parsed = ContentHash::parse(&spelled).unwrap();
        assert_eq!(parsed, hash, "{spelled}");
        assert_eq!(parsed.as_str(), hash.as_str(), "{spelled}");
    }
    let hash = ContentHashMother::random();
    assert_eq!(
        ContentHash::parse(&hash.to_ascii_uppercase()),
        Some(hash.clone())
    );
}

#[test]
fn any_64_hex_digits_are_a_hash() {
    for _ in 0..DRAWS {
        let digits = ContentHashMother::digits();

        let hash = ContentHash::parse(&digits).unwrap_or_else(|| panic!("{digits} refused"));
        assert_eq!(hash, digits.as_str());
        assert_eq!(ContentHashMother::create(&digits), hash);
    }
}

#[test]
fn a_text_that_is_no_hash_is_refused_whatever_keeps_it_from_being_one() {
    for (why, text) in ContentHashMother::not_hashes() {
        assert_eq!(ContentHash::parse(&text), None, "{why}: {text:?}");
        assert!(!hash::is_digest(&text), "{why}: {text:?}");
    }
}

#[test]
fn parsing_a_hash_again_changes_nothing() {
    for _ in 0..DRAWS {
        let spelled = ContentHashMother::respelled(&ContentHashMother::random());
        let once = ContentHash::parse(&spelled).unwrap();

        assert_eq!(ContentHash::parse(&once), Some(once.clone()), "{spelled}");
    }
}

#[test]
fn is_digest_says_exactly_what_parse_does() {
    let mut texts: Vec<String> = ContentHashMother::not_hashes()
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    texts.extend((0..DRAWS).map(|_| ContentHashMother::respelled(&ContentHashMother::random())));
    texts.extend((0..DRAWS).map(|_| random::text(70)));

    for text in texts {
        assert_eq!(
            hash::is_digest(&text),
            ContentHash::parse(&text).is_some(),
            "{text:?}"
        );
    }
}

#[test]
fn a_hash_is_written_as_its_digits_and_nothing_around_them() {
    let hash = ContentHashMother::random();

    assert_eq!(written(&hash), json!(hash.as_str()));
    assert_eq!(
        serde_json::to_string(&Some(&hash)).unwrap(),
        format!("\"{hash}\"")
    );
}

#[test]
fn hashes_order_as_their_digits() {
    for _ in 0..DRAWS {
        orders_as_its_text(&ContentHashMother::random(), &ContentHashMother::random());
    }
}

#[test]
fn a_folders_hash_is_the_trees_and_parses_as_any_other() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("SKILL.md"), "deploy").unwrap();

    let tree = hash::tree(dir.path()).unwrap();

    assert_eq!(tree.len(), ContentHash::LEN);
    assert_eq!(ContentHash::parse(&tree), Some(tree.clone()));
    // A tree is hashed with its names, so it is never its one file's bytes.
    assert_ne!(tree, hash::bytes(b"deploy"));
}

// ---------------------------------------------------------------------------
// ProjectId: made by the generator or the parser, 32 lowercase digits.
// ---------------------------------------------------------------------------

#[test]
fn a_new_id_is_32_lowercase_hex_digits_that_parse_back_to_it() {
    for _ in 0..DRAWS {
        let id = ProjectIdMother::random();

        assert_eq!(id.as_str().len(), ProjectId::LEN, "{id}");
        assert!(
            id.as_str()
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{id}"
        );
        assert_eq!(ProjectId::parse(id.as_str()), Some(id.clone()), "{id}");
    }
}

#[test]
fn no_two_new_ids_are_alike() {
    let ids: BTreeSet<ProjectId> = (0..1000).map(|_| ProjectIdMother::random()).collect();

    assert_eq!(ids.len(), 1000);
}

#[test]
fn any_32_lowercase_hex_digits_are_an_id() {
    for _ in 0..DRAWS {
        let digits = ProjectIdMother::digits();

        let id = ProjectId::parse(&digits).unwrap_or_else(|| panic!("{digits} refused"));
        assert_eq!(id.as_str(), digits);
        assert_eq!(ProjectIdMother::create(&digits), id);
    }
}

#[test]
fn a_text_that_is_no_id_is_refused_capitals_included() {
    for (why, text) in ProjectIdMother::not_ids() {
        assert_eq!(ProjectId::parse(&text), None, "{why}: {text:?}");
    }
}

#[test]
fn an_id_reads_and_is_written_as_its_digits() {
    let id = ProjectIdMother::random();

    assert_eq!(id.to_string(), id.as_str());
    assert_eq!(written(&id), json!(id.as_str()));
    // Debug says the type around the digits, as it is derived: a report
    // printed with `{:?}` names what it holds.
    assert_eq!(format!("{id:?}"), format!("ProjectId({:?})", id.as_str()));
}

#[test]
fn ids_are_equal_hashed_and_ordered_as_their_digits() {
    for _ in 0..DRAWS {
        let (a, b) = (ProjectIdMother::random(), ProjectIdMother::random());

        assert_eq!(a.cmp(&b), a.as_str().cmp(b.as_str()), "{a} against {b}");
        assert_eq!(ProjectId::parse(a.as_str()).unwrap(), a);
        let hasher = RandomState::new();
        assert_eq!(hasher.hash_one(&a), hasher.hash_one(a.clone()));
    }
}

// ---------------------------------------------------------------------------
// Redacted: made only by the redaction, so it holds no credential.
// ---------------------------------------------------------------------------

#[test]
fn every_way_a_credential_is_typed_into_a_url_is_taken_out() {
    for url in UrlMother::every_credentialed() {
        let redacted = Redacted::new(&url.typed);

        assert_eq!(redacted, url.clean.as_str(), "{}", url.typed);
        assert!(!redacted.contains(&url.secret), "{}", url.typed);
        assert!(!carries_credentials(&redacted), "{}", url.typed);
        assert!(carries_credentials(&url.typed), "{}", url.typed);
    }
}

#[test]
fn a_url_with_no_credential_is_left_as_it_was_typed() {
    for url in UrlMother::every_clean() {
        assert_eq!(Redacted::new(&url), url.as_str(), "{url}");
        assert!(!carries_credentials(&url), "{url}");
    }
}

#[test]
fn words_quoting_a_url_keep_every_word_but_the_credential() {
    for _ in 0..DRAWS {
        let quoted = UrlMother::quoted(&UrlMother::credentialed());

        let redacted = Redacted::new(&quoted.typed);
        assert_eq!(redacted, quoted.clean.as_str(), "{}", quoted.typed);
        assert!(!redacted.contains(&quoted.secret), "{}", quoted.typed);
    }
}

#[test]
fn every_url_in_a_text_loses_its_credential_wherever_it_stands() {
    for _ in 0..DRAWS {
        let urls: Vec<_> = (0..random::between(1, 4))
            .map(|_| UrlMother::credentialed())
            .collect();
        let typed: Vec<&str> = urls.iter().map(|url| url.typed.as_str()).collect();
        let clean: Vec<&str> = urls.iter().map(|url| url.clean.as_str()).collect();
        let glue = random::pick(&[" ", "\n", " and ", "; then "]);

        let redacted = Redacted::new(&typed.join(glue));

        assert_eq!(redacted, clean.join(glue), "{typed:?}");
        for url in &urls {
            assert!(!redacted.contains(&url.secret), "{typed:?}");
        }
    }
}

#[test]
fn a_text_with_no_url_is_kept_as_it_is() {
    for _ in 0..DRAWS {
        let text = random::text(60).replace("://", " ");

        assert_eq!(Redacted::new(&text), text.as_str(), "{text:?}");
        assert_eq!(RedactedMother::create(&text), text.as_str(), "{text:?}");
    }
}

#[test]
fn redacting_what_is_already_redacted_changes_nothing() {
    let mut texts: Vec<String> = UrlMother::every_credentialed()
        .into_iter()
        .map(|url| url.typed)
        .collect();
    texts.extend(UrlMother::every_clean());
    texts.extend((0..DRAWS).map(|_| UrlMother::quoted(&UrlMother::credentialed()).typed));
    texts.extend((0..DRAWS).map(|_| random::text(60)));

    for text in texts {
        let once = Redacted::new(&text);
        let twice = Redacted::new(&once);

        assert_eq!(twice, once, "{text:?}");
    }
}

#[test]
fn a_redacted_text_shows_and_writes_no_credential_however_it_is_read() {
    for url in UrlMother::every_credentialed() {
        let redacted = Redacted::new(&url.typed);

        for shown in [
            redacted.to_string(),
            format!("{redacted:?}"),
            written(&redacted).to_string(),
            redacted.clone().into_string(),
            String::from(redacted.clone()),
        ] {
            assert!(!shown.contains(&url.secret), "{}: {shown}", url.typed);
        }
    }
}

#[test]
fn a_redacted_text_reads_as_what_is_left_of_it() {
    for _ in 0..DRAWS {
        let url = UrlMother::credentialed();
        let redacted = Redacted::new(&url.typed);

        reads_as_its_text(&redacted, &url.clean);
    }
    let text = random::text(30).replace("://", " ");
    reads_as_its_text(&Redacted::new(&text), &text);
}

#[test]
fn a_redacted_mothers_values_hold_no_credential() {
    for _ in 0..DRAWS {
        for redacted in [RedactedMother::random(), RedactedMother::url()] {
            assert!(!carries_credentials(&redacted), "{redacted}");
        }
    }
}

// ---------------------------------------------------------------------------
// Origin: any text, kept as typed, written out without a credential.
// ---------------------------------------------------------------------------

#[test]
fn an_origin_holds_exactly_the_text_it_was_made_from() {
    for _ in 0..DRAWS {
        let text = random::text(40);

        reads_as_its_text(&Origin::from(text.as_str()), &text);
        assert_eq!(Origin::from(text.clone()), text);
        assert_eq!(Origin::from(&text), text);
        assert_eq!(Origin::from(Cow::Borrowed(text.as_str())), text);
        assert_eq!(OriginMother::create(&text), text);
    }
}

#[test]
fn an_origin_is_written_without_any_credential_it_was_typed_with() {
    for url in UrlMother::every_credentialed() {
        let branch = random::word();
        let origin = OriginMother::source(&url.typed, Some(&branch));

        assert_eq!(origin, format!("{}#{branch}", url.typed));
        assert_eq!(written(&origin), json!(format!("{}#{branch}", url.clean)));
        assert!(!written(&origin).to_string().contains(&url.secret));
    }
}

#[test]
fn an_origin_with_no_credential_is_written_as_it_reads() {
    for _ in 0..DRAWS {
        let origin = OriginMother::random();

        assert_eq!(written(&origin), json!(origin.as_str()), "{origin}");
    }
}

#[test]
fn an_origin_is_read_from_any_string_and_from_nothing_else() {
    for _ in 0..DRAWS {
        let text = random::text(40);

        let read: Origin = serde_json::from_value(json!(text)).unwrap();
        assert_eq!(read, text, "{text:?}");
    }
    for refused in [json!(null), json!(7), json!(true), json!([]), json!({})] {
        assert!(
            serde_json::from_value::<Origin>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn an_origin_written_and_read_again_is_the_same_origin() {
    for _ in 0..DRAWS {
        let origin = OriginMother::random();

        let again: Origin = serde_json::from_value(written(&origin)).unwrap();
        assert_eq!(again, origin);
    }
}

#[test]
fn each_place_an_origin_names_is_spelled_as_from_takes_it() {
    let url = UrlMother::https();

    assert_eq!(OriginMother::workspace(), "workspace");
    assert_eq!(OriginMother::loaded("../team"), "load:../team");
    assert_eq!(OriginMother::source(&url, None), url.as_str());
    assert_eq!(
        OriginMother::source(&url, Some("main")),
        format!("{url}#main")
    );
}

#[test]
fn a_sources_origin_is_its_address_and_branch_spelled_as_an_origin() {
    for _ in 0..DRAWS {
        let url = UrlMother::clean();
        let reference = random::chance().then(random::word);

        let source = SourceMother::create(&url, reference.as_deref());

        assert_eq!(
            source.origin(),
            OriginMother::source(&url, reference.as_deref()),
            "{url} {reference:?}"
        );
    }
}

#[test]
fn a_sources_origin_holds_no_credential_whatever_url_it_was_registered_with() {
    for url in UrlMother::every_credentialed() {
        let source = SourceMother::create(&url.typed, Some("main"));

        assert_eq!(source.origin(), format!("{}#main", url.clean));
        assert!(!source.origin().contains(&url.secret), "{}", url.typed);
    }
}

// ---------------------------------------------------------------------------
// ArtifactName: any text, a type of its own.
// ---------------------------------------------------------------------------

#[test]
fn a_name_holds_any_text_since_a_name_on_the_disk_may_break_the_rules() {
    for _ in 0..DRAWS {
        let text = random::text(40);

        reads_as_its_text(&ArtifactName::from(text.as_str()), &text);
    }
    for (why, text) in ArtifactNameMother::unwritable() {
        assert_eq!(ArtifactNameMother::create(&text), text.as_str(), "{why}");
    }
}

#[test]
fn a_name_the_mother_draws_is_one_every_kind_can_file_a_copy_under() {
    for _ in 0..DRAWS {
        let name = ArtifactNameMother::random();

        assert!(is_folder_name(&name), "{name}");
        for kind in Kind::ALL {
            assert!(is_installable_name(kind, &name), "{kind:?} {name}");
        }
    }
}

#[test]
fn a_route_is_a_rules_name_and_never_a_skills() {
    for _ in 0..DRAWS {
        let route = ArtifactNameMother::route();

        assert!(is_installable_name(Kind::Rule, &route), "{route}");
        assert!(!is_installable_name(Kind::Skill, &route), "{route}");
    }
}

#[test]
fn a_name_that_breaks_the_rules_files_no_rule() {
    for (why, name) in ArtifactNameMother::unwritable() {
        assert!(!is_installable_name(Kind::Rule, &name), "{why}: {name:?}");
    }
}

#[test]
fn a_name_that_is_no_one_folder_files_no_skill() {
    for name in ["", "..", ".", "a/b", "a\\b", "/abs", "../up"] {
        assert!(!is_folder_name(name), "{name:?}");
        assert!(!is_installable_name(Kind::Skill, name), "{name:?}");
    }
}

#[test]
fn a_name_joins_a_path_as_one_folder() {
    for _ in 0..DRAWS {
        let name = ArtifactNameMother::random();

        let joined = Path::new("skills").join(&name);
        assert_eq!(joined, Path::new("skills").join(name.as_str()));
        assert_eq!(joined.file_name(), Some(OsStr::new(name.as_str())));
    }
}

#[test]
fn a_name_written_and_read_again_is_the_same_name() {
    for _ in 0..DRAWS {
        let name = ArtifactName::from(random::text(40));

        assert_eq!(written(&name), json!(name.as_str()));
        let again: ArtifactName = serde_json::from_value(written(&name)).unwrap();
        assert_eq!(again, name);
    }
}

#[test]
fn names_sort_as_their_text_capitals_first() {
    let mut names: Vec<ArtifactName> = ["b", "a", "B", "a-b", "a/b"].map(ArtifactName::from).into();
    names.sort();

    assert_eq!(names, ["B", "a", "a-b", "a/b", "b"]);
    for _ in 0..DRAWS {
        orders_as_its_text(
            &ArtifactName::from(random::text(8)),
            &ArtifactName::from(random::text(8)),
        );
    }
}

#[test]
fn a_name_is_found_by_its_text_in_a_map_keyed_by_names() {
    let names: Vec<ArtifactName> = (0..50).map(|_| ArtifactNameMother::random()).collect();
    let map: HashMap<ArtifactName, usize> = names
        .iter()
        .cloned()
        .enumerate()
        .map(|(n, name)| (name, n))
        .collect();

    for name in &names {
        assert!(map.contains_key(name.as_str()), "{name}");
    }
}

// ---------------------------------------------------------------------------
// Revision: any text, as the remote said it.
// ---------------------------------------------------------------------------

#[test]
fn a_revision_holds_what_the_remote_said_as_it_said_it() {
    for _ in 0..DRAWS {
        let revision = RevisionMother::random();
        let text = revision.as_str().to_owned();

        assert_eq!(text.len(), 40);
        reads_as_its_text(&revision, &text);
        assert_eq!(RevisionMother::create(&text), revision);
    }
    let text = random::text(20);
    reads_as_its_text(&Revision::from(text.as_str()), &text);
}

#[test]
fn a_revision_written_and_read_again_is_the_same_revision() {
    for _ in 0..DRAWS {
        let revision = Revision::from(random::text(40));

        assert_eq!(written(&revision), json!(revision.as_str()));
        let again: Revision = serde_json::from_value(written(&revision)).unwrap();
        assert_eq!(again, revision);
    }
}

#[test]
fn a_revision_is_read_from_a_string_and_from_nothing_else() {
    for refused in [json!(null), json!(40), json!(["abc"])] {
        assert!(
            serde_json::from_value::<Revision>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

// ---------------------------------------------------------------------------
// Pin: made by the parser, a commit in full, 40 lowercase digits.
// ---------------------------------------------------------------------------

#[test]
fn a_pin_reads_as_the_commit_it_holds() {
    for _ in 0..DRAWS {
        let pin = PinMother::random();
        let text = pin.as_str().to_owned();

        assert_eq!(text.len(), Pin::LEN);
        reads_as_its_text(&pin, &text);
        assert_eq!(PinMother::create(&text), pin);
    }
}

#[test]
fn any_40_hex_digits_are_a_pin_kept_in_lowercase() {
    for _ in 0..DRAWS {
        let digits = random::hex(Pin::LEN);
        let shouted = digits.to_ascii_uppercase();

        assert_eq!(Pin::parse(&digits).unwrap(), digits.as_str());
        assert_eq!(Pin::parse(&shouted).unwrap(), digits.as_str());
    }
}

#[test]
fn a_text_that_names_no_commit_in_full_is_no_pin_whatever_keeps_it_from_being_one() {
    for (why, text) in PinMother::not_pins() {
        assert_eq!(Pin::parse(&text), None, "{why}: {text:?}");
    }
}

#[test]
fn a_pin_is_the_revision_a_gather_at_it_takes() {
    for _ in 0..DRAWS {
        let revision = RevisionMother::random();
        let pin = PinMother::of(&revision);

        assert_eq!(pin.revision(), revision);
        assert_eq!(PinMother::of(&pin.revision()), pin);
    }
}

#[test]
fn a_pin_is_written_as_its_digits_and_nothing_around_them() {
    let pin = PinMother::random();

    assert_eq!(written(&pin), json!(pin.as_str()));
    assert_eq!(written(&Some(pin.clone())), json!(pin.as_str()));
    assert_eq!(written(&None::<Pin>), json!(null));
}

// ---------------------------------------------------------------------------
// InputName and VariableName: any text, each a type of its own, and each
// with a rule `validate` asks of it.
// ---------------------------------------------------------------------------

#[test]
fn an_input_name_holds_any_text_since_a_definition_may_call_an_input_anything() {
    for _ in 0..DRAWS {
        let text = random::text(40);

        reads_as_its_text(&InputName::from(text.as_str()), &text);
        assert_eq!(InputNameMother::create(&text), text.as_str());
    }
    let drawn = InputNameMother::random();
    reads_as_its_text(&drawn, &drawn.clone().into_string());
}

#[test]
fn a_variable_name_holds_any_text_since_which_names_a_secret_takes_is_not_its_rule() {
    for _ in 0..DRAWS {
        let text = random::text(40);

        reads_as_its_text(&VariableName::from(text.as_str()), &text);
        assert_eq!(VariableNameMother::create(&text), text.as_str());
    }
    let drawn = VariableNameMother::random();
    reads_as_its_text(&drawn, &drawn.clone().into_string());
}

#[test]
fn an_input_name_is_writable_only_of_letters_digits_hyphens_and_underscores() {
    let alphabet = format!(
        "{}{}{}_-",
        random::LOWERCASE,
        random::LOWERCASE.to_ascii_uppercase(),
        random::DIGITS
    );
    for _ in 0..DRAWS {
        let drawn = InputNameMother::random();
        let any = InputName::from(random::drawn_from(&alphabet, random::between(1, 30)));

        assert!(drawn.is_writable(), "{drawn}");
        assert!(any.is_writable(), "{any}");
    }
    for (why, text) in InputNameMother::unwritable() {
        assert!(
            !InputName::from(text.as_str()).is_writable(),
            "{why}: {text:?}"
        );
    }
}

#[test]
fn a_variable_name_is_portable_only_as_posix_names_one() {
    let letters = format!(
        "{}{}_",
        random::LOWERCASE,
        random::LOWERCASE.to_ascii_uppercase()
    );
    let rest = format!("{letters}{}", random::DIGITS);
    for _ in 0..DRAWS {
        let drawn = VariableNameMother::random();
        let any = VariableName::from(format!(
            "{}{}",
            random::drawn_from(&letters, 1),
            random::drawn_from(&rest, random::between(0, 30))
        ));

        assert!(drawn.is_portable(), "{drawn}");
        assert!(any.is_portable(), "{any}");
    }
    for (why, text) in VariableNameMother::unportable() {
        assert!(
            !VariableName::from(text.as_str()).is_portable(),
            "{why}: {text:?}"
        );
    }
}

#[test]
fn input_and_variable_names_written_and_read_again_are_the_same_names() {
    for _ in 0..DRAWS {
        let input = InputName::from(random::text(40));
        let variable = VariableName::from(random::text(40));

        assert_eq!(written(&input), json!(input.as_str()));
        assert_eq!(written(&variable), json!(variable.as_str()));
        let input_again: InputName = serde_json::from_value(written(&input)).unwrap();
        let variable_again: VariableName = serde_json::from_value(written(&variable)).unwrap();
        assert_eq!(input_again, input);
        assert_eq!(variable_again, variable);
    }
    for refused in [json!(null), json!(7), json!(["token"])] {
        assert!(
            serde_json::from_value::<InputName>(refused.clone()).is_err(),
            "{refused}"
        );
        assert!(
            serde_json::from_value::<VariableName>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

// ---------------------------------------------------------------------------
// UpstreamName: any text, since a registry's answer is read whatever it names
// a server, and a rule asked of it before it goes into a request's path.
// ---------------------------------------------------------------------------

#[test]
fn an_upstream_name_holds_any_text_since_a_registry_may_answer_anything() {
    for _ in 0..DRAWS {
        let text = random::text(40);

        reads_as_its_text(&UpstreamName::from(text.as_str()), &text);
        assert_eq!(UpstreamNameMother::create(&text), text.as_str());
    }
    let drawn = UpstreamNameMother::random();
    reads_as_its_text(&drawn, &drawn.clone().into_string());
}

#[test]
fn an_upstream_name_is_well_formed_only_as_a_namespace_a_slash_and_a_name() {
    let namespace = format!("{}{}.-", random::LOWERCASE, random::DIGITS);
    let name = format!(
        "{}{}{}._-",
        random::LOWERCASE,
        random::LOWERCASE.to_ascii_uppercase(),
        random::DIGITS
    );
    for _ in 0..DRAWS {
        let drawn = UpstreamNameMother::random();
        let any = UpstreamName::from(format!(
            "{}/{}",
            random::drawn_from(&namespace, random::between(1, 40)),
            random::drawn_from(&name, random::between(1, 40))
        ));

        assert!(drawn.is_well_formed(), "{drawn}");
        assert!(any.is_well_formed(), "{any}");
    }
    let longest = format!("io/{}", "a".repeat(UpstreamName::MAX_LEN - 3));
    assert!(UpstreamName::from(longest.as_str()).is_well_formed());
    for (why, text) in UpstreamNameMother::ill_formed() {
        assert!(
            !UpstreamName::from(text.as_str()).is_well_formed(),
            "{why}: {text:?}"
        );
    }
}

#[test]
fn an_upstream_name_written_and_read_again_is_the_same_name() {
    for _ in 0..DRAWS {
        let name = UpstreamName::from(random::text(40));

        assert_eq!(written(&name), json!(name.as_str()));
        let again: UpstreamName = serde_json::from_value(written(&name)).unwrap();
        assert_eq!(again, name);
    }
    for refused in [json!(null), json!(7), json!(["io.github.a/b"])] {
        assert!(
            serde_json::from_value::<UpstreamName>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

// ---------------------------------------------------------------------------
// Across the values.
// ---------------------------------------------------------------------------

#[test]
fn values_holding_one_text_compare_with_it_and_not_with_each_other() {
    let text = random::kebab();
    let name = ArtifactName::from(text.as_str());
    let revision = Revision::from(text.as_str());
    let origin = Origin::from(text.as_str());

    // Each equals the text it holds; what keeps a name from being handed
    // where a revision is meant is the compiler, which no runtime test can
    // show, so this holds what can be: each reads as that one text.
    assert_eq!(name, text.as_str());
    assert_eq!(revision, text.as_str());
    assert_eq!(origin, text.as_str());
    assert_eq!(name.as_str(), revision.as_str());
}

#[test]
fn a_value_in_an_option_is_written_as_its_text_or_as_null() {
    let some = Some(ArtifactNameMother::random());
    let none: Option<ArtifactName> = None;

    assert_eq!(written(&some), json!(some.as_ref().unwrap().as_str()));
    assert_eq!(written(&none), json!(null));
}

#[test]
fn values_keep_their_text_in_a_sorted_set_as_strings_would() {
    let texts: BTreeSet<String> = (0..50).map(|_| random::text(10)).collect();
    let names: BTreeSet<ArtifactName> = texts.iter().map(ArtifactName::from).collect();

    let back: Vec<&str> = names.iter().map(ArtifactName::as_str).collect();
    let expected: Vec<&str> = texts.iter().map(String::as_str).collect();
    assert_eq!(back, expected);
}
