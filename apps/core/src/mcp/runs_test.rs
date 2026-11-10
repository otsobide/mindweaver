use super::*;

fn program(command: &str, args: &[&str]) -> Runs {
    Runs::Program {
        command: command.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

#[test]
fn a_program_is_read_off_its_command_and_its_arguments() {
    let entry = Value::object([
        ("type", Value::from("stdio")),
        ("command", Value::from("npx")),
        ("args", Value::array(["-y", "${EXAMPLE_WORKSPACE}"])),
        ("env", Value::object([("KEY", Value::from("${KEY}"))])),
    ]);

    assert_eq!(
        Runs::of(&entry),
        Some(program("npx", &["-y", "${EXAMPLE_WORKSPACE}"]))
    );
}

#[test]
fn a_program_with_no_arguments_written_is_read_as_one_with_none() {
    let entry = Value::object([("command", Value::from("server"))]);

    assert_eq!(Runs::of(&entry), Some(program("server", &[])));
}

#[test]
fn a_program_written_as_one_line_is_read_off_its_first_word_and_the_rest() {
    let entry = Value::object([
        ("type", Value::from("local")),
        (
            "command",
            Value::array(["npx", "-y", "{env:EXAMPLE_WORKSPACE}"]),
        ),
    ]);

    assert_eq!(
        Runs::of_line(&entry),
        Some(program("npx", &["-y", "{env:EXAMPLE_WORKSPACE}"]))
    );
    assert_eq!(
        Runs::of_line(&Value::object([(
            "command",
            Value::array(Vec::<&str>::new())
        )])),
        None
    );
}

#[test]
fn an_address_is_read_off_its_url_in_either_shape() {
    let entry = Value::object([
        ("type", Value::from("http")),
        ("url", Value::from("https://${TENANT}.example.com/mcp")),
        ("headers", Value::object(Vec::<(&str, Value)>::new())),
    ]);
    let address = Runs::Address {
        url: String::from("https://${TENANT}.example.com/mcp"),
    };

    assert_eq!(Runs::of(&entry), Some(address.clone()));
    assert_eq!(Runs::of_line(&entry), Some(address));
}

#[test]
fn an_entry_holding_neither_is_read_as_nothing() {
    let entry = Value::object([("type", Value::from("stdio"))]);
    let numbered = Value::object([
        ("command", Value::from("npx")),
        ("args", Value::array([Value::Integer(1)])),
    ]);

    assert_eq!(Runs::of(&entry), None);
    assert_eq!(Runs::of(&numbered), None);
    assert_eq!(Runs::of_line(&Value::from("npx")), None);
}

#[test]
fn a_program_is_said_as_the_shell_line_that_runs_it() {
    let said = program(
        "npx",
        &[
            "-y",
            "@example/server",
            "--workspace",
            "${EXAMPLE_WORKSPACE}",
            "{env:EXAMPLE_KEY}",
        ],
    )
    .to_string();

    assert_eq!(
        said,
        "npx -y @example/server --workspace ${EXAMPLE_WORKSPACE} {env:EXAMPLE_KEY}"
    );
}

#[test]
fn a_word_a_shell_would_split_or_read_otherwise_is_quoted() {
    let said = program(
        "/opt/my server/run",
        &[
            "two words",
            "",
            "it's",
            "{\"debug\":true}",
            "a;b",
            "$(x)",
            "$HOME",
        ],
    )
    .to_string();

    assert_eq!(
        said,
        r#"'/opt/my server/run' 'two words' '' 'it'\''s' '{"debug":true}' 'a;b' '$(x)' '$HOME'"#
    );
}

/// Braces a shell expands into words and a `~` it expands into a home are
/// quoted, as what the agent is given is the text: only braces around a
/// reference stay bare.
#[test]
fn a_word_a_shell_would_expand_is_quoted_and_a_reference_is_not() {
    let said = program(
        "server",
        &[
            "{a,b}",
            "{}",
            "~/notes",
            "KEY=~/x",
            "a:~/y",
            "a~b",
            "--token=${EXAMPLE_TOKEN}",
            "{env:EXAMPLE_KEY}/x",
            "${}",
            "${EXAMPLE",
        ],
    )
    .to_string();

    assert_eq!(
        said,
        "server '{a,b}' '{}' '~/notes' 'KEY=~/x' 'a:~/y' a~b --token=${EXAMPLE_TOKEN} \
         {env:EXAMPLE_KEY}/x '${}' '${EXAMPLE'"
    );
}

#[test]
fn an_address_is_said_as_it_is() {
    let said = Runs::Address {
        url: String::from("https://example.com/mcp?a=1&b=2"),
    }
    .to_string();

    assert_eq!(said, "https://example.com/mcp?a=1&b=2");
}

/// What a definition no agent holds yet starts or reaches, as it writes
/// it: each input between its braces, which a shell line shows quoted,
/// since an agent fills it in only once its own entry spells it.
#[test]
fn a_definition_s_transport_is_what_it_writes_each_input_between_braces() {
    use crate::mcp::McpDefinition;
    use crate::mother::McpMother;

    let stdio = McpDefinition::parse(&McpMother::stdio("acme")).unwrap();
    let streamed = McpDefinition::parse(&McpMother::http("acme")).unwrap();

    assert_eq!(
        Runs::of_transport(&stdio.transport).to_string(),
        "npx -y @example/server --workspace '{workspace}'"
    );
    let Transport::Http(endpoint) = &streamed.transport else {
        panic!("{:?}", streamed.transport);
    };
    assert_eq!(
        Runs::of_transport(&streamed.transport),
        Runs::Address {
            url: endpoint.url.to_string()
        }
    );
}
