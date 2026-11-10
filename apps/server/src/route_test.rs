use serde_json::json;

use super::*;

/// A route taking one of each shape, `reference` the one it needs.
fn taken() -> Vec<Parameter> {
    vec![
        Parameter::required("reference", Shape::Text, "what to read"),
        Parameter::optional("depth", Shape::Number, "how deep"),
        Parameter::optional("hidden", Shape::Flag, "whether hidden ones too"),
        Parameter::optional("root", Shape::List, "where to look"),
    ]
}

fn read(query: &str) -> Result<Value, Unfit> {
    arguments(&taken(), Some(query)).map(Value::Object)
}

#[test]
fn each_parameter_is_read_as_its_shape_says() {
    assert_eq!(
        read("reference=skill%3Adeploy&depth=3&hidden=true&root=%2Fa&root=/b+c").unwrap(),
        json!({
            "reference": "skill:deploy",
            "depth": 3,
            "hidden": true,
            "root": ["/a", "/b c"],
        })
    );
    assert_eq!(
        read("hidden=false&reference=").unwrap(),
        json!({ "reference": "", "hidden": false })
    );
}

#[test]
fn what_is_left_out_is_absent_and_no_query_is_no_parameter() {
    assert_eq!(read("reference=x").unwrap(), json!({ "reference": "x" }));
    assert_eq!(arguments(&[], None), Ok(Map::new()));
    assert_eq!(arguments(&[], Some("")), Ok(Map::new()));
}

#[test]
fn a_parameter_the_route_does_not_take_is_refused_by_its_name() {
    for (query, name) in [
        ("reference=x&token=secret", "token"),
        ("Reference=x", "Reference"),
        ("reference=x&depth[]=1", "depth[]"),
        ("reference=x&=1", ""),
    ] {
        assert_eq!(
            read(query),
            Err(Unfit::Unknown {
                name: name.to_owned()
            }),
            "{query}"
        );
    }
}

#[test]
fn a_parameter_that_takes_one_value_given_twice_is_refused() {
    for (query, name) in [
        ("reference=a&reference=b", "reference"),
        ("reference=a&depth=1&depth=1", "depth"),
        ("reference=a&hidden=true&hidden=false", "hidden"),
    ] {
        assert_eq!(read(query), Err(Unfit::Twice { name }), "{query}");
    }
}

#[test]
fn a_value_its_shape_does_not_read_is_refused() {
    for (query, name, shape, value) in [
        ("reference=a&depth=-1", "depth", Shape::Number, "-1"),
        ("reference=a&depth=1.5", "depth", Shape::Number, "1.5"),
        ("reference=a&depth=", "depth", Shape::Number, ""),
        ("reference=a&hidden=yes", "hidden", Shape::Flag, "yes"),
        ("reference=a&hidden", "hidden", Shape::Flag, ""),
        ("reference=a&hidden=TRUE", "hidden", Shape::Flag, "TRUE"),
    ] {
        assert_eq!(
            read(query),
            Err(Unfit::Malformed {
                name,
                shape,
                value: value.to_owned()
            }),
            "{query}"
        );
    }
}

#[test]
fn a_parameter_the_route_needs_left_out_is_refused() {
    assert_eq!(read("depth=1"), Err(Unfit::Missing { name: "reference" }));
    let roots = [Parameter::required("root", Shape::List, "where")];
    assert_eq!(
        arguments(&roots, None),
        Err(Unfit::Missing { name: "root" })
    );
}

#[test]
fn each_refusal_says_the_route_and_the_parameter() {
    let path = "/api/v1/files";
    let said = |unfit: Unfit, parameters: &[Parameter]| unfit.say(path, parameters);
    let unknown = || Unfit::Unknown {
        name: "token".to_owned(),
    };

    assert_eq!(
        said(unknown(), &taken()),
        "/api/v1/files takes no parameter `token`, only `reference`, `depth`, `hidden` or `root`"
    );
    assert_eq!(
        said(unknown(), &taken()[..1]),
        "/api/v1/files takes no parameter `token`, only `reference`"
    );
    assert_eq!(
        said(unknown(), &[]),
        "/api/v1/files takes no parameter, and was sent `token`"
    );
    assert_eq!(
        said(Unfit::Twice { name: "depth" }, &taken()),
        "/api/v1/files takes one `depth`, and was sent more than one"
    );
    assert_eq!(
        said(
            Unfit::Malformed {
                name: "hidden",
                shape: Shape::Flag,
                value: "yes".to_owned()
            },
            &taken()
        ),
        "/api/v1/files takes `hidden` as `true` or `false`, not `yes`"
    );
    assert_eq!(
        said(Unfit::Missing { name: "reference" }, &taken()),
        "/api/v1/files needs `reference`, and was sent none"
    );
}

/// Each its own code, whatever the words say of it.
#[test]
fn each_refusal_is_a_condition_of_its_own() {
    let refused = [
        read("reference=a&what=1"),
        read("reference=a&reference=b"),
        read("reference=a&depth=deep"),
        read("depth=1"),
    ]
    .map(|read| read.unwrap_err().condition().code());

    assert_eq!(
        refused,
        [
            "unknown_parameter",
            "repeated_parameter",
            "malformed_parameter",
            "missing_parameter"
        ]
    );
}

/// A route answering nothing, under `path`: a POST with a body, as one
/// must have.
fn route(method: Method, path: &'static str) -> Route<()> {
    Route {
        method,
        path,
        about: "nothing",
        parameters: Vec::new(),
        body: (method == Method::Post).then_some("Nothing"),
        answers: "null",
        query: |_| Ok(()),
    }
}

#[test]
fn a_table_of_plain_paths_taken_once_can_be_served() {
    let routes = [
        route(Method::Get, "/snapshot"),
        route(Method::Get, "/library/files"),
        route(Method::Post, "/plan-2_a"),
    ];

    assert!(check(&routes).is_ok());
}

#[test]
fn a_table_the_router_would_misread_or_panic_on_is_refused() {
    let mut doubled = route(Method::Get, "/files");
    doubled.parameters = vec![
        Parameter::optional("project", Shape::Text, "whose"),
        Parameter::optional("project", Shape::List, "whose"),
    ];
    let mut bodied = route(Method::Get, "/plan");
    bodied.body = Some("Selection");
    let mut bodiless = route(Method::Post, "/plan");
    bodiless.body = None;
    let mut asked = route(Method::Post, "/plan");
    asked.parameters = vec![Parameter::optional("depth", Shape::Number, "how deep")];
    let tables = [
        (vec![route(Method::Get, "snapshot")], "no path"),
        (vec![route(Method::Get, "/")], "no path"),
        (vec![route(Method::Get, "/files/")], "no path"),
        (vec![route(Method::Get, "/a//b")], "no path"),
        (vec![route(Method::Get, "/{entry}")], "no path"),
        (vec![route(Method::Get, "/{*rest}")], "no path"),
        (vec![route(Method::Get, "/Snapshot")], "no path"),
        (
            vec![route(Method::Get, "/files"), route(Method::Post, "/files")],
            "twice",
        ),
        (vec![doubled], "`project` twice"),
        (vec![bodied], "takes no body"),
        (vec![bodiless], "in its body"),
        (vec![asked], "not in parameters"),
    ];

    for (routes, why) in tables {
        let path = routes.last().unwrap().path;
        let error = check(&routes).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{path}");
        assert!(error.to_string().contains(why), "{path}: {error}");
    }
}

/// The server's own routes, as a description of the API walks them, each
/// pinned by its method, path, body and answer: a contract, as a backend's
/// route is, so one changed fails here rather than reach a script written
/// against it unnoticed.
#[test]
fn the_servers_own_routes_are_pinned_by_what_a_description_says() {
    let pinned: Vec<_> = Own::ALL
        .iter()
        .map(|own| (own.method(), own.path(), own.body(), own.answers()))
        .collect();

    assert_eq!(
        pinned,
        [
            (
                Method::Post,
                "/tasks/preview",
                Some(Sent::Task),
                Answer::Preview
            ),
            (Method::Post, "/tasks", Some(Sent::Task), Answer::Ran),
            (
                Method::Post,
                "/tasks/cancel",
                Some(Sent::Cancel),
                Answer::Cancelling
            ),
            (
                Method::Post,
                "/switch",
                Some(Sent::Switch),
                Answer::Switched
            ),
            (Method::Get, "/transcript", None, Answer::Runs),
            (Method::Get, "/events", None, Answer::Events),
            (Method::Get, "/openapi.json", None, Answer::Document),
        ]
    );
}

/// None a backend's route may take, and each refused as the server's own,
/// whether or not it is a path a backend's route could have been.
#[test]
fn no_route_of_a_backends_may_take_a_path_the_server_answers_itself() {
    for own in Own::ALL {
        let error = check(&[route(own.method(), own.path())]).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "the route `{}` is one the server answers itself",
                own.path()
            )
        );
    }
}

/// The codes of what a request to a route may be refused for, as a
/// description lists them.
fn codes(conditions: Vec<Condition>) -> Vec<&'static str> {
    conditions.into_iter().map(Condition::code).collect()
}

/// What every route may be refused for, whatever it is: the guards, the
/// token, a parameter it does not take and another method.
const EVERY: [&str; 5] = [
    "token_required",
    "unknown_host",
    "foreign_origin",
    "unknown_parameter",
    "method_not_allowed",
];

/// A change sent by cookie, and a body read as JSON.
const POSTED: [&str; 8] = [
    "same_origin_required",
    "x_mindflayer_required",
    "json_required",
    "malformed_json",
    "invalid_body",
    "body_too_large",
    "body_unreadable",
    "body_refused",
];

/// Each code `listed` holds, in the order of `Condition::ALL`.
fn ordered(listed: &[&str]) -> Vec<&'static str> {
    Condition::CODES
        .iter()
        .copied()
        .filter(|code| listed.contains(code))
        .collect()
}

#[test]
fn a_read_by_its_parameters_may_be_refused_for_what_each_shape_and_need_allows() {
    let bare = route(Method::Get, "/snapshot");
    let mut each = route(Method::Get, "/files");
    each.parameters = taken();
    let mut lists = route(Method::Get, "/scan");
    lists.parameters = vec![Parameter::optional("root", Shape::List, "where to look")];

    let read = [&EVERY[..], &["invalid_parameters", "backend_failed"]].concat();
    assert_eq!(codes(bare.conditions()), ordered(&read));
    assert_eq!(codes(lists.conditions()), ordered(&read));
    let checked = [
        &read[..],
        &[
            "repeated_parameter",
            "malformed_parameter",
            "missing_parameter",
        ],
    ]
    .concat();
    assert_eq!(codes(each.conditions()), ordered(&checked));
}

#[test]
fn a_read_by_its_body_may_be_refused_for_the_body_and_never_a_parameter_it_reads() {
    let posted = route(Method::Post, "/plan");

    let listed = [&EVERY[..], &POSTED, &["backend_failed"]].concat();
    assert_eq!(codes(posted.conditions()), ordered(&listed));
}

/// Pinned, as their table is: what each of the server's own routes hands
/// a request on to, and so what may fail under it.
#[test]
fn each_of_the_servers_own_routes_may_be_refused_for_what_it_reaches() {
    let task = [&EVERY[..], &POSTED].concat();
    let preview = [&task[..], &["backend_failed"]].concat();
    let run = [&task[..], &["backend_failed", "writer_stopped"]].concat();
    let cancel = [
        &task[..],
        &["not_running", "uncancellable", "backend_failed"],
    ]
    .concat();

    for (own, listed) in [
        (Own::Preview, ordered(&preview)),
        (Own::Run, ordered(&run)),
        (Own::Cancel, ordered(&cancel)),
        (Own::Switch, ordered(&run)),
        (Own::Transcript, ordered(&EVERY)),
        (Own::Events, ordered(&EVERY)),
        (Own::Document, ordered(&EVERY)),
    ] {
        assert_eq!(codes(own.conditions()), listed, "{own:?}");
    }
}

/// A path no route names is no route's: the 404 a description says of
/// the API as a whole, never among one route's answers.
#[test]
fn no_route_may_be_refused_as_a_route_that_is_not_there() {
    let mut each = route(Method::Get, "/files");
    each.parameters = taken();
    let listed = Own::ALL
        .iter()
        .flat_map(|own| own.conditions())
        .chain(each.conditions())
        .chain(route(Method::Post, "/plan").conditions());

    assert!(listed
        .into_iter()
        .all(|condition| condition != Condition::UnknownRoute));
}
