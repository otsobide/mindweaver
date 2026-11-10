//! Mothers of artifacts as their files hold them: a skill's `SKILL.md`, a
//! rule's markdown and an MCP server's definition.

use crate::agent::Agent;
use crate::mcp::{Address, Given, Program, Repeated, Unsayable};
use crate::mother::random;
use crate::mother::value::VariableNameMother;
use crate::value::{InputName, VariableName};

/// Makes the text of a skill's `SKILL.md`.
pub struct SkillMother;

impl SkillMother {
    /// A skill with a name and a description the Agent Skills format takes,
    /// and the text of its file.
    pub fn random() -> (String, String) {
        let name = random::kebab();
        let text = Self::create(&name, &Self::description());
        (name, text)
    }

    /// `name`'s file, saying it does `description`, front matter first.
    pub fn create(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
    }

    /// A description a person writes: a sentence of a few words.
    pub fn description() -> String {
        let words: Vec<String> = (0..random::between(2, 8)).map(|_| random::word()).collect();
        let sentence = words.join(" ");
        let mut characters = sentence.chars();
        let first = characters.next().map(|c| c.to_ascii_uppercase());
        format!(
            "{}{}",
            first.into_iter().collect::<String>(),
            characters.as_str()
        )
    }

    /// `name`'s file, declaring the licence `license`, written after the
    /// key as YAML takes it: `MIT`, or `""` for one written empty.
    pub fn licensed(name: &str, description: &str, license: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: {description}\nlicense: {license}\n---\n\n# \
             {name}\n\nSteps.\n"
        )
    }

    /// A licence as a skill's author names one: an SPDX name, or words
    /// pointing at a file it bundles, as Anthropic's own skills write.
    pub fn licence() -> String {
        random::pick(&[
            "MIT",
            "Apache-2.0",
            "MPL-2.0",
            "Proprietary. LICENSE.txt has complete terms",
            "Complete terms in LICENSE.txt",
        ])
        .to_owned()
    }

    /// `name`'s file with front matter that names no description.
    pub fn without_description(name: &str) -> String {
        format!("---\nname: {name}\n---\n\n# {name}\n")
    }

    /// A file with no front matter at all.
    pub fn without_front_matter(name: &str) -> String {
        format!("# {name}\n\nSteps.\n")
    }

    /// `name`'s file whose description is longer than the format allows.
    pub fn with_long_description(name: &str) -> String {
        Self::create(name, &"a".repeat(crate::skill::MAX_DESCRIPTION_LEN + 1))
    }
}

/// Makes the text of a rule.
pub struct RuleMother;

impl RuleMother {
    /// A rule with something to say and no front matter, as most are.
    pub fn random() -> String {
        Self::create(&random::word())
    }

    /// A rule about `topic`.
    pub fn create(topic: &str) -> String {
        format!("# {topic}\n\nAlways mind the {topic}.\n")
    }

    /// A rule about some files only: front matter naming `paths`.
    pub fn scoped(topic: &str, paths: &[&str]) -> String {
        let listed: String = paths
            .iter()
            .map(|path| format!("  - \"{path}\"\n"))
            .collect();
        format!("---\npaths:\n{listed}---\n\n{}", Self::create(topic))
    }
}

/// Makes the text of an MCP server's definition, a fixture for each
/// transport the format has.
///
/// Each holds what its transport can: inputs of every kind it takes, each
/// written where its kind says, with provenance or extras beside, so a
/// test of one key reads it among the rest. The name and the description
/// are drawn, or given; the rest is fixed, for a test to name what it reads
/// back.
pub struct McpMother;

impl McpMother {
    /// A definition of any of the three transports, under a name a server
    /// may be given ([`name`](Self::name)), and the text of its file.
    pub fn random() -> (String, String) {
        let name = Self::name();
        let text = match random::between(0, 2) {
            0 => Self::stdio(&name),
            1 => Self::http(&name),
            _ => Self::sse(&name),
        };
        (name, text)
    }

    /// A name every agent keys a server by: words of letters in either case
    /// and digits, joined by hyphens or underscores, starting with a letter,
    /// so that it has a case to turn, and ending in a digit, so that it is
    /// never one an agent keeps for itself, none of which holds one.
    pub fn name() -> String {
        let letters = format!(
            "{}{}",
            random::LOWERCASE,
            random::LOWERCASE.to_ascii_uppercase()
        );
        let alphabet = format!("{letters}{}", random::DIGITS);
        let mut name = random::drawn_from(&letters, 1);
        name.push_str(&random::drawn_from(&alphabet, random::between(0, 7)));
        for _ in 0..random::between(0, 2) {
            name.push(random::pick(&['-', '_']));
            name.push_str(&random::drawn_from(&alphabet, random::between(1, 8)));
        }
        name.push_str(&random::drawn_from(random::DIGITS, 1));
        name
    }

    /// Names no agent keys a server by, each beside why: what `validate`
    /// says is no key.
    pub fn unkeyed() -> Vec<(&'static str, String)> {
        let word = random::word();
        vec![
            ("a dot", format!("{word}.{word}")),
            ("a space", format!("{word} {word}")),
            ("a colon", format!("{word}:{word}")),
            ("an at sign", format!("@{word}")),
            ("a plus", format!("{word}+{word}")),
            ("a brace", format!("{{{word}}}")),
            ("a letter past ASCII", format!("{word}\u{e9}")),
        ]
    }

    /// Names no MCP server may be written under, each beside why: what a
    /// server's name is held to where every name is (empty, or past the
    /// limit), each that is no key ([`unkeyed`](Self::unkeyed)), and one an
    /// agent keeps for itself, `reserved`.
    pub fn unwritable() -> Vec<(&'static str, String)> {
        let mut names = vec![
            ("empty", String::new()),
            (
                "a segment over the limit",
                "a".repeat(crate::skill::MAX_NAME_SEGMENT_LEN + 1),
            ),
        ];
        names.extend(Self::unkeyed());
        let (reserved, _, _) = random::pick(&Self::reserved());
        names.push(("reserved", reserved));
        names
    }

    /// Every name an agent keeps for itself, beside the agent and the name
    /// as it spells it, each in a case drawn afresh: and for the two of
    /// Claude's whose space no key can hold, each spelling of it one can,
    /// with an underscore, a hyphen or nothing in its place.
    pub fn reserved() -> Vec<(String, Agent, &'static str)> {
        let mut names = Vec::new();
        for word in ["workspace", "claude-in-chrome", "computer-use"] {
            names.push((Self::recased(word), Agent::Claude, word));
        }
        for word in ["Claude Preview", "Claude Browser"] {
            for space in ["_", "-", ""] {
                names.push((
                    Self::recased(&word.replace(' ', space)),
                    Agent::Claude,
                    word,
                ));
            }
        }
        for word in ["servers", "timeout"] {
            names.push((Self::recased(word), Agent::OpenCode, word));
        }
        names
    }

    /// Every name an agent uses for one of its own without refusing a
    /// server's, beside the agent and the name as it spells it, in a case
    /// drawn afresh.
    pub fn claimed() -> Vec<(String, Agent, &'static str)> {
        vec![(
            Self::recased("anthropic-skills"),
            Agent::Claude,
            "anthropic-skills",
        )]
    }

    /// Names that are near one an agent keeps for itself and are none,
    /// each beside why.
    pub fn near_reserved() -> Vec<(&'static str, String)> {
        vec![
            ("more after it", String::from("workspaces")),
            ("less of it", String::from("claude")),
            ("a digit after it", String::from("claude-preview-2")),
            (
                "a hyphen written as an underscore in a name with no space",
                String::from("claude_in_chrome"),
            ),
            (
                "two separators for one space",
                String::from("claude__preview"),
            ),
            ("another word", String::from("claude-desktop")),
        ]
    }

    /// `name` in another case, its first letter's turned: the name a
    /// definition beside `name`'s clashes with. `name` holds a letter.
    pub fn twin(name: &str) -> String {
        let at = name
            .find(|c: char| c.is_ascii_alphabetic())
            .unwrap_or_else(|| panic!("{name} holds no letter to turn"));
        let mut twin = name.to_owned();
        let letter = twin.remove(at);
        let turned = if letter.is_ascii_uppercase() {
            letter.to_ascii_lowercase()
        } else {
            letter.to_ascii_uppercase()
        };
        twin.insert(at, turned);
        twin
    }

    /// `word`, each of its letters in a case drawn afresh.
    fn recased(word: &str) -> String {
        word.chars()
            .map(|c| {
                if random::chance() {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect()
    }

    /// `name`, saying it is for `description`, reached over `transport`
    /// (`stdio`, `http` or `sse`) as [`stdio`](Self::stdio),
    /// [`http`](Self::http) and [`sse`](Self::sse) reach it: for a fixture
    /// that has to read the same every run.
    pub fn create(name: &str, description: &str, transport: &str) -> String {
        let head = Self::head(name, description);
        match transport {
            "stdio" => Self::program(&head),
            "http" => Self::streamed(&head),
            "sse" => Self::evented(&head),
            other => panic!("{other} is no transport the format has"),
        }
    }

    /// A program the agent starts: an environment holding a secret, under a
    /// name of its own rather than its variable's, and a setting, an
    /// argument somebody supplies, and extras for Claude.
    pub fn stdio(name: &str) -> String {
        Self::program(&Self::head(name, &SkillMother::description()))
    }

    /// A server at an address with a part of it supplied, reached over
    /// streamable HTTP with a secret header, as a registry offered it.
    pub fn http(name: &str) -> String {
        Self::streamed(&Self::head(name, &SkillMother::description()))
    }

    /// A server at an address, reached over server-sent events with a
    /// secret header.
    pub fn sse(name: &str) -> String {
        Self::evented(&Self::head(name, &SkillMother::description()))
    }

    fn program(head: &str) -> String {
        format!(
            "{head}\n\
             [transport]\n\
             type = \"stdio\"\n\
             command = \"npx\"\n\
             args = [\"-y\", \"@example/server\", \"--workspace\", \"{{workspace}}\"]\n\
             env = {{ API_KEY = \"{{api_key}}\", LOG_LEVEL = \"info\" }}\n\
             cwd = \".\"\n\
             \n\
             [[inputs]]\n\
             name = \"api_key\"\n\
             kind = \"env\"\n\
             description = \"The key the server signs in with\"\n\
             required = true\n\
             secret = true\n\
             env_var = \"EXAMPLE_API_KEY\"\n\
             \n\
             [[inputs]]\n\
             name = \"workspace\"\n\
             kind = \"arg\"\n\
             env_var = \"EXAMPLE_WORKSPACE\"\n\
             \n\
             [extras.claude]\n\
             timeout = 30000\n",
        )
    }

    fn streamed(head: &str) -> String {
        format!(
            "{head}\n\
             [transport]\n\
             type = \"http\"\n\
             url = \"https://{{tenant}}.example.com/mcp\"\n\
             headers = {{ Authorization = \"Bearer {{token}}\" }}\n\
             \n\
             [[inputs]]\n\
             name = \"tenant\"\n\
             kind = \"url_var\"\n\
             required = true\n\
             env_var = \"EXAMPLE_TENANT\"\n\
             \n\
             [[inputs]]\n\
             name = \"token\"\n\
             kind = \"header\"\n\
             required = true\n\
             secret = true\n\
             env_var = \"EXAMPLE_TOKEN\"\n\
             \n\
             [provenance]\n\
             registry = \"https://registry.modelcontextprotocol.io\"\n\
             upstream_name = \"com.example/server\"\n\
             version = \"1.2.0\"\n\
             fetched_at = 2026-11-04T09:30:00Z\n",
        )
    }

    fn evented(head: &str) -> String {
        format!(
            "{head}\n\
             [transport]\n\
             type = \"sse\"\n\
             url = \"https://example.com/sse\"\n\
             headers = {{ X-Api-Key = \"{{key}}\" }}\n\
             \n\
             [[inputs]]\n\
             name = \"key\"\n\
             kind = \"header\"\n\
             secret = true\n\
             env_var = \"EXAMPLE_KEY\"\n",
        )
    }

    /// The program of [`stdio`](Self::stdio) with no folder to start in:
    /// one Claude and OpenCode are given whole, and Codex, which passes a
    /// variable on only under its own name, none of.
    pub fn without_cwd(name: &str) -> String {
        Self::stdio(name).replacen("cwd = \".\"\n", "", 1)
    }

    /// The program of [`whole_secret_env`](Self::whole_secret_env), its
    /// argument a secret: what Codex, with no interpolation, cannot be
    /// given, and nothing else of the program.
    pub fn secret_argument(name: &str) -> String {
        Self::whole_secret_env(name).replacen(
            "kind = \"arg\"\n",
            "kind = \"arg\"\nsecret = true\n",
            1,
        )
    }

    /// The program of [`stdio`](Self::stdio), its secret the whole value of
    /// an environment variable named as the one it is read from: what Codex
    /// passes on to a program (`env_vars`).
    pub fn whole_secret_env(name: &str) -> String {
        Self::stdio(name).replacen(
            "API_KEY = \"{api_key}\"",
            "EXAMPLE_API_KEY = \"{api_key}\"",
            1,
        )
    }

    /// The program of [`whole_secret_env`](Self::whole_secret_env), its
    /// secret written beside other text: what Codex passes on no variable
    /// for.
    pub fn composite_secret_env(name: &str) -> String {
        Self::whole_secret_env(name).replacen("\"{api_key}\"", "\"key={api_key}\"", 1)
    }

    /// The program of [`whole_secret_env`](Self::whole_secret_env), its
    /// argument written as it is rather than supplied, with extras for
    /// Codex beside Claude's: a program every input of which Codex reads
    /// from its variable, and is given whole.
    pub fn program_for_codex(name: &str) -> String {
        Self::whole_secret_env(name)
            .replacen("\"{workspace}\"]", "\".\"]", 1)
            .replacen(
                "[[inputs]]\nname = \"workspace\"\nkind = \"arg\"\nenv_var = \"EXAMPLE_WORKSPACE\"\n\n\
                 [extras.claude]\n",
                "[extras.codex]\nstartup_timeout_sec = 20\n\n[extras.claude]\n",
                1,
            )
    }

    /// The program of [`program_for_codex`](Self::program_for_codex) with
    /// no input, every value written as it is: what every agent is given as
    /// the definition holds it.
    pub fn plain_program(name: &str) -> String {
        Self::program_for_codex(name)
            .replacen("EXAMPLE_API_KEY = \"{api_key}\", ", "", 1)
            .replacen(
                "[[inputs]]\nname = \"api_key\"\nkind = \"env\"\n\
                 description = \"The key the server signs in with\"\nrequired = true\n\
                 secret = true\nenv_var = \"EXAMPLE_API_KEY\"\n\n",
                "",
                1,
            )
    }

    /// The program of [`program_for_codex`](Self::program_for_codex), its
    /// extras for Codex `extras`, the lines of a TOML table, in place of
    /// the timeout it holds.
    pub fn with_codex_extras(name: &str, extras: &str) -> String {
        Self::program_for_codex(name).replacen("startup_timeout_sec = 20\n", extras, 1)
    }

    /// The program of [`whole_secret_env`](Self::whole_secret_env), its
    /// argument no secret and read from no variable: a value somebody
    /// gives, which a definition may leave unsaid and no entry can refer
    /// to.
    pub fn unread_input(name: &str) -> String {
        Self::whole_secret_env(name).replacen("env_var = \"EXAMPLE_WORKSPACE\"\n", "", 1)
    }

    /// The program of [`whole_secret_env`](Self::whole_secret_env) with no
    /// folder to start in, its extras for Claude `extras`, the lines of a
    /// TOML table, in place of the timeout it holds.
    pub fn with_claude_extras(name: &str, extras: &str) -> String {
        Self::whole_without_cwd(name).replacen("timeout = 30000\n", extras, 1)
    }

    /// The program of [`stdio`](Self::stdio), its extras for OpenCode
    /// `extras`, the lines of a TOML table, beside Claude's: a program
    /// OpenCode is given whole but for what `extras` gives.
    pub fn with_opencode_extras(name: &str, extras: &str) -> String {
        format!("{}\n[extras.opencode]\n{extras}", Self::stdio(name))
    }

    /// The program of [`whole_secret_env`](Self::whole_secret_env) with no
    /// folder to start in: one every agent is given whole, for a shape to
    /// be written into that is the only thing an agent cannot be given.
    fn whole_without_cwd(name: &str) -> String {
        Self::whole_secret_env(name).replacen("cwd = \".\"\n", "", 1)
    }

    /// The address of [`http`](Self::http), its supplied part a secret:
    /// what Codex cannot be given inside a URL.
    pub fn secret_in_url(name: &str) -> String {
        Self::http(name).replacen(
            "kind = \"url_var\"\n",
            "kind = \"url_var\"\nsecret = true\n",
            1,
        )
    }

    /// The address of [`http`](Self::http), its secret header written
    /// beside other text, under a scheme other than `Bearer`: what Codex
    /// reads from no variable.
    pub fn composite_secret_header(name: &str) -> String {
        Self::http(name).replacen(
            "Authorization = \"Bearer {token}\"",
            "Authorization = \"Token {token}\"",
            1,
        )
    }

    /// The address of [`http`](Self::http), its secret header the whole
    /// value of a header of its own: what Codex reads from a variable as it
    /// reads `Authorization: Bearer`.
    pub fn whole_secret_header(name: &str) -> String {
        Self::http(name).replacen(
            "Authorization = \"Bearer {token}\"",
            "X-Api-Key = \"{token}\"",
            1,
        )
    }

    /// The address of [`http`](Self::http), written whole rather than in
    /// part supplied, a header of its own written as it is beside its
    /// secret one: an address every input of which Codex reads from its
    /// variable, and is given whole.
    pub fn address_for_codex(name: &str) -> String {
        Self::http(name)
            .replacen(
                "https://{tenant}.example.com/mcp",
                "https://example.com/mcp",
                1,
            )
            .replacen(
                "Authorization = \"Bearer {token}\"",
                "Authorization = \"Bearer {token}\", X-Client = \"mindweaver\"",
                1,
            )
            .replacen(
                "[[inputs]]\nname = \"tenant\"\nkind = \"url_var\"\nrequired = true\n\
                 env_var = \"EXAMPLE_TENANT\"\n\n",
                "",
                1,
            )
    }

    /// The address of [`address_for_codex`](Self::address_for_codex) with
    /// no input, its one header written as it is: what every agent is given
    /// as the definition holds it.
    pub fn plain_address(name: &str) -> String {
        Self::address_for_codex(name)
            .replacen("Authorization = \"Bearer {token}\", ", "", 1)
            .replacen(
                "[[inputs]]\nname = \"token\"\nkind = \"header\"\nrequired = true\n\
                 secret = true\nenv_var = \"EXAMPLE_TOKEN\"\n\n",
                "",
                1,
            )
    }

    /// The address of [`address_for_codex`](Self::address_for_codex), its
    /// secret header the whole value of a header of its own rather than a
    /// bearer token.
    pub fn whole_header_for_codex(name: &str) -> String {
        Self::address_for_codex(name).replacen(
            "Authorization = \"Bearer {token}\"",
            "X-Api-Key = \"{token}\"",
            1,
        )
    }

    /// The address of [`http`](Self::http), its secret header read from
    /// `variable`.
    pub fn header_read_from(name: &str, variable: &str) -> String {
        Self::http(name).replacen(
            "env_var = \"EXAMPLE_TOKEN\"",
            &format!("env_var = {}", toml::Value::String(variable.to_owned())),
            1,
        )
    }

    /// The address of [`http`](Self::http), its supplied part read from
    /// `variable`.
    pub fn url_read_from(name: &str, variable: &str) -> String {
        Self::http(name).replacen(
            "env_var = \"EXAMPLE_TENANT\"",
            &format!("env_var = {}", toml::Value::String(variable.to_owned())),
            1,
        )
    }

    /// The program of [`stdio`](Self::stdio), its secret environment read
    /// from `variable`.
    pub fn env_read_from(name: &str, variable: &str) -> String {
        Self::stdio(name).replacen(
            "env_var = \"EXAMPLE_API_KEY\"",
            &format!("env_var = {}", toml::Value::String(variable.to_owned())),
            1,
        )
    }

    /// Variables Claude Code reads as empty in a remote server's `url` and
    /// `headers`, each beside why, in a case drawn afresh: one of each list
    /// and each shape it keeps from one.
    pub fn blanked_variables() -> Vec<(&'static str, String)> {
        let word = random::word().to_ascii_uppercase();
        let digits = random::drawn_from(random::DIGITS, random::between(1, 3));
        let mut variables = vec![
            ("Claude's own key", String::from("ANTHROPIC_API_KEY")),
            ("a package registry's token", String::from("NPM_TOKEN")),
            ("a review service's token", String::from("SONAR_TOKEN")),
            (
                "a proxy a tool is sent through",
                String::from("https_proxy"),
            ),
            (
                "a registry's token under a workflow's input",
                String::from("INPUT_NPM_TOKEN"),
            ),
            (
                "a background session's, kept only as it is",
                String::from("CLAUDE_BG_RV_AUTH"),
            ),
            (
                "telemetry's settings",
                String::from("OTEL_EXPORTER_OTLP_HEADERS"),
            ),
            ("git's configuration", format!("GIT_CONFIG_VALUE_{digits}")),
            (
                "git's configuration whole",
                String::from("GIT_CONFIG_PARAMETERS"),
            ),
            (
                "a Cargo registry's token",
                format!("CARGO_REGISTRIES_{word}_TOKEN"),
            ),
            (
                "a Cargo registry's token under a workflow's input",
                format!("INPUT_CARGO_REGISTRIES_{word}_TOKEN"),
            ),
        ];
        for (_, variable) in &mut variables {
            *variable = Self::recased(variable);
        }
        variables
    }

    /// Variables Claude Code passes on to a remote server, each beside
    /// why: each near one it reads as empty, and none of them.
    pub fn passed_variables() -> Vec<(&'static str, String)> {
        let word = random::word().to_ascii_uppercase();
        vec![
            ("GitHub's token", String::from("GITHUB_TOKEN")),
            ("Hugging Face's token", String::from("HF_TOKEN")),
            ("a server's own", String::from("EXAMPLE_TOKEN")),
            (
                "a background session's under a workflow's input",
                String::from("INPUT_CLAUDE_BG_RV_AUTH"),
            ),
            (
                "telemetry's prefix inside a word",
                format!("{word}_OTEL_KEY"),
            ),
            (
                "git's count of its settings",
                String::from("GIT_CONFIG_COUNT"),
            ),
            (
                "git's setting with no number",
                String::from("GIT_CONFIG_KEY_"),
            ),
            (
                "a Cargo registry's with no name",
                String::from("CARGO_REGISTRIES__TOKEN"),
            ),
            ("more after a name", String::from("NPM_TOKEN_FILE")),
        ]
    }

    /// Texts Claude Code reads a reference to a variable in, each beside
    /// why, the variable's name drawn afresh: what a definition means as
    /// it is written, and Claude would expand.
    pub fn references() -> Vec<(&'static str, String)> {
        let variable = random::word().to_ascii_uppercase();
        vec![
            ("a variable whole", format!("${{{variable}}}")),
            ("inside a longer value", format!("--home=${{{variable}}}/x")),
            (
                "with a default",
                format!("${{{variable}:-{}}}", random::word()),
            ),
            ("with an empty default", format!("${{{variable}:-}}")),
            (
                "closed by the reference written for an input",
                String::from("${a:-{workspace}}"),
            ),
        ]
    }

    /// Texts that open no reference Claude reads as they are, each beside
    /// why, written in a value of the program
    /// [`with_text`](Self::with_text) writes them in, where its
    /// placeholders are written as references, its `args` or its `env`:
    /// one stands beside
    /// the placeholder of its input `workspace`.
    pub fn not_references() -> Vec<(&'static str, String)> {
        vec![
            ("a variable as a shell writes it", String::from("$HOME/x")),
            (
                "a dollar before a placeholder",
                String::from("${workspace}"),
            ),
            ("a brace naming no input", String::from(r#"{"debug":true}"#)),
            ("a dollar and a brace apart", String::from("$ {x}")),
        ]
    }

    /// Texts OpenCode fills in over its config's text, each beside why, the
    /// variable's name drawn afresh: what a definition means as it is
    /// written, and OpenCode would read as a variable's value or a file's
    /// contents.
    pub fn substitutions() -> Vec<(&'static str, String)> {
        let variable = random::word().to_ascii_uppercase();
        vec![
            ("a variable whole", format!("{{env:{variable}}}")),
            (
                "inside a longer value",
                format!("--home={{env:{variable}}}/x"),
            ),
            (
                "a file's contents",
                format!("{{file:./{}}}", random::word()),
            ),
            ("an opening a later brace may close", String::from("{env:")),
            (
                "closed by the reference written for an input",
                String::from("{env:a{workspace}"),
            ),
        ]
    }

    /// Texts that open nothing OpenCode fills in as they are, each beside
    /// why, written in a value of the program
    /// [`with_text`](Self::with_text) writes them in, where its
    /// placeholders are written as references, its `args` or its `env`:
    /// one is the placeholder of its input `workspace`.
    pub fn not_substitutions() -> Vec<(&'static str, String)> {
        vec![
            ("a brace naming no input", String::from(r#"{"debug":true}"#)),
            ("a reference in Claude's syntax", String::from("${HOME}")),
            ("a word that starts as one", String::from("{environment}")),
            ("a space after the brace", String::from("{ env:HOME}")),
            ("an input's placeholder", String::from("{workspace}")),
        ]
    }

    /// `name`'s definition holding `text` as it is, written where `place`
    /// says (`command`, `args` or `env` of the program of
    /// [`whole_secret_env`](Self::whole_secret_env) with no folder to start
    /// in, `cwd`, its folder, in the one that has one, or `url` or `headers`
    /// of the address of [`http`](Self::http)), beside where `show` says it
    /// is.
    pub fn with_text(name: &str, place: &str, text: &str) -> (String, String) {
        let quoted = toml::Value::String(text.to_owned()).to_string();
        match place {
            "cwd" => (
                Self::whole_secret_env(name).replacen("cwd = \".\"", &format!("cwd = {quoted}"), 1),
                String::from("transport.cwd"),
            ),
            "command" => (
                Self::whole_without_cwd(name).replacen(
                    "command = \"npx\"",
                    &format!("command = {quoted}"),
                    1,
                ),
                String::from("transport.command"),
            ),
            "args" => (
                Self::whole_without_cwd(name).replacen(
                    "\"{workspace}\"]",
                    &format!("\"{{workspace}}\", {quoted}]"),
                    1,
                ),
                String::from("transport.args[4]"),
            ),
            "env" => (
                Self::whole_without_cwd(name).replacen(
                    "LOG_LEVEL = \"info\"",
                    &format!("LOG_LEVEL = {quoted}"),
                    1,
                ),
                String::from("transport.env.LOG_LEVEL"),
            ),
            "url" => (
                Self::http(name).replacen(
                    "/mcp\"",
                    &format!("/mcp?q={}\"", &quoted[1..quoted.len() - 1]),
                    1,
                ),
                String::from("transport.url"),
            ),
            "headers" => (
                Self::http(name).replacen(
                    "Authorization = \"Bearer {token}\"",
                    &format!("Authorization = \"Bearer {{token}}\", X-Extra = {quoted}"),
                    1,
                ),
                String::from("transport.headers.X-Extra"),
            ),
            other => panic!("{other} is no place an agent fills text in"),
        }
    }

    /// The address of [`http`](Self::http), its supplied part written in
    /// its userinfo, `https://{tenant}@example.com/mcp`, where an address
    /// is kept with no credential: what `validate` says no agent is given.
    pub fn in_userinfo(name: &str) -> String {
        Self::http(name).replacen(
            "https://{tenant}.example.com/mcp",
            "https://{tenant}@example.com/mcp",
            1,
        )
    }

    /// The program of [`stdio`](Self::stdio), saying it is written in
    /// `format`: a newer one than this build reads, or one never written.
    pub fn in_format(name: &str, format: &str) -> String {
        Self::stdio(name).replacen("format = 1\n", &format!("format = {format}\n"), 1)
    }

    /// A file that does not read as TOML, a key given no value: no
    /// definition at all, as discovery, a template and `edit --contents`
    /// are each handed one.
    pub fn unreadable() -> String {
        String::from("format = 1\nname = \n")
    }

    /// A program that names nothing to start, and an address with no
    /// address: what `validate` calls incomplete.
    pub fn incomplete(name: &str, transport: &str) -> String {
        format!(
            "{}\n[transport]\ntype = \"{transport}\"\n",
            Self::head(name, &SkillMother::description())
        )
    }

    /// The program of [`stdio`](Self::stdio), with an input nothing uses
    /// and one declared twice.
    pub fn unreferenced(name: &str) -> String {
        format!(
            "{}\n\
             [[inputs]]\n\
             name = \"unused\"\n\
             kind = \"env\"\n\
             \n\
             [[inputs]]\n\
             name = \"workspace\"\n\
             kind = \"arg\"\n",
            Self::stdio(name)
        )
    }

    /// The program of [`stdio`](Self::stdio), its secret input named with
    /// a brace in it, `api}key`, written where it goes all the same, and
    /// read from a variable with hyphens, which no agent can refer to:
    /// what `validate` says no placeholder and no reference can be made of.
    pub fn misnamed(name: &str) -> String {
        Self::stdio(name)
            .replacen("name = \"api_key\"", "name = \"api}key\"", 1)
            .replacen("\"{api_key}\"", "\"{api}key}\"", 1)
            .replacen("\"EXAMPLE_API_KEY\"", "\"EXAMPLE-API-KEY\"", 1)
    }

    /// The program of [`stdio`](Self::stdio), its secret input naming no
    /// variable to be read from: what `validate` says no agent can be given
    /// but as a value a file holds.
    pub fn secret_without_variable(name: &str) -> String {
        Self::stdio(name).replacen("env_var = \"EXAMPLE_API_KEY\"\n", "", 1)
    }

    /// A token of each shape a definition is refused for holding, beside
    /// what it starts with, as `validate` says it.
    ///
    /// Drawn afresh every run and put together from parts at run time, so
    /// that no file in the repository holds anything shaped like a real
    /// credential, which a secret scanner would stop a push for, and a test
    /// cannot pass on one value it happens to spell.
    pub fn tokens() -> Vec<(&'static str, String)> {
        let upper = random::LOWERCASE.to_ascii_uppercase();
        let alphanumeric = format!("{}{upper}{}", random::LOWERCASE, random::DIGITS);
        let drawn = |length: usize| random::drawn_from(&alphanumeric, length);
        let github = random::pick(&["ghp", "gho", "ghu", "ghs", "ghr"]);
        let github_prefix: &'static str = match github {
            "ghp" => "ghp_",
            "gho" => "gho_",
            "ghu" => "ghu_",
            "ghs" => "ghs_",
            _ => "ghr_",
        };
        vec![
            (github_prefix, format!("{github}_{}", drawn(36))),
            (
                "github_pat_",
                format!("github_pat_{}_{}", drawn(22), drawn(59)),
            ),
            (
                "sk-",
                format!(
                    "sk-{}{}{}",
                    random::pick(&["", "proj-", "ant-api03-"]),
                    random::drawn_from(random::DIGITS, 1),
                    drawn(47)
                ),
            ),
            (
                "Bearer",
                format!(
                    "{} {}.{}",
                    random::pick(&["Bearer", "bearer"]),
                    drawn(24),
                    drawn(16)
                ),
            ),
        ]
    }

    /// One token of the shapes [`tokens`](Self::tokens) draws, beside what
    /// it starts with.
    pub fn token() -> (&'static str, String) {
        random::pick(&Self::tokens())
    }

    /// `name`'s definition holding `token` as it is, written where `place`
    /// says (`args`, `env`, `headers`, `url`, or `userinfo`, the user of
    /// the address; `env_var`, pasted where a secret input's variable is
    /// named; `input`, as a secret input's name, written where it goes as
    /// its placeholder; or `extras`, in a table of Claude's, and
    /// `extras_args`, in an array of Codex's), beside where `validate`
    /// says it is.
    pub fn with_literal(name: &str, place: &str, token: &str) -> (String, String) {
        let quoted = toml::Value::String(token.to_owned()).to_string();
        match place {
            "input" => (
                Self::stdio(name)
                    .replacen("name = \"api_key\"", &format!("name = {quoted}"), 1)
                    .replacen(
                        "\"{api_key}\"",
                        &toml::Value::String(format!("{{{token}}}")).to_string(),
                        1,
                    ),
                String::from("inputs[0].name"),
            ),
            "args" => (
                Self::stdio(name).replacen(
                    "\"{workspace}\"]",
                    &format!("\"{{workspace}}\", {quoted}]"),
                    1,
                ),
                String::from("transport.args[4]"),
            ),
            "env" => (
                Self::stdio(name).replacen(
                    "LOG_LEVEL = \"info\"",
                    &format!("LOG_LEVEL = \"info\", SERVICE_KEY = {quoted}"),
                    1,
                ),
                String::from("transport.env.SERVICE_KEY"),
            ),
            "headers" => (
                Self::http(name).replacen(
                    "Authorization = \"Bearer {token}\"",
                    &format!("Authorization = \"Bearer {{token}}\", X-Service-Key = {quoted}"),
                    1,
                ),
                String::from("transport.headers.X-Service-Key"),
            ),
            "url" => (
                Self::http(name).replacen(
                    "/mcp\"",
                    &format!("/mcp?key={}\"", &quoted[1..quoted.len() - 1]),
                    1,
                ),
                String::from("transport.url"),
            ),
            "userinfo" => (
                Self::http(name).replacen(
                    "https://{tenant}",
                    &format!("https://{}@{{tenant}}", &quoted[1..quoted.len() - 1]),
                    1,
                ),
                String::from("transport.url"),
            ),
            "env_var" => (
                Self::stdio(name).replacen(
                    "env_var = \"EXAMPLE_API_KEY\"",
                    &format!("env_var = {quoted}"),
                    1,
                ),
                String::from("inputs[0].env_var"),
            ),
            "extras" => (
                Self::stdio(name).replacen(
                    "timeout = 30000\n",
                    &format!("timeout = 30000\nheaders = {{ X-Service-Key = {quoted} }}\n"),
                    1,
                ),
                String::from("extras.claude.headers.X-Service-Key"),
            ),
            "extras_args" => (
                format!(
                    "{}\n[extras.codex]\nargs = [\"--key\", {quoted}]\n",
                    Self::stdio(name)
                ),
                String::from("extras.codex.args[1]"),
            ),
            other => panic!("{other} is no place a definition writes a value"),
        }
    }

    /// Texts that start the way a token does and are none, each beside
    /// why: what `validate` lets a definition hold. Each is judged with
    /// `{token}` an input's placeholder.
    pub fn not_tokens() -> Vec<(&'static str, String)> {
        let alphanumeric = format!("{}{}", random::LOWERCASE, random::DIGITS);
        vec![
            ("a placeholder after Bearer", String::from("Bearer {token}")),
            (
                "a reference in Claude's own syntax",
                String::from("Bearer ${GITHUB_TOKEN}"),
            ),
            (
                "a variable a shell expands",
                String::from("Bearer $GITHUB_TOKEN"),
            ),
            (
                "a scheme with nothing after it",
                String::from("--auth-scheme=Bearer"),
            ),
            ("a short word after a scheme", String::from("Bearer abc")),
            (
                "a prefix inside a word",
                format!("risk-{}", random::drawn_from(&alphanumeric, 30)),
            ),
            (
                "a package named with no digit",
                String::from("sk-mcp-server-for-every-tracker"),
            ),
            (
                "a prefix cut short",
                format!("ghp_{}", random::drawn_from(&alphanumeric, 12)),
            ),
            (
                "a prefix in capitals GitHub never gives",
                format!("GHP_{}", random::drawn_from(&alphanumeric, 36)),
            ),
            ("a path", String::from("/usr/local/share/sk-tools")),
            ("a token made of an input", String::from("ghp_{token}")),
        ]
    }

    /// What `add` is told of a program, every option of one given: an
    /// argument led by a dash, a setting, a secret passed on to its
    /// environment under its variable's name and an argument read from a
    /// variable, each read from a variable drawn afresh.
    pub fn given_program() -> Given {
        Given::Program(Program {
            command: String::from("npx"),
            args: vec![
                String::from("-y"),
                String::from("@example/server"),
                String::from("--workspace"),
            ],
            env: vec![(String::from("LOG_LEVEL"), String::from("info"))],
            secret_env: vec![VariableNameMother::random()],
            arg_env: vec![VariableNameMother::random()],
        })
    }

    /// What `add` is told of an address, over server-sent events when
    /// `events` and streamable HTTP otherwise, every option of one given: a
    /// header written as it is, one whose whole value is a secret, a bearer
    /// token, and a part of the address read from a variable, each read
    /// from a variable drawn afresh.
    pub fn given_address(events: bool) -> Given {
        Given::Address(Address {
            url: String::from("https://{tenant}.example.com/mcp"),
            events,
            headers: vec![(String::from("X-Client"), String::from("mindweaver"))],
            secret_headers: vec![(String::from("X-Api-Key"), VariableNameMother::random())],
            bearer: Some(VariableNameMother::random()),
            url_vars: vec![(InputName::from("tenant"), VariableNameMother::random())],
        })
    }

    /// What `add` is told of any of the three transports.
    pub fn given() -> Given {
        match random::between(0, 2) {
            0 => Self::given_program(),
            1 => Self::given_address(false),
            _ => Self::given_address(true),
        }
    }

    /// What `add` is told of a server, `token` given as it is where `place`
    /// says (`arg`, `env` or `header`, a value written as typed; `url`, in
    /// the address; or `secret_env`, pasted where a secret's variable is
    /// named), beside where `validate` says the definition holds it.
    pub fn given_with_literal(place: &str, token: &str) -> (Given, String) {
        let Given::Program(mut program) = Self::given_program() else {
            unreachable!("a program is given as one");
        };
        let Given::Address(mut address) = Self::given_address(false) else {
            unreachable!("an address is given as one");
        };
        match place {
            "arg" => {
                program.args.push(token.to_owned());
                (Given::Program(program), String::from("transport.args[3]"))
            }
            "env" => {
                program
                    .env
                    .push((String::from("SERVICE_KEY"), token.to_owned()));
                (
                    Given::Program(program),
                    String::from("transport.env.SERVICE_KEY"),
                )
            }
            "header" => {
                address
                    .headers
                    .push((String::from("X-Service-Key"), token.to_owned()));
                (
                    Given::Address(address),
                    String::from("transport.headers.X-Service-Key"),
                )
            }
            "url" => {
                address.url.push_str(&format!("?key={token}"));
                (Given::Address(address), String::from("transport.url"))
            }
            "secret_env" => {
                program.secret_env.push(VariableName::from(token));
                (Given::Program(program), String::from("inputs[1].name"))
            }
            other => panic!("{other} is no place `add` writes a value"),
        }
    }

    /// What `add` is told of a server that writes a definition `validate`
    /// calls invalid, each beside why and the code it says it with: none a
    /// rule of `add`'s own.
    pub fn given_unfit() -> Vec<(&'static str, Given, &'static str)> {
        let Given::Program(program) = Self::given_program() else {
            unreachable!("a program is given as one");
        };
        let Given::Address(address) = Self::given_address(false) else {
            unreachable!("an address is given as one");
        };
        // A hyphen, which an input's name may hold and a variable's may not,
        // so the one name is wrong only as the variable.
        let unportable = format!("{}-{}", random::word(), random::word()).to_ascii_uppercase();
        let again = program.secret_env[0].clone();
        vec![
            (
                "a program named by nothing",
                Given::Program(Program {
                    command: String::new(),
                    ..program.clone()
                }),
                "transport_incomplete",
            ),
            (
                "an address that is nowhere",
                Given::Address(Address {
                    url: String::new(),
                    url_vars: Vec::new(),
                    ..address.clone()
                }),
                "transport_incomplete",
            ),
            (
                "a part of the address the address does not write",
                Given::Address(Address {
                    url: String::from("https://example.com/mcp"),
                    ..address.clone()
                }),
                "input_unreferenced",
            ),
            (
                "a variable no agent can refer to",
                Given::Program(Program {
                    secret_env: vec![VariableName::from(unportable)],
                    ..program.clone()
                }),
                "env_var_malformed",
            ),
            (
                "one variable read twice",
                Given::Program(Program {
                    arg_env: vec![again],
                    ..program
                }),
                "input_repeated",
            ),
        ]
    }

    /// What `add` is told of a server that gives one key of a transport's
    /// table twice, which no TOML file holds, each beside why and the key
    /// found again.
    pub fn given_repeating() -> Vec<(&'static str, Given, Repeated)> {
        let Given::Program(program) = Self::given_program() else {
            unreachable!("a program is given as one");
        };
        let Given::Address(address) = Self::given_address(false) else {
            unreachable!("an address is given as one");
        };
        let variable = VariableNameMother::random();
        let repeated = |table, key: &str| Repeated {
            table,
            key: key.to_owned(),
        };
        vec![
            (
                "one setting written twice",
                Given::Program(Program {
                    env: vec![
                        (String::from("LOG_LEVEL"), String::from("info")),
                        (String::from("LOG_LEVEL"), String::from("debug")),
                    ],
                    ..program.clone()
                }),
                repeated("env", "LOG_LEVEL"),
            ),
            (
                "a setting written and read from a variable",
                Given::Program(Program {
                    env: vec![(variable.to_string(), String::from("1"))],
                    secret_env: vec![variable.clone()],
                    ..program
                }),
                repeated("env", variable.as_str()),
            ),
            (
                "a header written and read from a variable",
                Given::Address(Address {
                    headers: vec![(String::from("X-Api-Key"), String::from("x"))],
                    ..address.clone()
                }),
                repeated("headers", "X-Api-Key"),
            ),
            (
                "an authorization written beside a bearer token",
                Given::Address(Address {
                    headers: vec![(String::from("Authorization"), String::from("Basic x"))],
                    ..address
                }),
                repeated("headers", "Authorization"),
            ),
        ]
    }

    /// What a request could send of a server naming something beside a
    /// value with an `=` in the name, which no command line gives, each
    /// beside why and what is found of it: a setting's variable, a
    /// header's name, a secret header's and a part of the address's.
    pub fn given_unsayable() -> Vec<(&'static str, Given, Unsayable)> {
        let Given::Program(program) = Self::given_program() else {
            unreachable!("a program is given as one");
        };
        let Given::Address(address) = Self::given_address(false) else {
            unreachable!("an address is given as one");
        };
        let key = format!("{}={}", random::word(), random::word());
        let unsayable = |option| Unsayable {
            option,
            key: key.clone(),
        };
        vec![
            (
                "a setting whose variable holds `=`",
                Given::Program(Program {
                    env: vec![(key.clone(), String::from("1"))],
                    ..program
                }),
                unsayable("--env"),
            ),
            (
                "a header whose name holds `=`",
                Given::Address(Address {
                    headers: vec![(key.clone(), String::from("x"))],
                    ..address.clone()
                }),
                unsayable("--header"),
            ),
            (
                "a secret header whose name holds `=`",
                Given::Address(Address {
                    secret_headers: vec![(key.clone(), VariableNameMother::random())],
                    ..address.clone()
                }),
                unsayable("--secret-header"),
            ),
            (
                "a part of the address whose name holds `=`",
                Given::Address(Address {
                    url_vars: vec![(InputName::from(key.as_str()), VariableNameMother::random())],
                    ..address
                }),
                unsayable("--url-var"),
            ),
        ]
    }

    /// What `add` is told of a server holding `token` where it is refused
    /// before any definition is read for `validate` to judge, each beside
    /// why and the code it is refused with: as a key given twice, which no
    /// TOML file holds, or past the host of an address typed with a
    /// credential, whose refusal says what is left of it.
    pub fn given_refused_unread(token: &str) -> Vec<(&'static str, Given, &'static str)> {
        let Given::Program(program) = Self::given_program() else {
            unreachable!("a program is given as one");
        };
        let Given::Address(address) = Self::given_address(false) else {
            unreachable!("an address is given as one");
        };
        let twice = |value: &str| {
            vec![
                (token.to_owned(), value.to_owned()),
                (token.to_owned(), value.to_owned()),
            ]
        };
        vec![
            (
                "a token pasted twice for a secret's variable",
                Given::Program(Program {
                    secret_env: vec![VariableName::from(token), VariableName::from(token)],
                    ..program.clone()
                }),
                "add_mcp_key_repeated",
            ),
            (
                "a setting keyed by a token twice",
                Given::Program(Program {
                    env: twice("1"),
                    ..program
                }),
                "add_mcp_key_repeated",
            ),
            (
                "a header keyed by a token twice",
                Given::Address(Address {
                    headers: twice("x"),
                    ..address
                }),
                "add_mcp_key_repeated",
            ),
            (
                "an address typed with a user and a token in its query",
                Given::Address(Address {
                    url: format!(
                        "https://{}:{}@example.com/mcp?key={token}",
                        random::word(),
                        random::word()
                    ),
                    ..Address::default()
                }),
                "add_mcp_url_userinfo",
            ),
        ]
    }

    /// What every definition opens with: its format, its name and what it
    /// is for, each a TOML string whatever it holds.
    pub fn head(name: &str, description: &str) -> String {
        format!(
            "format = 1\nname = {}\ndescription = {}\n",
            toml::Value::String(name.to_owned()),
            toml::Value::String(description.to_owned())
        )
    }
}
