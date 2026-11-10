//! Unit tests for `schema.rs`: the export the API's schemas come from, and
//! every schema written by hand held to what serde writes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::*;
use crate::agent::Agent;
use crate::artifact::ValidationIssue;
use crate::browse;
use crate::contents::Withheld;
use crate::described::field_serializers;
use crate::install::Place;
use crate::kind::Kind;
use crate::ledger::SourceKind;
use crate::mcp::{InputKind, TransportType};
use crate::resolve::Reason;
use crate::scan;
use crate::settings::{ProjectId, Share};
use crate::status::State;
use crate::template::{Origin as TemplateOrigin, Template};
use crate::update::Does;
use crate::url::redacted;
use crate::value::{Origin, Redacted};
use crate::view::Health;
use crate::workspace::MarkerFile;

/// Where the API's schemas are written: `docs/api/schemas` in the
/// repository, whichever folder cargo was started in, beside the docs that
/// describe the API rather than inside the panel, which reads none of them.
const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api/schemas");

/// The export the API's schemas come from, which `make api/schemas` runs
/// with the feature: every root core gives, each into a file of its own.
#[test]
fn export_schemas() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    schemas.write(Path::new(FOLDER)).unwrap();
}

#[test]
fn no_two_roots_are_written_to_one_file() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    assert_eq!(schemas.shared(), Vec::<String>::new());
}

/// Across every root at once, as a description of the whole API holds
/// them: the CLI's test holds core's, its own and the server's together.
#[test]
fn no_two_types_core_gives_go_by_one_name() {
    let mut together = Together::default();
    roots(&mut together);

    assert_eq!(together.renamed(), Vec::<String>::new());
}

/// Two types of one name, from two modules.
mod one {
    #[derive(schemars::JsonSchema)]
    pub struct Twin;
}

mod two {
    #[derive(schemars::JsonSchema)]
    pub struct Twin;
}

/// What finds them, which a test that found none would pass unseen:
/// schemars names the second with a number, and says nothing.
#[test]
fn two_types_going_by_one_name_are_found() {
    let mut together = Together::default();
    together.given::<one::Twin>();
    together.given::<two::Twin>();

    assert_eq!(together.renamed(), vec!["Twin2".to_owned()]);
}

/// A root by the name a route gives its type, its file's schema whole,
/// every type it names in its `$defs`.
#[test]
fn a_root_is_found_by_the_name_its_type_goes_by() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    let snapshot = schemas.get("Snapshot").unwrap();
    assert_eq!(snapshot.get("title"), Some(&Value::from("Snapshot")));
    assert_eq!(
        snapshot.get("$schema"),
        Some(&Value::from("https://json-schema.org/draft/2020-12/schema"))
    );
    assert!(snapshot.pointer("/$defs/Project").is_some());
    assert!(schemas.get("Project").is_some());
    assert!(schemas.get("Nothing").is_none());
}

/// A type the API is sent and answers with both, as a task is, which
/// serde refuses a field it does not know in, with a map in it.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Both {
    name: String,
    labels: BTreeMap<String, String>,
}

/// An answer holds a field it does not name, since one added moves no
/// version, where what is sent refuses it as serde does; a map's values are
/// still held to what they are, opened or not.
#[test]
fn an_answer_is_open_to_a_field_it_does_not_name_and_what_is_sent_is_closed() {
    let mut schemas = Schemas::default();
    schemas.given::<Both>();
    let given = schemas.get("Both").unwrap().clone();
    let mut schemas = Schemas::default();
    schemas.taken::<Both>();
    let taken = schemas.get("Both").unwrap().clone();

    let row = json!({"name": "kit", "labels": {"team": "web"}});
    holds(&given, &row).unwrap();
    holds(&taken, &row).unwrap();
    let added = json!({"name": "kit", "labels": {}, "added": 1});
    holds(&given, &added).unwrap();
    assert!(holds(&taken, &added).is_err());
    let label = json!({"name": "kit", "labels": {"team": 1}});
    assert!(holds(&given, &label).is_err());
    assert!(holds(&taken, &label).is_err());
}

/// `T`'s schema as its file would hold it, the way it is given.
fn given<T: JsonSchema>() -> Schema {
    given_settings()
        .into_generator()
        .into_root_schema_for::<T>()
}

/// Whether `value` is one `schema` describes, and what keeps it from being
/// one when it is not, as a JSON Schema 2020-12 validator says it.
///
/// With no loader at all, so nothing a `$ref` names is read from anywhere:
/// every schema here holds every type it names in its own `$defs`.
fn holds(schema: &Schema, value: &Value) -> Result<(), String> {
    let mut compiler = boon::Compiler::new();
    compiler.use_loader(Box::new(boon::SchemeUrlLoader::new()));
    compiler
        .add_resource("file:///schema.json", schema.as_value().clone())
        .unwrap();
    let mut compiled = boon::Schemas::new();
    let index = compiler
        .compile("file:///schema.json", &mut compiled)
        .unwrap_or_else(|error| panic!("{error:#}"));
    compiled
        .validate(value, index)
        .map_err(|error| format!("{error:#}"))
}

/// That `value` holds to `T`'s schema, said with both when it does not.
fn holds_to<T: JsonSchema>(value: &Value) {
    let schema = given::<T>();
    if let Err(error) = holds(&schema, value) {
        panic!("{value} is no {}: {error}", T::schema_name());
    }
}

/// That `value` does not hold to `T`'s schema.
fn refused_by<T: JsonSchema>(value: &Value) {
    assert!(
        holds(&given::<T>(), value).is_err(),
        "{value} holds to {}",
        T::schema_name()
    );
}

/// That a word type's schema is a string, one of exactly the words serde
/// writes of `values`, every variant of `T`: a variant left out of
/// `values` shows as a word only the schema says, and a word only serde
/// writes is refused. And that what is no word of it is refused.
fn says_the_words_serde_writes<T: Serialize + JsonSchema>(values: &[T]) {
    let written: BTreeSet<String> = values
        .iter()
        .map(|value| match serde_json::to_value(value).unwrap() {
            Value::String(word) => word,
            other => panic!("{other} is no word"),
        })
        .collect();
    let schema = given::<T>();
    let said: BTreeSet<String> = schema
        .get("enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} lists no words", T::schema_name()))
        .iter()
        .map(|word| word.as_str().unwrap().to_owned())
        .collect();

    assert_eq!(said, written, "{}", T::schema_name());
    for word in &written {
        holds_to::<T>(&Value::from(word.as_str()));
    }
    refused_by::<T>(&Value::from("?"));
    refused_by::<T>(&Value::from(1));
}

#[test]
fn every_word_type_s_schema_says_the_words_serde_writes() {
    says_the_words_serde_writes(&Kind::ALL);
    says_the_words_serde_writes(&Agent::ALL);
    says_the_words_serde_writes(&State::ALL);
    says_the_words_serde_writes(&Health::ALL);
    says_the_words_serde_writes(&Share::ALL);
    says_the_words_serde_writes(&[
        Reason::Explicit,
        Reason::Activated,
        Reason::Above,
        Reason::Home,
    ]);
    says_the_words_serde_writes(&[
        MarkerFile::MindToml,
        MarkerFile::SettingsJson,
        MarkerFile::SettingsToml,
    ]);
    says_the_words_serde_writes(&[SourceKind::Git]);
    says_the_words_serde_writes(&[
        scan::Reason::Id,
        scan::Reason::Mind,
        scan::Reason::Agents,
        scan::Reason::Git,
    ]);
    says_the_words_serde_writes(&[Withheld::Binary, Withheld::TooLarge]);
    says_the_words_serde_writes(&browse::Is::ALL);
    says_the_words_serde_writes(&[Place::Workspace, Place::Loaded, Place::Shelf]);
    says_the_words_serde_writes(&[
        Does::Keep,
        Does::Refresh,
        Does::PutBack,
        Does::WriteOver,
        Does::Leave,
        Does::Orphaned,
        Does::Unsure,
    ]);
    says_the_words_serde_writes(&[
        TransportType::Stdio,
        TransportType::Http,
        TransportType::Sse,
    ]);
    says_the_words_serde_writes(&[
        InputKind::Env,
        InputKind::Header,
        InputKind::Arg,
        InputKind::UrlVar,
    ]);
}

/// A word type's schema goes by the name its TypeScript does, a scan's
/// reason and what a folder is under names of their own, and its id by
/// its Rust path, so the two `Reason`s are never taken for one.
#[test]
fn a_word_type_named_apart_goes_by_that_name() {
    assert_eq!(Kind::schema_name(), "Kind");
    assert_eq!(scan::Reason::schema_name(), "ScanReason");
    assert_eq!(browse::Is::schema_name(), "FolderIs");
    assert_eq!(Reason::schema_name(), "Reason");
    assert_ne!(Reason::schema_id(), scan::Reason::schema_id());
    assert_eq!(
        given::<Kind>().get("description"),
        Some(&Value::from(
            "A kind of artifact, by the word a command line takes for it."
        ))
    );
}

#[test]
fn an_issue_of_every_code_holds_to_its_schema_and_only_to_its_own_member() {
    let schema = given::<ValidationIssue>();
    let members = schema.get("oneOf").and_then(Value::as_array).unwrap();
    let issues = ValidationIssue::every();
    assert_eq!(members.len(), issues.len());

    for (issue, member) in issues.iter().zip(members) {
        let json = serde_json::to_value(issue).unwrap();
        holds_to::<ValidationIssue>(&json);
        assert_eq!(member["properties"]["code"]["const"], issue.code());
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut required: Vec<&str> = member["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        required.sort_unstable();
        assert_eq!(keys, required, "{}", issue.code());
    }
}

/// An issue serde never writes: a code that is none, one without what its
/// code carries, and one carrying a count as text.
#[test]
fn an_issue_serde_never_writes_is_refused() {
    refused_by::<ValidationIssue>(&serde_json::json!({
        "code": "nope", "warning": false, "message": "",
    }));
    refused_by::<ValidationIssue>(&serde_json::json!({
        "code": "name_segment_too_long", "segment": "a", "warning": false, "message": "",
    }));
    refused_by::<ValidationIssue>(&serde_json::json!({
        "code": "description_too_long", "length": "1100", "warning": false, "message": "",
    }));
}

#[test]
fn a_project_id_holds_to_its_schema_and_nothing_else_written_does() {
    for _ in 0..8 {
        holds_to::<ProjectId>(&serde_json::to_value(ProjectId::generate()).unwrap());
    }
    for not_one in [
        "0123456789ABCDEF0123456789ABCDEF",
        "0123456789abcdef0123456789abcde",
        "0123456789abcdef0123456789abcdef0",
        "",
    ] {
        refused_by::<ProjectId>(&Value::from(not_one));
    }
}

/// A template's origin, the folder or file it is or none for one built in,
/// as its serializer writes it, and nothing else.
#[test]
fn a_template_s_origin_holds_to_its_schema_as_serde_writes_it() {
    let template = |origin: TemplateOrigin| Template {
        name: "full".to_owned(),
        kind: Kind::Skill,
        origin,
    };

    for origin in [
        TemplateOrigin::BuiltIn,
        TemplateOrigin::Path(PathBuf::from("/work/full")),
    ] {
        holds_to::<Template>(&serde_json::to_value(template(origin)).unwrap());
    }
    refused_by::<Template>(&serde_json::json!({"name": "full", "kind": "skill", "origin": 1}));
}

/// A skill's metadata, a list of pairs in Rust, is an object in JSON, and
/// its schema says so: the list a derive would say of its type is refused.
#[test]
fn a_skill_s_metadata_holds_to_its_schema_as_an_object() {
    let shown = |metadata| Shown {
        kind: Kind::Skill,
        name: "deploy".to_owned().into(),
        path: PathBuf::from("/work/skills/deploy"),
        project: PathBuf::from("/work"),
        file: PathBuf::from("/work/skills/deploy/SKILL.md"),
        summary: None,
        description: None,
        allowed_tools: None,
        license: None,
        compatibility: None,
        metadata,
        paths: None,
        transport: None,
        inputs: None,
        expressibility: None,
        body: String::new(),
    };

    let written =
        serde_json::to_value(shown(Some(vec![("team".to_owned(), "infra".to_owned())]))).unwrap();
    holds_to::<Shown>(&written);
    holds_to::<Shown>(&serde_json::to_value(shown(None)).unwrap());
    let mut listed = written;
    listed["metadata"] = serde_json::json!([["team", "infra"]]);
    refused_by::<Shown>(&listed);
}

#[test]
fn every_code_core_gives_holds_to_its_schema_and_nothing_else_does() {
    let codes: Vec<&str> = code::ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .collect();
    let said = given::<CoreCode>();

    assert_eq!(said.get("enum"), Some(&Value::from(codes.clone())));
    for code in codes {
        holds_to::<CoreCode>(&Value::from(code));
    }
    refused_by::<CoreCode>(&Value::from("nope"));
}

/// Each function a field is written through, on a field of the type it
/// takes, as every field it writes holds: schemars reads that type, as
/// ts-rs does, so the schema of the type is what the API is said to give.
/// `in_order`, which writes a skill's metadata, says its schema with
/// `schemars(with)`, held by the test of the metadata. Beside them, a URL
/// and words that may quote one, each held by a value object that writes
/// itself without a credential ([`Redacted`], [`Origin`]), whose schema is
/// a string's.
#[derive(Serialize, schemars::JsonSchema)]
struct Written {
    #[serde(serialize_with = "crate::paths::lossy")]
    path: PathBuf,
    #[serde(serialize_with = "crate::paths::lossy_all")]
    paths: Vec<PathBuf>,
    url: Redacted,
    detail: Option<Redacted>,
    none: Option<Origin>,
    origin: Origin,
}

/// Every function a field of core's names, each held to its schema by the
/// test of [`Written`]: one new joins it there first, as it joins the
/// TypeScript's.
const FIELD_SERIALIZERS: [&str; 3] = ["in_order", "paths::lossy", "paths::lossy_all"];

/// A path whose bytes are no UTF-8, where a path can be one: `lossy` writes
/// it as text all the same, which is why its schema is a string.
fn unprintable_path() -> PathBuf {
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        PathBuf::from(OsStr::from_bytes(b"/work/skills/\xffdeploy"))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from("/work/skills/deploy")
    }
}

#[test]
fn every_field_serializer_writes_what_the_schema_says_of_its_field() {
    let url = "https://user:secret@example.com/skills.git";
    let path = unprintable_path();
    let written = Written {
        path: path.clone(),
        paths: vec![path, PathBuf::from("/work/rules")],
        url: Redacted::new(url),
        detail: Some(Redacted::new(&format!("cloned {url}"))),
        none: None,
        origin: Origin::from(format!("{url}#main")),
    };

    let json = serde_json::to_value(&written).unwrap();
    holds_to::<Written>(&json);
    assert_eq!(json["url"], *redacted(url));
    assert_eq!(json["origin"], *redacted(&format!("{url}#main")));
}

#[test]
fn every_field_serializer_in_core_is_one_held_to_its_schema() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let named = field_serializers(&source).unwrap();

    assert_eq!(
        named,
        FIELD_SERIALIZERS.map(str::to_owned).into(),
        "a field written through a function the test of `Written` does not hold"
    );
}

/// What every value object of [`crate::value`] says its schema is: a
/// `String`'s, inline, with no definition of its own in `$defs`, holding
/// what serde writes of one a mother drew and refusing anything but text.
fn schema_is_a_strings<T: JsonSchema + Serialize>(value: &T) {
    let name = std::any::type_name::<T>();

    assert!(T::inline_schema(), "{name} is no inline schema");
    assert_eq!(T::schema_name(), String::schema_name(), "{name}");
    assert_eq!(
        given::<T>().as_value(),
        given::<String>().as_value(),
        "{name}"
    );
    holds_to::<T>(&serde_json::to_value(value).unwrap());
    for refused in [
        serde_json::json!(7),
        serde_json::json!(null),
        serde_json::json!({}),
    ] {
        assert!(
            holds(&given::<T>(), &refused).is_err(),
            "{name} holds {refused}"
        );
    }
}

#[test]
fn every_value_objects_schema_is_the_strings_serde_writes() {
    use crate::mother::{
        ArtifactNameMother, ContentHashMother, OriginMother, RedactedMother, RevisionMother,
    };

    for _ in 0..20 {
        schema_is_a_strings(&ContentHashMother::random());
        schema_is_a_strings(&RedactedMother::url());
        schema_is_a_strings(&OriginMother::random());
        schema_is_a_strings(&ArtifactNameMother::random());
        schema_is_a_strings(&RevisionMother::random());
        schema_is_a_strings(&crate::value::SourceName::derived(
            &crate::mother::UrlMother::https(),
        ));
    }
}

#[test]
fn a_project_ids_schema_holds_every_new_id_and_refuses_every_text_that_is_none() {
    use crate::mother::ProjectIdMother;

    let schema = given::<ProjectId>();
    for _ in 0..100 {
        let id = ProjectIdMother::random();
        assert_eq!(
            holds(&schema, &serde_json::json!(id.as_str())),
            Ok(()),
            "{id}"
        );
    }
    for (why, text) in ProjectIdMother::not_ids() {
        assert!(
            holds(&schema, &serde_json::json!(text)).is_err(),
            "{why}: {text:?}"
        );
    }
}
