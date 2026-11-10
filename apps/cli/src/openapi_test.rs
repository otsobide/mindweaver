//! Unit tests for `openapi.rs`: the export of the API's OpenAPI document,
//! and how it names what each route takes and gives.

use std::fs;

use super::*;

/// Where the document is written: `docs/api/openapi.json` in the
/// repository, whichever folder cargo was started in, beside the schemas.
const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api/openapi.json");

/// The export of the document, which `make api/openapi` runs: laid out two
/// spaces a level, its keys in order, and ended as a text file is, so a
/// change to it reads as a diff of the lines it changed.
#[test]
fn export_openapi() {
    let mut text = serde_json::to_string_pretty(&document()).unwrap();
    text.push('\n');

    fs::write(FILE, text).unwrap();
}

/// Every `$ref` in `value`, at the place it is.
fn references(value: &Value, at: &str, found: &mut Vec<(String, String)>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                match (key.as_str(), value) {
                    ("$ref", Value::String(reference)) => {
                        found.push((at.to_owned(), reference.clone()));
                    }
                    _ => references(value, &format!("{at}/{key}"), found),
                }
            }
        }
        Value::Array(list) => {
            for (index, value) in list.iter().enumerate() {
                references(value, &format!("{at}/{index}"), found);
            }
        }
        _ => {}
    }
}

/// Every definition the document holds, and every reference in it, each
/// to one of them.
fn definitions_and_references() -> (Value, Vec<(String, String)>) {
    let document = document();
    let mut found = Vec::new();
    references(&document, "", &mut found);
    (document, found)
}

#[test]
fn every_reference_names_a_definition_the_document_holds() {
    let (document, references) = definitions_and_references();
    let schemas = &document["components"]["schemas"];

    for (at, reference) in references {
        let name = reference
            .strip_prefix("#/components/schemas/")
            .unwrap_or_else(|| panic!("{at}: {reference} names no definition"));
        assert!(schemas.get(name).is_some(), "{at}: {reference}");
    }
}

/// None held for no route: each is named by a route, or by a definition
/// one names.
#[test]
fn every_definition_is_named_by_a_reference() {
    let (document, references) = definitions_and_references();
    let named: BTreeSet<&str> = references
        .iter()
        .filter_map(|(_, reference)| reference.strip_prefix("#/components/schemas/"))
        .collect();

    let unnamed: Vec<&String> = document["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|name| !named.contains(name.as_str()))
        .collect();
    assert_eq!(unnamed, Vec::<&String>::new());
}

/// The schema at `pointer`, a JSON pointer into the document.
fn at<'a>(document: &'a Value, pointer: &str) -> &'a Value {
    document
        .pointer(pointer)
        .unwrap_or_else(|| panic!("nothing at {pointer}"))
}

/// A task is sent closed, a field serde reads as none left out of
/// `required`, and answered in a plan open, every field there: two
/// definitions, the one sent named apart, each where its route goes. A
/// word, described alike both ways, is one.
#[test]
fn a_type_sent_and_answered_otherwise_is_two_definitions_and_one_alike_is_one() {
    let document = document();
    let schemas = &document["components"]["schemas"];

    assert_ne!(schemas["Task"], schemas["TaskSent"]);
    assert_eq!(
        at(
            &document,
            "/paths/~1tasks/post/requestBody/content/application~1json/schema"
        ),
        &json!({"$ref": "#/components/schemas/TaskSent"})
    );
    assert_eq!(
        at(
            &document,
            "/components/schemas/Planned/properties/tasks/items"
        ),
        &json!({"$ref": "#/components/schemas/Task"})
    );
    assert!(schemas.get("Kind").is_some());
    assert!(schemas.get("KindSent").is_none());
    let sent = serde_json::to_string(&schemas["TaskSent"]).unwrap();
    let answered = serde_json::to_string(&schemas["Task"]).unwrap();
    assert!(sent.contains("\"additionalProperties\":false"), "{sent}");
    assert!(
        !answered.contains("\"additionalProperties\":false"),
        "{answered}"
    );
    assert!(!answered.contains(&format!("#{PENDING}/")));
}

/// Each place the server carries the backend's JSON is said of the CLI's
/// type for it, on the route that answers with it, beside the server's own.
#[test]
fn what_the_server_carries_is_the_cli_s_type_on_each_route_that_answers_it() {
    let document = document();
    let answer = |path: &str, method: &str| {
        let path = path.replace('/', "~1");
        at(
            &document,
            &format!("/paths/{path}/{method}/responses/200/content/application~1json/schema"),
        )
        .clone()
    };

    assert_eq!(
        answer("/tasks/preview", "post")["allOf"][1]["properties"]["confirm"]["items"],
        json!({"$ref": "#/components/schemas/Confirm"})
    );
    assert_eq!(
        answer("/tasks", "post")["allOf"][1]["properties"]["outcome"]["anyOf"][0],
        json!({"$ref": "#/components/schemas/Outcome"})
    );
    let switched = serde_json::to_string(&answer("/switch", "post")).unwrap();
    assert!(
        switched.contains("#/components/schemas/Switchable"),
        "{switched}"
    );
    assert!(switched.contains("\"null\""), "{switched}");
    assert_eq!(
        at(
            &document,
            "/paths/~1events/get/responses/200/content/text~1event-stream/x-itemSchema/allOf/1"
        ),
        &json!({"properties": {
            "scopes": {"items": {"$ref": "#/components/schemas/Holder"}},
            "progress": {"$ref": "#/components/schemas/Step"},
        }})
    );
    assert_eq!(
        at(
            &document,
            "/paths/~1tasks~1cancel/post/requestBody/content/application~1json/schema"
        ),
        &json!({"$ref": "#/components/schemas/Cancel"})
    );
    assert_eq!(
        answer("/tasks/cancel", "post"),
        json!({"$ref": "#/components/schemas/Cancelling"})
    );
}

/// What the CLI's backend refuses with: core's codes and its own, as a
/// 404 only for a read named by its parameters.
#[test]
fn a_read_named_by_its_parameters_alone_may_be_refused_as_absent() {
    let document = document();
    let backend = json!({"anyOf": [
        {"$ref": "#/components/schemas/CoreCode"},
        {"$ref": "#/components/schemas/CliCode"},
    ]});

    for route in PanelBackend::routes() {
        let method = route.method.as_str().to_ascii_lowercase();
        let path = route.path.replace('/', "~1");
        let responses = at(&document, &format!("/paths/{path}/{method}/responses"));
        let absent = route.method == Method::Get && !route.parameters.is_empty();
        assert_eq!(responses.get("404").is_some(), absent, "{}", route.path);
        let unfit = &responses["422"]["content"]["application/json"]["schema"]["allOf"][1]
            ["properties"]["code"];
        let backend_codes = match route.method {
            Method::Get => unfit,
            Method::Post => &unfit["anyOf"][1],
        };
        assert_eq!(backend_codes, &backend, "{}", route.path);
    }
}

/// The document says the version its routes are under.
#[test]
fn the_document_names_where_the_api_is_served_and_its_version() {
    let document = document();

    assert_eq!(document["openapi"], OPENAPI);
    assert_eq!(document["servers"][0]["url"], API);
    assert_eq!(document["info"]["version"], "1");
}
