# Architecture

## The shape of the tree

Every unit of the product is an app under `apps/`, with its own manifest
(`Cargo.toml`, or `package.json` for the panel) and its own `Makefile`. The
root `Makefile` delegates (`make <app>/<target>`) and CI invokes those
targets, so adding a front end is adding a directory, a line in `APPS` and the
workspace `members`, and its test and build jobs in CI, never reshaping the
tree.

```
apps/core    mindweaver-core     the engine
apps/cli     mindweaver-cli      the `mind` and `mindweaver` binaries
apps/server  mindweaver-server   the local server behind the web panel
apps/web     mindweaver-web      the web panel, in Vue
```

The layout exists because there would be more than two, and the server and
the panel are the apps [D-17](#d-17-the-next-front-end-is-a-local-web-panel)
adds. Both sit beside the CLI rather than around it. The panel is in `APPS`
with the crates, first, so `make test` and `make build` need Node (see [in
`make test`, and in CI](#in-make-test-and-in-ci)).

`scripts/agent-probe/` is not an app, and nothing builds or tests it. It is
the probe behind [docs/agents.md](agents.md), run by hand against the real
agents with the operator's own login, which is why CI never runs it.
`scripts/serves-panel.sh` is not one either: it is what `make
cli/release/check` runs on a release CI built ([in `make test`, and in
CI](#in-make-test-and-in-ci)). Nor is `docs/api/schemas`, the API's JSON
Schemas, which the crates' tests write and `make api/schemas` writes
again ([the API's schemas come from Rust](#the-apis-schemas-come-from-rust)),
nor `docs/api/openapi.json`, the API's OpenAPI document, which a test of
the CLI's writes and `make api/openapi` writes again ([the API's
description is written from both
tables](#the-apis-description-is-written-from-both-tables)).

### The examples are a workspace

`examples/` sits beside `apps/` and is content, not an app: a weaver
workspace (`examples/.mindweaver/weaver.toml`) holding two skills and two
rules of its own, so there is something to install while gathering is
paused. A workspace rather than files the binaries embed or `mindweaver init`
copies in, because [loading](#loading-a-third-place-read-live) already offers
one's own skills and rules to any other, read live from the clone: the
examples take the path a team's workspace takes, and need no command of their
own.

Its `[directories]` are spelled out, so it reads the same whatever a new
workspace defaults to
([D-05](#d-05-a-workspaces-library-lives-in-mindflayer)). They sit beside the
marker rather than in `.mindweaver/library`, because the examples are meant
to be read where the repository is browsed. No agent working on this
repository reads them there: each looks for skills and rules in a folder of
its own, such as `.claude` or `.agents`, not in one called `skills` or
`rules`.

`apps/cli/tests/examples_test.rs` uses the folder where it is rather than a
copy, since that folder is what a person loads. It holds every example to
`validate` with neither a problem nor a warning, installs `commit-style` with
the reference it carries and both rules where [the
reference](reference.md#try-it-with-the-examples) says they go, checks that
nothing it ran wrote into the folder, and finds the reference's walk-through
in the output of the commands it shows.
An example whose summary changes changes the reference with it.

### The README tours the panel; the reference holds the rest

`README.md` is the first page anybody reads, here and on GitHub, so it says
what Mindweaver is in a few lines, how to install it, `mindweaver ui`, and tours
the panel in screenshots ([the README's
screenshots](#the-readmes-screenshots)), with the few commands a newcomer
needs. It had grown into a reference of close to 2,800 lines, every
command's literal output among them, which buried all of that. The
reference moved, whole and unchanged in substance, into
[docs/reference.md](reference.md), how to build and test into
[docs/development.md](development.md). No literal output is left in the
README but one: the commands' is the reference's, where `examples_test.rs`
reads its walk-through of the examples, and the HTTP API's is [its
guide's](#the-apis-guide-runs-its-examples), written since. The one left is
what an MCP server's install prints (#95): what each agent will run is the
first thing somebody adding one needs to see, in the section that tells
them how.

Every relative link in the README and under `docs/` is held to lead
somewhere by `apps/cli/tests/docs_test.rs`: the file it names is there, an
anchor names a heading as GitHub spells it (lowercased, punctuation
dropped, spaces made hyphens, a repeated one numbered), and every image in
`docs/images` is shown by some page, since one nobody shows is weight the
history carries for nothing. A Rust test rather than a link checker from
npm, so it runs in `test (cli)` and `make cli/test` with no Node, as CI's
Rust jobs have none, and offline: a link to another site is not fetched,
or somebody else's outage would fail the build. It reads Markdown just far
enough for that, skipping fenced blocks and code spans, where a link or a
`#` is an example. Its first run found one anchor that led nowhere, in
this document since before the move, which no check had read until then.

### The API's guide runs its examples

[docs/api.md](api.md) is what somebody calling [the HTTP
API](#d-24-the-local-server-is-a-versioned-http-api) reads first (#219):
starting the server for a script, the token, the versioning rule, every
read with its answer, a change previewed and then run, what goes wrong and
its codes, the events and where the API is described. Its answers are
literal, as the reference's are, and `apps/cli/tests/api_guide_test.rs`
holds them to the server as `examples_test.rs` holds the reference's
walk-through to the commands. It builds the guide's workspace with the
commands its first block shows, run in process, and finds what they
printed there; starts `mindweaver ui` as the guide does, `--token-file`
included, on port 0 with a temporary home and every agent home empty; and
sends every `curl` line of a `console` block over loopback with the
client `ui_test` uses (`common::panel`), holding each answer to the
OpenAPI document (`common::contract`) before it holds it to what the guide
shows under the line. Once it stops the server with a `SIGTERM`, as the
guide does, what `ui` printed after its link is in the guide too, and the
token file is gone. Every route of both tables has an example, and the
guide's table of the server's codes is held to `Condition::ALL`, each with
its status, so a route or a condition added fails the test until the guide
says it.

**The lines are read, not run.** curl and jq are not on every machine the
suites run on, and a missing one would have to fail under `$CI` and be
skipped elsewhere, as a shell is in `shells_test`, for a check that needs
neither. So the test reads each line as a POSIX shell and curl would, as
far as the guide writes them: single and double quotes, `$api` and
`$token`, `-s`, `-N`, `-H`, `-d`, `-w` and `-X`, one URL, and an answer
piped through jq with a filter that is a path. Anything else fails, naming
it, rather than be sent as something the line does not say. It sends what
curl sends besides, a body with no `Content-Type` as a form, so a line
that forgets the header is answered the 415 a reader would get. An answer
jq lays out is held to the guide's as JSON, the same values whatever the
spaces between them, since jq's layout is not serde's; one printed as it
came, its status after it by `-w`, as text, byte for byte. An example
that shows no status is a 200. The few lines of shell that start the
server in the background and wait for its link are a recipe with no
answer, and are not run.

**Pinned as the reference's examples are.** The home is a temporary folder
that reads as `/home/you`, the port and the token are the reference's, and
each project's id the panel's fixtures'; the guide says which of a
reader's own values differ, and shows no time, so none is pinned. The
event stream is shown to its first two events, `resync` and the watcher's
word, which a subscriber is told before anything else: what follows a run
waits on the watcher's debounce, and would come in an order no literal
example could hold. Unix alone, as the guide's lines are a POSIX shell's
and the server is stopped by `kill`.

### The CLI end to end, a use case a test

Every other suite of the CLI's stops short of the process: it parses a line
with the real parsers and runs it inside the test (`common::mind`,
`common::weaver`), its options pinned so it reaches no real home. What lies
between a person's shell and that function goes untested there: the
binaries' `main`, the environment they read, the exit status, which stream
a line goes to, and standard input. `apps/cli/tests/e2e_test.rs` crosses it,
one use case a test: it builds the world that use case needs, runs one
command line in it as the built binary, and holds the exit status, stdout
and stderr word for word, and what the command left on the disk. It runs in
`test (cli)` with every other suite of the app, a target of its own and not
a job, as the tests are split by app and by nothing else ([in `make test`,
and in CI](#in-make-test-and-in-ci)).

**The world is built, never shared.** Each test makes its own (`Scenario`,
in `tests/e2e/scenario.rs`), in a temporary folder, so any one runs alone
and none can depend on another having run. What a person or a git server
makes is made directly: folders, a repository to be a project (gix, so no
`git` binary), a file an editor saved, a source of skills committed once
and again. What Mindweaver keeps is made only by its commands (`given`),
each held to exit 0 with nothing on stderr, as the panel's browser suite
builds its fixture: a step that warned would be building something other
than what the test names, and a database written by hand would test what no
command writes. What an older build made is the one exception, since no
command of this build writes it: a project marked by a `mind.toml` is made
by core's `MindProject::init`, kept to write that marker for such a test,
and the rest of its state by the commands, as for any other.

**The line run is the line typed.** A command is the binary
(`CARGO_BIN_EXE_mind`, `CARGO_BIN_EXE_mindweaver`) started from the folder
the line is typed in, with an environment holding only `PATH` and the
scenario's home, rather than the developer's with `MINDWEAVER_*`,
`MINDFLAYER_*` and the agent homes taken out: what any other variable would
change is a test's to say, for that command alone, as an activated workspace is. No option is
added to the line, which the in-process suites must do (`common::PINNED`)
since they share one process and its environment. Words are split as a
shell would (`shell-words`), and a leading `~` is the home.

**Paths read as the reference's.** The folder is read as `/home` in what a
command prints, and its home is `you`, so an expected output reads
`/home/you/Projects/collapse` where the reference does, and a test is the
reference's example with its scenario around it. A test may spell a path in
a line from `/home` too. What would still differ from run to run is spelled
from the scenario: the commit a source is on (`head`), a project's id
(`project_id`). A source is gathered by its path from the workspace, never
its absolute one: a table pads every row to its longest, so a temporary
folder's length would leak into the spaces of every row, and the shelf
folder a source is gathered into is named from it.

Rejected: case files read by a runner (trycmd's `.toml` and `.md`), which
would put the input and the output side by side with no Rust, but leave the
scenario to a folder copied in, which builds Mindweaver's state by hand, or
to steps in a format of their own. A test in Rust builds the world with the
commands, names the use case in its own name, as every suite here does, and
says what it holds on the disk besides the output, as an install's copies.

Unix alone: a Windows path, its drive and its backslashes, never reads as
the reference's, so every use case printing one would fail there for how
the path is spelled.

### The server end to end, a request a test

The server has no binary: what a person or a script runs is `mindweaver ui`,
which serves it over the CLI's backend. Its suites run against a fake
backend, `backend_test` drives the real one with no port bound,
`ui_test` checks every route and guard on the suites' common fixtures,
and `api_guide_test` replays the guide's one walk-through; none of them
is a use case built for itself. `apps/cli/tests/api_e2e_test.rs` is, as
`e2e_test` is for the command line: each test builds the world it needs
with the CLI suite's `Scenario`, types `mindweaver ui --no-open` in one of
its folders, in the same environment as every command there, reads where it listens and its token
from the link it prints, and sends one request over loopback as curl
sends it (`tests/api_e2e/served.rs`), holding its status and its answer,
and what it left: on the disk, in what a command reads next, or in what
the server tells or prints after. It lives in the CLI's crate because the
binary does, and runs in `test (cli)`.

**A part of an answer, as a script reads it.** An answer is held whole
where it is small (a preview, a run, a refusal, a plan), and otherwise
the part the use case is about, picked by a JSON pointer as the guide
picks one with jq (`answers_at`): the snapshot holds times, and a
project's tally and templates would make every test about a project hold
what it is not about. The paths in a request are spelled under `/home`,
and read back so in an answer, as in the CLI's suite. Every answer to a
route the API describes is held to its OpenAPI document besides
(`common/contract.rs`), so a use case meeting one outside it fails.

**What only the API leaves is built through it.** Mindweaver's state is
made by the commands (`given`), as in the CLI's suite; what is the
server's alone, the transcript of what it ran, is made by tasks held to
run clean (`Served::given`), since no command writes it.

**An event is waited for, never counted.** The watcher takes in what
happens together before it tells it, so how many `changed` events a
change makes depends on the machine: a test reads the stream until the
event it is about (`Stream::until`) and holds that one, and leaves the
others out of what a run is told (`command_started`, `command_finished`).

Rejected: the suite in `apps/server`, finding `mindweaver` in `target/`.
Cargo names a binary to the tests of its own crate alone
(`CARGO_BIN_EXE_<name>`), so the server would need the CLI as a
dev-dependency, which `crate_test` refuses since the CLI depends on it, or
a path guessed from the build's layout. Unix alone, as the server is
stopped by a SIGINT `kill` sends.

### Core end to end, a call a test

Core has no binary and prints nothing: to the CLI, the TUI and the panel it
is its public API, whose answers they render and whose read types they
print as JSON. Its suites hold a module each, or a flow through several
built by its mothers over values drawn afresh (`flows_test`), each to the
Rust values it returns. `apps/core/tests/e2e_test.rs` holds it as a front
end meets it, one use case a test: it builds the world that use case needs
(`World`, in `tests/e2e/world.rs`), makes one public call in it, as a
command makes it, and holds what core answered as a front end reads it,
and what it left on the disk. It runs in `test (core)`.

**Mindweaver's state is made by core's own calls.** As the CLI's suite
makes it with the commands, never by hand: each step is the call a command
makes for it, `MindProject::init_asked` for `mind init`,
`WeaverWorkspace::init` and `known::remember` for `mindweaver init`,
`WeaverWorkspace::link`, `lifecycle::add`, `assign::install`, handed what a
front end hands them (the home, and the agents' own folders under it,
`AgentHomes::defaults_under`), and held to go cleanly: an install that
added nothing, or left something out, would be building another world than
the one the test names.

**An answer is read as a front end reads it.** A read type is held as its
JSON, every path under the world read as `/home`, the home being
`/home/you`, so an answer reads as the reference's and the API guide's do:
that is the contract every front end prints, and a field named otherwise
fails here as it would fail a script. A refusal is held by its `code()`,
which a front end acts on, and its words, which it shows. What a change
did is held on the disk and in the snapshot every front end reads next.

**A gather names its source by its absolute path.** The tests share their
process's folder, so a source cannot be named from the workspace as the
CLI's suite names it, and the shelf folder a gather names from the address
spells the temporary folder out: a test holds that folder by its shape,
and every skill gathered to lie in it, rather than word for word.

Unix alone, as the other two: a Windows path never reads as the
reference's.

### The TUI end to end, in a terminal of its own

The screen's suite drives its state with key events and renders it into
ratatui's test backend, so what lies between that and a person is never
run there: the event loops, raw mode and the alternate screen, keys as a
terminal sends them, and the transcript printed once the window is given
back. `apps/cli/tests/tui_e2e_test.rs` runs the binary in a
pseudo-terminal, one use case a test, on the CLI suite's scenario.

**The terminal is the screen's own.** crossterm reads the window's size
from `/dev/tty`, the controlling terminal, and not from what its standard
streams are: a child of the test, in the test's session, would be sized
by the developer's terminal, or by none under CI. So the screen is started
as a shell starts a job, the leader of a session of its own with the
pseudo-terminal as its controlling one (`setsid`, then `TIOCSCTTY`, in
`pre_exec`), at a size the test sets, 120 by 24. rustix, which crossterm
builds already, opens the terminal; `vt100` reads what is drawn on it back
as rows, the one crate the suite adds beside vte.

**A test waits for what is drawn, never sleeps.** The screen draws when it
is ready and again when something changes, so each step waits, bounded,
until the rows show what it expects, and fails with the screen as it was.
The one pause is after an escape: one read together with the key after it
is that key with Alt held, in any terminal, and a person's next key comes
later than a test's.

**The terminal answers where its cursor is.** ratatui asks before it
clears the screen (`ESC [ 6 n`, a device status report), which the
screen does when it takes its window back from an editor, and gives up
drawing when no answer comes within two seconds. Every terminal a person
uses answers it, and a pseudo-terminal read by a parser does not, so the
suite answers as a terminal does, with where the cursor is when it was
asked: on the thread that reads what the screen writes, with a parser of
its own fed everything, so an answer never waits on a test's next step.
The editor itself is the use case's (`EDITOR`, given to that one
screen), one that writes and waits for a key, so a test sees the window
while it is the editor's, off the alternate screen, before the screen
draws itself again.

**A gather waits on a server the test holds.** A source on the disk is
gathered before a key could stop it, so the suite serves one on loopback
that holds each request until the test lets it answer, and answers that
there is no such repository. A gather waiting on the network takes a
cancel only once it hears back, as [a gather runs
apart](#a-gather-runs-apart) says: so the test waits until the server
has read the request, presses `esc` or Ctrl-C, sees the cancel said, and
only then lets the server answer, when the gather stops with nothing on
the shelf, no clone in the cache and nothing in the ledger. A server that
never answered would hold the gather until the HTTP client gave up,
thirty seconds on; one answering at once would race the key, which is how
a first version of these tests passed only when the key came before the
connection. A stand-in runner, as the screen's own suite uses, would stop
a gather that is not gix's.

**A row with a path in a box is held by what it says.** The screen pads a
box's rows to its width around the path it shows, so a path read back as
`/home` is shorter than the one drawn, and the box's edge moves: such a
row is held by its text, every other row word for word.

Rejected: expect-style crates (rexpect, expectrl) that match a stream of
bytes, which an alternate screen redrawn cell by cell never reads as a
row, and portable-pty, which brings crates of its own for what rustix,
built already, does in a few calls.

### No em dash in what the repository says

The owner finds the em dash reads as machine-written, so none is written:
not in docs, comments, messages, test expectations or commit messages. A
comma, a colon, parentheses or a new sentence takes its place, rather than a
hyphen or a spaced en dash, which would keep the tic under another glyph.
`make lint` runs `lint/em-dash` before clippy, and it fails on one in any
tracked file. A Makefile target rather than a clippy lint, because the
character turns up in Markdown and in the panel as readily as in Rust. Only
tracked files are searched, since an untracked folder (an agent's worktree,
a scratch clone) is not the repository's text, and git failing is a failure
rather than a clean tree. Commit messages are not checked: the check reads
the tree, the history from before the rule holds some that cannot be
rewritten, and a new one is its author's to watch.

## Where the line between core and a front end falls

`mindweaver-core` knows what a mind project is, what a weaver workspace is, and
what each kind of artifact is. It reads and writes files, and the only other
I/O it does is fetching a repository when asked to gather, through a port whose
adapter is `gix`, and reading the clock ([values, ports and
adapters](#values-ports-and-adapters)). It has no idea a terminal exists.

`mindweaver-cli` parses a command line, asks core for values, and renders them.
It holds no rules about artifacts: not the name limits, not the front matter
format, not which folder a kind lives in, not what a change does to the
record of an install.

It ships **two binaries**, `mind` and `mindweaver`, and they are two entry points
into one parser rather than one wrapping the other. `mindweaver <cmd>` and `mind
mindweaver <cmd>` reach the same function, so there is a single implementation of
every workspace command and the two spellings cannot drift apart or disagree
about an exit code. A wrapper that re-executed `mind` would also have to
forward arguments, stdio and exit codes correctly, which is three chances to
get it wrong in exchange for nothing.

The TUI is in the same crate, behind the same parser, rather than a front end
of its own: it does everything by running these commands. See [the
TUI](#the-tui).

The local server is a crate of its own, and it holds no rules either: it
carries JSON between the browser and whichever front end implements its
`Backend`, the CLI's for the panel, and depends on neither core nor the CLI.
See [the local server](#the-local-server). Nor does the panel it serves: it asks through the
server and draws the answers. See [the web panel](#the-web-panel).

The line is there so that when a second front end arrives, it cannot disagree
with the first about what a valid skill is. Every rule that could drift lives
in one crate, and the front ends only choose how to show its answers.

Two consequences worth stating, because they are what the split buys:

- **Sorting happens in core**, not in the renderer. Two front ends listing the
  same projects produce the same order.
- **Rendering happens in the front end**, and the CLI builds its output into a
  string before printing it, which is what lets its tests assert on the exact
  text a user sees.
- **Reading happens through one value**, the snapshot core builds of a
  workspace ([the read model](#the-read-model)), so two front ends showing
  one workspace show one answer, put together once.

### How the CLI crate is laid out

The grammar is in `parse.rs`, the work in `commands.rs` and a module per
family of commands under `commands/`, and what a command produced, each line
on stderr with its code ([failures are collected, not
raised](#failures-are-collected-not-raised)), with the helpers every report is
laid out with and the envelope its JSON goes out in, in `render.rs`.
`CliError` has `error.rs`, with every code the CLI gives of its own
(`CODES`), `homes.rs` decides where the default workspace's home and
each agent's own folder are, and gathers them with what `-W` and
`$MINDWEAVER_WORKSPACE` say (`Lookup`), `progress.rs` says what a gather's
steps read as and draws them on stderr, `task.rs` holds the work a front end
asks for as the command lines it is ([every change is a command
line](#every-change-is-a-command-line)), `admit.rs` holds a task sent from
outside to the path policy and `backend.rs` is the panel's backend ([what a
browser may name](#what-a-browser-may-name)), which `commands/ui.rs` serves
([launching it](#launching-it-mindweaver-ui)), and `lib.rs` only says what is
public. The backend, `ui` and the server they need are behind the crate's
`ui` feature, on by default. `typescript.rs` and `schema.rs` describe what
the CLI hands the panel and the API, under the `ts` and `schema` features
([its types come from Rust](#its-types-come-from-rust), [the API's schemas
come from Rust](#the-apis-schemas-come-from-rust)).

The split is by family. One file holding the parser, every command and the
rendering was where every change to a command met every other, so pieces of
work on different commands collided in it. One module per command would
scatter what a family shares instead: `load` and `unload` report the same way,
and `list`, `show` and `validate` read one catalog. `commands.rs` dispatches,
and asks core what a command acts on, the one thing every family needs: which
workspace is core's `resolve` to decide (see [which workspace a command
uses](#which-workspace-a-command-uses)), and which of its projects `-p` names
is `WeaverWorkspace::pick`.

## Values, ports and adapters

Core is written hexagonally, at the owner's request
([D-25](#d-25-core-is-values-and-ports)): the rules in the middle, built of
value objects, depend on nothing outside them; what they need from outside
is a port; and an adapter on the edge implements each port with a library:
`gix` for a source, SQLite for the ledger, the system's clock for the time.
The front ends are the other side's adapters, so a new store or a new
interaction module is written against what is there, and no rule changes
for it.

**Values.** `value` holds the value objects every record, report and view
core builds is an aggregate of, each a type holding one text that can be
made only the way its rule allows:

- **Held to a rule**: `ContentHash`, which only the recipes in `hash` make
  (and `ContentHash::parse`, from a digest somebody hands over, and the
  ledger, as it holds one); `ProjectId`, generated or parsed; and
  `Redacted`, a URL or words that may quote one, made only through
  `Redacted::new`, which takes every credential out. A function handed one
  need not check it again: the rule is the type.
- **Named apart**: `ArtifactName`, `Origin` and `Revision`, and an MCP
  server's `InputName` and `VariableName`, which hold any text, since core
  reads leniently ([strict when writing, lenient when
  reading](#strict-when-writing-lenient-when-reading)) and a name on the
  disk that breaks a rule is listed all the same, but each a type of its
  own so that one is never handed where another is meant. `Origin` writes
  itself out redacted whatever built it, since a source's address is part
  of it. `InputName` and `VariableName` each say a rule they are not made
  by (`is_writable`, `is_portable`), for `validate` to ask
  ([what an MCP server's definition
  declares](#what-an-mcp-servers-definition-declares)).

A `String` for each was what core had, and it said nothing of what it held:
a hash could be passed where a name was meant, and a URL still carrying a
token stored where only one without may go, with only a reviewer to notice.
Each value reads as its text (`Deref` to `str`, comparisons with text
either way round, `Display` and `Debug` as the text), so code that only
reads one reads it as it read the `String`, and goes out as that text: serde
writes a string, ts-rs says `string` (a hand-written `TS` impl, since a
derive even with `ts(as)` names the type and exports a file of its own) and
schemars says what it says of a `String`, inline. So turning a field into
one changes nothing the JSON, the panel's types or the API's schemas say,
which the drift checks hold, and nothing a `--json` reader was promised.
`ProjectId` is the one exception, kept as it was published, with a
TypeScript type and a pattern of its own. What is still a `String` is what
no value names yet: a project's name and a registry entry, a path inside a
project, an error's fields. Those join `value` as a rule is found for them,
the same way.

**Ports.** `port` holds what core needs of the world beyond its files, each
a trait in core's own words: a `Remote` fetches a source into the cache and
scrubs a clone of what a credential left in it, a `LedgerStore` keeps a
workspace's records (its sources and what each supplied, every installation
and its copies, the projects it knew, the action log), and a `Clock` tells
the time the ledger stamps by. A port speaks only core's types: what an adapter's
library says is turned into those at the edge (an error into text, a commit
into a `Revision`), never carried through, which would tie core's public API
to the library's version and let a rule lean on how one library works. So
`GitError` and the clone a fetch answers with (`port::Cloned`) are the
port's, since they carry only text, and `LedgerError` carries what a store
said boxed (`Box<dyn Error + Send + Sync>`), so it names no store's type;
the SQLite adapter, which alone knows what it put there, looks inside it
with a `downcast_ref` where it has to tell one failure from another.

**Adapters.** `adapter` implements them: `adapter::gix::Gix`, which clones
with `gix` and asks the credential helpers through the callback that never
prompts ([a gather never prompts for a credential
itself](#d-16-credentials-come-from-the-front-end-never-from-a-prompt));
`adapter::sqlite::Sqlite`, every query, every schema step and the backup
before one ([why the ledger is SQLite](#why-the-ledger-is-sqlite)); and
`SystemClock`. Each is the default its port is given wherever nothing else
is handed over, the seam [D-14](#d-14-core-owns-http-behind-a-seam) chose
for HTTP: a gather's `Request` fetches through the remote it is handed
(`Request::through`), `Gix` unless another is, and the ledger stamps by the
clock it is given (`Ledger::with_clock`), the system's unless another is,
and keeps its records in the store it is handed (`Ledger::new`), SQLite
unless another is: every other constructor opens a `Sqlite`. A
test's fake is one more adapter: `port_test.rs` gathers through a remote
that writes files and has no git at all, and stamps the ledger by a clock
that does not move, and what each comes to is what it comes to through the
real ones. The scrub `ledger_to_write` runs goes through `Gix` whatever a
gather is handed, since what it repairs is what an older build's gix clones
kept.

**What is not behind a port.** The files are not. What core promises of
them is how they are written (`create_new`, a temporary renamed into place,
a link never followed out of its holder, a lock held while a writer
changes the workspace), and those promises are made of the operating
system's own calls: an adapter that only seemed to keep them is the failure
a filesystem port would invite, and every test already runs on real
temporary folders. Reading what git keeps of a project, where its common
folder is and whether its index tracks a copy (`exclude`), is the same: it
reads files git wrote, through `gix` with every system and user setting
left out, and nothing of it reaches out of the machine.

**The ledger, a facade over its store.** Every caller holds a
`ledger::Ledger`, as it did when that was SQLite itself, and asks it in
core's words; none writes SQL. It holds a `Box<dyn LedgerStore>` and hands
each call to it through `Deref`, so the methods are declared once, on the
port, and no caller changed when they moved there. The two things a trait
object cannot carry stay on the facade: the constructors, which open the
SQLite store, and `relinked`, whose write is of any type, run by the
store as an erased one (`LedgerStore::relinked_by`, a write that says
whether to keep what it moved). The store's own tests are the adapter's
(`adapter/sqlite_test.rs`), since they reach its connection to make a row
an older build wrote; what any store must do is held by the suites that run
core through the facade.

**The driving side.** A front end drives core through its functions, and
every change through the command lines every front end runs
(`commands::run_line`, [every change is a command
line](#every-change-is-a-command-line)), which is already a port: the TUI,
the panel's backend and the CLI itself are its adapters. A new interaction
module, an MCP server or a desktop shell, is one more: it builds the same
`Task` and runs its lines, as [D-18](#d-18-the-panel-writes-by-running-commands)
and [D-24](#d-24-the-local-server-is-a-versioned-http-api) keep every
writer doing, and gains every rule with no change to one.

## The kinds, and how a kind is described

An artifact is a skill, a rule or an MCP server's definition, and the
difference is a **payload**, not a field:

```rust
pub enum Declared {
    Skill(SkillManifest),
    Rule(RuleManifest),
    Mcp(McpDefinition),
}
```

`Declared` *is* the kind, so an artifact cannot carry a discriminant that
disagrees with what was parsed: there is only one. Each payload says what its
kind requires: a skill without a description is not a state the type can hold,
a rule's front matter is optional all through, and no `Option` field on
`Artifact` sits permanently `None` for one kind because only the other has it.

Where a kind lives and what shape it has is a second, smaller thing:

```rust
pub enum Layout {
    /// One directory per artifact, holding a manifest with a fixed name.
    Directory { manifest: &'static str },   // skills
    /// One file per artifact, at any depth.
    Files { extension: &'static str },      // rules
    /// One file per artifact, directly in the kind's folder.
    Flat { extension: &'static str },       // MCP servers
}
```

Discovery matches on `Layout` and nothing else has to know the difference. The
shapes decide the nesting rule between them: a skill's directory belongs to
the skill, assets and all, so walking into it would turn its own files into
artifacts; skills are therefore flat. A rule is a loose file, so folders under
`rules/` are free to group, and a rule's name is its **route** without the
extension. `git/no-force-push` and `ci/no-force-push` are two rules; the stem
alone would make them one name for two files. An MCP server's name becomes a
key in each agent's config file (D-10), which holds no folder, so its file
sits directly in `mcp/` and its name is the stem: a definition further down
would be listed under a name no install could key it by, or under its stem
beside another of that stem, so a folder below is never walked.

`Kind` is a closed enum. Every kind ships in this crate, so every `match` is
exhaustive and the compiler is the checklist for adding the next one. A
registry that took kinds at runtime would trade that for an extensibility
nobody has asked for.

### Why a qualifier uses a colon

`skill:commit-style`, not `skill/commit-style`. A rule's name *is* a route, so
the two namespaces would otherwise share a delimiter: `rules/skills/naming.md`
is the rule `skills/naming`, and with a slash qualifier that string would parse
as "the skill `naming`": a listing printing a name its own `show` rejects, or
worse, resolves to a different artifact. A rules folder grouping rules *about
writing skills* is not an exotic thing to have.

A colon is not a path separator anywhere, and on Windows a filename cannot
contain one at all, so the collision is gone rather than narrowed.

### Where a name comes from

From wherever it is declared. A skill declares one in its front matter, so that
is its name and disagreeing with its directory is a problem `validate` reports.
A rule's front matter has no name in it (a `name:` there passes through like
any key Mindweaver does not read), so its name is its route. This is why the
two are not symmetric, and the asymmetry is the honest one.

### What a listing shows for a rule

A skill has a `description`. A rule has its body: the file, less any front
matter. The body's **opening line** (the first line carrying any text, with
leading `#` stripped) is what a listing shows, and it is captured when the file
is loaded, because the file was read anyway. A rule without front matter is
summarised exactly as it always was.

Not the file's opening line, which for a rule with front matter is a fence, so
every scoped rule would be listed as `---`. And not a `description` the rule
declares either: the body is what an agent is given once the front matter is
taken off, which is how a rule reaches the `AGENTS.md` block (#42), so its
opening line is what the agent meets first, and a listing that shows something
else would describe the rule by a line no agent reads.

That one value does double duty: it is the summary a listing prints, and its
absence is the one thing that makes a rule invalid, since a body with no line of
text has nothing to offer an agent. Storing the fact rather than the verdict is
what keeps `validate` pure: every check it makes was decided when the file was
read, so asking whether something is valid cannot itself fail.

### What a skill declares

A skill's front matter is the Agent Skills format
([agentskills.io/specification](https://agentskills.io/specification)), the
one Claude Code, Codex and OpenCode all read, and `SkillManifest` holds the
fields it defines and no others:

| Key | | Held to |
|---|---|---|
| `name` | required | lowercase letters and digits joined by single hyphens (`^[a-z0-9]+(-[a-z0-9]+)*$`), at most 64 characters, and the same as its folder |
| `description` | required | 1 to 1024 characters |
| `license` | optional | |
| `compatibility` | optional | 1 to 500 characters |
| `metadata` | optional | a map from strings to strings |
| `allowed-tools` | optional | tools separated by spaces |

Every other key (Claude Code's `paths` and `disable-model-invocation`, say)
belongs to an agent, and passes through: not modelled, not rejected, and kept
byte for byte by everything that writes a manifest, since `rename` rewrites one
line and gathering and installing copy files. Modelling them would make this
crate the place where three agents' formats are reconciled, which is not a job
anybody asked it to take on.

`allowed-tools` is split on spaces, as the specification writes it, and on
commas, as Claude Code first wrote it and older skills still do, but never
inside parentheses, so a permission pattern such as `Bash(git add *)` stays one
tool. A YAML list is read too.

`metadata` is read leniently. The specification makes it a map of strings, but
a shipping agent writes nested maps and booleans in it
([agentskills#514](https://github.com/agentskills/agentskills/issues/514)),
and failing the whole manifest over one entry would leave the skill out of
every listing. So the string entries are read, the others are named for
`validate` to report, and nothing is rewritten: what is ignored here is still
in the file for whichever agent wrote it. `compatibility` is read the same way,
since a list of the agents a skill was made for is the obvious thing to write
in place of the specification's sentence: a value that is not a string is
ignored and reported. Neither builds what it ignores, only skips it, and
`metadata` is read an entry at a time rather than into a map, so a key written
twice is reported (the first is read) instead of failing the manifest, as a map
would.

The name rule is held to every name, a rule's route segment by segment
included. A looser one for rules was the alternative, and it would only have
been a second rule to keep in step with the first.

### What a rule declares

Most rules declare nothing: a rule is a markdown file that gives context.
Claude Code's rules can open with front matter saying which files they are
about, and [D-03](#d-03-rules-reach-every-agent-through-agentsmd) sends a rule
that does to `.claude/rules/mindweaver` only, so a rule may carry some.
`RuleManifest` holds the two keys Mindweaver reads, both optional:

| Key | | Read as |
|---|---|---|
| `paths` | optional | glob patterns: a list of them, or one on its own |
| `description` | optional | a string |

Rules were written long before any of them was read this way, and markdown is
free to open with a `---` horizontal rule. So a fenced block at the top is
front matter only when it holds a YAML mapping. Anything else is content, as
it always was: an opening fence that never closes, prose between two
horizontal rules that YAML reads as a plain string, a list, two horizontal
rules with nothing between them, and a
block of nothing but comments too, since what YAML reads past as a comment is a
heading to markdown. An empty block could have been front matter that declares
nothing, but taking its fences off would drop two lines that read today and
gain nothing. The block is parsed as a YAML value before anything is read from
it, so that what it *is* decides whether it is front matter; deserializing
straight into a struct would have to tell a sentence from a broken mapping by
the wording of its error.

A block that does not parse at all is content too, and `validate` warns about
it, since it is most likely front matter written wrong: a glob opening with `*`
is a YAML alias unless it is quoted. Refusing the rule, as a skill whose front
matter does not parse is refused, was the alternative. It would hide from every
listing a rule that still works as text, over how its first lines are written;
a skill without its front matter has no name to be found by, and a rule never
needed one.

The line is YAML's, not the author's intent, so prose between two horizontal
rules can land on either side of it. A sentence shaped like `Note: never
force-push` is a mapping with one key nothing reads, so it is front matter and
left out of the body an agent is given, without a word. One opening with `**`,
a backtick or a link does not parse, so it is content, with the warning above.
Both are rare at the top of a rule. Warning about a mapping that declares
neither `paths` nor `description` would catch the first, and was turned down:
it would warn just as loudly about every rule carrying only another agent's
keys, such as Cursor's `globs`, which pass through by design.

The keys are read leniently, as a skill's `metadata` and `compatibility` are: a
`paths` that is not a pattern or a list of them, or a `description` that is not
a string, is ignored, reported as a warning and left in the file. A list
holding anything but strings is ignored whole, since keeping its strings would
scope the rule to fewer files than its author wrote. One pattern is never split
on commas, because a brace expansion such as `src/**/*.{ts,tsx}` has one inside
it. Every other key (Cursor's `globs`, a `name`) passes through, as it does on
a skill.

A key with nothing in it is unstated, as `paths:` with no value is: a blank
`description`, and a `paths` that is a blank string or an empty list. A blank
pattern matches no file, so one in a list is left out and the rest read. This
matters for #42 and #43, where a rule that states `paths` goes to
`.claude/rules/mindweaver` only: read as stated, `paths: []` would scope a rule
to no file, which is a rule no agent is ever given, and `show` would print an
empty description or a `paths:` heading with nothing under it. Warning about
an empty key, as `compatibility: ""` is warned about on a skill, was the
alternative; but a skill's `compatibility` is shown to an agent as written,
while an empty `paths` says nothing an agent could act on.

`Artifact::contents` is the body without the front matter, for a rule as for a
skill. It is what a renderer puts in front of an agent, so the fences are never
pasted into `AGENTS.md`, and what `show` prints below the description and the
`paths` a rule declares. Claude's rule files are the one place a rule goes
whole, fences and all, since Claude reads `paths` from them ([Claude's rule
files](#claudes-rule-files)). Whether a rule is about some files only is
`RuleManifest::is_scoped`'s to say, `paths` stated in the shape it is read in,
so the block and those files never disagree on it.

### Strict when writing, lenient when reading

`Artifact::validate` gives one list of problems, and it is treated three ways:

- **Refused** when Mindweaver writes a skill into a project, if the name or
  the description breaks the rules. `mind add`, `rename` and `install` all
  ask `artifact::strict_issues`, which keeps what `ValidationIssue::is_strict`
  picks out of that list, for the skill as it is about to be written, so one
  change to the set reaches all three. So does `mindweaver adopt`, whose library
  copy is a write, and whose project copies become what an install would
  have written ([adopting what a project already
  keeps](#adopting-what-a-project-already-keeps)). `rename` asks too, although the name is
  all it writes, because what it leaves behind is a skill Mindweaver wrote;
  `mind add` checks the name and the description before building anything as
  well, only for messages of its own. Those are what agents refuse: Codex skips a skill whose name is over 64
  characters or whose description is empty, so writing one would put a skill
  in the project that never loads. So is a name that is not the folder the
  skill is filed in, since an agent finds a skill by one or the other (asked
  of the folder Mindweaver is about to write, not the one the files were read
  from). `compatibility` and `metadata` are not refused, because an agent reads
  past them. A rule `mind add` writes from a template is refused for whatever
  its front matter would leave unread (`artifact::rule_issues`), which reading
  only warns about ([Templates live inside the
  markers](#templates-live-inside-the-markers)). `edit --contents` writes
  what it is given whole, so it asks the same of it before a byte is
  written: a skill's through `artifact::strict_issues`, a rule's through
  `artifact::rule_text_issues`, which is `rule_issues` and having nothing to
  say ([contents given whole](#contents-given-whole)). An MCP server's
  definition `add` writes, from a command line or a template, is held to
  everything `validate` would call it invalid for
  (`artifact::mcp_strict_issues`), as `rename` and `edit --contents` hold
  one ([adding one](#adding-an-mcp-server)). An editor's edit is
  not refused anything: the file is written by the time Mindweaver sees it,
  and `edit` says what it broke.
- **Warned about** when gathering, which takes the skill anyway, as the
  agentskills.io client guide advises for skills written for other agents. The
  shelf is a record of what a source holds, and leaving a skill off it would
  hide the problem along with the skill.
- **Reported** by `validate`, in full, and at the workspace level for the shelf
  too, so what gathering warned about can be found again later. Only a
  problem makes an artifact invalid and the exit non-zero; what an agent reads
  past (`ValidationIssue::is_warning`, the same `compatibility` and `metadata`
  that are not refused, and whatever a rule's front matter leaves unread) is
  listed as a warning and leaves the verdict at `ok`.
  Counted as invalid, a skill installed with nested metadata would fail its
  project's `validate` from then on, and the only way to clear it would be to
  edit the copy, which leaves it edited, never updated again unless forced.
  The TUI's `✗` follows the same line.

### What an MCP server's definition declares

One file a server, `mcp/<name>.toml`, in TOML, the format the markers are
written in, and in a format of its own, `format = 1` (`MCP_FORMAT`), since a
definition is copied and loaded with no marker beside it to say what it is.
It is read by `McpDefinition::parse`, in `apps/core/src/mcp.rs`, which reads
`format` before anything else, so a definition in a newer one is refused as
that (`mcp_format_too_new`) whatever else it holds, rather than as a shape
this build does not know:

| Key | | Read as |
|---|---|---|
| `format` | required | `1`; a newer one is refused, one never written too |
| `name` | required | the server's name, held to its file's stem by `validate` (`name_file_mismatch`), to the keys every agent takes and to what each keeps for itself ([which names a server is given](#which-names-an-mcp-server-is-given)) |
| `description` | required | what it is for, its first line what a listing shows |
| `transport` | required | `type = "stdio"` with `command`, `args`, `env` and `cwd`; or `"http"` or `"sse"` with `url` and `headers` |
| `inputs` | optional | each a `name`, a `kind` (`env`, `header`, `arg` or `url_var`), and optionally a `description`, `required`, `secret` and the `env_var` it is read from |
| `provenance` | optional | `registry`, `upstream_name`, `version` and `fetched_at`, as a registry said them |
| `extras` | optional | a table for each agent, by its name, passed through for its renderer |

Neutral rather than one agent's shape, and a subset of the MCP registry's
`server.json` rather than that file as it is (D-10): an agent's own shape
would make every other agent's a translation of it, and `server.json` holds
what no agent reads and none of what an input needs to say. An input is
what somebody supplies when an agent starts the server, never a value the
file holds (D-11): it is written between braces where its value goes,
`{token}`, as the registry writes a variable, and its kind says where that
may be (a value of a program's `env`, one of its `args`, a value of an
address's `headers`, or its `url`). A brace naming no input is left as it is
written, so a JSON argument means what it says. Each renderer turns a
reference into its agent's own syntax (#90 to #92); D-11's words for what
an agent cannot express, a secret argument or one inside a URL, are the
kinds of input this names.

Beside its name and its secrets ([a secret is an environment
reference](#a-secret-is-an-environment-reference)), `validate` holds a
definition to its format, each a problem it reports rather than a file
refused, so the definition is still listed: the file reads, the transport
names what it needs to start or reach the server (`transport_incomplete`),
every input is named so that its placeholder can be written
(`input_name_malformed`) and, read from a variable, from one every agent
can refer to (`env_var_malformed`), and every input is used where its kind
says (`input_unreferenced`), once (`input_repeated`). A key the format does
not define is read past, as a skill's front matter's is.

An address is held as a `value::Redacted`, as every URL core keeps is, so a
credential typed into its userinfo is not kept. An input written there went
with it, so `validate` says it is written where no agent is given it
(`input_in_userinfo`), never that it is used nowhere while telling where it
already is: the names of the inputs only its userinfo held are kept beside
the address (`Endpoint::withheld`), read off the text as written before it
is redacted, and none of the text around them. An input's name and the
variable it is read from are `value::InputName` and `value::VariableName`,
named apart, read whatever they hold, and judged by a rule each says of
itself and is not made by. An input's name is letters, digits, hyphens and
underscores (`InputName::is_writable`), the alphabet a server's name is
held to, so a `}` never ends a placeholder early nor a `{` opens another;
and a variable's is the name POSIX gives one (`VariableName::is_portable`),
the one every agent's reference holds alike, where Claude's `${VAR}` reads
`:-` after a name as a default and OpenCode's `{env:VAR}` ends at a `}`.

No escape is needed for a value that has to hold an input's placeholder
as text, `{token}` while an input is named `token`, and format 1 has none:
an input's name is the definition's own to choose, so such a value names
its input otherwise, and since an input's name holds only those
characters, a brace around anything else (`{"debug":true}`) is never one.
An escape would be a new format, which an older build would read as text;
what stays open is an import that cannot choose
([open questions](#open-questions)). The extras are kept as the TOML they
are, which has floats, so `Declared` and `Artifact` are `PartialEq` and no
longer `Eq`: holding them as text to keep `Eq` would hand a renderer a
string to parse again.

`show` prints a definition's description, then how the server is started or
reached (`transport:`) and what is supplied for it (`inputs:`), each laid
out by `reading::ShownTransport` and `reading::ShownInput`, then how each
agent is given it ([what each agent cannot be
given](#what-each-agent-cannot-be-given-of-a-server)). Not its file, as a
skill's body is printed below its front matter: every key an agent is
given is laid out above, the file is at the path heading it, and `--json`
holds it whole as `body`. A definition is written by `add` from how a
command line says the server is started or reached, or from a template
([adding one](#adding-an-mcp-server)), and installing one writes an entry
into each agent's config file ([installing an MCP
server](#installing-an-mcp-server)); gathering one and adopting one wait.
`add`, `edit --contents` and `rename` hold a definition to every problem
`validate` reports (`artifact::mcp_strict_issues`, a name an agent only
uses for its own being a warning and no problem), its name to its file as
a skill's is to its folder, and what sits beside it to what is in the
folder it is written in, and rename rewrites the `name` value through
`toml_edit`, the comment on its line kept (`mcp::declaring`, which `add`
sets a template's `name` and `description` by).

### Which names an MCP server is given

A server's name is the key each agent's config names it by, and its
definition's file (D-10), so it is held to what every agent keys a server
by, `^[A-Za-z0-9_-]+$` (`mcp::name`), and to the 64 characters every name
is: wider than the kebab-case of a skill's and a rule's names, which #86
held a server's to until #87 said which names an agent takes. Kebab-case is
what an agent is asked to invoke a skill by; nobody invokes a server by its
name, and kebab-case would call `GitHub` or `my_server` invalid for a rule
no agent has. `rename` and `add` refuse a name through
`artifact::mcp_name_strict_issues`, the strict half of what `validate` says
of it. An install's filing asks less
(`install::is_installable_name`, through `mcp::name::is_key`): that the name
is a key every agent takes and one file's name, never whether an agent keeps
it for itself, so a reserved name is refused by `validate` and by whatever
writes the definition, not by the filing (#93, which installs one, decides
whether it asks more).

Two names that differ only by case are one name. A file system that folds
case, macOS's and Windows' by default, cannot hold `GitHub.toml` beside
`github.toml`, so a library holding both is another library on a
teammate's machine, though an agent's config would key the two apart. So
discovery tells each definition of the others in its folder named as it is
in another case, and `validate` calls each invalid (`mcp_name_case_clash`);
`add`, `rename` and `edit --contents` read the same off the folder they
write in (`lifecycle::twins_beside`), a rename that changes only a name's
case excepted, and refuse it through the one strict function, a draft is
judged as it would be once written, and a definition loaded on its own,
as `edit` loads the file its editor closed on, is told the same
(`lifecycle::load`), so that `edit`'s verdict is `validate`'s. Listing the
first of the two and passing over the other was the alternative, and would
hide a server the moment somebody added its twin.

Neither change repairs a pair of twins that each hold another problem as
well (a token written as it is, a secret naming no variable).
`edit --contents` keeps the file's name, so contents that fix the other
problem are refused for the clash they cannot remove, and `rename` holds
the definition it renames to every problem `validate` reports, so it asks
for the other one fixed first. No command makes such a pair, which comes
of a hand edit or of a pull from a repository written where case is not
folded, so it is repaired as it was made: in an editor (`edit` without
`--contents`, which refuses nothing, since the file is written by the time
Mindweaver sees it, and says what is left), by hand, or by removing one. A
skill whose folder a hand edit named against the rules, its description
empty too, is in the same place. Letting `edit --contents` pass over a
clash its contents neither add nor could remove was the alternative, and
would write a definition `validate` still calls invalid, which strict when
writing refuses; the panel, which has no editor, is left with removing one
([open questions](#open-questions)).

A name an agent keeps for itself is refused (`mcp_name_reserved`), in any
case, since two names that differ by case alone are one: Claude Code's
`workspace`, `claude-in-chrome` and `computer-use`, and the names colliding
case-insensitively with its `Claude Preview` and `Claude Browser`; and
OpenCode's `servers` and `timeout`, keys its `mcp` table holds beside the
servers its V1 shape keys there (D-12). `Claude Preview` and `Claude
Browser` hold a space no key can, so the rule as #87 wrote it would hold
nothing: Mindweaver takes them to cover each spelling of that space a key
can hold, an underscore, a hyphen or nothing (`claude_preview`,
`claude-preview`, `claudepreview`), in any case. Which of those Claude
compares is not known, since none of #18's checks asks it, and holding all
three costs no name anybody would give a server of their own; a hyphen
written as an underscore elsewhere is another name (`claude_in_chrome`).
`anthropic-skills`, the name Claude gives the skills Anthropic provides, is
only warned of (`mcp_name_claimed`, a `ValidationIssue::is_warning`): Claude
refuses no server by it, and a definition named so is valid. Each agent's
names are a table matched on the agent with no wildcard, docs-only in
[docs/agents.md](agents.md#mcp-servers).

### A secret is an environment reference

D-11 says a secret input must be an environment reference, and #86's inputs
hold no value: one is supplied when an agent starts the server, and never
written in the file. So what the rule asks of the model is that a secret
input name the variable it is read from (`env_var`), which every agent's
reference will name, Claude's `${VAR}`, Codex's `env_vars`,
`env_http_headers` and `bearer_token_env_var`, OpenCode's `{env:VAR}`: one
naming none could reach an agent only as its value written into the entry,
and is invalid (`secret_without_env_var`). An input that is no secret may
name none, its value being somebody's to give when it is installed (#89),
and written as it is.

The other way to hold a secret in a definition is to write it where an
input would go. A value of the transport's `args`, `env`, `headers` or `url`
that holds what looks like a token outside an input's placeholder is
invalid, with a code of its own (`secret_literal`): a `Bearer` value, or one
starting `sk-`, GitHub's `ghp_` (and `gho_`, `ghu_`, `ghs_` and `ghr_`, its
other tokens' prefixes) or `github_pat_`, long enough and at the start of a
word, an `sk-` key holding a digit as one drawn at random does and a
package named in words does not (`mcp/token.rs`). It is a heuristic, and
says so: no rule tells a credential from text shaped like one, and a false
alarm costs a definition somebody can still write with an input, where a
miss costs a token committed with the project. A value an input is written
into is judged without its placeholder, so `Bearer {token}` holds none, and
a brace naming no input is text, judged as text. The issue says where the
token is, by its key path (`transport.env.API_KEY`, `transport.args[4]`),
and what it starts with, never the rest, in its message and its JSON, since
the message goes wherever `validate` is read. An address is judged as it
was written, before it is redacted, as the inputs its userinfo held are
(`Endpoint::tokens`, beside `withheld`), so a token typed as an address's
user is found too, and kept as its prefix alone. Every token a test needs is
put together at run time from parts (`McpMother::tokens`), so no file in
the repository holds one and no secret scanner stops a push.

Two places beside the transport reach an agent's entry, and are judged the
same way. An input's `env_var` is written into each agent's reference, and
the value pasted where its variable's name goes is the likeliest way a token
reaches the file: a GitHub token is letters, digits and underscores, so it
passes for a variable. One there is `secret_literal` at `inputs[N].env_var`,
said instead of `env_var_malformed`, whose message names the variable whole.
And the extras are passed on to each agent's entry as they are, so every
text they hold, at any depth, is judged as a value of the transport is
(`extras.claude.headers.X-Api-Key`, `extras.codex.args[1]`). So is an
input's name, though a renderer turns it into a reference and never writes
it as text: `add` names an input after the variable it is told to read it
from, so a token pasted there lands
in the name as surely as in `env_var`, and the file holds it. One there is
`secret_literal` at `inputs[N].name`, and nothing else is said of that
input until it is renamed, since every other word about it (declared twice,
used nowhere, read from no variable) would print its name whole. Still
left out are a program's `command`, which names one, its `cwd`, the keys of
its `env` and an address's `headers`, and the keys that describe the
server rather than reach it, its description and provenance ([open
questions](#open-questions)).

### What each agent cannot be given of a server

A definition every agent reads alike is not one every agent can be given:
D-11 refuses an agent a shape it cannot express rather than write a literal
for it. So core says, for each agent, what a definition holds that the
agent cannot be given as it is written (`McpDefinition::inexpressible`),
matched on the agent with no wildcard, so a new one says what it cannot
express before it compiles. Each is an `Inexpressible`, with a code of its
own, which the renderer for the agent (#90 to #92) asks before it writes
anything, and refuses the agent by or writes the entry without
(`Inexpressible::refuses`):

| Agent | Cannot be given | Then |
|---|---|---|
| Codex | a secret argument (`secret_in_argument`) | refused |
| Codex | a secret inside a URL (`secret_in_url`) | refused |
| Codex | a secret header beside other text, but `Authorization: Bearer` and the token (`secret_header_composite`) | refused |
| Codex | a secret in a program's environment beside other text (`secret_env_composite`), or as the whole value of a variable named otherwise than the one it is read from (`secret_env_renamed`) | refused |
| Codex, OpenCode | server-sent events (`sse_unsupported`), which OpenCode's V2 shape, the one D-12 writes, does not take | refused |
| Claude Code | a program's folder (`cwd_unsupported`) | written without it |
| Claude Code | an input of an address read from a variable it reads as empty toward a remote server (`variable_blanked`) | refused |
| Claude Code | `${` written as it is in a command, an argument, the environment, the URL or a header, where no input stands (`text_expanded`) | refused |
| Claude Code | an extra for it giving a key its entry is written with from the transport, `type`, `command`, `args` and `env` or `url` and `headers` (`extra_repeated`) | refused |
| Codex | an extra for it giving a key its entry may be written with from the transport, `command`, `args`, `env`, `env_vars` and `cwd` or `url`, `http_headers`, `env_http_headers` and `bearer_token_env_var` (`extra_repeated`) | refused |
| Codex | an extra for it giving `experimental_environment` or `environment_id` (`extra_withheld`) | refused |
| OpenCode | `{env:` or `{file:` written as it is in a command, an argument, the environment, the folder, the URL or a header, where no input stands (`text_substituted`) | refused |
| OpenCode | an extra for it giving a key its entry may be written with in either of its shapes, `type`, `command`, `enabled`, `environment` and `cwd` or `type`, `url`, `enabled` and `headers` (`extra_repeated`) | refused |

Codex has no interpolation, so a secret reaches a server there only through
the keys that name a variable, each in one shape: `env_vars` for a
program's environment variable that is the secret whole, under the name of
the variable it is read from, since `env_vars` passes a variable on as it
is; `env_http_headers` for a header that is the secret whole; and
`bearer_token_env_var` for `Authorization: Bearer` and the token, the first
header so named. The shapes are `codex_mcp`'s (`passed_on`, `bearer`), as
the keys of Claude's entry are `claude_mcp`'s, and the report asks them of
every place a secret is written rather than of its kind: a placeholder
stands for its input wherever it is written, and `validate` asks only that
it stands where its kind says, so a secret of a program's environment
written in an argument as well is a secret argument. A registry's
`GITHUB_PERSONAL_ACCESS_TOKEN = "{token}"`, read from `GITHUB_TOKEN`, is
refused Codex for it, where `add --secret-env` writes the variable under
its own name. Claude expands a
reference everywhere, but toward a remote server it reads as empty the
credentials of its own and of the tools on a machine (`ANTHROPIC_API_KEY`,
`NPM_TOKEN`, `HTTPS_PROXY` and the like), so that a repository's
`.mcp.json` cannot send them to a server of its choosing: an input of an
address read from one would reach its server with nothing in it, so the
server is refused Claude rather than written for it to fail. Which names
those are was read from Claude Code 2.1.318's binary, docs-only, and is a
table of its own (`mcp/blanked.rs`): the names Claude keeps from a remote
server in every environment, in any case and under a workflow input's
`INPUT_` where Claude looks there, and the shapes it keeps whatever they
are called (`OTEL_`, git's `GIT_CONFIG_*`, a Cargo registry's token). A
program's environment is no remote server's, and is given whole. And
since Claude expands `${VAR}` and `${VAR:-default}` wherever it expands an
input's reference, text a definition writes there as it is, `${HOME}` in an
argument, would reach the server as a variable's value, a default or
nothing, where a brace naming no input means what it says: any `${`
outside an input's placeholder refuses the server Claude, rather than only
what 2.1.318's pattern takes (a name, then `}` or `:-`), since a reference
written for an input after it may close what it opens. A `$` just before a
placeholder is the `$` and the value, and stays. OpenCode fills `{env:NAME}`
in over its config file's text before it parses it, as #18's runs saw in
the command, the environment, the URL and the headers, and its docs say
`{file:path}` too, so a definition's `{env:HOME}` written as it is reaches
the server as a variable's value or nothing: any `{env:` or `{file:`
outside an input's placeholder, in any value its entry writes, the folder
among them, refuses the server OpenCode, for the reason any `${` refuses
it Claude. Every agent's extras go beside its entry, never over a key the
entry writes (`claude_mcp::keys`, `codex_mcp::keys`, `opencode_mcp::keys`),
which would leave one of the two lost; Codex's are held to every key its
entry may be written with, since it writes each only when it holds
something, and OpenCode's to every key either of its shapes may be written
with, since which shape a server is given is its file's to say, so what
`show` says does not turn on which a definition fills or where it goes.
OpenCode's are held to V2's `disabled` too, which no entry writes, a
definition having no switch for it: it is V2's word for the `enabled` V1's
shape is written with, and beside it would mix the two shapes in one entry.
Nor do Codex's say the environment it
runs a server in (`experimental_environment`, `environment_id`,
`codex_mcp::WITHHELD`): an entry of Mindweaver's is for a server the
project's own machine starts or reaches, as every other agent's is. `show` says every
agent's (`McpDefinition::expressibility`) between the description and the
file, and `show --json` the same as `expressibility`, each shape by its code
under `refused` or `dropped`, and `null` for a skill or a rule: a name
added, so no version moved.

An input read from no variable is not among them, though every agent's
renderer refuses it (`claude_mcp_input_unread`, `codex_mcp_input_unread`,
`opencode_mcp_input_unread`): every agent could be given its value as it is,
and none is given it only because no install hands a renderer one, which
#95 left as it was (in the open questions). Said here, `show` would call every agent
refused a server each of them can take. Nor is an input that is no secret
written where Codex reads no variable, an argument read from one
(`add --arg-env`), a part of the address (`--url-var`): Codex's renderer
refuses it (`codex_mcp_input_in_text`) for the same reason, a value it
could be given and is not handed.

None of it is `validate`'s. A shape one agent cannot express says nothing
wrong with the definition, which every other agent is given whole, and a
server that speaks only sse is no less a server for Codex having no way to
reach it. Called invalid, it would fail a project's `validate` for a server
nobody meant for Codex; warned of, it would be said of the same server on
every `validate` for as long as it is kept, with nothing its owner could
change but the server. So it is said where an agent is about to be given
it, by its renderer, and by `show` whenever asked.

### Adding an MCP server

`mindweaver add mcp NAME DESCRIPTION` (`mind add mcp` in a project whose
settings name a folder for servers) writes a definition from how an agent
starts the server, `--command` with `--arg`, `--env K=V`, `--secret-env
VAR` and `--arg-env VAR`, or where it reaches one, `--url` with `--sse`,
`--header K=V`, `--secret-header H=VAR`, `--bearer-env VAR` and `--url-var
NAME=VAR`. Core is told what they say as an `mcp::Given`, a program or an
address and never both, and writes the definition's text from it
(`Given::text`): a template of its own, made with `create_new`, its comments
saying what each part is for, as a marker's say what each key is, and
every value quoted as TOML quotes one, so a value holding a quote or a line
break reads back as typed.

Every value is a name or a literal the definition holds as #86's model
does. `--env` and `--header` write a value as typed, judged as every value
of a transport is, so one shaped like a token is refused. Every other
option declares an input read from the variable it names, never given a
value (D-11), and named after that variable, so its placeholder says which
one (`{GITHUB_TOKEN}`). Which of those is a secret is said once, in
`given.rs`: what reaches a server through its environment (`--secret-env`,
the variable passed on under its own name, the shape Codex's `env_vars`
gives one) or a header (`--secret-header`, the header's whole value,
Codex's `env_http_headers`; `--bearer-env`, `Authorization: Bearer` and the
token, Codex's `bearer_token_env_var`) is; what goes into its arguments
(`--arg-env`) or its address (`--url-var`) is not, being what a process
list, a log or a proxy shows, and what Codex reads no secret into. A secret
argument, or one in an address, is written by hand, and `show` says Codex
is given none of it. `add` says nothing of whether an input is `required`,
which it cannot know.

What no definition can hold is the grammar's to refuse, before core is
told anything: both transports, an option of one with the other or with
neither, and a transport, or any option of one, with `--template`, which
says how a server is started as surely. Each option of one transport
conflicts with the other's and with `--template` as well as requiring its
own, since clap passes over a requirement that conflicts with something
given, and would take `--header` with `--command`, or `--arg` with
`--template`, and drop it; a `Given` cannot say any of it, so core needs no
rule for it. An option taking `NAME=VALUE` given no `=` is refused by a
parser of its own (`parse::Pair`), which never says what it was given:
clap says a value it refuses whole, and one with no `=` is as likely a
secret whose name was left out, or a header written as curl takes one,
`Authorization: Bearer` and the token. Everything else is core's (`create::plan`): what `add` writes
is held to the strict function `validate`, `rename` and `edit --contents`
hold a definition to, before a byte is written. Its name, to the keys
every agent takes, to what each keeps for itself and to the definitions
beside it named as it is in another case (`add_name_unfit`), then the
definition as it would read (`add_mcp_unfit`, every issue `validate` would
call it invalid for, a token said by where it is and what it starts with,
never whole): `add` refuses exactly what `validate` would call invalid,
with no copy of a rule. Four things beside it are no definition's to
judge, and refused with codes of their own: a transport given for a skill
or a rule (`add_transport_not_mcp`); a key of `env` or `headers` given
twice, by two options or one twice, which no TOML file holds
(`add_mcp_key_repeated`, as given, so two headers in another case are two
keys, as the file and `validate` hold them); a name given beside a value
that holds `=` (`add_mcp_key_unsayable`, `Given::unsayable`), which no
command line gives, since each option of a pair ends the name at the
first one, but a request can, and its line would then say another pair
than the one its draft was judged as, so the path policy refuses such a
task before its line is built (`policy::sayable`, by the same code); and
an address typed with something before its host (`add_mcp_url_userinfo`,
by `url::carries_credentials`, the test a gather refuses a URL by), which
the file would keep though the definition never does, said without it.
Since no definition holds a key or an address so for `validate` to
judge, each is looked through
for a token as `validate` looks through a value (`mcp::token_in`): a key
that looks like one is said by what it starts with, and so is an address
whose query or path still holds one once its userinfo is gone, said as the
address alone. An `add mcp` given nothing to write a transport from is `add_mcp_unscaffolded`, the code #86 gave that
condition, which now says what to give rather than to write the file by
hand: a code is never moved to another condition, and this is the one it
named.

A template for a server is a definition of its own, `<template>.toml` in a
holder's templates folder, found as every template is ([templates live
inside the markers](#templates-live-inside-the-markers)). `add mcp NAME
DESCRIPTION --template T` takes its transport and its inputs as they are
and sets its `name` and `description` through `toml_edit`, every other byte
kept, comments included, then reads it back to make sure only those two
changed (`mcp::declaring`, the rewrite `rename` makes of a name), and holds
the result to the same strict function. No `{{name}}` is filled in a
server's template, as a skill's body is: a placeholder pasted into TOML
breaks it at the first quote a description holds, and none of a server's
values needs its name. None is built in.

The TUI's add form steps through the kinds `add` writes from a name and a
description alone (`create::scaffolds`), which a server, needing how it is
started, is not: a form of fields for a transport is a form of its own, and
offering the kind with a name and a description would offer a choice only
to refuse it. The panel's MCP page has that form (#97, [its
views](#its-views)), and its task is the same `add` line told the options
(`Task::Add`'s `server`, an `mcp::Given`), each value after its option as
every value of a task is; its draft is judged as the line would write it
(`draft::added` told the `Given`).

`edit` changes how a server is started or reached from the same options
(`mindweaver edit mcp:<name> --command ...`, `Editing`'s `server`, the
`ServerOptions` `add` flattens): its `transport` and `inputs` are written
again as `add` writes them, and every other key of the file, its name,
description, provenance and extras with their comments, is kept, through
`toml_edit` (`mcp::retransported`, the new tables put where the old
transport stood), then held to every rule contents given whole are
(`lifecycle::retransport`, through `replace_contents`), after `add`'s own
checks of a credential in the address and a key given twice
(`create::writable`), so the two refuse alike. Refused for another kind
(`edit_transport_not_mcp`), and for a definition that does not read
(`edit_server_unreadable`), whose transport cannot be told from the rest.
The options rather than a file of TOML, so a front end with a form sends
what the form asks and never writes the format itself; and the transport
and inputs whole rather than an option at a time, so a form shows the
server as it is and sends it as it should be. What it starts from is the
definition read back as the options would say it (`Given::of`), none when
it holds what no option says, an input described or required, a program's
folder, a secret under a name of its own, or an argument read from a
variable before one written as it is: a form starting from part of it
would write the rest away, so such a definition is changed as text. It
reads `env` and `headers` back in the order of their keys, the order the
definition holds them in, which is no order an agent reads, so options
saying what the file's `transport` and `inputs` hold already, as TOML
reads their values, leave the file as it is, and `edit` writes nothing:
a form changed in nothing would otherwise rewrite a file `add` wrote in
the order typed, and say it was edited. The grammar refuses the options beside an editor or contents, as beside a
template, through the group clap gives `ServerOptions`, which `--template`,
`--editor` and `--contents` each conflict with.

### Adding a third kind

A variant on `Kind`, its folder and layout, a variant on `Declared`, a
constructor on `Artifact`, and an arm in every match on the kind (discovery
and discovering the shelf, `create`, `lifecycle::load` and `lifecycle::rename`,
what `lifecycle::replace_contents` holds contents to, the refusal in
`gather`, what `install` holds a copy to, and where the
[target table](#agents-and-where-each-kind-reaches-them) puts it for each
agent), which the compiler lists, since the matches are exhaustive. Reports need nothing:
columns, labels and counts are all driven from `Kind::ALL` and from what the
catalog actually found.

MCP servers were the third (#86), and the first whose payload is no folder
and no file an agent reads, so they needed what the first two did not:

- **A layout of their own**, `Layout::Flat`, since a name that becomes a key
  in a config file can be no route, and every match on the layout answers
  for it (where an artifact's file is, where one of a name goes, what a
  removal takes away, what a read of it for the panel is bounded by).
- **No folder by default.** `Directories::default_for` answers `None` for a
  kind with no folder unless a marker names one, so a `mind.toml`, which
  names the folders of the kinds it knew, reads no `mcp/` at a project's
  root, a version 1 one none inside `.mind`, and neither `mind init` nor
  `mind migrate` names one ([where a project keeps its
  artifacts](#where-a-project-keeps-its-artifacts)). A workspace keeps one
  in its library unless its marker names a folder for it, which none
  `mindweaver init` writes does (D-05).
- **Words of its own in a sentence**, `Kind::noun` and `Kind::plural`
  (`MCP server`, `MCP servers`), since a message spelled `{kind}s` holds a
  kind to having a slug a sentence can make a plural of, which an
  abbreviation is not. The slug and the folder are both `mcp`, which `mind
  list mcp` takes in either place.
- **Saying what no release does yet**, by a predicate each a front end can
  ask: `install::installs`, behind the refusal in `install::targets` that
  every install, adoption and survey of a project meets, which says yes of
  every kind since #95, `install::targets` refusing a server by a code of
  its own (`not_copied`), since it is installed as entries, never into a
  folder, and `create::scaffolds`, whether `add` writes a kind from a name and a
  description alone, which the TUI's add form steps through, so neither
  screen offers a choice only to refuse it; `add mcp` is told the rest by
  its options ([adding one](#adding-an-mcp-server)). Each is a match on
  the kind with no wildcard.
- **Where an agent reads it**: nowhere in `Agent::reads` or the target
  table, since each agent reads its servers as entries of its own config
  file, the files `Agent::reads_files` names already ([what says an agent
  works in a project](agents.md#what-says-an-agent-works-in-a-project),
  whose table names `.mcp.json`, `.codex/config.toml` and `opencode.json`;
  [MCP servers](agents.md#mcp-servers) says what each agent does with
  them). The
  renderers that write there are #90 to #94's.

## The two levels

A **mind project** is a directory carrying `.mind`, the way a repository
carries `.git`. Its skills live in `skills/<name>/SKILL.md` by default, beside
the code, and are committed with it, so they travel with the code they
describe. The `.mind` itself is the checkout's: its settings keep themselves
out of git unless the project is shared
([D-01](#d-01-a-projects-mind-ignores-itself)).

A **weaver workspace** carries `.mindweaver` and references the mind projects
it manages, so their skills can be handled together. It is the level above, and
it does not have to be a repository at all: the directory your repos happen to
sit in is the usual case. It also holds skills and rules of its own: the ones
that belong to all of its projects, written once.

Both are identified by a **marker file**, not by the directory alone: a
project by its `settings.toml`, or the `settings.json` or `mind.toml` an older
build wrote, a workspace by its `weaver.toml`. An empty `.mind` left behind by
a failed copy is not a project, and saying so costs an `is_file()` or two
(`MindProject::is_marked`, which everything that looks for a project asks).

Both are found by walking up from a starting directory, so `mind list` works from
anywhere inside a project, like every git command. A workspace can also be
named, with `-W` or by activating one, and then it wins over the walk: see
[which workspace a command uses](#which-workspace-a-command-uses).

Nothing stops one directory from being both.

### The command surface is split the same way

`mind <cmd>` acts on the project you are standing in; `mind weaver <cmd>`, and
therefore `mindweaver <cmd>`, acts on the workspace above it, or the one `-W` names
or an activation chooses. The split decides two things that used to be
guesses:

- **What is in scope.** `mind list` is the project's own artifacts, never its
  neighbours'. `mindweaver list` is the workspace's own and every registered
  project's. Before the split one
  command tried to be both and its answer changed depending on which directory
  it was run from.
- **How an artifact is labelled.** Only the workspace level qualifies one by
  what holds it (a project, `workspace` for its own, or `shelf/<source
  folder>` for what `mindweaver validate` finds on the shelf), because inside a
  single project that says nothing.
  The same rule governs the kind: `mind list` grows a kind column, and
  `validate` starts printing `rule:x` instead of `x`, only once more than one
  kind is in play. One rule, applied twice, so a report never spends a column
  on something the reader could not have been confused about.

A workspace is in scope for exactly the projects it was **told** to manage. A
project that merely sits inside the workspace directory is not one of them
until it is linked. Guessing from the directory tree would make `mindweaver list`
answer a different question after an unrelated `mkdir`.

### Every command, at both levels

What can be done to a project can be done from the workspace, and the other
way round where it means anything. `mindweaver add`, `edit`, `rename`, `remove` and
`templates` act on the workspace's own artifacts, or on one of its projects
with `-p`; `mind link`, `unlink`, `install` and `uninstall` act on the project
you are in, against its workspace: the one named or activated, or else the one
above it. `mindweaver list`, `show` and
`validate` read everything (the workspace's own and every project's) or one
project with `-p`; `mindweaver validate` reads the shelf as well, labelling what it
finds there `shelf/<source folder>`. `mindweaver status` reads every project, or
one with `-p`, and `mind status` the one you are in: not the workspace's own,
which nothing is installed into ([what each project
has](#what-each-project-has)). `mindweaver doctor` and `mind doctor` read
the same projects the same way ([what no agent
says](#what-no-agent-says)). `mind agents`, `share` and `targets` change
the project you are in, and `mindweaver agents`, `share` and `targets` the one
`-p` names, which they need: the workspace's own have no agents, and reach
one by being installed.
`mind migrate` moves the project you are in from its `mind.toml` to
settings, and `mindweaver migrate -p` one the workspace links, or
`--all-projects` every one ([moving a project to its
settings](#moving-a-project-to-its-settings)). `mind update` brings what is
installed in the project you are in up to date, and `mindweaver update` what is
installed in each project `-p` names, or with `--all` in every one, needing
one or the other, as a change of more than one place says which ([bringing
what was installed up to date](#bringing-what-was-installed-up-to-date)).
`mindweaver install` and `uninstall` take `-p` once for each project too, or
`--all-projects` for every one, and need one or the other ([one artifact
into many projects](#one-artifact-into-many-projects)); `uninstall
--orphaned`, at either level, takes out what nothing offers any more in
place of one name ([taking out what nothing
offers](#taking-out-what-nothing-offers)). `mindweaver remove --everywhere`
takes one of the workspace's own out of every project it links before the
library, and has no `mind` form, nor a `-p`: a project's own was installed
nowhere ([removing one of the workspace's own
everywhere](#removing-one-of-the-workspaces-own-everywhere)).
`mindweaver adopt -p <project> <name>` brings a skill somebody keeps in the
project's agents' folders into the workspace's library and records the
project's copies as installed from there, and has no `mind` form: what it
writes is the workspace's library, which `mind` changes nowhere, as
`mindweaver add` without `-p` has none ([adopting what a project already
keeps](#adopting-what-a-project-already-keeps)).
`mindweaver relink <old> <new>` follows a project the workspace links to where it
moved, and has no `mind` form: the project at `old` is gone, and nothing at
`new` is linked yet to say which workspace to tell. `mind init --new-id`
gives the project in the directory it works in an id of its own, as `init`
makes one there rather than in a project above, and has no `mindweaver` form,
since a copy is most often not linked yet ([an entry whose project
moved](#an-entry-whose-project-moved)).
Reads look everywhere; a change needs one place, and without `-p` that place
is the workspace.

There is one implementation of each. `mind add` and `mindweaver add -p alpha` are
the same function given the same project, and `mindweaver add` is that function
given the workspace's own holder, so the levels differ in what they hold and
never in the rules. What only has meaning at one level (the shelf, `gather`,
`scan`) is reachable from the other as `mind weaver <cmd>`, which is the
command itself rather than a copy of it.

### The workspace's own artifacts

They are held the way a project holds its own: in directories the marker
names (`[directories]` in `weaver.toml`), found by the same catalog, created,
renamed and removed by the same functions. `WeaverWorkspace::own` hands the
workspace out as a `MindProject`-shaped holder for exactly that reason:
everything that touches an artifact asks its holder where it is, what it is
called and where each kind lives, and a workspace answers those as a project
does, so there is no second copy of any rule to drift.

Where a project's default is beside the code, a workspace's is its library,
inside `.mindweaver`
([D-05](#d-05-a-workspaces-library-lives-in-mindflayer)):

```toml
[directories]
skills = ".mindweaver/library/skills"
rules = ".mindweaver/library/rules"
```

A project's folders sit where its agents look, because the agents read them.
A workspace's are read by nothing but Mindweaver, which installs from them,
and beside the projects, where they used to be, they put instruction-shaped
files in the directories above projects: `~/skills` and `~/rules`, for the
workspace in a home directory. No agent scans `.mindweaver`, and a workspace
that keeps them there is one folder wherever it is made.

`mindweaver init` spells both lines out, as `mind init` does. A kind the marker
does not name is in the library too (`Directories::library_for`), never in a
folder at the root, which for the default workspace is the home directory;
that is also what lets a kind added later land in every workspace's library,
whatever its marker says. `own` names those kinds in the holder it hands out,
since a `MindProject` asked about a kind nobody named answers with a folder
beside the code, or, for a kind added later, none.

MCP servers are that kind (#86): every workspace keeps its own in
`library/mcp` in its folder, `.mindweaver/library/mcp`, or
`.mindflayer/library/mcp` in one a Mindflayer build made, and never in an
`mcp` at its root, however old its marker. `mindweaver init` writes no line
for them: the marker's comment already says a kind it does not name is in
the library, so the line would tell a reader nothing, and leaving it out
keeps the marker a new workspace gets the one an older build wrote, which
is also what the mother of a Mindflayer build's workspace has to make. One
that names `mcp` keeps its servers there, as a marker naming any kind does.
No version moved: an older build reading a marker that names `mcp` reads
past a key it does not know, and one that names none says nothing to it.

A workspace made before the library keeps the folders its marker names,
`skills` and `rules` beside the projects as `mindweaver init` used to write them,
and is read exactly as before: the default changed, the format did not, so no
version moved. Moving them into the library is for `mindweaver migrate --library`
to do when asked ([below](#moving-them-into-the-library)), never for a read.

The folders can be anywhere inside the workspace, `.mindweaver` included, but
in what is reserved for gathering. A directory that is the shelf of either kind
or the cache, is inside one, or holds one, as `.mindweaver` itself does, is
refused by `mindweaver init` before anything is made
(`WorkspaceError::OverlapsGathered`). Gathering replaces and deletes folders
there by names of its own, so an artifact somebody wrote among them could go
with the next gather; and a rules folder holding the cache would read every
markdown file in every clone as one of the workspace's rules. The rules shelf
is reserved too, though nothing gathers rules yet, rather than matched on the
kind as `gather` is: scrubbing a source already deletes its folder from every
kind's shelf, and a kind made gatherable later has to find its shelf free. The
comparison ignores ASCII case, since on macOS and Windows `.Mindweaver/Skills`
is the shelf.

Unlike a project's, the folders are not made by `mindweaver init`, wherever they
are. A workspace told to keep them beside its repositories puts nothing among
other people's work until there is something to put, and the library follows
the same rule rather than a second one: the first artifact added makes its
folder.

A project holds what is in its own folders, because that is what its agents
read; the workspace's artifacts are not inherited out of sight. They reach a
project by being installed into it (the install screen and `mind install` offer
them beside the shelf) and are then managed there like anything else
Mindweaver installed.

When the workspace's root is itself a project it manages, a folder both keep a
kind in is the project's (`own_kinds`), and a catalog counts each file once
however many holders can see it: otherwise every artifact there would be listed
twice. With the library, the two share a folder only when a marker says so, as
an older workspace's does. Reports name a workspace's own artifact's holder
`workspace`, told by where the file sits rather than by the root it was found
from, since those two share a root.

### Moving them into the library

`mindweaver migrate --library` moves a workspace's own from the folders its marker
names into `.mindweaver/library/<kind>`, and points `[directories]` there
(`migrate::library`). Without `--yes` it plans, says what would happen and
changes nothing (`migrate::library_plan`). That dry run is a report, and exits
as one, where `remove`'s is a refusal: what it gives is a line per artifact to
read before saying yes, not one deletion that was asked for and did not
happen.

- **What moves.** Every artifact of each kind whose folder is not the
  library's, found the way discovery finds it but without reading it
  (`Catalog::locate`). A skill whose front matter is broken is no less the
  workspace's, and left behind it would drop out of every listing once the
  marker points elsewhere. It goes to the same route below the library's
  folder, by a rename: the library is inside the workspace, so on its
  filesystem but for a mount in between, and a move across two fails naming
  both paths, as an install's does, rather than turning into a copy. A folder
  a move empties goes too, up to and including the kind's, with `remove_dir`,
  which keeps a folder holding anything else: the workspace in a home leaves
  no empty `~/skills`, and a `README.md` beside the skills keeps it.
- **What a project holds stays.** A project the workspace manages, or one at
  its root whether it is linked or not, can keep a kind in a folder the
  workspace names too: a home made a project shares `~/skills` with the
  workspace in it. What such a project's discovery finds there, of any kind,
  stays where it is and is said, and the marker's line moves to the library
  all the same, where the workspace's own of that kind start empty. The root
  counts unlinked because the agents reading `~/skills` do not ask whether
  the workspace was told about the project. `own_kinds` draws that line
  elsewhere, counting what an unlinked root project shares as the
  workspace's own, so such a leftover was listed and offered by the
  workspace and stops being either; the plan marks it (`Left::counted`) and
  its line says so, since an install of it keeps a record no offer matches
  any more (an [open question](#open-questions)).
- **Refused as a whole.** Nothing moves while the plan has any reason not to,
  and every reason is said at once: something already where an artifact would
  go; a copy of the marker an earlier migration kept; a link whose target is
  relative, which a rename to another depth would aim elsewhere; a folder
  overlapping the library, another kind's folder or a project's root, where
  moving one file would carry another, or a project's, with it; a folder that
  cannot be listed; a project whose marker cannot be read and whose root
  holds a folder being moved from, which could keep anything there; a folder
  the marker names outside the workspace or overlapping the shelf or the
  cache, which `open` reads but `mindweaver init` refuses (`misplaced_own`, its
  check), since what is there is somebody else's, or gathering's; a folder
  that is a link, or is reached through one below the root, since what it
  holds is kept elsewhere on purpose, a dotfiles repository say, and a rename
  would empty that and leave the link aimed at nothing.
- **Planned again under the lock.** It holds the workspace's lock from its
  plan to its rewrite of the marker, and plans from the marker on disk once it
  has the lock, so neither a plan shown a minute before nor a writer in
  between decides what moves. The marker is rewritten with
  `edit_marker_held`, the edit `edit_array` makes once it has taken the lock,
  since the lock is not re-entrant.
- **All or nothing, as far as renames go.** The marker is kept first, beside
  itself as `weaver.toml.v<version>`, named after the version it holds as the
  ledger's copy is, with `create_new`, so a copy an earlier migration kept is
  never written over. Then every artifact is renamed, then the values in
  `[directories]` are rewritten through `toml_edit`, the comments and every
  other byte left alone. A step that fails puts back what had moved, the last
  first, and takes the kept copy away, so the workspace reads as it did; what
  cannot be put back is named (`MigrateError::stranded`).
- **A second run is a no-op.** A kind already in the library plans nothing,
  so a migrated workspace, or one made with its library, writes nothing, not
  even a copy of its marker. No format version moves, for the reason the new
  default needed none
  ([D-05](#d-05-a-workspaces-library-lives-in-mindflayer)): the marker spells
  its folders out.
- **The ledger is not opened.** An installation of one of the workspace's own
  records its origin as `workspace`, which names no folder, and its copies by
  where they are in the project. Nothing in it says where the workspace's own
  were, so every record reads the same once they have moved: an install
  compares the project's copy with the offer, found wherever the marker now
  says, and calls it current when they match. A workspace that loads this one
  names its root, and reads its folders through its marker.
- **Resolved as a read.** Even with `--yes` it finds its workspace with
  `Intent::Read`, so the default one is not made only to find that, being
  new, it keeps its own in its library already.
- **`--library` is required.** A flag per migration, rather than a bare
  `migrate` doing every one there is, so the next one is asked for by name
  and never run by a line typed for this one.

### Where a project keeps its artifacts

`.mind` is the marker and the configuration. The artifacts themselves sit
beside the code, in directories the marker names. A project's settings, the
`settings.toml` `mind init` writes ([a project's
settings](#a-projects-settings)), name them in a table, as a `mind.toml`, the
marker of a project an older Mindweaver made, does:

```toml
[directories]
skills = "skills"
rules = "rules"
```

The `settings.json` between the two named them in an object,
`"directories": {"skills": "skills", "rules": "rules"}`, and is read the way
it was written.

Beside the code rather than inside `.mind`, because these are files the
**agents** read, and an agent looking for skills does not know what a `.mind`
is. Every project already has a place its agents look (`.claude/skills`,
`docs/rules`), and the marker is how a project says which, so Mindweaver
follows the repository rather than the repository following Mindweaver.

Both are keyed by the kind's folder name, which is the plural spelling the
CLI already accepts, so there is no second table to keep in step. A key this
build does not recognise is carried along rather than rejected, for the reason
unknown front matter keys are.

`init` writes every kind's directory even when it is the default. The answer to
"where do this project's skills go" then lives in the file somebody opens
rather than in a function they have to find. In settings it is also the only
way to have the folder at all, since a kind they leave out has none (below), so
`mind init` hands core only the folders it was told about, and core gives each
other kind its default (`MindProject::init_asked`). Which folders a new project
gets is a rule ([D-06](#d-06-projects-keep-authoring-beside-the-code)), and a
front end that filled them in itself could disagree with this one;
`MindProject::init_settings` takes its folders as given, for what needs a
project that keeps none.

A directory has to be inside the project. An absolute path, or one that climbs
out with `..`, describes a project whose artifacts are not the project's, and
that contradicts the one thing a mind project is for. `mind init --skills
/etc/skills` is refused rather than made, before anything is written.

Nor, in settings, inside `.mind` (`InsideMind`), which is Mindweaver's: the
marker, the templates and an install's stage. A local project's keeps itself
out of git, so `mind init --skills .mind/skills` would hide what the project
wrote from its team without a word, where authoring beside the code is so it
is committed with it. A shared project is refused the same, since a change to
its share may make it local later. `init` and `change_settings` refuse it,
and a settings file somebody wrote that way is still read: only a version 1
`mind.toml` kept its kinds inside `.mind` on purpose.

Because this changed where a project's artifacts are, the project marker went
to version 2, and a `mind.toml` written before it is read the way it was
written: version 1 had no such question, so every kind lived inside `.mind`,
and reading one with today's default would point it at directories it never
had and list nothing without saying why. `DIRECTORIES_VERSION` is what draws
that line.

What a kind left out means is the marker's version's to say, and version 3
says something else again. In a `mind.toml` it is the default beside the code,
for the kinds that have one; in a project's settings it is no folder at all,
so a kind added later is never given a folder at the project's root that
nobody asked for. MCP servers were the first (#86), and have no default even
in a `mind.toml` (`Directories::default_for`, matched on the kind with no
wildcard): a `mind.toml` names the folders of the kinds it knew, a version
1 one kept those inside `.mind`, and a folder called `mcp` at a repository's
root is as likely to be the code of a server as anywhere definitions go, so a
project an older Mindweaver made reads none, at its root or in `.mind`, and
says nothing of one. Nor does `mind init` name one in the `settings.toml` it
writes, nor `mind migrate` in the one it moves a project to, for the same
reason, so `mind init` makes no `mcp/` beside the code: a project that
authors its own definitions names the folder in `[directories]` by hand, and
adding one where it names none is refused with that said
(`WorkspaceError::NoDirectory`, which says to `mind migrate` first for a
project marked by anything but a `settings.toml`). Naming one by default was
the other way, and would have needed no version (a build that reads settings
reads past a key it does not know, and misreads nothing), but it would make
every new project's `mcp/` a Mindweaver folder, server code and all. A
`mind.toml` that names `mcp` by hand is read as naming it, as it is read for
any kind: the rule is the default's, not the key's. `MindProject::directory_for` answers `None` for such a kind. A reader
finds nothing there, an absence and not a failure, as in a folder nobody has
made; what needs the folder (adding an artifact, renaming one) asks
`required_directory`, which refuses with `NoDirectory`. Installing does not
need it: a project marked by its settings is installed into the folders its
agents read, never into the one it authors in ([It only manages what it
installed](#it-only-manages-what-it-installed)), so one naming no folder for
skills takes installs like any other. One naming no agent a kind is sent to
has nowhere to put it, and `install::targets` refuses it with `NoTarget`.
`install::survey` offers such a project nothing rather than refusing it: the
install screen surveys every project the workspace links, so a refusal for
one would hide all the others, and the screen shows that project with core's
line saying why it can take nothing, rather than sending somebody to gather
for it. Naming one skill for it (`install::offered`) or installing one is
refused, since a project was named; uninstalling one goes by its record, so
what was installed while the project named an agent still comes out.
Answering with a default instead was turned down: an `Option` costs every
caller a line, and a default would have put the next kind beside every
project's code. A listing still reads such a project's agents' folders ([read
where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)):
keeping no folder of its own is no reason to hide what its agents load.

### A project's settings

From version 5 (`SETTINGS_TOML_VERSION`) a project is marked by
`.mind/settings.toml`, which holds what the hub needs to know about it and
nothing else ([D-06](#d-06-projects-keep-authoring-beside-the-code),
[D-09](#d-09-project-settings-live-in-the-project)), written from a template
whose comments say what each key does (`Settings::text`, [why the markers are
TOML written from a
template](#why-the-markers-are-toml-written-from-a-template)):

```toml
# Mindweaver mind project: what Mindweaver needs to know about the project
# this .mind is in, and nothing else. No agent reads it.
#
# `name` is what the project is shown as, and `agents` the agents that work
# in it (claude, codex, opencode): what is installed into the project is
# copied where each of them reads. `mind agents` changes them.
# ...
version = 5
id = "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5"
name = "collapse"
agents = ["claude", "codex", "opencode"]
share = "local"
edit_tracked = false

[directories]
skills = "skills"
rules = "rules"
```

Versions 3 and 4 (`SETTINGS_VERSION`, `TARGETS_VERSION`) held the same keys
as `.mind/settings.json`, in strict JSON, the owner's first choice for
D-06. Such a file is read the way it was written, through the same reader,
and changed by nothing but its move to a `settings.toml` ([moving a project
to its settings](#moving-a-project-to-its-settings)).

The agents that work in the project, by the names `Agent` takes; whether what
is installed into it is the team's or this checkout's
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)); whether a
team file git tracks may be edited to reach an agent, off unless the file says
so; the folders it authors in; a name; and an id. A project that installs a
kind somewhere other than where its agents read names where in `targets` too,
which a new project leaves out ([a project's own
targets](#a-projects-own-targets)). They describe the files
physically in the project, and several workspaces may link it, so they live
in the project, where every workspace reads the same ones. The id is 128 bits
drawn from the operating system through `std`'s hasher keys, as a stage's name
is, written as 32 lowercase hexadecimal digits: random rather than derived
from the name or the path, which both change, so only a copy of the project
shares one.

Minimal, because `.mind` is meant to stay out of the way (vision point 8). No
agent reads it, so nothing instruction-shaped sits where an agent looks, and
a file of seven keys is one a person reads at a glance and edits by hand,
its comments telling them what each one does.
Anything that is not about the project's own files stays out of it: what was
installed is the ledger's, and which workspaces link the project is theirs.

A local project ignores itself, as a Python venv does
([D-01](#d-01-a-projects-mind-ignores-itself)). `MindProject::init_settings`
writes a `.mind/.gitignore` holding `*` (`MIND_IGNORE`), with `create_new`, so
one somebody changed is left as they left it, and before the settings, so they
never show in `git status`, not even for a moment. The rule lives inside the
folder Mindweaver owns: it needs no repository yet, goes with a copy of the
project and goes when `.mind` does, and no team file is edited for it. A shared
project gets none, and its settings are the team's to commit. A test asks gix,
in process, whether a local project's settings are ignored. A second run, as
for `init`, rewrites nothing and only makes what the marker asks for and is
missing, a deleted `.gitignore` of a local project included; a project an
older marker marks is not given a `settings.toml`, since moving it is `mind
migrate`'s ([moving a project to its
settings](#moving-a-project-to-its-settings)).

`mind init` writes them (#35, a `settings.toml` since #166): every agent
unless `--agents` names fewer, local unless `--shared`, and `skills/` and
`rules/` unless `--skills` or `--rules` says otherwise. Every agent, so a project made without a thought
for it reaches whichever agent is opened there; local, because that is what a
new project is (D-02). The TUI's offer to make a project and the minds
screen run that same `mind init`, so they write the same marker. The CLI's
test makes a fresh repository a project and asks gix, in process, what `git
status` would list: nothing, since `.mind` ignores itself and git lists no
empty folder, until the first artifact is written into `skills/`, which is
there for the team to commit. `MindProject::init`, which writes a `mind.toml`,
is no front end's any more; it stays for the tests that start from a project
an older build made, which is still read the way it was written.

Any of the three files makes a directory a project, and when more than one
is there the newest wins, `settings.toml`, then `settings.json`, then
`mind.toml` (`MindProject::open`): an older one beside it is one an upgrade
left or a copy brought back. Refusing the project would hide every artifact
in it over a stray file, so it is read, and `MindProject::warnings` names
each one ignored ([open questions](#open-questions)). Each file holds
versions of its own, and `MarkerFile::of_version` says which: a `mind.toml`
up to 2, a `settings.json` 3 or 4, a `settings.toml` 5. A version found in
another file, a `mind.toml` claiming 3 or a `settings.toml` claiming 4, is
refused as malformed: what it would mean is nobody's to say. Both settings
files are read into one tree (`config_entry::document`) and held to the keys
by one reader, a key at a time, so the same key is refused the same way in
either.

A key the format does not define is read past and kept, as unknown front
matter is: the format grows, and a project a newer build configured is still a
project. A key that changes what an older build would do with the project is
the exception, and comes with a version of its own, as `targets` came with 4
in a `settings.json`; a `settings.toml` is 5 whatever it sets, since every
build that reads one reads `targets` too. Every key it does define is held to
what it holds: a missing `id`, an agent no `Agent` names, a `share` other than
`local` and `shared`, a `targets` entry that is not a list of folders or names
none, are refused, each named, rather than guessed at. `edit_tracked`,
`directories` and `targets` may be left out, and read as off, as no folders
and as where the agents read. A folder `targets` names that no install may use
is read and said, not refused, since everything else the file says still
stands.

`MindProject::change_settings` changes them, the way `weaver.toml` is
changed: through the same `rewrite`, which reads the file again rather than
trusting what was read when the project was opened, and writes back only
what changed, and through toml_edit (`settings::edited`). Its caller changes
a copy, and only the keys whose values changed are edited, each value written
where the old one stood, keeping the spacing around it and a comment after it
on its line, `directories` and `targets` an entry at a time. The template's
comments, a comment somebody added, a key this build does not know, the
order of the keys and a layout made by hand all survive, and a key the file
left out goes in after the others only when it says something its absence
does not, so turning nothing on writes no `edit_tracked = false`. `targets`
emptied takes its `[targets]` table out, so setting and resetting them gives
the file back byte for byte, unless somebody wrote a comment above the
header, which keeps the header, empty. A changed file goes through a
temporary and a rename; an unchanged one is not written, its modification
time included. The version and the id are refused: a version moves with a
migration, which writes the file its version describes, and an id names the
project for good. So is a folder the change moves out of the project, as
`init` refuses one, or a target no install may use, while one written there
by hand is still read and stops no change to something else.

Only a `settings.toml` is changed (`MindProject::changeable`). A project a
`settings.json` marks is refused as one a `mind.toml` marks is, naming the
file and pointing at `mind migrate`: the settings are edited one way, in
the grammar this build writes, rather than a third way through
`config_entry`, which is for the agents' own files, and the JSON editor and
its rule for the version a change of `targets` brought with it went with
the grammar.

What changes them from a command line is `mind agents` and `mind share`
(#37), and `mind targets` (#40), at either level, and since a project's
agents, targets and share mode decide where what is installed into it goes
and whether git sees it, those carry the installs along too, through
`reapply`, holding the lock of the workspace that links the project ([when a
project's agents, targets or share mode
change](#when-a-projects-agents-targets-or-share-mode-change)).

### A project marked by its settings is read where its agents read

What an agent loads is what sits in its folders, whoever put it there, and a
project marked by its settings says which agents work in it. So discovery
reads it where they read (`Catalog::discover_kinds`): the folder each of its
agents is sent copies in (`MindProject::targets`, the project's target table,
the standard one overridden by its `targets`), and beside them the folder the
project authors in, from `directories`. Reading only the authored
folder, as every project was read before, would list the project's own work
and none of what its agents run: an install's copies, and the skills
somebody already keeps in `.claude/skills` or `.agents/skills` by hand. A
project a `mind.toml` marks names no agents, so it is read from the one folder
its marker names, exactly as before.

- **Each folder once.** An authored folder that is one of the targets,
  `"skills": ".claude/skills"` say, is read once, matched by its spelling and
  then on disk (`same_directory`), so a skill in it is listed once. Whether
  that skill is the project's own or an install's is the ledger's to say,
  below. So are two targets that are one folder on disk, `.claude/skills`
  linked to `.agents/skills` so that Claude and Codex share it, under the
  first spelling: read twice, each failure there would be said twice.
  `Catalog::folders` is the list read, in order, which `list` names when it
  finds nothing.
- **One artifact a name.** Every copy of one name the project holds, in a
  target, a folder one of its agents reads or the folder it authors in, is
  one artifact (`Artifact::copies`), described by its first copy: the
  project's own when it has one, then the rest by path. An installation
  writes one copy per folder ([D-07](#d-07-one-copy-per-agent-folder)), and
  a listing that showed each would show one skill twice, under a name the
  agents see once. The project's own leads because it is what a change from
  the project reaches (below), so `show` prints the file `edit` opens.
- **With the agents that see it.** `Artifact::agents` is every agent the
  project names that reads one of its copies, by `Agent::reads`, which is not
  the target table ([agents, and where each kind reaches
  them](#agents-and-where-each-kind-reaches-them)): OpenCode reads
  `.claude/skills` and is sent a copy only in `.agents/skills`, so a skill in
  `.claude/skills` alone lists with Claude and OpenCode, and one in both with
  all three. An agent the project does not name is left out, since nothing
  of its reaches the project.
- **Copies that differ are said.** Claude reads one standard folder, Codex the
  other, and OpenCode loads whichever it takes of the two (V1 at random), so
  two copies an agent reads that do not hold the same mean the agents do not
  run the same skill. They are compared by `hash::tree`, the hash an install
  records, which refuses a link rather than reading wherever it points, as a
  copy here is anybody's; one folder reached twice, through a link to it, is
  one copy without being read. Differing copies are still one artifact
  (`Artifact::copies_differ`), and a failure, `DiscoveryFailure::Diverged`: a
  `warning:` and a non-zero exit, like anything else discovery could not make
  one thing of. `validate` counts them apart from unreadable files
  (`DiscoveryFailure::is_unreadable`). A copy that cannot be hashed is said
  too (`Uncompared`), and is not taken to differ. Comparing reads every file
  of each copy compared, on every listing, where discovery otherwise reads
  only `SKILL.md` ([an open question](#open-questions)).
- **Each copy is validated.** `Artifact::validate` judges every copy on its
  own file and folder, adding what another has to what the first has, since
  an agent loads the copy in the folder it reads: one copy filed under a
  folder its name does not match fails there and nowhere else.
- **What the project authors where no agent reads is a copy no agent
  sees.** The default `skills/` is read by no agent: what is there is the
  project's own work, which reaches an agent only once installed. A skill
  there is one more copy of its name, with no agent of its own, so the
  agents an entry lists are still only those that load it, and it is
  compared with no other copy: it is the source an install delivers from,
  and a source edited since it was delivered is for `status` to report once
  it tells an outdated copy from a current one (#48), not a warning on every
  listing until it is installed again. Listing it
  apart, as a second entry of the same name, was turned down: a project
  level `list` has no column to tell the two apart, and a screen naming a
  row by its name alone would aim a change at the agents' copy and reach
  the project's own.

Who put a copy there (`Provenance`) is the ledger's, and discovery reads no
ledger: a listing that says nothing of it opens none. `Catalog::with_records`
is handed the workspace's, and marks installed each copy whose path an
installation records (`Ledger::copies`), not each whose name it does, so an
install into one folder never claims what somebody keeps under that name in
another. Unrecorded, a copy in a folder the project authors in is
`Authored`; one only in an agent's folder is `Foreign`, and an artifact is
`Installed` only when every copy is. Records are a workspace's own, so a
project the workspace does not link has none in it, and a ledger that cannot
be read leaves every copy unrecorded, which nothing changes, rather than
installed, which an uninstall may delete. `list`, `show` and `validate`
hand none over, since none says who put what where; `status` does, and the
TUI's rows with it ([what each project has](#what-each-project-has)).

Foreign is never touched. A change to a project's own artifacts looks only in
the folders it authors in (`Catalog::discover_authored`, from
`lifecycle::find`), so `edit`, `rename` and `remove` never reach a copy in an
agent's folder the project does not author in. A name the project authors is
one entry whose first copy is its own, and a change reaches that copy alone;
one found only in an agent's folder is refused as such
(`LifecycleError::NotAuthored`) rather than reported missing, since a listing
has just shown it. Installing writes a copy in each target, and in the
authored folder only when that is one of them, so what a project authors and
what it was installed are apart: a change to the first touches no record of
the second. Settings that author in a target, `"skills": ".claude/skills"`,
put an installed copy where a change reaches it, and that change sees to the
record of that copy alone: kept through an edit, carried through a rename,
dropped by a removal ([A record must not outlive the copy it is
about](#a-record-must-not-outlive-the-copy-it-is-about)).

### Moving a project to its settings

`mind migrate` (#38), or `mindweaver migrate -p` for a project the workspace
links and `mindweaver migrate --all-projects` for every one an older marker
marks, moves a project an older Mindweaver made to a `settings.toml`
(`migrate::project`): from a `mind.toml`, with what is installed into it
moved to where its agents read; from a `settings.json`, the same settings in
TOML (below). Without `--yes` it plans, says what would happen and changes
nothing (`migrate::project_plan`), a report as `mindweaver migrate --library`'s
dry run is.

- **Explicit, never on a read.** Every format here is read the way it was
  written, and moved forward only when somebody asks: a workspace's own by
  `mindweaver migrate --library`, a ledger only after it is copied aside. A
  project has three reasons more. The move changes what a team committed,
  the `mind.toml` and the installs in `skills/`, which is theirs to see in a
  diff and commit, not something to find after a `mind list`. It needs
  choices only whoever owns the project can make, which agents and whether
  it is shared, and a read would have to guess them. And a build from before
  settings, on a teammate's machine or beside this one, finds no project
  once the `mind.toml` has gone, so moving it is a step a team takes on
  purpose. A read that rewrote a marker would also break what `Intent::Read`
  promises: that a read leaves things as it found them.
- **What the settings say.** The `mind.toml`'s name, and its folders
  written out (`Directories::with_defaults`): a kind it left out had the
  default beside the code, where one settings leave out has none. An id of
  their own. Every agent unless `--agents` names fewer, as `mind init`. And
  shared unless `--local`, as [D-02](#d-02-installs-stay-local-unless-a-project-shares-them)
  has it, because such a project was committed by design; `--shared` says
  it out loud. Written from the template as `mind init` writes them, a
  `settings.toml` straight from a `mind.toml`, with `create_new`. What else
  the `mind.toml` held stays in the copy kept.
- **The marker is kept, in `.mind/tmp`.** Moved to
  `.mind/tmp/mind.toml.v<version>`, named after its version as
  `weaver.toml.v2` is. Not beside the settings, where it would be a
  leftover the project warns about and an older build reads instead; and
  `.mind/tmp` carries its own `.gitignore`, so the kept copy is out of git
  in a shared project too. One already there, kept by an earlier migration,
  refuses the move. Put back in place of `settings.toml`, it is the project
  as it was, though its installs have moved.
- **The installs move as for a change of agents.** It is one, from none to
  the ones asked for, so each recorded installation is brought to the
  targets the settings give through `reapply::retarget`, as `mind agents`
  brings it ([when a project's agents, targets or share mode
  change](#when-a-projects-agents-targets-or-share-mode-change)): a new copy
  is made from a copy that still holds what was installed, never from the
  offer, which may have changed since; an untouched copy in a folder no agent is
  sent copies to, which for such a project is the folder it authors in, is
  taken out; one changed since is left, and its record dropped, its
  author's. A project whose `mind.toml` kept its skills in `.claude/skills`
  keeps its copy there and gains one in `.agents/skills`. Somebody's where a
  copy would go is left, and said. A rule's copy is not moved: a rule
  reaches a project marked by its settings through the block of its
  `AGENTS.md` and Claude's files, which a render writes, so the copy and its
  record are left as they are for the next render of the project's rules,
  which takes the copy out ([installing rules](#installing-rules)).
- **A record from before hashes is held to its offer.** Most of such a
  project's records were written before schema 3, with no hash, and
  [D-04](#d-04-an-edited-copy-is-detected-not-disowned) reads one as current
  only while its copy matches its offer byte for byte. So the migration finds
  the offers (`install::offers`) and hands `retarget` those a record may have
  come from (`install::offered_for`, which tells every record's standing by
  the same answer): by the origin it records, or for a record from before
  origins by its shelf source's id, or else among the workspace's own and the
  loaded ones, any of which may be it. A copy matching one is compared as
  `install::is_current` compares (`copy::same_leaving`), moved, and its record
  given the hash every copy now holds (`Ledger::migrated`), so the next change
  of agents compares it as any other; one matching none is edited; with no
  offer left, it cannot be told, and is left. A change of agents does not look
  for offers, and reads such a record as one it cannot compare.
- **What the project wrote stays.** Every artifact in a folder the settings
  keep naming that no installation records, rules included, is left where
  it is and said, as what sits unrecorded in a folder its agents are now
  sent copies to is said to be somebody's: the project is listed from there
  from now on.
- **Local when asked.** `--local` writes `.mind/.gitignore` and keeps each
  copy out of git in the project's new group of the exclude block, before
  the copies land, as `mind init` and an install do. A `.gitignore` keeps
  out only what git does not track yet, and such a `.mind` was committed, so
  when gix's index says git tracks something there (`exclude::tracks`), the
  command says `git rm -r --cached .mind` stops it. Said and never run:
  Mindweaver runs no git command, and taking files out of a team's index is
  not its to do.
- **All or nothing, for the project.** Held by the lock of the workspace
  that links it, and planned again once held. Then, in an order that shows
  nothing in `git status` for a moment: a local project's exclude entries
  and `.gitignore`; the new copies, each installation's from one stage
  (`copy::Stage::place`); the copies taken out, moved into that stage rather
  than deleted; the settings; the marker; and last the ledger, every
  installation and the log in one transaction (`Ledger::migrated`), so that
  each step before it can still be undone. A step that fails puts back every
  one before it, the last first, so the `mind.toml` marks the project again
  and the ledger names what it named, and what cannot be put back is said
  (`MigrateError::left_behind`). Only once the ledger holds the move are the
  copies taken out deleted, with the stage. A process killed part way is not
  a failure, and nothing puts it back ([open questions](#open-questions)).
- **Refused before anything is written**, every reason at once: a folder
  the `mind.toml` keeps a kind in that settings cannot name, inside `.mind`
  as a version 1 project kept every kind it named no folder for, or outside
  the project; no agent; a kept marker; and a copy that would land in an
  agent's own folder or where a loaded workspace keeps its own, as an
  install refuses one. A folder a version 1 `mind.toml` does name is kept,
  as every other command reads it (`MindProject::directory_for`), so
  naming one outside `.mind`, as the refusal asks, gets such a project
  past it.
- **A second run is a no-op.** A project its `settings.toml` marks has
  nothing to migrate, and nothing is written for it, not even its ledger
  opened.
- **Each project on its own.** `--all-projects` moves every project the
  workspace links that a `mind.toml` or a `settings.json` marks, one at a
  time, each all or nothing: one refused or failed is an `error:` naming it,
  the rest go ahead, and the command exits non-zero. Without `--yes` it
  lists every one.
- **A project nothing links still moves.** Its marker is its own
  ([D-09](#d-09-project-settings-live-in-the-project)), so in a project no
  workspace `mind` consults links, `mind migrate` writes its settings and
  moves no copy, and says with a `hint:` that nothing installed was carried
  along. The hint never says that no workspace links it, which walking up
  cannot know: it names `-W`, which reaches a workspace that links it from
  beside it, and without which that one's records are lost to the move all
  the same. A project whose workspace cannot be read is refused instead,
  in a dry run too, where a change of agents goes ahead beside a
  `warning:`. That change can be run again; the move is made once, and
  what the workspace records decides which copies it carries. Made without
  them, nothing carries those copies once settings mark the project, and
  one recorded without a hash, which only the move holds to its offer, is
  dropped the next time its agents change.
- **One migration a line.** `mindweaver migrate` takes `--library`, `-p` or
  `--all-projects`, one at a time, for the reason `--library` is a flag of
  its own ([above](#moving-them-into-the-library)), and a line naming none is
  told which there are, rather than shown clap's usage.
- **From a `settings.json`, the same settings in TOML.** A project a
  `settings.json` marks has its agents, share mode and folders already, so
  its move writes what the file holds as a `settings.toml`, at version 5,
  and keeps the `settings.json` as `.mind/tmp/settings.json.v<version>`,
  with `create_new`, as a `mind.toml` is kept; one an earlier migration kept
  refuses it. A folder written there by hand that a change would refuse is
  carried as it is: the move changes the grammar, not what the settings
  say, so the project reads the same before and after. Asked for, never
  made on a read, as a move from a `mind.toml` is: a build from before #166
  finds no project in a `settings.toml`, so the move is a step a team takes
  on purpose, and a read leaves things as it found them. No copy moves, since
  every one is where the agents send it already, so no workspace is read
  (`migrate::carries_installs`): no lock, no ledger, and a workspace that
  cannot be read is a `warning:` beside the move, where it refuses a move
  from a `mind.toml`. `--agents`, `--local` and `--shared` are what a
  `mind.toml` is given, and the report's first line says the agents and
  share mode the file keeps. A key this build does not know stays in the
  copy kept. Nothing else writes a `settings.json` any more, and a second
  move at once finds the `settings.toml` there, which `create_new` refuses,
  so the move needs no lock of its own.
- **The TUI points at it.** Its header counts the projects an older marker
  marks, in yellow after the workspace's name, where a narrow terminal keeps
  it, naming each file once (`2 projects marked by mind.toml or
  settings.json`), and names the command: `mindweaver migrate --all-projects`,
  or `mind migrate` for a project on its own. `t` and `T` on a project a
  `settings.json` marks say it is changed by nothing but the move before a
  form opens, as they say of a `mind.toml`. It has no key for the move
  ([open questions](#open-questions)). `mindweaver status` and `mind status`
  say it too, a hint for each such project ([what each project
  has](#what-each-project-has)).

### Why the markers are TOML written from a template

New markers are written from a string template, not serialized. Serializing
would drop the comments, and the comment at the top of a marker, saying what
each key does, is the first documentation anyone opening the file will read.

A project's marker is TOML again from version 5, the owner's amendment to
[D-06](#d-06-projects-keep-authoring-beside-the-code): `.mind/settings.toml`,
like `weaver.toml`, so both markers are written and edited the same way.
Versions 3 and 4 were JSON, `.mind/settings.json`, the owner's first choice:
any tool can edit JSON, and seven keys that name what they hold seemed to
explain themselves. JSON has nowhere to put a comment, though, so what each
key did was in this document and the README rather than in the file
somebody opens to change one, and the settings needed an editor of their
own, a third way beside `weaver.toml`'s and `mind.toml`'s, which was
`config_entry`'s JSON (held to RFC 8259 so `jq` and `JSON.parse` took what it
wrote). Now `Settings::text` fills a template as `weaver.toml`'s is filled,
its comments saying what each key does and which command changes it, every
folder spelled out and a `[targets]` table only when it names a kind; and
`config_entry` is for the agents' own config files, whose formats are
theirs. A `settings.json` is still read, the way it was written, and moved
forward only by `mind migrate`.

Editing an existing marker has the same constraint and a harder job, which is
why `toml_edit` is a dependency: `mindweaver link` rewrites the one `projects`
array and leaves every other byte alone, and a change of a project's settings
rewrites the keys whose values changed, each where it stands, keeping the
comment on its line ([a project's settings](#a-projects-settings)). Both go
through one `rewrite`, which reads the file, hands its text to the edit and
writes back only what changed. Reading `weaver.toml` and a `mind.toml` stays
serde's job; a project's settings, in either grammar, are read a key at a
time (`config_entry::document` and `settings::parse`), so a newer version is
refused before any key it may have given a new meaning, and a key this build
does not know is read past rather than refused. The two halves agree because
an edit re-reads the file afterwards rather than trusting what it believes
it wrote, and the write itself goes through a temporary file and a rename,
so a crash leaves either the old config or the new one. An edit of
`weaver.toml` holds the workspace's lock from its read to that re-read, so a
second writer edits what the first one wrote ([Writers at the same
time](#writers-at-the-same-time)); an edit of a project's settings takes
none of its own, and the one change that makes it (`reapply::apply`) holds
the lock of the workspace that links the project around it.

### Formats are versioned

Each marker carries a `version`, and each has its own: `MIND_FORMAT_VERSION`
for a project's marker, `WEAVER_FORMAT_VERSION` for `weaver.toml`. A marker
written by a newer Mindweaver is refused with a message naming which of the
two it is and which version this build reads, which is what lets a format
change later without an old binary silently misreading a newer project or
workspace.

Two constants rather than one, because the files change for different reasons.
Shared, the project marker going to 3 would have written every new
`weaver.toml` as a 3 too, and an older build would refuse a workspace that had
not changed at all. A project's marker reads up to 5 today, a
`settings.toml`, and a workspace's up to 2, and neither moved when the other
did.

A project's newest version is the one `mind init` writes, a `settings.toml`
(#166), as it waited to write a `settings.json` (#35) until this build read
such a project (#29), listed it (#33) and installed into it (#34). 5 for a
`settings.toml` whatever it sets, though it holds the keys a version 4
`settings.json` does, because a version names the file it is written in as
well as what the file holds, as 3 did between `mind.toml` and
`settings.json` (`MarkerFile::of_version`): one found in another file is
refused rather than guessed at. An older build finds no project without the
file it looks for, so a build from before #29 says a new one is not a
project, and one from before #166 the same of a `settings.toml`, or that a
workspace's entry for it is stale, rather than misreading it. Except where
that answer leads it to make one: a build from before #29 asks only whether a
`mind.toml` is there, so its `mind init`, or its TUI's making a repository a
project, writes one beside the settings and reads the project as a `mind.toml`
one from then on, while this build reads the settings and passes the
`mind.toml` by ([open questions](#open-questions)).
`MIND_TOML_VERSION` is the newest a `mind.toml` holds, which only
`MindProject::init` still writes.

A key that an older build can read past without misreading anything moves no
version: `known`, the list of [the workspaces this machine
knows](#the-workspaces-this-machine-knows) that the default workspace keeps,
says nothing about the workspace holding it, and every edit of a marker keeps
a key it does not know.

An older marker is read the way it was written, never refused, and moved
forward only when somebody asks: a workspace's own folders by `mindweaver migrate
--library`, a project's `mind.toml` or `settings.json` by `mind migrate`
([moving a project to its settings](#moving-a-project-to-its-settings)).
An MCP server's definition carries a format of its own, `format = 1`
(`MCP_FORMAT`), since it is an artifact, copied and gathered with no marker
beside it, and is refused from a newer format as a marker is ([what an MCP
server's definition declares](#what-an-mcp-servers-definition-declares)).
Adding the kind moved no marker's version: a new `weaver.toml` and a new
`settings.toml` name no folder for it, so they read to an older build as
they did, and one that names `mcp` by hand names a key an older build reads
past.

Four lines are drawn so far, all project-marker versions, since a
workspace's number says nothing about how a project keeps its artifacts:
`DIRECTORIES_VERSION`, before which every kind lived inside `.mind`;
`SETTINGS_VERSION`, from which the marker is a settings file and a kind it
does not name has no folder; `TARGETS_VERSION`, from which the settings may
say where installs land; and `SETTINGS_TOML_VERSION`, from which the
settings are `settings.toml`. A `settings.json` was written at 4 only while
it set `targets`, so one is a 3 or a 4 by what it says rather than by which
build wrote it: one that never set them is read by every build since
settings, and one that did is refused by the builds that would read past
them ([a project's own targets](#a-projects-own-targets)). The
ledger carries its version in SQLite's `user_version` and is refused from the
future the same way, but an older one is migrated forward rather than read as
written, after a copy of it is set aside ([Why the ledger is
SQLite](#why-the-ledger-is-sqlite)).

## Failures are collected, not raised

Discovery returns what it found *and* what it could not read. One skill with
broken front matter must not hide the forty next to it, and a stale entry in a
workspace registry must not stop the other projects from being managed. Two
copies of one skill that differ in a project's agents' folders are collected
the same way: listed, and said.

The CLI prints those on stderr as warnings and exits non-zero: visible, but not
in the way of the answer. Asked for JSON, it puts them in the envelope beside
the data too, each with its code ([JSON, in one
envelope](#json-in-one-envelope)).

Each line it says beside a report is a `render::Warning`, an `Outcome`'s and a
`Failure`'s alike: the text a terminal shows, word for word, and beside it the
code of what it says ([a code for what went wrong](#a-code-for-what-went-wrong)),
taken from the value the line renders, core's error or failure or the CLI's
own. The panel acts on a warning by its code, and a front end running a
command line hands it the code with the text. A hint or a note has none, nor
does a line the CLI words from what it found rather than from something that
went wrong; a line built from text core handed over has none either, until
core hands it over with its code, as a render's warnings come (`code::Said`).
Kept on the line rather than in a list beside the lines, which every line
pushed would have to keep in step with, and compared with a string by its text
alone, so a test of what a terminal shows reads as it did. The TUI keeps the
text alone: it shows a line as a terminal would.

The distinction the code draws is between a problem and an absence. A project
with no skills folder yet is not a failure, it is a project nobody has added a
skill to. A skills folder that exists and cannot be listed is a
failure.

## JSON, in one envelope

A reading command's `--json` prints what it read inside one envelope,
whatever the command (`render::json`):

```json
{"version": 2, "data": ..., "warnings": [...], "coded_warnings": [...]}
```

`status` was the first to take it ([what each project
has](#what-each-project-has)), and every later reading command takes the same
one rather than a shape of its own: `scan --json` ([Scanning](#scanning)),
`workspaces --json` ([the workspaces this machine
knows](#the-workspaces-this-machine-knows)), and, since #64, `list`, `show`,
`validate`, `templates` and `gather list`, so that what a front end shows of
a workspace's holders, their templates and its offers a script or an agent
can print: not yet the whole snapshot (an open question below). `doctor
--json` (#96) prints its report in it too ([what no agent
says](#what-no-agent-says)). One shape
is one parser for a script, and for the panel's backend when it reads
through the CLI.

- **The warnings are inside it, with their codes, and on stderr too.**
  Failures are collected, not raised, in JSON as in text: what a table says
  on stderr as `warning:` lines goes into `warnings`, without the prefix,
  so a script reading stdout misses nothing. `coded_warnings` says the
  same lines again, in the same order, each as its `code` and its
  `message`, the code `null` for a line none is given to ([a code for what
  went wrong](#a-code-for-what-went-wrong)). A name of its own rather than
  objects in `warnings`, whose strings a version 2 script reads: changing
  what a field holds moves the version, and adding one does not. Codes in a
  list beside the strings, paired by place, was the other way, rejected for
  the reason a `Warning` keeps its code on the line: two lists every writer
  has to keep in step. The exit status is the text's. Every warning is
  still said on stderr, as the text says it, since #64 asked for it: the
  envelope is a script's, which reads each warning there and need not read
  stderr, and stderr is the terminal's, where somebody piping the JSON into
  `jq` would otherwise see the answer and nothing of what could not be
  read. Leaving them off stderr, as #36 first did so that a script reading
  both channels would read each once, was rejected for that, and for every
  command's JSON at once, `status`, `scan` and `workspaces` among them,
  since one rule (`render::json`) says them all. A hint, advice for whoever
  reads the terminal, stays on stderr alone. An error that stops a command
  is still text on stderr, `--json` or not: every error has a code since
  #61, but an error's is not printed yet.
- **Laid out from the text's own outcome.** A command whose JSON reads what
  its text reads builds its text, warnings and all, and `render::as_json`
  lays that outcome out as JSON: `data` in place of stdout, each `warning:`
  line said in the envelope, stderr left as the text has it, and the exit
  status kept, since a report can fail with no warning at all
  (`validate`, for an artifact that does not pass). Built from the outcome
  rather than beside it, so that a warning a command adds on the way out,
  a workspace it consulted that could not be read, reaches the envelope as
  it reaches stderr; and the warnings and the exit status cannot differ
  from the text's, as two reads of them could.
- **What each command puts in `data`** is core's, built from what the
  command read:
  - `list`: the snapshot's holders, a `view::Listing`: the library, for
    `mindweaver list` without `-p`, and every entry the workspace links, or the
    one project, each as the snapshot holds it, read for the kinds asked
    ([the read model](#the-read-model)). Its warnings and exit status are
    the listing's, read as the text reads it, and its data a read of the
    snapshot's beside it, since how each stands is the snapshot's to say
    and the listing never opens the ledger. `mind list --json` reads the
    project against the workspace `mind status` reads it against, the one
    `mind` consults, when that one links it: alone, a copy that workspace
    installed would stand as somebody's. It is the one JSON that reads what
    its text does not, so what that read finds wrong is no warning and no
    refusal, which would give the JSON an exit status the text does not
    have: a workspace found that cannot be read, or one `-W` or the
    activation names that is no workspace, leaves the project read alone,
    why said among its `failures`, as a ledger that will not open is.
    `mind status` warns or refuses there instead, its report being what
    that workspace installed.
  - `show`: `reading::Shown` for each artifact the name matches: where it
    is, what its front matter declares that `show` prints, and its body.
    The text is laid out from the same values, the file read once.
  - `validate`: `reading::Validation`, each artifact checked with its
    issues and whether it is invalid, and the counts its verdict follows
    (`invalid`, `unreadable`, `differing`), which the text takes too, so
    the two come to one verdict. Not the snapshot's rows: `mindweaver validate`
    reads the shelf, which the snapshot holds as what the ledger records
    rather than as artifacts read and checked, and neither `show` nor
    `validate` reads the ledger, so neither says how a copy stands, which
    read without it would call an installed copy somebody's.
  - `templates`: core's `Template`s, the list the snapshot holds of each
    holder; `gather list`: the `Offer`s it lists, what can be installed
    from a loaded workspace or the shelf, which the snapshot only counts.
- **Its names are a contract.** Every type in `data` derives `Serialize` in
  core, each field by its snake_case name, and `Kind`, `Agent` and `State`
  are serialized by the word a command line takes for them (`skill`,
  `opencode`, `missing`), written by hand rather than derived: a derive
  spells the variant, so `Agent::OpenCode` would print `open_code`, a name no
  command line or settings file holds, and a rename would change it. `Kind`,
  `Agent` and `Share` are read back the same way, through what reads a
  command line's word (`FromStr`, `Share::named`), so a task a front end
  hands back to be run ([every change is a command
  line](#every-change-is-a-command-line)) takes the words a command line
  takes and refuses the ones it refuses. A path is text, whatever bytes it
  holds, so one unprintable path cannot fail the report. A name once printed
  stays: a field that goes or changes what it holds moves
  `ENVELOPE_VERSION`, and one added does not, since a reader passes over a
  key it does not know; `list --json` prints the snapshot's own types, so a
  change to a holder that moves `view::FORMAT` moves it too. A test reads
  each report back into structs of its own that refuse any field they do
  not name (`common::enveloped`), and holds `coded_warnings` to say what
  `warnings` says, so a change shows there before it reaches a script.
  It moved once, to 2, when `status` took the five states an installation
  stands in ([what each project has](#what-each-project-has)): `installed`
  and `partial`, which a version 1 script knew, went, and a value a script
  never met came in their place, which is a field changing what it holds.
- **Pretty-printed**, two spaces a level, since a person reads it as often as
  a script does, and `jq` takes either.
- **The envelope is the CLI's**, in `render.rs` beside the table helpers: how
  a report is laid out is a front end's to choose. What goes in `data` is
  core's, which builds and orders it, so the table and the JSON cannot
  disagree about what is there.

### The read types serialize

The reports a command prints are not the only types a front end reads. The
panel shows what the TUI shows, and the snapshot it is served (#62) is built
from the same types the CLI renders, so #61 made the read types it named
serializable, under the rule the reports already kept, and every new read
type derives `Serialize` from the start:

- **Already there**, for a command's `--json`: the status report (`Status`,
  `ProjectStatus`, `ArtifactStatus`, `Unreached`, `Missed`, `State`,
  `MarkerFile`), a scan (`Scan`, `Found`, `Reason`), the workspaces this
  machine knows (`Known`, `KnownWorkspace`), `Kind`, `Agent` and `ProjectId`.
  None of them changed.
- **Added**: a `Reference`, as typed and as read; an `Artifact` and each
  `ArtifactCopy`, as a listing shows them, with their `Provenance` and every
  `ValidationIssue`; a `Template`; a source to gather from (`gather::Source`),
  a gather's `Report` and what it `Harvested`; a source the ledger keeps
  (`ledger::Source`, its `SourceKind`) and what was `Gathered` from it; what
  can be installed (`Offer`, `Candidate`) and how it stands (`Standing`); a
  workspace's `Member` and `Loaded` entries, with a project's `Share` mode.
- **Handed over by the snapshot** ([the read model](#the-read-model)), the
  first change to give a front end types older than the rule: how a
  workspace was chosen (`resolve::Reason`, as the slug of its rule,
  `explicit`, `activated`, `above` or `home`, not the words a screen puts it
  in), and the ledger's history (`ledger::Entry`, its target and detail
  through `url::redacted`). `Resolved` does not: it only pairs a workspace
  with its reason, which the snapshot says beside the workspace's name and
  path.
- **Printed by a command since #64**: a `Template` by `templates --json`,
  an `Offer` by `gather list --json`, and three new to it: what `list
  --json` prints of the snapshot (`view::Listing`), and what `show` and
  `validate` say of each artifact (`reading::Shown`, `reading::Validation`
  and its `Checked`).
- **Planned for the panel by #78**: a box of a selection
  (`install::Cell`, which the panel sends, and so reads too, refusing a
  field it does not name), and what a selection amounts to
  (`install::Plan`, its `Change`, `Conflict` and `Locked`), each name by
  the field, a kind by its slug, an origin, a cell's or an install's, as a
  `value::Origin`, written redacted, and why a box clashes or is locked as a
  `code::Said`. The panel is answered with the backend's `Planned`, each
  change the task that runs it, so a `Change` is tagged as a task is, its
  type beside its fields.
- **Surveyed for the panel by #80**: what an update of every project
  would do (`update::Updates`, each project's `ProjectUpdates` and each
  installation's `Pending`), a new read type rather than `update::Plan`
  made to serialize: the plan holds what applying it came to, its errors
  and the candidates it writes from, none a reader is given. What an update
  does is a word (`update::Does`, by its slug, said in TypeScript from the
  serializer's own function), a copy a route from its project, an origin
  as a `value::Origin`, and why one is left a
  `code::Said` of `update::Left`.
- **Judged for the panel by #82**: what core says of a draft before a
  command writes it (`draft::Judged`), each issue a `ValidationIssue` as
  `validate` reads it and why the command would write nothing a
  `code::Said`; and the hash a file read for the panel carries
  (`contents::Contents`'s `hash`, by `hash::bytes`), what `edit
  --if-unchanged` takes, a field added to a type the panel read already.
- **Told for the panel by #83**: how the workspace a snapshot is of stands
  with each workspace this machine knows (`view::KnownHere`), the row
  `mindweaver workspaces` prints (`KnownWorkspace`) flattened beside the entry
  it loads it by (`loaded_as`) and whether it could load it (`loadable`),
  as a shelf row's origin is flattened beside the ledger's row: what one
  workspace loads is nothing that list of the machine says.
- **Listed for the panel by #212**: the folders directly inside one
  (`browse::Folders`, each `Folder` and each `Crumb` of its trail), a path
  through `paths::lossy`, what a folder is a word (`browse::Is`, by its
  slug, said in TypeScript as `FolderIs`, since `Is` alone says nothing in
  a folder of the panel's types), and what it could not read or name a
  `code::Said` of `browse::BrowseFailure`.
- **Said by `show` since #87**: what each agent cannot be given of an MCP
  server (`mcp::Expressibility`), the agent by its name and each shape a
  `code::Said` of `mcp::Inexpressible`, under `refused` or `dropped`, a
  field added to `reading::Shown`.
- **Said by `show` since #88**: how an MCP server is started or reached
  (`reading::ShownTransport`, its `type` a word, `mcp::TransportType`, by
  the slug the format writes, and its address a `value::Redacted`) and
  what is supplied for it (`reading::ShownInput`, its `kind` a word,
  `mcp::InputKind`, by its slug, and a secret by the variable it is read
  from alone), as `transport` and `inputs`, fields added to
  `reading::Shown`. One record for every transport rather than a variant
  each, so its `type` is the word the format writes, never one a derive
  spells from a variant; a key of the other transport goes out empty.
- **Listed by `status` and the snapshot since #93**: each MCP server's
  entry in a project's agents' config files (`status::EntryStatus`), its
  agent by its name, its file a route from the project, its state the word
  an artifact's is said in and its origin a `value::Origin`, as `entries`,
  a field added to `ProjectStatus` and `view::Project`, which moves
  neither `ENVELOPE_VERSION` nor `view::FORMAT`.
- **Found by `doctor` since #96**: what keeps a project's agents from
  what it gives them (`doctor::Report`, each project's `Examined` and each
  `Finding`), a finding its code, the agents by their names, its path
  through `paths::lossy`, its message and its fix, and what could not be
  read a `code::Said`: derived, the condition a `doctor::Symptom` with a
  code of its own that goes out as the finding's `code` and message, as an
  error goes out through `code::Said`. Handed over by the snapshot too, as
  `doctor`, a name added, which moves neither `ENVELOPE_VERSION` nor
  `view::FORMAT`.
- **Carried for the panel by #97**: each of the library's MCP servers as
  each agent would be given it (`view::LibraryServer`, and for each agent
  a `view::GivenTo`, the agent by its name, what it is refused or would be
  written without a `code::Said` each, what its entry would run or reach
  a `mcp::Running`, whether it reaches an address and the line as
  `mcp::Runs` displays it, and what it asks before it starts one an
  `agent::Starts`), as `servers`, a name added to the snapshot's
  `library`, and so to what `list --json` prints of it, which moves
  neither `ENVELOPE_VERSION` nor `view::FORMAT`. `Starts` is derived, by
  its variant in snake_case (`once_approved`, `in_a_trusted_project`,
  `unasked`): a word of core's a front end says in words of its own, and
  no word a command line takes. How a server is started or reached as
  `add mcp`'s options say it (`mcp::Given`, `given` on a server, none when
  no option says all of it) serializes both ways, since the panel sends
  it back in a task and a draft: tagged by `type` inside the object, each
  pair an array of its two words, each secret the name of the variable it
  is read from, and a field it does not name refused. And the backend's
  `Asking`, each install of a plan that waits for a yes, as `asks` on
  `Planned`: the task it confirms by its place, the lines it was shown
  with, and the task carrying the yes.
- **Not yet**, being older than the rule and handed to no front end so far:
  the ledger's record of an install (`InstallRecord`), and what an update or
  a change of agents would do as each plans it (`update::Plan`,
  `reapply::Plan`). Each derives `Serialize` in the change that first hands
  it to one, under the same rule.

Each one a crate's `typescript::roots` names (what the panel reads, and
what `list`, `show` and `validate` print), and what it names, has
TypeScript too, written from it under the `ts` feature ([its types come from
Rust](#its-types-come-from-rust)): derived beside `Serialize` where serde's
is derived, and said from what the serializer writes where it is written by
hand, so the panel is told what serde writes, not what the Rust variants are
called. The other reports a script reads, what `status`, `scan`,
`workspaces` and `gather list` print, have none until a view reads them
([open questions](#open-questions)).

What each says is held to the rule above, and three more things:

- **Each field is named as the type names it**, whether derived or written by
  hand. By hand only where a derive would print the wrong thing: a word a
  command line takes (`Standing`, `Provenance`, `Share`, `SourceKind`, the
  place an `Offer` comes from), a type holding more than a reader is given
  (an artifact's front matter is `show`'s, and how each copy stands sits in
  a placement no front end sees), or an error. A `ValidationIssue` goes out
  as its code, what it is about, whether it is only worth a warning, and
  its message: `warning` is core's to say, so the panel needs no list of
  its own to tell an artifact that does not work from one that only says
  something odd.
- **An error goes out as its code and its message** (`code::Said`), never as
  its fields: one holds an `io::Error` or a parser's own, which JSON has no
  shape for, and only the two are meant to leave core. A member whose entry
  opens no project says why as `error`, which is what a status report's
  `stale` says, with the code beside it.
- **A URL goes out only through `url::redacted`**, and is held as a value
  that says so ([values, ports and adapters](#values-ports-and-adapters)):
  a source's URL, the action log's words and every `Said`'s message are a
  `value::Redacted`, which nothing makes from a text that keeps a
  credential, and an origin is a `value::Origin`, which writes itself out
  redacted whatever built it. A `String` redacted where it is written, the
  way this was first kept (`url::serialize_redacted`, on each field), left
  every new field to remember it. The ledger hands back no URL with a
  credential in it, but a request is built before a URL that carries one
  is refused, and a test builds a source with one to prove that neither the
  source nor an offer's origin prints it.

What a failure collected beside a report says stays out of the report, as a
scan's does: a gather's `failures` and `warnings` are a front end's
warnings, which in JSON are the envelope's. Every type's test pins its JSON
name by name; a serialized name is a versioned contract from the first
release that prints it, as a report's is.

### A code for what went wrong

The panel has to tell a workspace another writer holds from a project that
has gone, and the server hands it what went wrong as JSON: matching the
English would break the first time a message is reworded. So every public
error type in core has a `code()`, a snake_case word for the condition
(`workspace_busy`, `offered_twice`, `not_a_project`), and a
`ValidationIssue` has one from the same list.

So does a refusal, which is no error: an install that finds somebody's
copy, or one changed since, answers `Installed::Foreign` or `Edited` and
writes nothing, and the front end says so and goes on with the rest, as an
uninstall's `Removed` does. The panel still has to tell the two apart, and
the CLI's warning carries `foreign` beside its text, so `Installed::refused`
and `Removed::refused` say why as an `install::Refused` (`foreign`, `edited`,
`not_installed`). An update that leaves a copy changed since says `edited`
(`update::Step::refused`), and a change of a project's agents or targets
that leaves somebody's skill where a copy would go says `foreign`
(`reapply::Why::refused`), by the same codes: one condition, met by another
command, and named by core, so no front end decides which of core's
conditions a code names. One type for both rather than a code on each
outcome, since a copy that is somebody's refuses an install and an
uninstall alike: two types would give `foreign` twice, which the rule below
forbids.

- **Unique across core**, not only within one type. A code reaches the panel
  without the type it came from, so `workspace_busy` has to mean the lock
  whichever command met it. Unique within a type was the other way, which
  leaves the panel to know which type each code came from, and the server
  hands it none.
- **A variant that carries another error takes its code** when that error is
  what went wrong and the variant adds only where, or what came of it: an
  install that met a busy lock says `workspace_busy`, as the lock does. One
  that says something the error it carries does not, a record dropped while
  its exclude entry stays (`still_excluded`), has a code of its own. A type
  whose every variant carries another error, as a status report's failures
  do, gives no code of its own.
- **Named for the condition**, in as few words as tell it from every other:
  the variant's own name where that is enough, and its subject before it
  where the bare name says nothing alone (`exclude_read_failed`,
  `adopt_unfit`).
- **A versioned contract, as a serialized name is.** A code is never renamed
  nor moved to another condition, since a front end acting on it would stop
  without a word. A new variant gets a new code, and rewording a message
  changes none.

Each type is given its codes with `code::codes!`, which writes `code()` as a
match with no wildcard, so a new variant does not compile until it has one,
and the list of the codes it gives (`CODES`) from the same arms, so no code
is given without being listed. Each arm is `Self::` and one variant, with
its fields in braces or parentheses: the macro takes no other pattern, so a
wildcard or an or-pattern, which would give several variants one code and
list it once, does not compile, and neither does an enum given the one
code a struct takes. Every code listed is then one variant's own.
`code::ALL` gathers every type's list, by the module it is defined in, and
a test holds no two codes in it alike, which holds every variant to a code
of its own; a second reads core's source for every public type that derives `Error`, and
finds a new one that never joined `ALL`. A match written by hand beside a
list kept by hand was the other way, and nothing fails when such a list
misses a code. Building one value of every variant in a test, with a match
to say when one is missing, was a third, rejected for the some three hundred
variants it would build, many around an `io::Error`, or a parser's or
SQLite's error a test has no plain way to make, and for a match that says a
variant is missing from it but not from the list beside it.

A member's `error` was the first type to hold one, and the snapshot every
front end reads holds them wherever it says what went wrong ([the read
model](#the-read-model)), each a `code::Said`, the code beside its message;
the CLI's warnings and failures carry them beside their text ([failures are
collected, not raised](#failures-are-collected-not-raised)), which the
panel's backend hands on, each line a run says on stderr with its code and
what stopped it with its own (`backend::Run`). A warning's is printed too,
in the JSON envelope's `coded_warnings` ([JSON, in one
envelope](#json-in-one-envelope)); an error's, which stops a command, is
not printed, and reaches only the panel. So does a refusal of the panel's
backend, in the server's reply ([what a browser may
name](#what-a-browser-may-name)).

**The CLI's errors have codes too**, since what a command could not do
reaches the panel beside what core could not: `CliError::code`, given by the
same `codes!`, which core exports for it. A variant that carries one of
core's errors takes its code, as one of core's does. One that says a
condition core names in words of the CLI's, with the command that would put
it right (`not_managed`, `unknown_project`, `not_installed`), echoes the code
core gives it, listed after `echoed`, which `code()` gives and `CODES` leaves
out, so the panel meets one code for one condition whichever way it was
reached. The rest are the CLI's own (`install_no_projects`,
`not_a_terminal`), and so are a refusal's ([every change is a command
line](#every-change-is-a-command-line)). `mindweaver_cli::CODES` lists them
as `code::ALL` lists core's, and a test holds none of them to be given
twice, there or in core, and each echoed one to be one core gives. A code
the CLI took of its own for a condition core names was the other way, and
would have the panel tell one condition apart from itself.

## The read model

Every front end reads a workspace through one value, `view::Snapshot`, its
read contract: the TUI's now, and the panel's views next, which the server
serves it to (#65) and which draw it from #72 on. It is a workspace in one
read, built and ordered in core from the read types the commands print:

- the workspace: its name, its path, the rule that chose it (`reason`) and
  whether it is made;
- every kind, agent, state and health core knows, from `Kind::ALL`,
  `Agent::ALL`, `State::ALL` and `view::Health::ALL`, so a kind added to
  core reaches a front end that lays its columns out by them with no change
  of its own
  ([D-21](#d-21-the-panels-views-follow-the-capabilities-they-show)), and
  a front end that sorts by one orders them as core does;
- what it holds, counted (`summary`): the projects its registry links and
  the entries that open none, each directory once however many entries
  spell it, as `known` counts them, and a tally of what those projects,
  its library and its shelf hold;
- its library, the workspace's own, with where it keeps each kind, the
  templates `mindweaver add` finds, the `origin` `--from` takes for each of its
  own and a copy installed from one names (`workspace`), and the projects
  holding a copy of each (`holders`), each by the registry entry it is
  listed under. Those are the projects whose install the ledger tells came
  from it (`install::from_the_library`), the rule `mindweaver remove
  --everywhere` takes its copies by, rather than the copies whose origin
  is `workspace`: an install recorded before origins were kept names none,
  and is a copy of the library's own all the same while no loaded
  workspace offers its name. A front end comparing origins would miss it,
  and would hold a rule core holds already. Beside them, each of its MCP
  servers whose definition reads (`servers`, `view::LibraryServer`): what
  `add mcp`'s options would say of it (`given`, `mcp::Given::of`, none when
  it holds what no option says), and for each agent what it cannot be
  given, as `show` says it, what its entry would run or reach, as the
  entry spells it (`mcp::Running`, the line `mcp::Runs` displays, read off
  each renderer's `entry`, OpenCode's in the shape D-12 writes), and what it
  asks before it starts one (`Agent::starts_a_server`, as its word): what
  the panel's MCP page draws, rather than the definition for the panel to
  put through three agents' syntaxes and a shell's quoting;
- each project its registry names, in marker order, a stale entry with why
  it opens none, and a second spelling of one directory as a project of its
  own, since each entry is what `unlink` is given, as the TUI has always
  listed them (a report lists one directory once). A project says its entry,
  path, name, marker, id, agents and settings, where it authors each kind,
  its templates, what its agents miss (`unreached`, `missed`,
  `misplaced`), as `status` says it, how many of its artifacts are of each
  kind and stand in each state (`tally`), and how it stands as a whole
  (`health`, below);
- each artifact a holder holds, with its kind, summary, state, the agents
  that read it, where it came from, where it is held (`targets`: every copy
  found and every place the ledger records putting one), everything
  `validate` says of it, and which copy's file does not read and why
  (`unread`, its own place named beside the reason, since its row keeps
  how a readable copy of the name elsewhere stands, the report's), the
  rules a project's `AGENTS.md` block holds among them
  ([D-23](#d-23-what-a-project-received-is-an-installed-view));
- each MCP server's entry in a project's agents' config files, one an
  agent, an installed one as it stands and somebody's as `foreign`
  (`entries`, as `status` lists them, [what each project
  has](#what-each-project-has)), read from the ledger and those files, as
  D-23 has it, since discovery parses no config file;
- what doctor finds of each entry in its registry, in the order
  `projects` lists them (`doctor`, `doctor::Examined`), told the agent
  homes and the names of the variables set the front end decided
  (`doctor::Machine`), and told what `status` says each project's agents
  miss, which the project was read for already ([what no agent
  says](#what-no-agent-says));
- the workspaces it loads, each with how many of its own it offers; its
  shelf and the sources it is filled from; the workspaces this machine
  knows, each with whether it loads it and by which entry, and whether it
  could (`view::KnownHere`: one that opens, is not this one, is loaded by no
  spelling yet and has a path a command line can hold), which only a
  comparison of where each path leads tells, so a front end offering a load
  offers what `mindweaver load` takes; the latest actions, newest first
  (`view::HISTORY`); and the warnings that belong to none of those. Each
  load, each thing on the shelf (`view::Shelved`) and each source
  (`view::ShelfSource`) says its
  `origin`, what `--from` takes for what it offers and what an install of
  that records, each spelled by the one function that spells it for an
  install (`ledger::Source::origin`, `install::loaded_origin`): a second
  spelling of a load the first's, as its offers are. The shelf's rows and
  the sources are the ledger's with the origin flattened beside them, the
  names `gather list --json` prints an offer from the shelf by, rather than
  a shape of the snapshot's own; working the origin out in the panel, from
  the address and the branch, was the other way, which would have put the
  rule for `--from` in TypeScript;
- everything there is to install (`offers`, `view::Offered`), from
  wherever it is offered, kind by kind in the order an install screen
  lists it, each with its `place` (`workspace`, `loaded` or `shelf`) and
  its `origin`; and each entry's boxes of them that are not empty
  (`boxes`, `view::Boxes`), in the registry's order: for each offer of a
  name the project holds something of or records, the state of the name
  and whether its box is ticked (`view::HeldBox`), every state one can
  stand in listed apart (`box_states`, `State::of_boxes`: what
  `State::of_standing` says of some `install::Standing`, so `states` but
  `authored`, which no offer is), for a legend of the boxes to list rather
  than leave a state out by its name, and each kind none of its boxes can
  be asked about, by the code a plan locks one by (`view::Closed`:
  `no_target` from `install::receives`, or why its entry opens no
  project). Told by the install screen's own functions, over the
  offers read once rather than once a project (`install::survey_among`,
  then `install::holding`), so the panel's matrix opens on the boxes the
  screen opens on, and each box asked as the snapshot shows it plans
  nothing, which a test holds it to. The offers rather than the library,
  the shelf and the loads put together by the panel: only core's list
  leaves out a name no copy can be filed under, and reads what each loaded
  workspace offers, which nothing else in a snapshot lists. In the snapshot
  rather than a query of its own, as the scan is: the boxes are of what the
  workspace holds, which every live update reads again, and the survey
  they need hashes no copy the report does not hash already; the rest is a
  look at each offer's place in each folder a project installs in
  ([open questions](#open-questions)).

One value rather than each front end putting its view together from the
reports, as the TUI did until #62: which unreadable file is which artifact's,
which install nothing of is found, how much a load offers counted across two
spellings of it, how many projects hold something outdated, are answers, and
a second front end working them out would come to a second set of them. So
the panel draws counts the snapshot carries rather than counting the lists
beside them, which would be a rule written in TypeScript (#72). `Snapshot::read` resolves the workspace as a
read does, whatever its inputs intend; `Snapshot::of` builds one for a front
end that resolved the workspace itself, to scan it too. `view::Project::read`
and `LoadedWorkspace::of` read the same way what the TUI shows of a project
no workspace links, and `view::Project::read_examined` what doctor finds of
it beside that read, as a snapshot holds both of every project it lists.

A project's health is the one word a front end sorts and filters its
projects by, from nothing to do to nothing to open:

- **ok**: nothing in it asks for anything.
- **attention**: everything in it reads, and something asks for a command:
  an install outdated, edited, missing or orphaned, which an update, a
  forced one or an uninstall answers; a kind or a rule some of its agents
  read nowhere; a folder its targets name that no install uses; copies of
  one name that do not hold the same; or something `validate` only warns
  of.
- **broken**: something in it does not read, a file, a folder, the ledger
  or its templates, or does not pass `validate`. Said over attention, since
  what to do cannot be told of a file nobody can read.
- **stale**: its entry opens no project.

Somebody's copy (`foreign`) asks for nothing: it is never Mindweaver's to
change, and only an install of its name, which refuses, meets it. Told in
core, where a failure still says whether it is of something that does not
read or of copies that read and disagree, which the `Said` a project keeps
no longer tells. The panel weighing each project's lists itself was the
other way, which would have put the rule in TypeScript, where nothing but a
second copy of it could check it ([open questions](#open-questions)).

- **A read, and nothing more.** The ledger is opened only through
  `ledger_for_reading`, the default workspace is never made for it, and no
  scrub runs ([a read leaves the ledger as it finds
  it](#a-read-leaves-the-ledger-as-it-finds-it)). A test reads one of a
  fixture, and of a home with no workspace, and finds no path made and no
  byte changed.
- **No scan.** What could be linked is a walk of the disk, bounded but not
  free, and a snapshot is what a [live update](#live-updates) rebuilds
  every time something changes. So the scan is a query of its own, run when somebody
  asks, which the TUI runs beside the snapshot and the panel's page of what
  could be linked asks for by hand (`Query::Scan`, [its
  views](#its-views)). A test counts the scans its thread runs, with a
  counter only a test build has, and finds none.
- **Its names are a contract**, as a report's are ([the read types
  serialize](#the-read-types-serialize)), under a version of its own,
  `view::FORMAT`, carried as `format`: the snapshot is no command's report,
  so the envelope's version says nothing of it. It moves when a field goes
  or changes what it holds, and not when one is added. A test pins the whole
  of a fixture's JSON, every key in its order, written out in the test
  rather than kept in a file a snapshot crate rewrites, so a change to it is
  a change somebody reads in the diff. The panel holds it to the same names
  in TypeScript written from these types, which is the contract on its side
  ([its types come from Rust](#its-types-come-from-rust)): a field that goes
  is a panel that no longer compiles, rather than one that reads nothing.
- **Every URL through `url::redacted`, where it is written**: a source's, an
  origin, the log's target and detail, and the message of every error it
  carries, which can quote one. The ledger hands back none with a
  credential, but a row an older build wrote keeps its own until a command
  that writes scrubs it, and a read scrubs nothing; a test writes such rows
  and finds none of them in the JSON.
- **Failures are collected beside what they are about**: a project's in its
  `failures`, an entry that opens nothing as its `stale`, a load that would
  not open as its `error`, a known workspace that has gone in its row, and
  the rest (two projects holding one id, what a load cannot offer, a shelf
  or a log that would not read) in `warnings`. One list of everything was
  the other way, which a front end showing one project would have to sort
  back out by project. A ledger that cannot be read is about every
  project's records and the shelf alike, so each says it, as the TUI's rows
  always have.
- **What went wrong is a `code::Said`**, its code and its message, public
  down to its fields, since the TUI reads the snapshot as Rust rather than as
  JSON.

`list --json` prints a part of it, a `view::Listing`: its holders, the
library and the projects or one of them, each read for the kinds asked, so
that what a script prints of a project is what a front end draws of it, and
a test of the panel compares like with like ([JSON, in one
envelope](#json-in-one-envelope)). Read for those kinds rather than whole
and cut down after, since the listing reads only them: a folder of a kind
not asked for is no failure of its reply. Of every kind, it is the
snapshot's library and projects, which a test holds it to. Of fewer, a
project's `tally` and `health` are of what it read: its health is told
from those kinds and from `misplaced`, which names every kind's folders as
the snapshot does, so a skill that does not read leaves `ok` a project
listed for its rules. Reading every kind for the health alone was the
other way, which would weigh a failure the reply says nowhere.

What a front end reads beside the snapshot is what no snapshot holds: a scan,
the folders directly inside one, for the panel's explorer to choose one from
(`Query::Folders`, core's `browse::Folders`, [choosing a
folder](#choosing-a-folder)), which no command line has, since a shell has
`ls` and completion and a front end with no terminal lacks both, a plan of
the boxes somebody ticked and unticked, asked for each time they
change and never kept beside the snapshot (`Query::Plan`, [a batch is
planned in core](#a-batch-is-planned-in-core)), what an update of every
project would do (`Query::Updates`, [bringing what was installed up to
date](#bringing-what-was-installed-up-to-date)), what core says of a draft
about to be written (`Query::Validate`, [authoring in the
library](#authoring-in-the-library)), what the panel may switch to, which
depends on where it was launched (`Query::Workspaces`, [switching
workspace](#switching-workspace)), and which workspace `mind`
consults from each project, which depends on the
front end's own `-W`, activation and home, and which the TUI asks to know
when a row's lines need `-W .` ([the TUI looks where it was
told](#the-tui-looks-where-it-was-told)). The screen is the snapshot laid
out: every test of the TUI passed unchanged when it moved to reading one.

## How entries are stored

`mindweaver link` records a project as a route **relative to the workspace**, so
the workspace and the projects under it can be moved together without the
registry going stale. Entries are written with forward slashes, so a workspace
registered on one platform still resolves on the other, and a command reports
the entry the way the file spells it rather than the way the local separator
would.

The arithmetic is lexical (`apps/core/src/paths.rs`): it never touches the disk
and never resolves symlinks. That keeps a path in the shape the user typed and
lets a route be computed for a directory that need not exist.

But arithmetic on paths is only true when no component is a symlink. If the
workspace root is spelled through one, a `..` climbs out of the link's target
rather than out of the directory the name suggests, and the route points
somewhere that does not exist; `/tmp` is a symlink to `/private/tmp` on every
Mac, so this is not exotic. So `link` **checks its route against the
filesystem** before storing it, and falls back to an absolute path when it does
not land where it should. The check happens there and only there: its answer
decides which spelling to store and is never stored itself, so entries stay
portable rather than frozen to one machine's symlink layout.

Matching is by where an entry **points**, not by how it is spelled, so
`collapse` and `./collapse` are one entry. Arithmetic settles most of it, and
two spellings only the filesystem can equate are settled by canonicalising.

`loaded` and the default workspace's `known` are stored and matched the same
way ([the workspaces this machine knows](#the-workspaces-this-machine-knows)).
An entry dropped from any of the three leaves its place's layout to the entry
after it, so dropping the first of `["a", "b"]` leaves `["b"]` rather than
`[ "b"]`, and a list laid out a line each keeps its lines. It takes its own
comments with it and leaves everybody else's: the ones above it and the one
after it on its line go, while a comment above the entry after it, or at the
end of the line before it, stays where it was. toml_edit keeps what comes
before an entry as one string, from the comma ahead of it, so the end of the
line above and the comments above the entry share it. Each is split at its
first line break: the part before it stays with the line above, the part after
it with the entry below. Handing the next entry the dropped one's string whole
would put the dropped entry's comment above it in place of its own.

`link` and `unlink` match against **the array they are about to edit**, not
against the copy parsed when the workspace was opened. The marker file is meant
to be editable by hand, so that copy can be stale, and acting on a stale index
is how you remove a project nobody asked you to remove. An edit that changes
nothing does not rewrite the file at all.

`link` is idempotent and `unlink` is not, deliberately. Linking twice means
"make sure this is registered", and it is; the call reports the spelling
already in the file rather than the one it would have written. Unlinking
something that was never there is a typo far more often than a no-op, and
saying so turns a silent success into a fixable mistake. `unlink` removes
*every* entry pointing at the project, because two spellings of one directory
are one project and removing half of them while reporting success would leave
it registered. It takes a path rather than a project because the entry most
worth removing is one whose directory has moved away, and that cannot be opened
as a project any more.

Rewrites go through a temporary file and a rename, so a crash leaves either the
old config or the new one. The temporary is named for the run that writes it,
never shared with another writer ([Writers at the same
time](#writers-at-the-same-time)). The original's permissions are copied onto the
temporary first, and a symlinked config is followed to the file it names;
otherwise an edit would quietly widen a chmodded config or detach a shared one.
A hard link is the case this cannot preserve: a rename breaks it, and keeping
it would mean giving up the atomic write.

### An entry whose project moved

A route is what moving the project changes. Moved alone, a repository leaves
its entry stale, and every record of what was installed into it names a
route nothing is at, so its copies read as somebody's once it is linked again
where it went. Moved with the workspace, the route stays true, which is why
it is a route; moved without, nothing in the entry can follow it. What does
move with it is the `id` its settings hold ([D-09](#d-09-project-settings-live-in-the-project)),
128 random bits, so that only a copy shares it. So the ledger keeps the id it
last saw at each route ([schema 5](#why-the-ledger-is-sqlite)), and a
project found somewhere else holding an id last seen at an entry where no
project is any more is that entry's project, moved. `mindweaver scan` says so
(`moved: <old> -> <new>`, [scanning](#scanning)), and `mindweaver relink <old>
<new>` (`relink::relink`) follows it.

The route stays the key of every record, with the id beside it
(`installations.project_id`), rather than the id taking its place. A project
a `mind.toml` marks has no id and goes on being found by its route, and so
does one whose id nothing recorded yet; keyed by the id, both would need a
second way in, and every record written so far re-keying.

The ledger learns the ids from the projects themselves, never from the
entries: a step to schema 5 is told those of every project the workspace
links, and the ledger, opened to write, is told them again every time
(`WeaverWorkspace::ledger_to_write`, `Ledger::knew`), as the scrub runs every
time, so an entry written by hand, a project given a new id and one whose
route holds another project now are all followed without each command that
links remembering to. A stale entry keeps the id last seen there, which is
what finds its project, and only a relink moves it. An id two linked
directories hold is told to nobody.

A relink rewrites the entry where it stands, with the comments around it,
which a person wrote about the project rather than about where it was, and
moves every record and every file Mindweaver made there to the new route in
the same transaction as the rewrite of the marker (`Ledger::relinked`): an
entry moved without its records would leave them under a route no entry
names, and records moved without it would name a directory the workspace
does not link. It holds the workspace's lock from before it reads the ledger
for its checks. A local project's exclude lines go with it
([what keeps an install out of git](#what-keeps-an-install-out-of-git)). It
refuses, with nothing written:

- a directory holding another id than the one last seen at the entry, which
  is another project, and one holding the id the ledger last saw at another
  entry, which is where it moved from, if anywhere;
- an entry that still opens a project, which has not moved: with the same id,
  a copy; and one whose marker is there and would not open (a typo, a newer
  version), which still holds its project, whose id nothing read, so a copy
  of it would pass for it moved (`MindProject::is_vacant` tells the two);
- another linked project holding the id, a copy too;
- a project a `mind.toml` marks, which has no id (moves of those are found
  by nobody, by design);
- records at the entry with no id, as a ledger from before ids were kept
  holds them for a project not read since: nothing says they were that
  project's, and a record is what lets Mindweaver write over and delete what
  sits at the path it names.

A workspace with no ledger, or whose ledger records nothing at the entry,
has nothing to keep, and the entry alone moves.

A repository copied with its `.mind` holds the same id as the one it was
copied from, and no record can tell which of the two it was made for. So a
copy is never taken for a move: two directories holding one id, among what a
scan found and what the workspace links, or among what `status` reports, are
a failure collected with the rest that names every one (`relink::Duplicate`),
and a relink refuses one it can see. `mind init --new-id` (`relink::new_id`)
gives a copy an id of its own and keeps every other byte of its settings:
the one change `MindProject::change_settings` refuses, an id naming the
project for good, made apart from it (`MindProject::change_id`) through the
same rewrite and toml_edit. A local copy's exclude lines follow it to the new
id, and a workspace linking it knows it, and every record of it, by the new
one, so what was installed stays the project's. A `settings.json`, which
nothing but `mind migrate` changes, is pointed there.

## Gathering

A workspace has a **shelf**: artifacts collected from somewhere else, held by
the workspace and belonging to no project yet.

```
.mindweaver/
  weaver.toml
  mindweaver.db
  lock                       held by whoever is writing
  cache/<source>/            the clone
  skills/<source>/<name>/    what was taken out of it
```

Gathering fills that shelf and stops. Nothing is written into a mind project,
because which of the gathered skills a project should carry is a separate
decision made by somebody, and a `git clone` that edited repositories on the
way past would be the wrong kind of convenient. It also keeps the two halves
independently testable: what a source yields does not depend on what any
project wants.

### Everything is namespaced by its source

Two repositories may both offer `commit-style`, and both are worth having:
choosing between them is what the shelf exists to make possible. So each source
owns a folder, named after its URL and recorded once, and the same name from a
different source is a different thing rather than a collision. This is the same
answer `Catalog::find` gives inside a project (return both, decide nothing),
one level up.

The folder a skill lands in keeps the name the **source** gave it, not the name
the skill declares. When those disagree that is something gathering warns about
and `mindweaver validate` reports; renaming the folder on the way in would repair
the symptom and hide it.

### The clone is kept, and re-made

The clone stays under `cache/` so a gather can be looked at afterwards, and
because what a source actually contained outlives the report about it.

It is re-cloned rather than fetched into. The clone is shallow, so a re-clone
costs about what a fetch would, and it is one code path instead of three:
clone, fetch, and reconcile a checkout somebody may have edited. Gathering is
not something anybody runs in a loop.

Placing an artifact **replaces** its folder rather than merging into it, so a
file the source deleted does not survive as a leftover of an older revision.
An artifact that has not changed is left alone entirely, down to its
modification times, which is what lets a second gather answer the only question
worth asking the second time: what moved.

Failures are collected, as everywhere else: one skill with broken front matter
is a warning beside the forty that came through, not a reason to be told
nothing. One that reads but breaks a rule is taken, with a warning for each
problem ([Strict when writing, lenient when
reading](#strict-when-writing-lenient-when-reading)).

### Following a gather, and stopping it

A gather is the network, so it can take long enough that somebody wants to
know how it is going, and to change their mind. A `gather::Request` carries a
`Watch`: a callback the gather tells each `Progress` to, and a flag that
cancels it. Both are core's own types. `gix` has a progress tree and an
interrupt flag of its own, and handing those to the front ends would tie each
of them, the server that will stream progress (#112) included, to gix's
version and to the names it gives its steps.

The steps are connecting, receiving objects, resolving deltas, checking out
files, reading the folders that hold a manifest, `n` of `m`, and placing what
was read on the shelf, `n` of `m`. The counts during the fetch are gix's: the
clone hands it a progress tree and reads it from a thread of its own ten times
a second, because gix counts most of its work on shared atomics, from threads
of its own, without telling the progress anything. The counts are picked out
by the identifiers gix gives them, not the names it shows, which it is free to
reword. That thread is joined before the clone says its next step, so a front
end never hears a count after the step that follows it. The callback is a
plain `Fn` that is `Send` and `Sync`, rather than a channel, so each front end
chooses how the news travels: the CLI writes a line, the TUI sends it to the
thread that draws, and a server can stream it.

The flag is gix's interrupt, so a cancel stops the fetch or the checkout where
gix checks, and the clone it had begun is removed. gix does not check it while
it connects, only once the pack is coming, so a cancel while connecting waits
for the connection to be made or to fail: as long as a server takes to answer,
or a prompt to be answered. One made while the gather waits for [the
workspace's lock](#writers-at-the-same-time) waits too, since nothing checks
the flag until the lock is held: as long as the writer holding it takes, and a
wait that runs out ends the gather as busy rather than cancelled, unlogged all
the same. The harvest checks it once more after every folder
is read and before anything is placed, which is why it reads them all first
rather than reading and placing each in turn. That is the last moment a cancel
is honoured: past it the shelf and the ledger change, and a gather stopped
halfway through that would leave them neither as they were nor as the source
has them. A cancel that comes later is too late, and the gather finishes.
`Progress::cancellable` says which steps are which, so a front end that offers
a cancel stops offering it when the gather stops honouring it, rather than
knowing where the check is. Cancelled in time, the shelf and the ledger are as
they were: the gather is not logged, and a source it registered is forgotten
again. It is registered before the fetch because the clone needs the folder
name registering gives it; holding the registration in a transaction until the
gather ends was the other way, and is rejected, because every other write to
the ledger would wait as long as the network does. The clone is not part of the
promise: every gather re-makes it, so a cancelled one can leave a known source
with the new clone, or with none.

`mindweaver gather` on a terminal draws each step over the last on one line of
stderr, and wipes it before the report. Piped or captured it draws nothing,
so what it prints there is what it always printed. The binaries run every
line with that line's watch (`run_watched`, `run_weaver_cli_watched`), and
`run` and `run_weaver_cli`, which the tests call, with one nobody follows. The
TUI runs a gather on a thread of its own, and `esc` cancels it ([A gather runs
apart](#a-gather-runs-apart)).

### Why `gix` rather than running `git`

Nothing depends on a `git` being installed, or on which `git` is first on the
PATH, and core keeps doing its own I/O rather than supervising a process. The
cost is that authentication for private repositories is ours to solve rather
than inherited whole from `git`.

Half of it is solved: a fetch **never prompts** for a credential. When a server
asks who is fetching, `gix`'s own answer runs the credential helpers git config
names and, when none of them has a credential, asks a person: an askpass
program, then the terminal. The TUI owns the terminal while a gather runs, so
that prompt is drawn over, its keys raced for, and the server that comes next
has no terminal at all. So every connection `gix` lets a clone configure is
given a credentials callback (`gather::credentials`) that runs the same helper
cascade with the prompt and askpass turned off. A credential no helper has is
`GitError::Credentials`: the URL without its credentials, and the two ways to
provide one, a git credential helper or, for an ssh URL, ssh-agent. Over ssh,
`gix` runs the system `ssh`, which authenticates with keys of its own and asks
the helpers only for a user name, so a login ssh refuses is reported the same
way. Whether a fetch went over ssh is read from the URL `gix` connected to, not
the one typed: git config's `url.<base>.insteadOf` can turn a typed https URL
into an ssh one, or the reverse. What ssh itself asks on the terminal is
outside the callback's reach, and so is the one connection `gix` opens without
letting the clone configure it; both are [open questions](#open-questions).

The callback is set on the connection rather than asked for through git
config (`gitoxide.credentials.terminalPrompt=false`), which would also stop
the prompt: the guarantee then lives in code a test reaches, not in a string
among the user's own settings, and the credential a front end supplies needs
the callback to arrive through anyway. That other half is
[D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt)'s.

`gix`'s error types are large and would become part of this crate's public API
if they were carried, tying its version to gix's, so `GitError` keeps what they
said and not what they were.

### A URL is kept without its credentials

A source is stored, logged, listed and named in errors by its URL, so a token
typed into one would be repeated everywhere: in `mindweaver.db`, in the
clone's `.git/config`, in every report, in the snapshot every front end reads
([the read model](#the-read-model)), in `--json` output and soon in the
panel. One function takes it out, `url::redacted`, and
everything that keeps or shows a URL goes through it.

What it takes out is everything before the `@` of a `scheme://` URL, except
the user name of an ssh URL: that is who ssh logs in as, part of where the
repository is rather than a secret, and a later gather from the stored URL
needs it. Over http(s) a user name goes as well, because GitHub takes a token
in that place and a credential helper is what names the user. The scp-like
form (`git@host:path`) is left alone, since `gix` reads everything before its
first `:` as user and host, which leaves no room for a password. A password
with a `/` in it is looked for past the first `/`, whenever what comes before
that `/` has a `:` and no port: every parser reads `https://me:p/ss@host` as
host `me` and port `p`, so nothing else would find the password, and the
refusal for an unparseable URL would store and print it whole. It works on
text rather than on one URL, because the other place a credential turns up is
`gix`'s own error messages, which quote the URL they failed on, and the URL a
fetch connected to may carry a token the typed one did not when git config
rewrites it with `url.<base>.insteadOf`. Parsing with `gix::url` and dropping
the user was rejected: the parser also rewrites what it keeps, a default port
dropped or a scheme lowercased, and cannot find a URL inside a sentence.

A gather URL that carries a credential is **refused** before anything is
registered or fetched, rather than used for that one fetch and stored without
it: a token on the command line is already where `ps` and the shell history
show it, and refusing is what points at a git credential helper, which keeps
it off the command line altogether, as the front-end provider of
[D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt) will.
So is such a URL typed where the branch or the folder goes: the ledger keeps
a source's branch and folder word for word, since neither is a URL, and a
`GitError` would quote the branch, so a token typed there, found by the
suite that tries every credentialed form in every place a gather takes text,
was stored and printed until it was refused alike. Behind that refusal the rest holds whichever caller is writing. The ledger
redacts every URL on the way in and again on the way out, so nothing it hands
back carries a credential, scrubbed yet or not. Every `GitError` redacts the
URL and what `gix` said. A clone is scrubbed as soon as it is made (the gix
adapter's `Remote::scrub`, [values, ports and
adapters](#values-ports-and-adapters)): `gix` writes the URL into the remote's entry in `.git/config`
and into every reflog as `clone: from <url>`, and those files are rewritten
through the same function. A clone a gather makes now has nothing to take out,
and finding that costs a few small reads. A `--from` typed with credentials
names the source they were taken out of. The CLI's report prints URLs
redacted, and a task's command line is redacted where it is built for
showing (`task::shell`), so the box that says what is running and the
transcript printed on the way out both leave the token out.

What a Mindweaver from before the refusal stored is **scrubbed** when a
command that writes opens the ledger. `WeaverWorkspace::ledger_to_write` is
the one way to open it for writing: it runs `gather::scrub`, which only core
can call, and hands back what that took out for the front end to say, the way
`resolve` with `Intent::Write` is the one way to find a workspace that may be
created. Which commands write is the front end's to know, but what opening to
write involves is core's, so no front end can write first and scrub after.
`gather git`, `install` and `uninstall` open it that way, as do
`install::disown` and `install::carry`, through which core's lifecycle changes
(`lifecycle::add`, `rename` and `remove`) drop or carry a record. Each hands
back what the scrub took out (its result's `scrub`) beside what it did to the
record, since a scrub says what it found only the first time and a change that
dropped it would leave it unsaid for good. `rename` opens the ledger once for
each name and tells what both scrubs found once (`Scrub::absorb`): the second
finds only the clones the first could not rewrite. So does the install screen,
in both screens, when what it marked is applied rather than as it opens,
which is a read ([A read leaves the ledger as it finds
it](#a-read-leaves-the-ledger-as-it-finds-it)): applying runs the `install`
and `uninstall` lines its marks amount to, and the first of them opens the
ledger to write, scrubbing it, as from the shell. A scrub can make two sources
one row, and a mark on the row that went would install from folders the scrub
removed, or copy the files and then fail to record them. Each install line
names its offer by its origin (`--from`), which is shown without credentials
before a scrub as after it, so once a scrub has taken out a source the line
finds the row it became one with, through the lookup any `--from` goes
through; a line for what only the row that went had gathered is refused as
not offered, said under that line. Scrubbing as the screen opened would have
spared that, and is rejected: opening it is a read. An edit writes
nothing to the ledger, since it keeps the record as it was, so `finish_edit`
only reads it (`ledger_for_reading`) and scrubs nothing.

The scrub rewrites the sources whose URL carries a credential, each clone made
from one, the origin of every installation from one, which schema 3 copied
from the URL as it was stored, and the action log. A clone is rewritten before
its row, so a clone that cannot be rewritten keeps its row tainted and is
tried again next time, as a warning; what was taken out is a note, said once,
since the command did what it was asked and an exit code that failed because
of a fix would break a script for nothing it did. When the URL without
credentials is already another source's, with the same branch and folder, the
two are one place gathered twice and become one row: two rows would offer each
skill twice under one origin, and SQLite's UNIQUE would refuse the rewrite
outright when a branch is set. The row kept is the one whose shelf was filled
most recently, since its folder holds the newer files, and a row whose gather
failed before it took anything loses however recently it was tried. What the
other row gathered goes with it, and its clone and shelf folder are removed (a
folder that will not go is a warning), rather than its records moving to the
kept row: they would point into a folder the ledger no longer reserves, which
the next source with the same readable name would be given and gather over.
The ledger turns on SQLite's `secure_delete`, so a rewritten row leaves no
copy of its old text in the file's free space.

It runs every time rather than once, as a schema step would, because an older
build could keep writing to the same file after a newer one had scrubbed it,
and the schema steps are spoken for by the changes that need a new version.
Schema 3 is a file such a build refuses, so the scrub could now be part of
that step; whether it should is open (see the open questions). With
nothing to do it still reads, every time a command writes: a scan of the
sources and one of the action log, in a transaction. Neither can use an index,
so the log's grows with the log, and every entry naming an ssh URL, which
keeps its `@`, is read and found clean again each time. Recording that the log
had been scrubbed would skip that, and is rejected for the reason the scrub
runs every time: an older build writing a token again would go unnoticed. The
cost ends when the scrub can stop (see the open questions). A read is left
alone: what it shows is redacted on the way out anyway, and a read should not
write. Until a command that writes scrubs it, a place an older build gathered
both with a token and without one is read as the file has it, two sources
offering each skill under one origin. Deleting the old
clones instead of rewriting them would have been simpler and was rejected:
the clone is kept so a gather can be looked at afterwards, and a scrub has no
reason to take that away.

### Why the ledger is SQLite

Everything else Mindweaver writes is a file a person opens: TOML with comments
explaining itself. `mindweaver.db` is not, and the exception is deliberate. It
answers questions a file cannot (which of two identically named skills came
from which repository, at which revision, and what happened the last four times
a gather ran), and answering them from a flat file would mean writing a query
engine badly.

It sits in `.mindweaver/`, beside the marker it belongs to, so it travels with
the workspace and two workspaces never share a history. It carries its schema
version in SQLite's own `user_version` header field, and a database from the
future is refused exactly as a marker file from the future is. The first
command that writes to it makes it; a read never does, and changes an existing
one only to migrate it ([A read leaves the ledger as it finds
it](#a-read-leaves-the-ledger-as-it-finds-it)).

Six things are recorded: the **sources** gathered from, the **artifacts** on
the shelf and which source each came from, the **installations** (which skill,
rule or MCP server Mindweaver put into which project, from where, every copy it
wrote and a hash of what it wrote, or for a server every entry it wrote into
an agent's config file and the fingerprint of each), the **files Mindweaver
made** in a project, which it
may take away again, the **projects** by the id each was last seen holding,
and an **action log**. The log keeps failures too: a log
of only what worked cannot answer the question anybody opens it with.
Timestamps are Unix seconds, which needs no date library and which SQLite
renders with `datetime(at, 'unixepoch')`.

No URL this build writes into it, or reads back out of it, carries a
credential. One an older build stored stays in the file until the next command
that writes scrubs it, and for good in the copy a schema step took before that
(see the open questions). An older build could store one again after a scrub
until schema 3, which it refuses
([A URL is kept without its credentials](#a-url-is-kept-without-its-credentials)).

A source's shelf folder is stored rather than derived: two URLs can reduce to
the same readable name, and where a source's files went is a fact about the
past that must not move when the naming rule changes.

The schema has had six versions, each a block in the SQLite store's
migration (`adapter::sqlite::Sqlite::migrate`) and a
constant beside it:

1. `sources`, `artifacts` and `actions`: what was gathered, from where, and
   what happened.
2. `installations`: which artifact went into which project, which is what
   lets an uninstall tell Mindweaver's copy from somebody's.
3. `installation_targets`, one row for each copy an installation wrote, with
   the agent it is for when it is one agent's
   ([D-07](#d-07-one-copy-per-agent-folder)); and on `installations`, the
   `origin` it came from, spelled as `--from` takes it, and the
   `content_hash` of what it wrote
   ([D-04](#d-04-an-edited-copy-is-detected-not-disowned)). The
   installation's own path moved into its one target, so a copy's path is
   kept in one place (#30).
4. A `content_hash` of each target's own, for a kind whose copies hold
   different bytes: a rule's section of the block in `AGENTS.md` and its
   file for Claude ([installing rules](#installing-rules)), each of which
   can be edited apart from the other. A skill's targets leave it empty,
   every one holding the tree the installation's hash names. And
   `created_files`, each file Mindweaver made in a project rather than
   wrote into, so the block goes with the last rule and the file with it
   only when Mindweaver made it (#45).
5. `projects`, the id the ledger last saw at each route, one id a route and
   one route an id, and on `installations`, the `project_id` each record was
   made for, so a project that moved is found again by its id ([an entry
   whose project moved](#an-entry-whose-project-moved), #57).
6. `installation_entries`, one row for each entry an MCP server's
   installation wrote into an agent's config file
   ([D-10](#d-10-an-mcp-server-is-one-owned-entry-per-agent)): the agent,
   the file as a route from the project, the server's name it is keyed by,
   and the `fingerprint` of what was written ([the
   fingerprint](#the-fingerprint)), one entry an agent (#93). A table of its
   own rather than more targets: a target is a copy found by its path, and
   an entry is found by its key in a file every server of the project
   shares. The definition's origin is the installation's `origin`, which
   version 3 added, so the step adds nothing for it.

An older ledger is migrated step by step, so one made two versions ago ends
where one made today begins, and two rules hold for every step:

- **The file is copied aside first.** Before the first step, the ledger is
  copied beside itself as `mindweaver.db.v<old>.bak` (`Sqlite::back_up`), and
  nothing is migrated unless the copy is whole and opens at the old version.
  A step that turns out wrong then leaves the file it started from: the copy
  opens at the old version, and the build that wrote it reads it as it
  always did. It is named after the version it holds, so which build reads
  it is in its name, and a ledger brought forward by several builds in turn
  leaves one for each version it had. It is taken once the migration holds
  the write lock, not before: a copy taken after an unlocked read of the
  version could be of a ledger another process has since migrated, and would
  replace that process's copy of the old version with one of the new. It is
  SQLite's online backup rather than a copy of the file, because opening the
  file outside SQLite while a connection holds it would drop that
  connection's locks on Unix, and it reads through a second connection,
  because SQLite will not copy from one in the middle of writing. It goes
  through a temporary and a rename, so a copy cut short is never mistaken
  for a whole one, and the temporary is made with the ledger's permissions
  before anything is written to it, so a ledger kept private is not readable
  through its copy even while the copy is written. A step to come is one
  more block in `migrate`, and the copy is taken for it without its asking.
- **The steps run in one transaction**, with the stamp of the new version.
  Run a statement at a time, a step that failed, or a process that died
  halfway, left a database half migrated under its old version, which the
  next open would try to migrate again and fail on.

A step fills in what it can from what the version before recorded, spelled
as that version's code spelled it, in the step's own SQL rather than through
the code that spells it now: a later change to the spelling would otherwise
change what an earlier step wrote. Version 3 gives an installation from a
source that source's origin, and one from the workspace or a load none, since
version 2 kept nothing to say which; it makes up no hash for a copy nobody
hashed when it was written, and D-04 says how a missing one reads. Version 4
fills in nothing: no build before it installed a rule, and a skill's targets
need no hash of their own. Version 5 fills in what no SQL could: the file
kept no id, so the step is handed the id of each project the workspace
links, opened for its settings, with the route its records are under
(`Ledger::open_knowing`), and its own SQL writes them in. The one exception
to what a step is filled from, and the reason for it holds: what it is handed
is what the projects hold, not how today's code spells something. A project
a `mind.toml` marks, an entry that will not open and an id two linked
directories hold are handed in for none, and their records keep their route
alone. Version 6 fills in nothing: no build before it installed an MCP
server. D-04's states (#47) took no step of their own:
what tells them apart is the hash each copy was written with, which versions
3 and 4 keep, and what the place it came from offers now, which is read where
it is offered rather than kept. A record with no hash is held to its offer
by the code that reads it (`install::standings`), since a step has no offer
to fill one in from, and the first install over it stores its hash. Each step
comes with a fixture of what the version before it wrote
(`apps/core/tests/fixtures/ledger-v<N>.sql`, dumped from a ledger that build
made), from which a test migrates, checks every row is kept, and opens the
copy at the old version.

The hash is `hash::tree`, its only definition: SHA-256 over every folder and
file under a copy, by their `/`-separated paths in byte order, each file's
length before its bytes. The order is not the filesystem's and the separator
not the platform's, so one tree hashes alike on Linux, macOS and Windows, and
alike in every agent's folder, since the copy's own folder is not part of it.
Folders count, so two copies hash alike exactly when `copy::same` calls them
the same. A link is refused rather than followed: Mindweaver never writes one
into a copy, since `copy::tree` copies what a link points at, and following
one could read anything. For a skill it is kept once per installation, not
once per target: every target is written from one copy, so one hash is what
each is compared with, and a copy somebody edited is the one whose own hash no
longer matches it. A rule's targets each keep their own, `hash::bytes` of
what each holds, since its section and its file are not the same bytes. An install records the hash of the copy as it stands once
written; one that cannot be read back is recorded with none rather than not
at all, since without a record it would be Foreign for good. That is a
`warning:` and a non-zero exit, and the log's entry says why, rather than a
plain success: a record with no hash is read as one from before hashes were
kept. No install leaves a copy it can tell holds what the hash refuses. A
link where no install writes one is an edit, which an install that is not
forced refuses and a forced one writes over, even when it points at the
same bytes, which `copy::same` follows it to (`Stage::place_over`): left as
it was, the copy would be recorded with no hash and read as edited still.
What is left is a copy changed between landing and being read back.

## Installing

Gathering fills the shelf; installing is the other half, and the only thing in
Mindweaver that writes into a mind project. A skill is copied wherever the
project's agents read it ([D-07](#d-07-one-copy-per-agent-folder)): one copy
in each of its targets (`install::targets`), the folders the target table
sends its agents to (`MindProject::targets`), so a project naming all three
gets a copy in `.claude/skills` for Claude and one in `.agents/skills` that
Codex and OpenCode share, and one whose settings name `targets` a copy in
each folder they name, for all of them ([a project's own
targets](#a-projects-own-targets)). A project a `mind.toml` marks names no
agents, and gets one copy in the folder its marker names, as before. A rule is
no folder, and reaches a project's agents otherwise: through the block of its
`AGENTS.md` and Claude's files, rendered with every other rule it has
([installing rules](#installing-rules)). Nor is an MCP server, whose payload
is an entry: one in the config file of each agent the project names, by the
fingerprint of what was written there ([installing an MCP
server](#installing-an-mcp-server)).

Not the folder a project's settings say it authors in
([D-06](#d-06-projects-keep-authoring-beside-the-code)): what the workspace
offers goes where the agents read, what the project writes itself stays where
it writes it, and neither is mistaken for the other. Two targets that are one
folder on disk, `.claude/skills` linked to `.agents/skills`, are one target
under the first spelling, as discovery reads them once; two copies put in
place in one folder would find each other there.

### The folder is named twice, on purpose

Gathering keeps the folder name the **source** used. Installing files the copy
under the name the skill **declares**. That looks inconsistent and is not.

On the shelf, renaming would repair a symptom and hide it: a skill whose
directory disagrees with its front matter is something `validate` exists to
report, and the shelf is a record of what a source actually contained. Inside a
project the same mismatch is not a report, it is a skill that lists fine and
never loads: the directory name is how an agent finds what it was told to
invoke. So the copy is filed under the declared name, and the disagreement
stays where it belongs, upstream.

### Held to the rules on the way in

What gathering took leniently, installing takes strictly: a skill whose name
or description breaks the rules is offered, so nothing gathered is hidden, and
refused, so nothing an agent would skip lands in a project. The check reads the
files about to be copied rather than what the ledger noted when they were
gathered, because the shelf is a folder anybody can edit and the workspace's own
are edited as a matter of course. For the same reason it compares what they
declare with the folder the copy is about to be filed in, the name the ledger
recorded: a shelf copy edited to declare another name, or a skill its source
renamed while the record of its old name stayed, would otherwise land under one
name declaring another. A refused skill is fixed upstream and gathered again,
or, if it is the workspace's own, fixed where it is.

### It only manages what it installed

The ledger records every installation against `(project, kind, name)`, with
each copy it wrote as a target of it (its path, and the one agent it is for,
or none for `.agents/skills`, which two agents share), and that record is what
separates a file this tool put there from one somebody wrote. Standing is
worked out over every target, by the path each record names rather than by
its name (`Ledger::copies`, as a listing marks installed copies): a skill
present anywhere it would be written, and unrecorded there, is
`Standing::Foreign`, whatever the other targets hold. Foreign is inert in both
directions: never overwritten, never deleted, and the caller is told so,
with the path, rather than obeyed quietly. It becomes Mindweaver's only when
somebody adopts it by name ([adopting what a project already
keeps](#adopting-what-a-project-already-keeps)), as it is. So somebody's skill in
`.claude/skills` refuses the whole install, and `.agents/skills` is not
written either: a copy beside it would hand OpenCode, which reads both, two
different skills of one name. Only the targets refuse, as D-07 has it, not
every folder an agent reads (`Agent::reads`): a project that names OpenCode
and not Claude is sent nothing in `.claude/skills` or `.opencode/skills`,
though OpenCode loads what is there, so somebody's `ship` in one of them does
not stop `.agents/skills/ship` from being written, and OpenCode is then
handed two versions of one skill, which nothing says ([an open
question](#open-questions)). A name installed into one folder does not claim
what somebody keeps under it in another, as when a project names an agent
after the install. Where every copy there is recorded, what each copy the
record names holds says how the installation stands ([an edited copy is
left alone](#an-edited-copy-is-left-alone)): one gone since is
`Standing::Missing`, and the next install puts it back. A listing says the
same of a copy in a project's agents' folders
(`Provenance::Foreign`), and no change to the project's own reaches one
([read where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)).

Uninstalling goes by the record, not by where the project would install now:
every copy it names is removed, and nothing else, so a folder of that name in
a target the record does not name stays somebody's. A route the record holds
is followed only when it is a folder of the project named after the skill
(`InstallError::Recorded` otherwise): before records had targets, a removal
was only ever the name joined onto a folder of the project's, and the ledger,
though the workspace's own, is not a reason to delete anywhere else.

Without that, an install screen is a thing that can delete a colleague's work
because a checkbox looked untidy. The rule costs one query and removes the
whole class.

The record names the project by its route from the workspace, taken to where
the workspace's own entry for it leads (`WeaverWorkspace::spelling_of`)
rather than to however the command spelled the project. `mind` spells its
project the way the operating system reports the directory it runs in,
through no symlink, while a workspace named with `-W` or activated keeps the
spelling it was given. The route between those two is not the entry's, so a
record written under it would be one `mindweaver -p` never finds: a copy nobody
may update, or, the other way round, a record that outlives its copy. A
spelling that already matches the entry as written is kept, so no record
written before is named differently now.

A project has one directory per artifact name in each target, so two shelf
entries offering `commit-style` cannot both be installed. The screen settles
it by unticking the other rather than letting both apply and the second win: a
conflict resolved where somebody can see it happening. A plan handed both
says so rather than choosing ([a batch is planned in
core](#a-batch-is-planned-in-core)).

### An edited copy is left alone

The record keeps a hash of what was written in each copy
([D-04](#d-04-an-edited-copy-is-detected-not-disowned)): once for a skill's
installation, since every copy is staged from one tree, and once a target
for a rule's section and its file, which hold different bytes. That hash,
and what the place the record says it came from offers now
(`install::offered_for`), tell five standings of an installation apart, as
`install::standings` says of each installation and `survey` of each offer of
its name:

- **Current**: every copy holds what was written, and the offer holds it
  still.
- **Outdated**: every copy holds what was written, and the offer has moved
  on. An update brings it up to date, as an install of it does ([bringing
  what was installed up to date](#bringing-what-was-installed-up-to-date)).
- **Edited**: a copy holds anything else: a hand edit, `mind edit`, a
  skill's rename, which rewrites its `name` line, a file added, or a link
  where a file was, which the hash refuses and no install writes. A rule's
  rename moves its file as it is, and is no edit: the copy holds what was
  installed, orphaned while nothing offers it by its new route.
- **Missing**: a copy the record names is not where it was put, and an
  update puts it back, as an install of it does. Every copy gone is still missing rather than absent:
  the ledger records it, and `mindweaver uninstall` forgets it. The install
  screen opens its box empty all the same (`Standing::present`), saying a
  copy is gone, so marking it puts every copy back, as it did when every
  copy gone read as absent: a `git clean -dX` takes every copy of a local
  project at once (and its `.mind` with them, an open question). Held, the
  box could only be marked to be taken out, so one with a copy left is
  taken out with `mindweaver uninstall`.
- **Orphaned**: the place it came from no longer offers it. No offer carries
  it onto the install screen, so `standings` is where it is said, and
  `status` and an update say it from there.

When more than one holds, edited is said first, then orphaned, missing,
outdated: an edit is what a change must not undo, an offer gone leaves
nothing to put back or bring up to date, and a copy put back is put back as
the offer holds it now. Outdated asks for the offer itself, read where it is
offered: a skill's copy is compared with it as an install would write it
(`copy::same_leaving`, without what no copy holds), a rule by the hash of its
file against the one its record says every copy was rendered from.

An edited copy is treated as a foreign one is. `install` and `uninstall`
refuse the whole change, nothing written or taken out in any target, and name
each edited copy (`Installed::Edited`, `Removed::Edited`), unless forced
(`install_with` and `uninstall_with` given `Edits::Forced`, which `--force`
asks for). Every copy or none: writing over the untouched copies beside an
edited one would hand the agents reading both folders two versions of one
skill. The look is made under the workspace's lock, after the one for
somebody's, and wherever the record has its copies, since an install records
its copies afresh and one in a folder it no longer writes would be left with
no record. Forcing reaches only what the ledger records: a copy nothing
records stays out of reach whatever is asked. The install screen never
forces, and holds an edited row's box as it holds a foreign one's. An
update leaves an edited installation as an install does, every copy of it,
and says so, unless forced.

A rule's copy from before a project had settings, in the folder its
`mind.toml` named, is not one of the copies told in a project marked by
them. The next render takes it out while it holds what was copied and
leaves it, the project's own, when it does not, forced or not ([moving a
project to its settings](#moving-a-project-to-its-settings)), so an edit
there refuses nothing: refused, it would be a change `--force` does not
make either.

A record with no hash, from before version 3 of the ledger or of a copy
that could not be read back, is held to its offer byte for byte: current
while its copy matches, edited otherwise, and edited when nothing offers it
any more. An untouched copy whose offer has moved on then reads the same as
an edit, and taking it for one is the safe mistake. The first install over a
copy that matches stores its hash, and from then on it is told as any other.

### Bringing what was installed up to date

`mindweaver update` and `mind update` (`update::plan`, `update::apply`) act on
the standings: each installation outdated or missing is written again from
the place its record says it came from, as that place offers it now
(`Surveyed::source`), and nothing else is. A current one has nothing to
change. An edited one is left, every copy of it, as an install leaves it,
and said (`Step::Leave`) unless the update is forced, when it is written
over and the copies it wrote over are said (`Step::WriteOver`). An orphaned
one has nothing to be written from, edited or not, so it is said and left
as the project holds it (`Step::Orphaned`): a note, never a failure, since
nothing could have been done for it, and `uninstall` is what takes it out.
A record from before origins were kept may stand for an offer of the
workspace's own or of any workspace it loads; the one holding what an
untouched copy holds is where it came from, and with none left to tell by
it is not guessed (`Step::Unsure`), as an install with no `--from` does not
guess between two.

- **Written as an install writes.** Each skill, and each rule copied into a
  project a `mind.toml` marks, goes through the install's own body
  (`install::install_held`), so its copies are staged and put in place
  together, its record and hash written afresh, and a local project's
  exclude entries written before a copy lands. A copy a local project lost
  comes back kept out of git as it was: one deleted by hand, or the
  `AGENTS.md` it made, which a `git clean -X` takes, since it is a file in a
  folder git tracks. Every other copy is in a folder of its own, which only
  `git clean -dX` goes into, and that takes the project's `.mind` too, so
  the project has to be marked again before an update can find it (open
  questions). Under the lock the body looks again at everything an install
  refuses, so a copy edited between the plan and the write is left all the
  same.
- **One lock, a plan made under it.** `apply` opens the ledger to write,
  takes the workspace's lock, reads each project again and makes the plan
  again under it, then writes every installation, as `reapply` and
  `migrate` hold it from plan to write. The lock is not re-entrant, so the
  install it writes with is the body `install_with` hands to once it holds
  the lock, rather than `install_with` itself. A workspace with no ledger
  file is given none: it records nothing to update.
- **Failures are said where they happened.** A project that cannot be read
  again refuses the whole update before anything is written. After that,
  each installation is written whole or not at all, and one that fails
  says so in its own `done`, beside the rest, which go ahead; the same
  update run again finishes what is left.
- **A project's rules come together.** The block in `AGENTS.md` is one file
  written whole, so a project's rules are brought up to date by one render
  (`rules::update_held`), which gives each rule as its offer holds it now.
  Updating one rule therefore brings each other outdated or missing rule
  of the project along, and the plan names each, so the lines say what was
  written; leaving them behind would mean reading each back from the block,
  as a render reads back only an edit it keeps. That render keeps a rule no
  longer offered as the project holds it, as an uninstall of another rule
  does, rather than refusing the rest for it, and writes over only the
  edits of the rules the update was forced to write over. Every other rule
  somebody edited, named or not, is kept whole (`Change::leaves`): each
  copy as the project holds it, the one not edited too, and its record as
  it was. A rule has two copies where Claude reads no block, its section
  and its file for Claude, and keeping only the edited one, as an install's
  render does, would write the other from the offer and leave Claude and
  the agents reading the block with two versions of one rule.
- **Names narrow it.** Only the installations the names given name are
  planned, by kind and name as `install` takes them, every one when none is
  given; a name that matches nothing installed in any project asked is
  said, as a typo usually is.
- **A dry run without `--yes`**, which reads the ledger without making it,
  as every dry run does, and says the same lines with the same exit status:
  an edited copy left alone is a warning either way, as an install refused
  over it is, since what was asked for is not all done.
- **Nothing is gathered.** An update copies what the shelf holds now; a
  gather is what moves the shelf, and refreshing a source is its own
  command (#100).

The CLI lays it out (`commands/update.rs`): a line for each installation
written, `updated`, `put back ... at`, or `updated ..., writing over the
change at`, then, for a project's rules, what the render said of the rules
it wrote (`Rendered::warnings_of_update`), but no edit it kept, which the
update says itself.

A front end reads the same plan of every project the workspace links as
one value (`update::Updates`, #80), the panel's page of updates: each
installation an update says something of, every one not current, by
project in the registry's order and each directory once, with its kind,
name and origin, its state as `status` says it, what an update does with
it as a word (`update::Does`, the step without what it names), the copies
that step names as routes from the project, for one it leaves, why
(`update::Left`, as a `Said`), and whether a change reaches it only forced
(`forced`): a copy changed since it was installed, which an update not
forced leaves and, once nothing offers it, an uninstall not forced leaves
too, so the panel sends `--force` by core's word rather than telling the
refusal its change would meet from the state and the step. Beside them,
how many stand in each state that asks for a command
(`State::asks_for_a_command`, the rule a
project's `attention` is told by), whether updating every project, and
each one, writes anything, and each entry that opens no project. Why it
leaves one is core's words with a code, so the panel says it as core does
and acts on the code: an edit left alone goes by the code an install
refuses an edited copy with (`edited`, echoed rather than given again,
since a code names one condition wherever it is met), and an offer gone
and a place no copy tells have codes of their own (`no_longer_offered`,
`origin_unknown`), no install being refused for either. A read, as the dry
run is, and a query of its own rather than a part of the snapshot: telling
what an update does reads where each installation came from again, which
the snapshot, read again on every change, has no need of. The CLI keeps
its own words for the same three, in the past tense once it has run; the
panel's are present, since they describe what an update would do. `mindweaver update` takes `-p` once for each project or
`--all`, refusing neither given, and `mind update` the project it is in,
through the workspace that manages it, as `mind install` does. Both resolve
it as a read, so neither makes the default workspace in home, not even with
`--yes`: one made for the occasion links no project, and the update would be
refused all the same.

### Installing an MCP server

An MCP server's payload is no copy but an entry, one in the config file of
each agent the project names (`Agent::mcp_file`), written by that agent's
renderer ([Claude Code's `.mcp.json`](#claude-codes-mcpjson), [Codex's
`.codex/config.toml`](#codexs-codexconfigtoml), [OpenCode's
`opencode.json`](#opencodes-opencodejson)), every byte outside it kept
(`install::mcp`, #94). It is one installation a project, as a skill is,
recorded with an entry target for each file it wrote and the fingerprint of
what was written there ([how an entry stands](#how-an-entry-stands)): an
entry carries no mark of its own, so that record is the one thing that
makes it Mindweaver's (D-10). A project a `mind.toml` marks names no agent,
and is given none (`no_target`).

- **Who is given what is decided first.** An agent whose renderer refuses
  the definition, a shape it cannot express ([what each agent cannot be
  given](#what-each-agent-cannot-be-given-of-a-server)) or an input read
  from no variable, is not reached, and said beside the installation with
  that refusal's code and reason (`install::mcp::Rendered`'s `unreached`);
  the others are given it all the same, as a rule about some files only
  reaches Claude alone, since refusing every agent over one would keep a
  server from the agents that can run it. So is an agent whose file git
  tracks in a local project that does not set `edit_tracked` (D-02), by the
  code a block of rules withheld from such a file is said with
  (`withheld_tracked`), as Claude's `CLAUDE.md` is left alone there; and
  Codex in a project its user's config does not mark trusted, which is
  written all the same and counted as reaching no Codex
  (`codex_mcp_untrusted`), since trusting a project is the user's to do. A
  name the user's own Codex config holds (`codex_mcp_collision`) is met as
  Codex's entry is written, before its file is read, and leaves Codex
  unreached the same way. A server that reaches none of the agents the
  project names is refused, each reason said (`mcp_unreached`), with
  nothing written.
- **All of it or none.** Each file is read before any is written: one its
  renderer would refuse to write into refuses the install; somebody's
  server of the name, which nothing records in that file for that agent,
  refuses it whole, as a copy nothing records does (`Installed::Foreign`),
  though each agent reads a file of its own; and so does an entry changed
  since it was written, wherever the record has it, unless forced
  (`Installed::Edited`). Then each file is written in report order, and the
  first that fails puts back those written before it as they were, a file
  made taken away with the folder made on its way, and records nothing, as
  a record that cannot be written does too. An install over current
  entries writes nothing, each renderer leaving a file that holds what it
  would write as it is (`Installed::Unchanged`).
- **An agent its renderer now refuses loses what an earlier install
  wrote.** When the definition has changed so that an agent's renderer
  refuses it, an install over the installation, an update's included,
  takes that agent's entry out of its file as an uninstall would, and out
  of the record, the agent said as not reached: left there, it would go on
  running what the library no longer gives it, reading as outdated while
  no update could write it again. An edited one refuses the install first,
  unless forced, as anywhere else. An agent whose file is withheld, which
  git tracks in a local project without `edit_tracked`, keeps its entry,
  recorded as it was, since taking it out is an edit of that file too and
  dropping the record would leave the entry nobody's: it reads as current
  or outdated as its offer has it, and an update leaves it until the file
  may be edited.
- **Out of git, only what it made.** In a local project each config file
  the install makes is kept out of git, a line a file in the project's
  group of the exclude block, written before the file lands and taken back
  out when it does not land ([what keeps an install out of
  git](#what-keeps-an-install-out-of-git)), and the ledger notes that
  Mindweaver made it (`created_files`), for the file to go with its last
  server and only then. One somebody had, git tracking it or not, is
  theirs to commit, and gets no line.
- **Uninstalling goes by the record.** Each entry it names is taken out of
  the file it names, OpenCode's whichever of its four that was
  (`opencode_mcp::remove_from`), and nothing else, every other server and
  byte there kept; a file Mindweaver made goes with its last server, as its
  renderer decides, and its line in the exclude block with it. One
  somebody took away is forgotten with the record the same way, its line
  and its mark (`created_files`) with it, whatever server another record
  still names there: the line would keep the file written there later out
  of git without a word, the mark would have a later uninstall take that
  file away as Mindweaver's, and the install that puts the other server
  back makes the file, and so marks it again. An entry
  changed since refuses the uninstall unless forced, as an edited copy
  does, and forcing never reaches an entry nothing records: with no record,
  a server of the name is somebody's, and said (`Removed::Foreign`). An
  entry in a file git tracks in a local project without `edit_tracked`
  refuses it too (`mcp_entry_unedited`), as a rule's section in a block
  that may not be edited does. Each file is changed in turn, and the first
  that fails puts back those changed before it and keeps the record.
- **An update writes through the install's body.** An outdated or missing
  installation is written again from where its record says it came from
  (`install_held`), an edited one left, every entry of it, unless forced,
  and an orphaned one said and left ([bringing what was installed up to
  date](#bringing-what-was-installed-up-to-date)).
- **Named as every kind is** (#95). `install::installs` says yes of a
  server, so `survey` and `receives` hand one to `install::mcp`, the
  snapshot's boxes of one are read as its entries stand, and a name typed
  bare is asked of it wherever a skill's is ([which kind a name
  is](#which-kind-a-name-is)): `mindweaver install -p collapse github`. Its
  record says where it came from, which is what tells the box it holds
  (`install::is_current`, as a rule's). `install::targets` refuses it by a
  code of its own (`not_copied`), since it is never a copy in a folder,
  which keeps every caller asking a kind's folders from reading one for a
  server.
- **What each agent will run is said before it is written.** Who is given
  what is decided first (`decide`), as above, and a preview asks the same
  of each renderer's plan with nothing written (`install::mcp::preview`,
  `codex_mcp::planned` and `opencode_mcp::planned`, which read and refuse
  as their `render` does, the user's Codex config included). What an
  install hands back, and a preview, says of each agent the program it
  runs or the address it reaches as its own entry spells it
  (`mcp::Runs`, read off the entry by its renderer, never spelled again
  from the definition: Claude is given an input as `${VAR}`, OpenCode as
  `{env:VAR}`), what its renderer said of the entry, each agent not
  reached by the code of why, and each variable the entries read where
  the agents run (`Rendered::variables`), a secret's marked. The CLI
  prints them under the install's line, the agents not reached and what a
  renderer wrote an entry without as warnings with their codes, Codex in
  a project it does not trust among them, and once something is written,
  which variables to set and what each agent asks before it starts the
  server (`Agent::starts_a_server`, from [docs/agents.md](agents.md#what-an-agent-asks-before-it-starts-one)):
  Claude to approve it in a session, OpenCode nothing, Codex nothing more
  than the trust its warning is about. An update that writes one says
  the same, and so does its dry run of what each agent would be given
  (`Update::previewed`, the install's preview), wherever the definition
  came from: a definition changed since its install, a loaded
  workspace's or the shelf's among them, is shown before the `--yes`
  that writes it, as an install's is. A line printed before the write
  would have needed the CLI to print as it runs, which its outcomes,
  built and then printed, do not; the dry run below is where the lines
  come before anything is written.
- **One from elsewhere asks first.** A server is a command every agent in
  the project runs, or an address each sends what it is given to, so one
  whose definition does not come from the workspace's own library, a
  loaded workspace's or the shelf's, is written only once somebody says
  yes (`install::asks_first`, `install::Confirmed`): without `--yes`, at
  either level, core's batch previews it rather than installing it
  (`assign::install`), and the command says what each agent would run and
  `nothing installed`, writing and recording nothing, with the exit
  status the install would have. The preview comes to what the install
  would: `would update` what the ledger records already, and nothing to
  say yes to, `already current`, while each entry it would write is in
  its file as recorded and none would be taken out (`Installed::Unchanged`
  rather than `Unconfirmed`). A batch whose every part is only previewed
  reads the ledger without making it (`ledger_for_reading`), as an
  update's dry run does: opened to write, it would be made where there is
  none, told the projects' ids, and scrubbed or migrated where an older
  build left one, for a command that writes nothing. One with a part
  refused is asked again of the ledger opened to write, since a refusal
  read before the scrub can be one it takes away, two rows of one shelf
  source offering a name twice until then. The library's own is written as
  a skill is: it is the workspace's own, written by whoever keeps it. Asking of
  every server was the alternative, and would ask of the library's own
  what a skill is never asked. The panel asks it on the matrix, as the
  install screen does ([its changes](#its-changes)): a plan hands each such
  install's lines beside its task (`Planned::asks`), and the task carrying
  the yes, which the panel sends in its place only once those lines are
  ticked as read.
- **The writers hold the lock.** An install and an uninstall hold the
  workspace's lock from before the record is read until it is written, as
  `install::install` does, an update writing through the body that takes
  none; the renderers take none ([writers at the same
  time](#writers-at-the-same-time)).

### Built where no agent looks

An install never writes beside the copy it replaces. It builds the new copy in
a stage of its own under the project's `.mind/tmp` (`copy::Stage`), and renames
it into place once it is whole. Beside is where the agents look: OpenCode's
first line globs hidden folders in a skills folder too, so the
`.deploy.mindflayer-new` that staging used to put next to `deploy` was a skill
it could load half made, and one an interrupted run left there it would load
every time after. A name taken from the skill was also shared by every run
installing that skill at once, each deleting what the other was building.

- **`.mind/tmp`, made by the first install.** Inside `.mind`, a folder
  Mindweaver owns and no agent reads, and inside the project, so putting the
  copy in place is a rename on one filesystem. The system's temporary folder was
  the alternative, and it is often another filesystem, where a rename fails and
  a second copy would be the half-made state all over again. A move across
  filesystems is not attempted: a rename that fails is an error naming both
  paths. The folder holds a `.gitignore` of `*`, written with `create_new`, so
  nothing staged is committed even while `.mind` is. A rule of its own rather
  than an entry in [the exclude block](#what-keeps-an-install-out-of-git), for
  the reason [D-01](#d-01-a-projects-mind-ignores-itself) gives `.mind` one:
  a folder Mindweaver owns can carry it, so it needs no repository and goes
  with the folder. It is not the team's `.gitignore`, which is never edited.
- **A stage per run.** Named after the run's process id and a random suffix,
  and made with `create_dir`, which refuses a name that is there rather than
  sharing it: two installs at once, from two terminals or two workspaces that
  link one project, never build in one folder. The copy is built in the
  stage's `new`, the one it replaces moves aside to its `old`, and the stage
  goes when the install is done.
- **What an interrupted run left is swept by the next, and said.** Every run
  holds a shared lock on `.mind/tmp/.lock` while its stage lives. A run that
  can take the lock exclusively is alone, so every stage it finds was left by
  a run that stopped halfway: it removes them, and `Installation::swept` names
  them for the CLI to print under what the install did. When the copy then
  fails, `InstallError::swept` names them instead, printed before the error:
  they were cleared all the same. A ledger that fails after the copy stops
  everything, as it does wherever it fails, and they go unsaid with the rest.
  A run that cannot take the lock exclusively sweeps nothing, because what it
  finds may be in use. A lock rather than the process ids in the names,
  because whether a process still runs is a question each platform answers
  differently, and a lock the system drops when its process dies answers it on
  all of them. Only folders named the way a stage is are swept; anything else
  in there, somebody else put there. A stage that will not be removed is left
  for a later run, unsaid, rather than failing this one: it is litter in a
  folder nothing reads. On a filesystem without locks nothing is swept at all,
  since no run there can tell a live stage from a leftover, and what is left
  stays until somebody deletes it; the names still keep runs apart. Any error
  taking the lock counts as such a filesystem, not only an unsupported one: NFS
  without its lock daemon says "no locks available", and refusing that would
  make every install there fail.
- **What earlier versions left beside is cleared too.** A project that
  upgraded can still hold the `.deploy.mindflayer-new` or `-old` of an install
  an earlier version did not finish, and no sweep of `.mind/tmp` would find
  it. The next install of that skill removes both, whether or not it changes
  anything else, and says so as it says a swept stage. It does so without the
  lock, because no run of this version stages there, and those versions
  cleared each other's anyway. One it cannot remove fails the install, as it
  did when staging there, rather than staying where an agent reads.
- **The folder the copy goes in is made when it is missing.** Staging beside
  made it on the way. A fresh clone has no empty folders, so a project's
  `skills/` can be missing although its marker names it.
- **Every copy from one tree, all or none.** An installation's copies are
  built once in the stage, and each target that does not already hold that
  tree gets a copy of it (`copy::Stage::place`), so the copies are the same
  bytes even when the offer changes while they are made, a workspace's own
  skill being edited say; copied each from the offer, OpenCode, which reads
  both folders, could be handed two versions of one skill. A target that
  already holds it is left alone, down to its times. Only once every copy is
  whole is any put in place, a rename each, and one that cannot be put in
  place takes those already moved back out, the copies they replaced put
  back: each target is as it was, and nothing is recorded. The installation
  is `Added` when every copy is new, `Unchanged` when none is, and `Updated`
  otherwise, a copy put back in one folder included. One hash is recorded,
  of the copies as they stand, which every one of them holds; copies that
  cannot be hashed, or that turn out not to hold the same, are recorded with
  none and said, as one copy that cannot be hashed always was.

So an agent reading a skills folder finds the old copy, the new one, or, for
the moment between the two renames of an update, neither, but never part of
one and never anything beside it. Gathering still stages beside the shelf copy
(`copy::replace`), because no agent reads `.mindweaver`; see the open
questions.

### Two places offer, one command takes

What can be installed comes from the workspace's own skills, offered first,
the workspaces it loads (below), and the shelf (`install::Offer`); a rule from
the first two, by its route, since nothing gathers one. Every
installation records its origin, in the spelling `--from` takes; one from the
shelf also points at the source it came from, and one of the workspace's own
at none, because it came from nowhere but the workspace.

`mind install <name>` and `mindweaver install -p <project> <name>` install one
without the screen, through `install::offered_typed`: the one candidate of that
name, or an error naming every place that offers it when there is more than
one; `--from` then says which, because which of two lands in the project's one
folder of that name is not something to guess. It takes an origin as printed,
and looser spellings of one: a source's address without its branch, a loaded
workspace's name. The origin as printed wins (`install::offered`), so
`<url>`, one address gathered at its default branch, is that offer and not
also `<url>#main`'s, whose address it is: refused as offered twice, the
first would have no `--from` of its own, and the line the install screen runs
for its box would be refused.

### Which kind a name is

A name is typed bare or after its kind, `rule:git/commits`, with the colon
every command takes ([why a qualifier uses a
colon](#why-a-qualifier-uses-a-colon)), and core reads it (`Reference`).
Bare, it is the one kind that offers it (`install::offered_typed`), asked
only of the kinds a copy could be filed under that name
(`install::is_installable_name`), so `git/commits`, which no skill can be
called, is a rule's however it is typed. A name two kinds offer is refused
(`InstallError::OfferedAsKinds`), naming what to type for each, as `mind show`
refuses one: a skill and a rule of one name are two things. Defaulting a bare
name to a skill, what every install meant before rules could be installed, was
turned down: a route would need its qualifier for nothing, and a name both
kinds offer would quietly install the skill. When no kind offers it, the first
refusal that says more than that nothing is offered is the one given, so a
project with nowhere to put a skill is told so, as it was. `--from` is asked
of each kind as typed, and only when that kind is offered nothing, as a path
to a loaded workspace (`install::resolve_from`): asked once for every kind,
the refusal of a project with nowhere to put a skill would hide a rule the
path offers.

A server's name is a key (`mcp::name::is_key`), so `github` is asked of the
skills, the rules and the servers alike, and `git/commits` of the rules
alone (#95). A kind with nowhere to go in the project is then said over
another that offers nothing of the name only when the name is offered as
that kind: a project a `mind.toml` marks names no agent to give a server,
and a typo in a skill's name there would otherwise be answered with why no
server can go in, rather than that nothing of the name is offered.

Taking one out reads the ledger instead (`install::uninstall_typed`): a bare
name is the one kind recorded under it, one recorded as two is refused
(`InstallError::InstalledAsKinds`), since taking both out would be twice what
was asked, and one recorded as neither changes nothing whichever kind it is
taken for, so each kind it could be says what is there, somebody's of either
before the first's absence.

A line about installing says a rule as `rule <route>`, as `add` and `remove`
say one, and a skill by its name alone, as every such line did before: a
route reads like a path, and one name can be both.

### One artifact into many projects

`mindweaver install <name>` takes `-p` once for each project, or
`--all-projects` for every one the workspace links, and `mindweaver uninstall`
takes them the same way: vision point 2's assigning, from the command line.
Core's `assign::install` and `assign::uninstall` are the batch, and each
project's part of it is the install or uninstall that project would have
alone (`install::offered_typed`, then `install::install_with`, or
`install::uninstall_typed`), so a project reached this way is held to every
rule it is held to reached alone, and a front end asking for many decides
nothing that one install would not.

- **Each project once, in the order asked.** Two spellings of one directory
  are one project (`same_directory`), as an update takes them.
- **A part for each, beside the rest.** A project offered nothing of the
  name, one whose install fails and one whose copy is somebody's each say so
  in their own part, and the others go ahead. Each project's copies and
  record are written together or not at all by its own install, so nothing
  one project's part does depends on another's.
- **The ledger opened once, the lock taken by each install.** The batch
  opens the ledger to write, scrubbing it, once, and only that refuses it as
  a whole; a batch of no project opens nothing. Each project's install takes
  the workspace's lock as it does alone, in the lines the install screen
  runs as anywhere, rather than the batch holding it across them: nothing
  spans two projects, so holding it once would keep every other writer
  waiting for all of them, and would need a body of each install, rules'
  included, that runs under a lock it does not take, as an update has
  (`install::install_held`), to guard nothing a project's own install does
  not.
- **The front end names the projects.** `-p` picks each as everywhere
  (`WeaverWorkspace::pick`), and one the workspace does not link refuses
  the line before anything is installed anywhere: a typo in one name is no
  reason to change the rest without it. `--all-projects` is every entry
  that opens, with a warning for each that does not, as `mindweaver update
  --all` takes them, and resolves the workspace as update does, as a read:
  a default workspace nobody has made links no project, and making it for
  a command that then does nothing would leave a marker in the home for
  nothing. The flag is `mindweaver migrate`'s `--all-projects` rather than
  update's `--all`, which beside an artifact's name would read as every
  artifact.

The CLI says each project's part as an install into it alone says it
(`commands/install.rs`), in the order asked, so each line names its project
already, but for what rendering the project's rules warns of: that names a
rule or a file and not the project, which alone goes without saying, so
among several it is said after `<project>: `. One refused ends in
`error: <project>: ...`, and the command exits non-zero when any part did
not go through, while the others' lines stand, since what they did was done.
Two things are said once rather than once a project: what the loaded
workspaces could not offer, after every part, and a refusal every part
shares, such as a name nothing offers, which is the command's error. An
uninstall from several passes over a project that has nothing of the name,
which is as it was asked to be and what most projects answer when every one
is asked; when none of them has it, that is the typo it is for one project,
and refused as one. A batch of one project, named or the only one linked, is
said exactly as one install was before.

### A batch is planned in core

Ticking boxes is how both front ends assign: the install screen's, each a
project's row of an offer, and the panel's matrix (#79), each a project and
an offer. What the boxes ticked and unticked amount to is planned in one
place, core's `install::plan` (#78,
[D-18](#d-18-the-panel-writes-by-running-commands)), which is where every
rule of a batch lives: neither front end holds one, and the panel holds none
in TypeScript.

- **Which offer of a name a project holds** (`install::holding`): one box a
  name, the one its installation came from as the ledger records it
  (`install::offered_for`), however many places offer the name. Among
  several that a record from before origins were kept may have come from,
  or when nothing it came from offers it any more, the one whose content
  its copy is (`install::is_current`), then the first. Somebody's copy
  ticks every offer's box, being there whichever is asked for and moved by
  none, and an installation whose every copy has gone ticks none, for
  ticking it to put them back. By the record before the content: the
  content alone, which told it until #78, ticked the first offer for an
  installation outdated or edited since, which is no offer's content, so
  the screen ticked the first place's box whichever it came from, and the
  panel, which reads where each came from off the snapshot, would have
  drawn another box than the screen.
- **What a selection amounts to** (`install::Plan::of`): each box as it
  stands (`install::Asked`: its cell, its project's standing of the name,
  and whether the project holds it from there), one project's boxes of one
  name planned together, since the project has one copy of a name in each
  folder it installs in. One offer asked for, and not held from there, is
  installed from there, and a held one asked away beside it is replaced by
  that install rather than taken out first, which would leave the project
  without it were the install to fail. With none asked for, a held one asked
  away takes the name out, by its name. The changes are the lines the
  commands take already (`install::Change`): each offer's install one line
  with a `-p` for each project asking it, always with `--from`, each name's
  uninstall another, kind by kind in `Kind::ALL`'s order, each where the
  first box asking it is, so the screen runs the lines it ran before.
- **What cannot be planned is said, never guessed.** Two offers of one name
  asked of one project, or one box asked both ways, is a `Conflict`
  (`install::Clash`: `wanted_twice`, `asked_both_ways`), and nothing of the
  name is planned in that project. A change to somebody's copy, or to one
  edited since, is `Locked`, by the code the command would refuse it with
  (`Standing::refused`: `foreign` and `edited`, `install::Refused`'s), rather
  than by a second code for one condition. The other projects go ahead.
- **A read.** `install::plan` opens the ledger through `ledger_for_reading`,
  takes no lock, writes nothing and makes no default workspace. It reads
  each box against the offers its project is surveyed for, by its origin
  as printed, and, where none of them is from there, against what the
  ledger records, so an installation nothing offers any more is asked away
  by the origin the snapshot says it came from. It plans from that origin
  too, so a credential typed into a box neither splits one offer into two
  nor reaches a line. A box the disk cannot be
  asked about is locked by why, before the rest: a project that does not
  open, an offer its origin does not make (`not_offered`, or what
  `install::receives` says when the project has nowhere to put the kind,
  `no_target`), and a bare name two kinds are offered as from its origin
  (`offered_as_kinds`). One asked for holds back the boxes of its name in
  its project, each asking a change locked by the same why, as an edited
  copy holds back its name's: planned without it, a held offer asked away
  beside it would be taken out rather than replaced, which a load that
  stopped opening or a gathered entry gone since the snapshot would do as
  much as a typo. A project the workspace does not link refuses the whole
  plan, as a `-p` that picks nothing refuses its whole line.

The install screen plans through the same function, from the standings it
surveyed when it opened (`Screen::task`): every box that disagrees with how
it started, each project named as its lines name it
(`WeaverWorkspace::naming`), the box an offer ticked in its place unticked
asked away all the same, for core to see it replaced. Its keys still hold a
box somebody's or edited, and untick another offer of a name when one is
ticked, which is what the screen shows; neither is a rule of the batch,
which core says again of whatever it is handed. The panel asks the CLI's
backend (`Query::Plan`), whose answer is each change as the task that runs
it (`Planned`, by `Task::from`), for the panel to send back as one
`apply_install`, as the screen applies its marks: it never builds a task of
its own. Leaving the plan in the screen's `install/state.rs`, where the
rules lived until #78, was the alternative, rejected: the panel would have
had to spell them again in TypeScript, or go without a matrix.

The panel's matrix (#79) marks boxes as the screen does, and holds less:
it opens on the boxes the snapshot ticks (`boxes`, [the read
model](#the-read-model)), and sends only the boxes somebody changed, each
marked the other way round from what it shows, in the order marked, a box
marked back being no mark. It unticks nothing of its own when another offer
of a name is ticked: ticking it alone asks for that offer, which core plans
as the install that replaces the one held, and ticking a second asks for
two, which core says clash (`wanted_twice`). Nor does it hold a box
somebody's or edited: marked, the plan locks it, by `foreign` or `edited`,
and the panel draws the lock on the box and says why below, before anything
runs. Sending every box as it shows, rather than those changed, was the
other way, and would ask core about hundreds of boxes to plan a few, and
every offer of a name somebody's copy ticks would clash with the others. A
box of a kind its project can take none of, which the snapshot closes, is
the one it never marks: the plan would lock it by the same code
(`no_target`), which the snapshot says already.

### Taking out what nothing offers

An orphaned installation, one whose origin no longer offers it, stays as the
project holds it: no update can write it, and no install screen offers it
to untick. `uninstall --orphaned` takes every one out of the projects asked,
`mindweaver`'s by `-p` or `--all-projects` as an uninstall by name takes them and
`mind`'s the one it runs in, through core's `assign::uninstall_orphaned`,
which is the batch above with `install::orphaned` choosing each project's
names rather than the command line.

- **What nothing offers, edited or not.** `install::orphaned` takes each
  installation whose record `offered_for` matches to nothing, which is the
  orphaned standing and the edited one that is orphaned beneath it, an edit
  being said first; an update leaves both alike (`Step::Orphaned`). Each is
  uninstalled as by name (`install::uninstall_with`), so the edited one is
  refused unless forced, every copy of it, and said with `--force` as the
  way out, as an uninstall by name says it.
- **Not one from a loaded workspace that is away.** A loaded workspace that
  cannot be opened offers nothing, so what was installed from it reads as
  orphaned, as `status` says, while it is unmounted or moved. Taking it out
  on that would take out what that workspace offers still once it is back,
  so a record naming its origin is passed over while it cannot be opened,
  and one from before origins were kept while any loaded workspace cannot
  be, since it may be that one's. What the loaded workspaces could not
  offer is said after the rest, as an install says it, so what was passed
  over has its reason beside it.
- **Nor one whose artifact cannot be read.** A skill whose front matter no
  longer parses, or a folder that cannot be listed, offers nothing, so what
  was installed from it reads as orphaned too, as `status` says; but it is
  still there, somebody's to mend, and failures are collected, never a
  reason to take out what they are about. So a record is passed over while
  the place it came from, the workspace's own (`install::own_failures`) or
  a loaded workspace's (`LoadFailure::Artifact`), has a failure about the
  artifact's place or a folder it sits in (`install::orphaned`, by
  `lifecycle::place`, since what cannot be read cannot say its name). The
  workspace's own failures are said first, before the loaded workspaces',
  as the own is offered first.
- **None is no failure.** A project with nothing orphaned is as it was
  asked to be, so it says nothing, and every project asked having none is
  said in one line, exit zero; a project that cannot be surveyed, or an
  uninstall that fails, is an error of its project while the rest stand.
- **No ledger is made to say so.** The ledger is opened to write only when
  there is one, as an update opens it: with none, nothing was installed to
  be orphaned.

The panel's page of updates runs it for every project the workspace links
(`Task::UninstallOrphaned` of the workspace, `mindweaver uninstall
--all-projects --orphaned`), or `mind uninstall --orphaned` for one, never
forced, asked about first as every uninstall is. An edited copy it leaves
is taken out from its own row, by the forced uninstall core says it needs
(`update::Pending::forced`), asked about for the edit too.

### What no copy holds

A skill's own `.claude-plugin` folder, at its top, goes into no copy
(`install::LeftOut`), and the install says so with a `warning:`: Claude loads
a skill folder holding one as a plugin, with the hooks and MCP servers it
declares ([docs/agents.md](agents.md), docs-only, so it is left out whatever
Claude does with it). A skill is instructions an agent reads when it chooses
to; a plugin's hooks run commands on their own and its servers are started
for it, which installing a skill must not turn on behind anybody's back.
Gathering a plugin's skills brings that folder along, and the shelf keeps it,
as it keeps whatever a source held.

Left out of every install, whichever agents the project names, rather than
only out of Claude's copy: one copy differing from the other is what D-07
rules out, and a folder a project's `targets` name may be Claude's too. Only
at the top, where Claude looks: further down, a skill that teaches plugin
writing ships one as an example, and it is the skill's like any other file.
Left out rather than copied and deleted, so nothing in it is read, and a link
in it pointing outside the source refuses nothing. A copy is compared with
its offer without it (`copy::same_leaving`), so the next install of an
unchanged skill is `Unchanged`, and the install screen still knows which
offer the project holds.

### Never into an agent's own folder

A target is relative to its project, so in a project whose root is the
user's home `.claude/skills` is Claude's skills for every project, and
`.agents/skills` the one Codex and OpenCode read everywhere. An install there
would be the user-scope install
[D-22](#d-22-user-scope-installs-come-later) leaves for later, behind a
`--user` of its own, and nothing stops `mind init` in a home. So an install
whose copy would land inside one of the agents' own folders is refused
(`InstallError::UserScope`), naming the folder and who reads it, before
anything is looked at or written.

The table cannot tell, and core has no home to compare with: the front end
hands `install` the agents' folders (`AgentHomes`, [the agents' own
folders](#the-agents-own-folders)), resolved as far as each exists, so a home
reached through a link is still the folder it is. Inside rather than equal,
so a project kept in one of them, under `~/.claude` say, is refused too: it
is an odd place for a project, and nothing there is any one project's.
Refusing a project at the home outright was the other way, and was turned
down: whether a folder reaches every project depends on where the agents
read, which `AgentHomes` already says, not on where the user's home is, and
a home moved with `$CLAUDE_CONFIG_DIR` moves what is refused with it.

### Adopting what a project already keeps

Many repositories keep skills written by hand in `.claude/skills` or
`.agents/skills`, committed with the code. Each lists as foreign, and
foreign is inert both ways, so nothing Mindweaver does reaches one: no
update, and no copy for an agent that reads the other folder. `mindweaver adopt
-p <project> <name>` (`adopt::plan`, `adopt::apply`, #59) is the one way a
copy nobody installed becomes recorded: asked for by name, never done by a
scan or an install, it copies the skill into the workspace's library and
records the project's copies as an installation from there, with the hash
they hold. The project's bytes are never written: what is there is what is
recorded, and from then on it is managed as anything installed is, current
while nobody changes it, edited once somebody does, outdated once the
library copy moves on ([an edited copy is left
alone](#an-edited-copy-is-left-alone)). What a project authors stays its
own ([D-06](#d-06-projects-keep-authoring-beside-the-code)); this is about
what sits where its agents read.

- **Held to what an install could have written.** Every later look compares
  the copies with the recorded hash and with the library copy, so both are
  made from the bytes as they are: the hash is `hash::tree` of the copies,
  the library copy is made from the first with `copy::tree` and refused
  unless it hashes the same, and the record names the workspace's own as its
  origin. What no install writes is refused rather than recorded: a link,
  where the copy is or inside it, which the hash refuses and an install
  never leaves; something at the copy's place that is no folder; and a
  `.claude-plugin` at its top, which an install leaves out of every copy
  ([what no copy holds](#what-no-copy-holds)). Recorded with it, the copy
  would read as outdated at once, since an install's copy holds no such
  folder (`copy::same_leaving`), and the next update would take it out,
  turning off the plugin's hooks and servers behind somebody's back;
  recorded without it, the copy would hold what the record does not. So it
  is said, and moving it out of the skill is somebody's to do first.
- **Strict, as installing is.** The library copy is a write, so the skill is
  held to what an install refuses one over (`artifact::strict_issues`),
  filed under its name: a name that breaks the rules or does not match its
  folder, a description empty or too long.
- **Every copy one skill.** Copies in two targets that do not hold the same
  are refused, both named: one hash is recorded, and which of the two is the
  skill is not something to guess.
- **Only where an install writes, every target recorded.** It looks in the
  targets an install of it uses (`install::targets`), and records each,
  those with no copy too, so one with none reads as missing, and the next
  update puts a copy there from the library, as for any installation that
  lost one. Adopting writes nothing in a project, so it is the update, run
  when somebody chooses, that gives Codex and OpenCode a skill a repository
  kept only in `.claude/skills`. Recording only the copies found was turned
  down: current at once, but the agents of the empty folder would go
  without it and nothing would say so, until the next install recorded the
  copies afresh. So was refusing: most repositories keep one folder, and a
  new project names all three agents. A copy only in a folder no install
  writes, `.opencode/skills` say, is not adopted, since no install or update
  would reach it there and the next would make it somebody's again.
- **Somebody's, nothing else.** Not one the ledger records already, from
  wherever it came, which is Mindweaver's to update or uninstall; not one in
  the folder the project authors in, which a `mind.toml`'s target always is
  and settings may make one, since that is the project's own work and a
  change from the project reaches it; not one inside an agent's own folder
  ([never into an agent's own folder](#never-into-an-agents-own-folder)),
  nor one where a loaded workspace keeps its own. A rule is refused too: one
  reaches a project rendered from the library as a section of its
  `AGENTS.md`, and taking rules out of a hand-written one is a change of its
  own.
- **One name in the library.** A name the library holds already, by its
  folder or by what one of its skills declares, is refused with both paths:
  the workspace would offer two skills of one name. The copy's folder is
  made with `create_dir`, which refuses one made in the meantime, and the
  folder of the workspace's own skills with it when it is missing, as the
  first one added makes it. Built in place rather than beside: no agent
  reads the library.
- **Out of git only what git does not track.** In a local project each copy
  git tracks nothing in is kept out of git before anything else is written,
  as an install's copies are, so a block that cannot be edited refuses the
  whole adoption ([what keeps an install out of
  git](#what-keeps-an-install-out-of-git)). One git tracks, as a committed
  `.claude/skills` is, gets no line, then or after an update: a line would
  keep none of it out and hide every file added to it later. It stays the
  team's in git, and an update writes over it there, for the change to show
  in `git status`. Local mode edits a tracked team file only with
  `edit_tracked` ([D-02](#d-02-installs-stay-local-unless-a-project-shares-them));
  that is about a file Mindweaver edits inside its marks, an `AGENTS.md` or a
  config file, and a skill folder somebody asked by name to be managed is
  consent for that folder, which the dry run and a `note:` say. The plan
  reads the exclude file whenever a line would be written, so a dry run
  refuses a block that cannot be edited as the adoption would, and a local
  project no work tree holds (`Plan::outside`) is said to keep nothing out
  of git, in the dry run and the adoption both, as an install there says it,
  rather than listed as keeping each copy out. A shared project's exclude
  file, and that of one a `mind.toml` marks, is never opened.
- **One lock, planned again under it, together or not at all.** `apply`
  opens the ledger to write first, since its scrub can take the lock
  itself, then takes the workspace's lock, reads the project again and plans again,
  so a plan shown a minute before decides nothing. The exclude lines, the
  library copy and the record follow in that order, and a step that fails
  takes back what came before it: the library copy goes, and the exclude
  lines this added, not one the group held already. The action log is
  written last, as after an install.
- **A dry run without `--yes`**, which reads the ledger without making it and
  writes nothing, as every dry run does, and exits zero: what it gives is a
  line for the library copy, one for the copies recorded, one for each
  target with no copy, and in a local project one for each copy kept out of
  git, to read before saying yes. Resolved as a read with `--yes` too, as an
  update is: a workspace made for the occasion links no project to adopt
  from.
- **Where `gh skill` got it, shown.** A `SKILL.md` that `gh skill`
  installed says where it came from under `metadata` keys starting
  `github-` (`adopt::PROVENANCE_PREFIX`). The plan hands those back as
  written, and the CLI shows them under its first line, each key and value
  made printable as every row from a file somebody wrote is, so a carriage
  return cannot rewrite the line read before saying yes; nothing reads them
  further, and no other tool's lockfile is read or written. Taking them for
  a source to gather from would be a gather, which is its own command.

The TUI offers it on a row `status` calls foreign, with `A`, which asks and
then runs `mindweaver adopt -p <project> --yes skill:<name>`.

### Loading: a third place, read live

`mindweaver load <path>` adds a third place that offers: the own artifacts of
another weaver workspace, `install::Offer::Loaded`, between the workspace's own
and the shelf. Its entry sits in a `loaded` array in `weaver.toml`, beside
`projects` and edited the same way (`edit_array`), rather than in the ledger:
it is configuration a person may read and edit, like the registry, and a load
must not create a database a workspace that never gathered does not have.

Three decisions shape it:

- **Live, not copied.** An offer points at the artifact where the loaded
  workspace keeps it, so the next install of it takes whatever is there now.
  That is the difference from gathering, which snapshots a repository onto the
  shelf; a loaded workspace is usually one on the same disk that is still
  being worked on. An installation from it records no source id, like the
  workspace's own, because there is no shelf row to point at, only its
  origin, `load:<entry>`.
- **One level deep.** Only a loaded workspace's own artifacts are offered,
  never what it loads, so two workspaces loading each other is not a loop, and
  what a workspace offers is what its marker names, not a graph.
- **Into the workspace that is not the source.** Run from inside the workspace
  being loaded, the first workspace found walking up is that one, and a
  workspace cannot load itself. `resolve_except` walks on past it (and past
  every path given) to the next one, or the default in your home; loading the
  home workspace from inside it is refused. One named with `-W` or activated
  is taken as it is, since it was chosen, and loading it into itself is
  refused by the load: by `load_all`, before any of the paths given is
  written, so a source listed ahead of it is not left loaded by a command
  that failed.

A loaded entry's origin is `load:<entry>`: disjoint by construction from
`workspace` and from any shelf source's address, even a repository gathered
from the very directory that is also loaded. What `gather list` prints is what
`--from` takes; `install::resolve_from` also turns the path given to `load`,
typed from anywhere, into that origin.

A loaded workspace is read live and is usually somebody else's, which puts
installing's trust in the name a `SKILL.md` declares under strain. So:

- **A name must be one folder name** (`install::is_folder_name`). It is joined
  onto the project's folder, and replacing starts with deleting: `..` would
  delete the project, an absolute path anything at all. Such an artifact is
  not offered from anywhere, and `install`/`uninstall` refuse it outright. A
  rule's route is held to the rules every name is instead
  (`install::is_installable_name`), so it never climbs out of the folder it
  is filed in.
- **Symlinks are followed only inside where the artifact comes from**
  (`copy::tree`'s `within`): the loaded workspace, the workspace's own root,
  the clone being gathered. One pointing out is refused, not copied, because
  copying it is how a credential ends up in a committed project.
- **Never one folder onto another it contains.** Equal paths are left alone
  and recorded as nothing; one inside the other is refused (`Overlaps`).
- **Never over what a loaded workspace keeps as its own.** A project can be
  the workspace it loads, sharing a folder; a record from before the load
  would otherwise let install overwrite, and uninstall delete, that
  workspace's own work (`KeptByALoad`).
- **A folder by that name is somebody's, finished or not.** Standing counts
  any path there, not only one with a manifest, so a folder of notes started
  by hand is never replaced.

What a load offers is every kind, and what it cannot offer (a workspace that
has gone, an artifact that cannot be read, a name that is refused) is
collected by `install::offered_by_loads` and said as `warning:` lines wherever
offers are shown or used, so what `mindweaver load`, `gather list` and the TUI
count is what can be installed. The install screen's line for a row it could
not apply says why, and the lines after it run all the same.
Two spellings of one loaded directory are one source; `load_all`,
`unload_all` and `unlink_all` rewrite the marker once, for all of the paths or
none.

The panel loads and unloads from a page of its own by the tasks the TUI's
load form sends, a workspace this machine knows by its path or any other
typed, and one loaded by its entry ([its changes](#its-changes)); which of
those it knows the workspace could load is the snapshot's
([the read model](#the-read-model)).

### What keeps an install out of git

A project in local mode
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)) keeps what is
installed into it out of git, through a block of anchored entries in
`$GIT_COMMON_DIR/info/exclude` that `exclude` keeps. Which paths to exclude
is the installing side's to choose, and `exclude` only writes them down: an
install of a skill names one entry for each copy git tracks nothing in, a
render of Claude's rule
files one for each file ([Claude's rule files](#claudes-rule-files)), a
render of a project's rules one for an `AGENTS.md` it made ([installing
rules](#installing-rules)), and an install of an MCP server one for each
config file it makes ([installing an MCP
server](#installing-an-mcp-server)).

```
# mindweaver:begin
# project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5
/packages/app/.agents/skills/commit-style/
/packages/app/.claude/skills/commit-style/
# project 0123456789abcdef0123456789abcdef
/packages/web/AGENTS.md
# mindweaver:end
```

How an install uses it:

- **Before anything is written.** The entries go in before the copies are
  put in place, as a local project's `.gitignore` goes in before its
  settings, so no copy shows in `git status`, not even for a moment. A block
  that cannot be edited (`InstallError::Exclude`) refuses the install with
  nothing written: copies one machine has would otherwise sit in every
  teammate's way. Copies that then fail to land take back out the entries of
  every target left empty, since a line naming an empty place would keep
  whatever somebody writes there later out of git without a word.
- **Never a copy git tracks.** An ignore rule leaves alone what the index
  holds, so a line for a copy git tracks keeps none of its files out, and
  hides from `git status` every file added to it later, where a teammate
  would never see it to commit. So an install writes a line only for each
  copy git tracks nothing in (`install::untracked`), asked of the index as
  `exclude::tracks` asks it: a copy that lands where nothing was, as most
  do, is untracked, while one a team committed, as an [adopted
  one](#adopting-what-a-project-already-keeps) often is, or one a project
  that was shared kept when it turned local, stays in git, and what an
  update writes over it shows as the change it is. Mindweaver runs no git
  command and leaves the index as it is. An index that cannot be read takes
  nothing for tracked, and the exclude edit that follows reads the same
  repository.
- **Local projects only.** A shared project's copies are the team's to
  commit, and a project a `mind.toml` marks was committed by design, so for
  those the exclude file is not opened at all, by an install or an
  uninstall. A project whose share mode changes has its group rewritten
  then: forgotten whole when it turns shared, and given an entry for each
  copy the ledger records in place when it turns local ([when a project's
  agents, targets or share mode
  change](#when-a-projects-agents-targets-or-share-mode-change)).
- **Keyed by the project's id**, from its settings, which stays when the
  project is renamed or moved within its repository. A line names a path
  below the project's own directory, so the group's lines are carried
  whole when either changes (`exclude::carry`): to where a relinked project
  is now, anchored again there, in the same repository or the one it moved
  into, read from the folder nearest the old place that is still there
  ([an entry whose project moved](#an-entry-whose-project-moved)), and back
  to the old place, reached the same way, when the entry cannot follow; and
  to a copy's new id once `mind init --new-id` gives it one, before its
  settings say so, and back when they cannot. A copy elsewhere in the same tree keeps its own lines under the old
  id, since those name its copies. A repository moved whole has its lines
  right already, its exclude file with it.
- **Taken out after the copies.** An uninstall removes the copies, then the
  record, then the entries, so no copy shows in `git status` on its way out.
  A block that cannot be edited then is a `warning:` beside what was done
  (`Removed::Removed`'s `still_excluded`), not a reason to undo it: the
  copies are gone, and only a line naming their empty place is left. A
  change from the project that removes an installed copy, in a folder it
  authors in that is also a target, takes that copy's entry out the same
  way, and one that renames it moves the entry to where it went ([A record
  must not outlive the copy it is
  about](#a-record-must-not-outlive-the-copy-it-is-about)).
- **Outside git, a note.** A project outside a git work tree gets its copies
  and a `note:` saying nothing keeps them out of git
  (`Excluded::warning`), not a `warning:`: nothing went wrong, and a project
  need not be in git, but a repository made there later would offer them all
  to be committed.

Why that file, and why written that way:

- **Not `.gitignore`.** It is the team's file and it is committed, so a line
  in it puts copies only one machine has into every teammate's diff, which is
  the noise local mode exists to avoid. `core.excludesFile` is no better: it is
  the user's, read by every repository they have. Mindweaver edits neither,
  ever. `info/exclude` is read like a `.gitignore` and is never committed or
  pushed.
- **The common dir, not the git dir.** A linked worktree has a git dir of its
  own under the main repository's `.git/worktrees/`, but git reads
  `info/exclude` only from the common dir, so every worktree of a repository
  shares one file.
- **Anchored at the work tree root.** Entries are relative to the work tree,
  not to the project, so a project at `packages/app` gets
  `/packages/app/.agents/skills/<name>/`. Git anchors a pattern with a `/` in
  its middle on its own, but one segment matches at every level: without the
  leading `/`, a root project's `AGENTS.md` would keep every `AGENTS.md` in
  the tree out of git, a teammate's committed one in another project
  included. Every entry starts with `/`, so one rule covers both shapes. A
  directory ends in `/`, and a route's glob characters are escaped, so
  `apps/[legacy]` is that folder rather than a class of letters. The route is
  worked out from resolved paths, not lexically as elsewhere: git matches the
  tree as it is on disk and never follows a symlinked directory, so a project
  reached through `current -> packages/app-v2` is excluded under
  `packages/app-v2`.
- **One group per project.** Several projects can share a repository, so each
  owns the lines under its `# project <key>` header and an edit rewrites only
  those. The key is whatever the caller names the project by, which for an
  install is the project's id (#29), and it is compared, never parsed. The
  same path in two groups is two claims on it: dropping one leaves the other
  excluding it. A group goes with its last entry, and the block with its last
  group.

And how it edits:

- **Only between the markers.** Every line outside comes back byte for byte,
  its line ending included. New lines take the ending the file already uses.
  A last line without a newline gets one when a block has to follow it, and
  the block's end marker goes without one instead, so the file still ends the
  way it did and dropping the block gives that line back as it was. The file
  is replaced through a temporary and a rename, as a marker edit is.
- **A block it cannot read is left alone.** A begin with no end, an end with no
  begin, a second block or a project with two groups is a failure for the
  caller to collect, and nothing is written. Guessing where a missing end
  belongs could pull somebody's lines into the block, and the next edit would
  rewrite them as Mindweaver's.
- **Outside a git work tree it writes nothing, and says so.**
  `Excluded::Outside` is not a failure, since a project need not be in git,
  but it carries a line to say (`Excluded::warning`): whatever local mode
  installs there is kept out of nothing, and a repository made there later
  would offer all of it to be committed.
- **Found with `gix`, opened isolated.** No `git` binary, and neither the
  environment nor the user's git config takes part, so a `GIT_DIR` set in the
  shell cannot send one project's entries into another repository's file.
- **No version.** Unlike a marker, the block holds nothing but git's own
  pattern syntax and two kinds of comment, and an edit rewrites only its own
  group and keeps every other line where it is, so a block a newer Mindweaver
  wrote survives an older one's edit.

### When a project's agents, targets or share mode change

`mind agents [add|remove] <agent>...` and `mind share local|shared
[--edit-tracked]` (#37), `mind targets set <kind> <dir>...` and `mind targets
reset <kind>` (#40), and `mindweaver agents`, `mindweaver share` and `mindweaver targets`
with `-p` for a project the workspace manages, change a project's settings,
and with them where what is installed into it belongs: a copy in each folder
an agent it names is sent one ([D-07](#d-07-one-copy-per-agent-folder)), or
in each its `targets` name ([a project's own
targets](#a-projects-own-targets)), kept out of git while it is local
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)). Core's
`reapply` decides all of it, `reapply::plan` writing nothing and
`reapply::apply` doing it, so the TUI's toggles and form and the command line
say the same. A change of targets is one more `Change`, the whole list of a
kind's folders or none, brought to the target set as the agents are, so
everything below holds of it too: `targets set` moves Mindweaver's untouched
copies into the folders it names and out of the ones it stops using, leaves
somebody's at a new target where it is and says so, and lists what sits
unrecorded in a folder it drops; `targets reset` is the same the other way.

- **A dry run without `--yes`**, as `remove` and `mindweaver migrate --library`
  are: a line for each thing that would change, the settings first and then
  each copy, so what an agent change does to the installs is read before it
  is done. Made, the same lines say what was done.
- **Brought to the target set, not to the difference.** Every installation
  the ledger records is compared with where the settings, as changed, send
  copies, rather than with what the change itself moved. The same change run
  again does nothing once it is done, and finishes what one that stopped
  halfway, or a hand edit of the settings, left undone. Planning from the
  difference would leave both to somebody's memory.
- **A new copy is made from a copy, not from the offer.** A folder an agent
  the project now names is sent copies to is given a copy of one the record
  names that still holds what was installed, by the hash every copy was
  recorded with ([D-04](#d-04-an-edited-copy-is-detected-not-disowned)),
  built in a stage and put in place as an install's copies are
  (`copy::Stage::place`), and the installation records one more target, its
  hash, origin and time as they were (`Ledger::retargeted`). Installing
  again from where it came from was turned down: that is an update, which
  can bring a newer revision into one folder while the other keeps the old,
  the very thing D-07 rules out, and the offer may have gone since. An
  installation none of whose copies still holds what was installed is given
  none, and that is a warning (`Why::Unsourced`).
- **Only an untouched copy is taken out.** A recorded copy in a folder no
  agent the project names is sent copies to any more is removed when its
  hash is the one recorded and forgotten when it has gone already.
  Otherwise, changed since it was installed or not to be compared (a record
  with no hash, a link), it is left where it is and dropped from the record
  with its exclude entry: its author's now. That is not what an edit does to
  a copy where an agent reads, which stays recorded and reads as edited
  ([an edited copy is left alone](#an-edited-copy-is-left-alone)): this one
  is in a folder no agent the project names is sent copies to, which no
  install writes again. Deleting it on a guess was turned down, and so was
  keeping it recorded, which would name a copy in a folder no listing of the
  project reads, call the installation missing or edited for good, and leave
  it to the next install, which records its copies afresh, to drop all the
  same. Which
  folder a copy is in is told by the folder on disk, as `install::targets`
  makes two spellings of one folder one target, not by the route recorded:
  a copy recorded in `.agents/skills` stays when codex goes and Claude reads
  it through a `.claude/skills` linked there. It is recorded from then on by
  the spelling it is sent copies to (`Retarget::respelled`), its exclude
  entry with it, since that is the one discovery reads it by and an install
  looks for its record by: left under the old one, as it once was,
  Mindweaver's own copy read as somebody's, and `targets reset` with the
  agents still linked to `.skills` made the next install refuse it.
- **Somebody's is left, and said.** A folder where a new copy would go that
  holds something unrecorded is not written over, so that agent reads it
  rather than what was installed: a warning, as an install refused over
  somebody's skill is. Refusing the whole change over it was turned down: an
  install is refused because it would write a second skill of that name where
  an agent reads both folders, and the change writes no copy beside
  somebody's, while which agents a project names is its own to choose. What
  sits unrecorded in a folder the change stops sending copies to is left too,
  and listed, since no listing of the project reads that folder once no agent
  it names is sent copies there. The folder the project authors in is not one
  of those, being read whatever its agents. A change that sends a kind only
  to folders all holding somebody's skill of a name leaves that installation
  no copy, and the ledger forgets it, as an uninstall would: the settings
  said where its copies go, and somebody's is there already.
- **Turning shared forgets the whole group; turning local fills it.** A
  project turning shared has its `.mind/.gitignore` removed, only while it
  holds what Mindweaver writes (`MIND_IGNORE`), so one changed by hand stays
  and is said, and its group of the exclude block forgotten whole
  (`exclude::forget`), every line whichever workspace wrote it, since each
  names a copy that is now the team's to commit. One turning local is given
  the `.gitignore` and an entry for each copy the ledger records in place
  that git does not track and the group does not hold yet, both before its
  settings say so, as
  `mind init` writes the `.gitignore` before the settings. `edit_tracked` is
  written with the share mode every time, off unless `--edit-tracked` is
  given, so one command line says the whole of how the project shares.
- **Refused before anything is written**: a project a `mind.toml` or a
  `settings.json` marks, which has no `settings.toml` to change until `mind
  migrate` moves it to one ([moving a project to its
  settings](#moving-a-project-to-its-settings)), and is pointed there
  (`MindProject::changeable`); a change that leaves it naming no agent, which
  would take every copy out, an uninstall's to do by name, or that installs a
  kind in no folder, for the same reason; a target no install may use ([a
  project's own targets](#a-projects-own-targets)); and a copy that would land
  in an agent's own folder, or in one a loaded workspace keeps as its own, as
  an install refuses one.
- **Held by the workspace's lock.** `apply` takes the lock of the workspace
  that links the project from before it reads the settings again until
  every copy, record and exclude entry is written, so an install in that
  workspace comes wholly before it or wholly after it.
  `MindProject::change_settings` takes none of its own, as
  `config_entry::ConfigFile` takes none: what keeps its edit apart is its
  caller's, and here the change is a workspace's as much as the project's.
  A lock in `.mind`, the other answer the open question offered, would keep
  a second workspace linking the project away from the settings but not
  from its own installs into it, which go by that workspace's lock; that
  case stays open ([open questions](#open-questions)).
- **A project nothing links still changes.** Its settings are its own
  ([D-09](#d-09-project-settings-live-in-the-project)), so in a project no
  workspace `mind` consults links, `mind agents` and `mind share` change
  them, move no copy, and say with a `hint:` that nothing installed was
  carried along, as `mind status` says why nothing reads as installed.
- **In an order that shows nothing in `git status` for a moment**: the
  exclude entries of the copies about to land and the `.gitignore` of a
  project turning local, then the settings, then the copies and their
  records, and last what keeps out of git what has gone, or what a project
  turning shared means to commit. A refusal before the settings are written
  leaves everything as it was; what fails after is collected and said, and
  the same change run again finishes it.

### The screen, and why it is three files

`mindweaver install` is a two-column screen: projects on the left, and on the
right what can be installed, as seen from the project under the cursor, or,
turned round, the other way about. It is split so that only one of its three
parts needs a terminal:

- `install/state.rs`: what a key does, and the command lines what was
  marked amounts to. Marking a box is a statement of intent and touches
  nothing; it is a plain state machine a test drives by pressing the keys a
  person would.
- `install/ui.rs`: what the screen looks like. Rendered into an in-memory
  terminal by the tests, so the exact text a user sees is asserted on, the same
  way every other command's output is.
- `install.rs`: the loop that reads a real keyboard, and running the lines
  what was marked amounts to. The loop is the only part with no test, and it
  holds nothing but the loop for that reason.

What is marked is applied as the command lines it amounts to
(`Screen::task`), as core plans them ([a batch is planned in
core](#a-batch-is-planned-in-core)), kind by kind: a
`mindweaver install -p <project>... --from <origin> <kind:name>` for each offer
ticked, with a `-p` for each project ticked to take it, and a
`mindweaver uninstall -p <project>... <kind:name>` for each artifact unticked,
which takes it out whichever offer its copy came from. They are the TUI's `Install` and `Uninstall` tasks, which the TUI
runs as it runs any task, and which `mindweaver install` runs the same way
(`install::apply`): each line parsed and run from the workspace root by
`tui::execute`, with the screen's `Lookup`, so its agent homes and the
workspace it was opened on reach every line; and what each said is printed
afterwards as the TUI's transcript prints it (`state::transcript`), the line
and then what it said, with a non-zero exit when any did not go through. So
the report shows exactly what ran, and a session with the screen in it can be
run again without it. Every line is run whatever came of the ones before
(`Task::chained`), since each is boxes marked one by one, where a task's
lines otherwise stop at the first that fails. Inside a line, a project
refused is said as its own and the others' boxes are carried out, as the
command does for several `-p`; but a `-p` that picks nothing by the time
the line runs, a project unlinked from another terminal since the screen
opened or one whose marker no longer reads, refuses its whole line, before
anything is done for the other projects on it, as it does typed ([one
artifact into many projects](#one-artifact-into-many-projects)). The
screen's picture of the workspace has gone stale under it, and every line
naming that project says so. A line a project and an artifact would have
kept the others going, and is rejected: one artifact's ticks are one line,
which is what the transcript is read for, and a workspace changed under
the screen is no reason to change the rest without asking again.

- **`--from` on every install line**, though a name offered from one place
  needs none: the box is one offer, so its line installs that one or nothing
  when run again after another place starts offering the name, and whether a
  name needs `--from` is core's to say (`install::offered`), not the
  screen's to work out again.
- **`-p` as core says** (`WeaverWorkspace::naming`): a project's entry, or
  `./<entry>` when another project declares the entry as its name, which
  `pick` would otherwise refuse as naming two.
- **What a line warns of is the command's.** A line says what the command
  says typed by hand: a project's `AGENTS.md` git tracks, say, is said by
  each rule's line that renders the project's rules, where the screen, when
  it carried out its marks itself, said it once. That was the alternative,
  rejected: a second front end to installing, row by row through core with a
  report of its own, which said what the commands say a second way and left
  the transcript a `mindweaver install` nobody could run again.
- **What the lines said goes to stdout**, warnings and errors included, each
  under its line, as the TUI prints a session: on stderr, a warning would be
  cut off from the line it is about. `mindweaver install` puts on stderr only
  what its screen says itself (`install::finish`): a project that would not
  open, and what the loaded workspaces could not offer, which every install
  line says as it does typed by hand, so the screen says it only when no
  install line ran, rather than once more than the lines did.

- **A server from elsewhere is shown before its line says yes** (#95). An
  MCP server whose install waits for a yes (`install::asks_first`) is
  previewed for its project when the screen is built, as every standing is
  read then (`install::mcp::preview`, laid out by the lines an install
  says under its own, `entries_said`), and each one marked to install is
  shown in the confirmation `a` opens, each agent's program or address
  and why one is not reached, before anything is written
  (`Screen::previews`). Its line carries `--yes` (`Task::confirmed`) only
  because the screen applies only past that confirmation, so what each
  agent will run is on screen when somebody says yes to it, and the
  transcript prints the line with it. The confirmation is as tall as the
  rows its lines wrap into (`Paragraph::line_count`), and what the
  terminal cannot hold scrolls, the question and the keys kept in place;
  its yes waits until every row has been on screen (`Screen::unseen`, set
  as it is drawn, the one place that knows the terminal's size), since a
  box cut to the screen would ask a yes of commands nobody was shown. A
  line without `--yes`, which would write nothing and say so under its
  line, was the alternative: the confirmation would have been asked for
  nothing. The panel's matrix shows the same lines (#97), laid out by the
  same function (`install::foreseen`, under `install::heading`), and sends
  the yes only with the task its confirmation carries ([its
  changes](#its-changes)).

The right column holds every kind's offers, filtered to one kind at a time:
`tab` shows the next (`Screen::kind`), skills first, then rules and MCP
servers, and a project's marks of every kind are applied together. A filter rather than a column a kind, so the
screen keeps two columns however many kinds there are; and rather than every
kind in one list, where a skill and a rule of one name would sit side by side
as two boxes for what reads as one thing. A row is one kind and name, so
ticking one offer unticks another of the same kind and name, and leaves a
rule of a skill's name alone. Which offer of a name the project holds is
core's to say (`install::holding`): the one its record came from, a rule's
as a skill's, since nothing in a project holds a rule's bytes alone to
compare, and a skill's copy may be behind both places or edited since. A
project offered nothing of the kind
shown says what core says it cannot take (`install::receives`), or where that
kind comes from: a rule from the workspace's own or a load, never the shelf.
A row whose box cannot move says why: somebody's, which nothing of
Mindweaver's touches, and an install edited since, which the screen never
forces ([an edited copy is left alone](#an-edited-copy-is-left-alone)); every
offer of the name stands as its one installation, so none of them can be
marked over it either.

`v` turns the screen round by artifact: every offer of the kind shown on the
left (`Screen::artifacts`, each kind, name and origin once,
`Row::same_offer`), and every project on the right, ticked where it holds
the one under the cursor. Not a second set of marks: each box is still a
project's row, found by its offer (`Target::row_of`), so ticking one goes
through the same `toggle`, unticking another place's offer of the name as it
does the right way round, and what is applied is the one batch, however it
was marked. Turning keeps the box under the cursor, and the keyboard on the
same side. A project with nowhere to put the kind is offered none of it, so
turned round it has no box and says why, as it says so in place of its
offers the right way round. The key is `v` rather than `tab`, which the view
would have taken before rules could be installed: `tab` shows the next kind,
which means the same either way round. Either way round, one artifact's
ticks are one `mindweaver install` line and one `mindweaver uninstall` line, a `-p`
for each project, since each takes several ([one artifact into many
projects](#one-artifact-into-many-projects)).

The line of keys is cut short at the terminal's edge rather than wrapped, so
a long one never pushes what is waiting, or why a key did nothing, off the
bottom, and the keys are two spaces apart, as the TUI's are, so the longest
fits 80 columns.

`try_init` rather than `init`, here and in the TUI: these are the commands that
need a terminal, and being run without one deserves a sentence rather than a
panic.

## What each project has

`mindweaver status` answers vision point 1, which skills and rules each project
has, and from where (`status::Status`). It is built from discovery and the
ledger together, since neither answers alone. Discovery says what sits in the
folders a project's agents read and the one it authors in, and which agents
read each copy; it reads no ledger, so it cannot tell an install from
somebody's own. The ledger says which copies Mindweaver put where, and from
where (`Ledger::records`); it cannot see that one has gone since, nor a copy it
never wrote.

Each artifact a project holds is one entry, as discovery lists it: every copy
of one name together ([read where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)). Its
state is one of seven (`status::State`), the first five an installation's
[standing](#an-edited-copy-is-left-alone), told by the same core function an
install refuses by and an update acts on (`install::standings`), so the
report, a refusal and an update never disagree:

- **current**: the ledger records every copy, every copy it records is found
  where it was put holding what was written there, and the place it came
  from offers that still.
- **outdated**: the same, but the place it came from offers something else
  by now. An update brings it up to date ([bringing what was installed up
  to date](#bringing-what-was-installed-up-to-date)).
- **edited**: the ledger records a copy that holds something other than was
  written there, by its hash. A rule is edited in its section of the block
  or in its file for Claude. Still Mindweaver's install, left alone by every
  install, update and uninstall unless forced.
- **missing**: the ledger records a copy discovery does not find where it
  was put: deleted since, by hand or by a `git clean`, which the ignored
  copies of a local project are exposed to, or left so it no longer reads,
  which discovery says beside the report. An update puts every copy back,
  since it writes each target.
- **orphaned**: the place it came from no longer offers it, so nothing can
  bring it up to date or put a copy of it back; `uninstall` takes it out.
- **foreign**: unrecorded, in an agent's folder outside any the project
  authors in: somebody's, never Mindweaver's to change unless somebody
  adopts it ([adopting what a project already
  keeps](#adopting-what-a-project-already-keeps)).
- **authored**: unrecorded, in a folder the project authors in: its own work.

They took over from `installed`, now current, outdated or orphaned, and
`partial`, now missing, which is a name a script read changing what it
holds, so the JSON envelope moved to version 2 ([JSON, in one
envelope](#json-in-one-envelope)). Keeping `installed` and adding the
standing beside it, a field that would have moved no version, was turned
down: two words for one entry, which the table and the TUI would each have
had to choose between.

The state is discovery's answer (`Artifact::provenance`) with the standing
of the install laid over it, in the order a standing is said in (edited,
orphaned, missing, outdated): one of the five only when every copy is
recorded, and missing when the record names one that is not found, even
beside a copy the project authors of that name. That copy sits where the
install wrote nothing, or it would be the install's, so the next update puts
the gone one back all the same, and calling the entry authored would leave
an agent reading none of it unsaid. Not beside a foreign copy, which is said
first: the next install of the name is refused over it. Nor beside an edited
one, said next, for the same reason unless the update is forced, nor an
orphaned one, which nothing can put back. The record is
matched to the entry by the path of a copy it wrote, as discovery marks a copy
installed, never by name, so an install of a name into one folder neither
claims nor lends its origin to what somebody keeps under that name in another.
A recorded copy is missing when discovery did not find it, rather than when
nothing is at its path: a folder whose `SKILL.md` was deleted, or no longer
parses, loads in no agent, where asking the disk would have called it
current and reaching nobody. Its standing calls it edited, since it no longer
holds what was written, and the report says edited over missing, as it does
for any edit. A copy still holding what was written where discovery no
longer looks is the other way round: missing in the report, and current to
an update ([an open question](#open-questions)).

An install nothing of which is found is still in the report, missing, or
orphaned when nothing offers it any more, and reaching no agent, so that
what was installed is said once its every copy has gone. Unless the project holds something else by that name: that is then the
name's one entry, said as what is there, since a row is one name, as in every
listing, and the name is somebody's now ([an open
question](#open-questions)).

Beside its state, each entry names the agents among the project's that read
one of its copies (`Artifact::agents`), which for an install with a copy
missing are fewer than it was installed for, and where what was installed
came from, the record's origin, as `--from` takes it. Each project names its
own agents too, from its settings, so a front end can lay its rows out by
agent; a project a `mind.toml` marks names none, and reaches none as far as
a report can say.

The rules a project marked by its settings received are no copies discovery
finds, but sections of the block in its `AGENTS.md` and Claude's files, so
each is listed from its record and the block, an installed view
([D-23](#d-23-what-a-project-received-is-an-installed-view)): catalog
discovery parses no block, since a section is never authored in place but
rendered from a rule kept elsewhere. A rule is missing while a target its
record names is not found, its route among the block's sections or its file
for Claude where it was written, as a skill's copies are, and otherwise
stands as core stands it; one recorded with none has nothing missing. It is
edited, over missing, while its section or its file holds something other
than was rendered there, and orphaned while nothing offers it. It reaches
Codex and
OpenCode through the block, and Claude through its own file, or through the
block where the render gave it none: a render gives Claude every rule as a
file while Claude reads no block, so a rule in the block with no file of its
own is one Claude reads there (a file that was somebody's when the render ran
is the one case this tells wrong, and the render said it). A block that
cannot be read holds none of them, and the next render says why. A rule
copied into a project a `mind.toml` marks is discovered as its copy, as a
skill is. A name the project authors a rule of too keeps the one entry
discovery gives it, as a skill's does.

What a project received as MCP servers is entries in its agents' config
files, which discovery never parses (D-23), so each is listed from the
ledger and those files too (#93): in `entries` (`status::EntryStatus`), one
row an agent's entry, its agent, its file, the server's name, its state and
its origin, by agent in report order and then name. Every entry an
installation recorded is listed, looked for in the file its record names,
whichever agent it is for, as [it stands](#how-an-entry-stands); and every
entry the config file of each of the project's agents holds that nothing
records is listed as `foreign`, D-23's foreign entries, which no artifact of
the catalog is, since a name can be somebody's in one agent's file and
Mindweaver's in another's. Rows rather than one an artifact: what Claude's
file holds of a server can be edited while Codex's is not. A config file an
entry was recorded in that cannot be read, as its renderer would refuse it
(`mcp_json_not_strict` for a `.mcp.json` holding a comment), is a failure
beside the report (`DiscoveryFailure::Config`), and the entries recorded in
it read as missing. One read only for what nothing records is passed over,
and nothing in it listed: it holds nothing Mindweaver wrote, and a link
there, as a dotfiles setup makes `.mcp.json` or `.codex`, which the agent
follows and the renderer, built to write, refuses, would otherwise call a
project nothing was installed into broken. A server's installation has an artifact row of its own as well,
built as a rule's received is: missing while an entry it recorded is not
found, otherwise as core stands it from its entries, reaching the agents
whose entry is found, held in each of their files. The snapshot carries
the same `entries` on each project ([the read model](#the-read-model)), and
a project a `mind.toml` marks, which names no agent, lists none. The text
of `status` lists each entry as a row of its own, kind `mcp`, with the
agent whose file holds it, its state and where it came from, in the place
of its installation's row (#95): one row for an installation could not say
that Claude's entry was edited while Codex's is current, which is what
somebody reading it is about to act on. A foreign one is listed so too, so
whoever reads it is told of a server somebody keeps there as of a foreign
copy. The TUI's rows of a project keep each installation's row, whose
badges say which agents it reaches, and add somebody's beside it, as
foreign.

Each rule a project marked by its settings received that some of its agents
read nowhere is named in `missed`, with those agents, and so is each MCP
server it received that some of them are given no entry of, one an agent's
renderer refuses or whose file git tracks in a local project, as its install
said (#95). Either is a warning beside the report too: one about some files, which Claude alone reads, or one Codex
and OpenCode read nowhere because its block could not be written. Of each
rule rather than of its kind, as `unreached` says a skill's: a skill reaches
the agents that read the folders its kind goes to, the same for every skill,
while a rule reaches the ones what it states, and where its block may go, let
it. Only one installed whole, current, outdated or orphaned, from the rows
as the report lists them: one partly gone is missing already, which says
so.

Each project also names, in `unreached`, every kind it installs only in
folders some of its agents do not read, with those agents: what a project's
`targets` can leave ([a project's own targets](#a-projects-own-targets)),
and nothing under the standard folders, which each agent reads. It is a
warning beside the report, with a non-zero exit, as anything that hides an
install from an agent is: a missing install says one copy is gone, and this
says every copy is where an agent does not look. A folder the settings name
in `targets` that no install uses is a failure beside the report too
(`StatusFailure::Target`), since the report reads the installs as going
everywhere else they name. Both are in the TUI's rows ([where a project
installs skills](#where-a-project-installs-skills)).

- **Every project the workspace links, in its marker's order**, each
  directory once however many entries point at it, as every screen lists
  them, and each project's entries by kind and then name, sorted in core. Not
  the workspace's own, which are no project's and which nothing is installed
  into.
- **A stale entry is listed, not left out**, with why it would not open, and
  said as a failure beside the rest, as `mindweaver list` says one: it is the
  entry somebody has to find to unlink it.
- **An older marker is a hint.** Each project names the file its marker is
  read from (`ProjectStatus::marker`, `marker` in the JSON), and one an
  older Mindweaver wrote, a `mind.toml` or a `settings.json`, is a `hint:`
  after the report naming `mind migrate`, as the TUI's header points at the
  move ([moving a project to its
  settings](#moving-a-project-to-its-settings)). Nothing else in a report
  tells a `settings.json` from a `settings.toml`, which name the same
  agents, though only the second can be changed. A hint, not a warning: the
  project is read whole and the report is true of it, so the exit status is
  the report's. On stderr under `--json` too, as `mind status`'s hint about
  its workspace is, since the data says it in `marker`.
- **Read, not made.** The ledger is opened with `ledger_for_reading`, so a
  workspace that never installed anything is left without one, and a default
  workspace nothing has made is reported as one with nothing linked, as
  every read resolves it. Core opens it, in `ProjectStatus::read`, rather
  than each front end, which let the CLI refuse a report over a ledger that
  would not open while the TUI drew its rows without it. A ledger that
  cannot be opened or read leaves every copy unrecorded, as `with_records`
  does, and is said once in a report, and on each project's rows in the TUI,
  never a reason to hide the rest.
- **`mind status` asks the workspace `mind` consults** (`resolve::consulted`):
  the one named, activated or above, if it has been made. One that does not
  link the project, or none at all, leaves it without records, and it is
  reported all the same, as `mind list` reports a project nothing links, with
  a hint saying why nothing in it reads as installed. Refusing, as `mind
  install` refuses without a workspace that manages the project, was turned
  down: a read has nothing to protect by refusing. So one found that cannot
  be read is a warning beside the report, as it is beside a change, and the
  warning says why nothing reads as installed in place of the hint. One `-W`
  or the activation names that is no workspace still refuses, as it refuses
  every command: whoever chose it meant that one.

The CLI lays the report out as `list` lays out its own: a project column only
when its rows come from more than one project, or a stale entry's row, which
is nothing but its name, needs one; a kind column only when more than one
kind is in play; the agents comma-separated as `--agents` takes them, so the
column holds no space for a script splitting on spaces to cut, and `-` for
none, since reaching no agent is something to say. `--json` prints the
report as core built it ([JSON, in one envelope](#json-in-one-envelope)).

The TUI's rows show the same report, through the snapshot ([the read
model](#the-read-model)). Each project is read once, through
`ProjectStatus::read`'s own discovery, which hands back the catalog it read
beside the report, so a row's summary, its problems and its state come from
one discovery rather than two. Where each artifact is held comes back beside
the report too (`read_placed`) rather than in it: the report's JSON is a
contract `mindweaver status --json` prints, and the snapshot is where a front end
reads more of each artifact than a script was promised. A row shows its state, then a badge in each of the
project's agents' places, blank where that agent does not read it, so rows
read down by agent. The two letters (`CC`, `CX`, `OC`) are the screen's, its
own initials for each agent, since Claude Code and Codex share the first. A
row of the workspace's own says neither, and an install nothing of which is
found has a row like the rest, from which `i` puts it back.

## What no agent says

Some failures are silent: an untrusted Codex project ignores its
`.codex/config.toml`, an `AGENTS.override.md` hides `AGENTS.md`, a
`CLAUDE.local.md` stops Claude reading it, an unset secret variable breaks a
server and a skill at user scope can shadow a project's. `mindweaver doctor`
and `mind doctor` (#96) find them, a project at a time, and fix nothing.

Core builds the report (`doctor::Report`, a `doctor::Examined` a project).
Each finding is a `doctor::Symptom` with a code of its own (`doctor_*`),
said as a `doctor::Finding`: its code, the agents it keeps something from,
the path it is about, which is the one its fix acts on, its message, naming
that path first as every warning does, and its fix. A finding is no
failure: the project is read whole, and what is said of it is true. What
doctor could not read to tell, a Codex config that does not parse or a
folder that cannot be listed, is a failure collected beside the project's
findings (`Examined::failures`, `doctor::DoctorFailure`), with the code of
the error it carries, and on stderr as a `warning:`, never a reason to say
less of the rest.

**File reads alone, and never the environment.** Each check rests on a
fact docs/agents.md records, verified or docs-only, and reads what that
fact is about with the function that already reads it for an install or a
render, never a second reading of its own. No agent is run, which is the
probe's, by hand; nothing is written, no lock is taken and the ledger is
opened only to read (`ledger_for_reading`). The front end hands it the
agent homes and the names of the environment variables set
(`doctor::Machine`): core reads no environment, and names alone, since a
value doctor never had is one no report can say. The CLI reads both once
per command line into its `Lookup` (`Lookup::variables`, each value
dropped as it is read), so a line the TUI runs is told what the TUI was
opened in, and the panel what `mindweaver ui` was started in.

The checks, each where an agent of the project's reads, so a project a
`mind.toml` marks, which names no agent, is asked only of what an install
left and what `status` says:

- **Codex's trust.** Codex reads a project's `.codex/config.toml` only in a
  project its user's config marks `trusted`, and a project marked
  `untrusted` loses its `AGENTS.md` too (docs-only). Read through #91's
  reader of `$CODEX_HOME/config.toml` (`codex_mcp::UserConfig`), whose
  `trust` an install's `codex_mcp_untrusted` warning is told by, under the
  project's root as linked and as resolved: a project marked `untrusted`
  is found whatever it holds (`doctor_codex_distrusted`), one marked
  neither way only while it has a `.codex/config.toml` to lose
  (`doctor_codex_untrusted`).
- **What Codex reads of `AGENTS.md`.** An `AGENTS.override.md` beside it,
  which Codex reads in its place, no git root to read down from, and a
  chain from the git root near or past Codex's 32 KiB budget (docs-only),
  read by the render's own `agents_md::codex_warnings`, of the project as
  it stands rather than as a render left it: the override only while there
  is an `AGENTS.md` for it to hide (`doctor_codex_overridden`,
  `doctor_codex_no_git_root`, `doctor_codex_near_budget`,
  `doctor_codex_over_budget`). A chain that cannot be measured is a
  failure, `codex_unmeasured`, as the render says it.
- **Claude and the block.** A `CLAUDE.md`, `.claude/CLAUDE.md` or
  `CLAUDE.local.md` in or above the project keeps Claude from `AGENTS.md`
  (verified), and a render chose how Claude reaches the block when the
  rules were installed ([how Claude reaches the
  block](#how-claude-reaches-the-block)): one made since leaves the block's
  rules reaching Claude through nothing. `claude_md::unread_by` asks what
  `reach` asks, of the files as they stand, an import a file holds already
  counting and one a render is yet to add not; a rule whose file is in
  Claude's rules folder, under either names, reaches it there
  (`doctor_claude_misses_rules`). The fix is an install of one of them
  again, which renders every rule as the project now stands, as a test
  holds it to.
- **Copies of one skill that differ in folders one agent reads.** OpenCode
  reads `.agents/skills`, `.claude/skills` and `.opencode/skills`, and
  given one name twice keeps one (docs-only). Discovery compares the copies
  in the folders an install writes in alone ([open
  questions](#open-questions)), so doctor reads every folder `Agent::reads`
  names, and compares by `hash::tree`, the hash an install records
  (`doctor_copies_differ`).
- **A skill at user scope (D-22).** One name at both scopes resolves
  differently in each agent, so a project skill whose name a skill at
  user scope holds too, in a folder the same agent reads, with something
  else in it, is found, one finding for each pair with every agent that
  reads both (`doctor_user_skill_differs`). Which folders each agent reads
  there is core's, matched with no wildcard (`AgentHomes::skills_read_by`:
  Claude's `skills`, verified in part; `~/.agents/skills` for Codex,
  docs-only; OpenCode's own, the one in the home beneath the one
  `$OPENCODE_CONFIG_DIR` names, `~/.claude`'s whatever
  `$CLAUDE_CONFIG_DIR` says, and `~/.agents`'s, docs-only). A
  project at the home, whose folder is the one at user scope, is not
  compared with itself.
- **A server of an installed one's name at user scope.** Codex reads a
  project's servers over its user's key by key (docs-only), which an
  install refuses (`codex_mcp_collision`) and a server added to the user's
  config since is not; OpenCode merges a project's config over its global
  `config.json`, `opencode.json` and `opencode.jsonc` key by key
  (docs-only), in each of its folders (`AgentHomes::opencode_dirs`), read
  as Codex's is (`opencode_mcp::held_by_user`, through
  `config_entry::read_through`), in either shape
  (`doctor_server_at_user_scope`). Claude's servers at user scope are not
  read ([open questions](#open-questions)).
- **A variable an installed server's inputs read, not set** (D-11). The
  variables are those an install says to set (`install::mcp::variables`),
  a secret said as one, of the definition the installation came from, the
  first offered from its origin, which is what was written unless the
  offer moved on since, as `status` says; one no longer offered says
  nothing, being orphaned. Told by the names the front end says are set,
  where doctor runs: nothing of a V2 OpenCode server started elsewhere,
  which fills its references from its own environment
  (`doctor_variable_unset`, [open questions](#open-questions)).
- **What an install that did not finish left.** A stage in the project's
  `.mind/tmp` and an older version beside a copy, by the names a sweep and
  a replacement clear (`copy::stages_in`, `copy::left_beside`), the second
  where an agent may load it as a skill ([built where no agent
  looks](#built-where-no-agent-looks)). Said, never swept: doctor takes no
  lock, so it cannot tell a stage left from one in use, which the next
  install's sweep can (`doctor_leftover_stage`, `doctor_leftover_beside`).
- **What `status` says the project's agents miss** (D-02): a kind
  installed only where some of them read none of it, and a rule or a
  server some of them read nowhere, as `ProjectStatus::unreached` and
  `missed` say, whatever the share mode, since `status` says them whatever
  it is (`doctor_unreached`, `doctor_missed`).

Its exit status is a reading command's for what it found: non-zero once it
finds anything, as `validate` is once something does not pass, so it works
in a script or CI, and once something could not be read, as every reading
command's is. `--json` prints the report in the envelope, its warnings the
failures, the exit status the text's (`render::as_json`).

The snapshot carries what doctor finds of each entry, in the order
`projects` lists them (`doctor`, [the read model](#the-read-model)), told
what `status` says each project's agents miss, which the snapshot read
already, so a project is not read twice. A name added, so `view::FORMAT`
stays; `health` is not told by it, since what `health` holds is a verdict a
command answers, and weighing doctor's findings in, most of them about
files Mindweaver does not own, would change what it holds, which moves the
format: the panel's MCP page shows the findings of each project holding a
server beside it (#97). The TUI marks a project doctor
finds something in with `✚` and how many, and its pane says each with its
fix, in a strip of its own beside the one of what its agents do not see
([the TUI](#the-tui)), and what doctor could not read to tell in the one of
what could not be read, counted with it, as `mindweaver doctor` says it on
stderr; the panel draws them on its MCP page, for each project holding a
server. What the
snapshot now reads, what doctor reads, is watched as everything it reads is
(`watch::watched`): every folder an agent reads skills from in a project
and its `.mind/tmp`, under the project's scope, and the agents' own files
and folders under the workspace's, as the default workspace's marker is,
since they are no holder's and every project reads them. Above a project,
under its scope, each `CLAUDE.md`, `.claude/CLAUDE.md` and
`CLAUDE.local.md` Claude would read in place of its `AGENTS.md`, in it and
in every folder up to the one below `/` (`claude_md::looked_at`, the walk
`reads_agents_md` makes), and each `AGENTS.md` and `AGENTS.override.md`
Codex reads from the git root down (`agents_md::codex_looked_at`), or, in
no work tree, each `.git` that would give it one, named only while it is
not there: a `.git` that is there changes with every git command, and
holds nothing Codex reads, so none is ever watched. Each is a file, so
what is watched of a folder above is only a change to that name in it.

## The TUI

`mindweaver` on its own opens a screen over the whole workspace. It is not a
second front end beside the CLI: it lives in the CLI crate, and it **does
everything by running CLI commands**.

It is at both levels too. The workspace's own artifacts are its first row,
marked `◆`, and a key on that row runs the `mindweaver` command where a key on a
project's row runs `mind -C <project>` (`mind -W . -C <project>` for one the
workspace manages from outside its own tree; see [the TUI looks where it was
told](#the-tui-looks-where-it-was-told)): a task carries its holder, and the
holder decides the level of the line it becomes. `mind` on its own opens the
same screen on the project it was run in: inside the workspace that manages
it, where its neighbours and its installs are, or on the project alone when
none does. Alone, the keys that belong to a workspace say so instead of
running, and `l` is `mind link`, after which the screen is the workspace's.
A project's rows say how each artifact stands, as `mindweaver status` does ([what
each project has](#what-each-project-has)).

### Every change is a command line

A key that asks for work produces a `Task`, and `Task::commands` turns it into
the words a person would type: `mindweaver link beta`, `mind -C beta add skill
deploy 'Ship it'`. Those words are parsed by `Cli` and `WeaverCli`, the parsers
the binaries use, and run through `run_with` and `run_weaver_with`, what the
binaries' `run_watched` and `run_weaver_cli_watched` call once they have read
where to look for a workspace (`run` and `run_weaver_cli` are the same with a
watch nobody follows). The TUI has no code path of its own for linking, adding
or gathering, and it cannot grow one, because there is nowhere in it to put
one.

That is what turns "every feature has a command and a place in the TUI" from a
promise into a property. A feature is written once, as a command, and the TUI
gains it by producing that command's words. A test parses every task's lines
(free text shaped to break a naive command line included), so a task that could
not be typed cannot be written.

Two things follow, and both are deliberate:

- **A form shows what it is about to run, as it is typed**, as the command
  line. The TUI teaches the CLI rather than standing in front of it.
- **Closing the TUI prints a transcript**: every command that changed
  something, with what each said. The session can be read afterwards and
  repeated in a script. Commands that only read (show, validate, the shelf)
  are left out, because they changed nothing.

`Task` is not the TUI's own, though the TUI is where it was born: it lives in
`task.rs`, shared by every front end that runs commands, the TUI and the
panel's backend ([D-18](#d-18-the-panel-writes-by-running-commands)).
One type, so a line is spelled one way whichever front end asked for it; a
second front end building its own lines would be a second place for one to be
spelled wrong.

The panel will send a task as JSON, so it serializes both ways, as one flat
object tagged by its name in snake_case (`{"type": "link", "path": "beta"}`),
its holder and its agents line tagged the same way inside it. Its kind, agents
and share mode are the words a command line takes, read back through what
reads those ([JSON, in one envelope](#json-in-one-envelope)), and a field a
task does not have is refused rather than passed over: a task is a write, and
one sent with a field misspelled would run without it. A name, once a front
end sends it, is a contract as a report's is, so a test pins every one, and
reads every task back from its JSON as it was.

**No value a task holds is read as an option, or as a command.** A task the
panel sends holds whatever a browser put in it, so a description that reads
`--yes` has to reach `add` as a description, not confirm anything. A line's
own words, the command and the options the task sets, are spelled in
`task.rs` and never taken from a field, so a value cannot name a command;
and each value goes one of three ways. After the option that takes it,
joined to it by `=` when it starts with a dash (`--path=-odd`); as a `-C` or
a `-p` that starts with one, behind a `./`, which names the same directory
and the same project by its path; or among the positionals, which go behind
a `--` whenever one of them starts with a dash. So a task names each project
it gives `-p` as `WeaverWorkspace::naming` does, by its entry or a path from
the root, never by the project's name, which behind a `./` would be read as
a path instead: a `-p=-odd` would keep it a name, and change the line the
TUI has always shown for such a project. A test holds every variant to it:
fed values that start with a dash, the flags the grammar knows among them
and a couple of hundred more made from a fixed seed, each line is read as
the same line with a plain value is: the same commands, the same options,
and under each option or positional the value as given wherever the plain
one was, behind its `./` under a `-C` or a `-p`. Compared option by option
rather than as one bag of values, since a line holds a value in several
fields, and one field holding it would hide another that dropped its dash.
A property-test crate was the alternative, and one loop over every variant,
with no new dependency, is all this asks of one. Shown, a line is quoted for
a shell (`task::shell`), so one pasted from the transcript runs as one
command of exactly its words.

A `-C` or a `-W` in those words is relative to the workspace root, which is
how someone reading the transcript takes it, so it is resolved against the
root rather than against wherever the process started. What the screen was
opened with is passed along rather than spelled: its home, and the `-W` or
activation that chose its workspace (`tui::execute`), as they were given to
`mindweaver` itself. That is `commands::run_line`'s, which the panel's backend
runs its lines through too, so the two front ends resolve a line one way. A command that opens a screen of its own (`mindweaver`, `mindweaver
tui`, `mindweaver minds`, `mindweaver install` with neither a name nor a project,
`mind`, `mind tui`) is refused if it ever reaches that path, rather than drawn
inside this one. Half an install (`mindweaver install -p alpha`) opens none, and
is left to the command, which refuses it with a reason of its own.

That refusal is one part of a list every front end shares (`task::Refusal`):
what a front end with no terminal of its own, the panel's server, does not
run. A screen of its own; `edit` in an editor, which hands the terminal to
one, and `edit --contents -`, which reads standard input, the terminal's
too, where a file given in its place runs ([contents given
whole](#contents-given-whole)); `activate`, which prints code for a shell;
`gather git`, the network for as long as it
takes, until #112 brings to the server the progress and the cancel only a
screen has now; and `show`, which prints an artifact whole, read through any
link on the way, as a terminal's user may ask of their own disk, where a
front end without one reads a file at a time, held to its holder
([what a browser may name](#what-a-browser-may-name)). And `ui`, which
serves the panel until Ctrl-C, so a front end that ran it as a line would
wait on it for good. The TUI has a terminal, so of the list it refuses only
the screens, `ui`, since the screen takes Ctrl-C as a key and nothing would
stop it, and standard input, which is the screen's keys, read in raw mode,
where no key ends it. The list is of commands as the parsers read them, not
as they are spelled, so `mind weaver minds` is refused as `mindweaver minds` is, and it
is a match over every command with no wildcard, so a new command does not
compile until it says whether it is refused. A task is refused for the first
of its lines that is (`Task::refusal`), whichever variant it is, and each
refusal has a code of its own (`opens_screen`, `opens_editor`,
`reads_stdin`, `prints_shell_code`, `gather_unwatched`, `show_unheld`,
`serves_panel`),
held to the rules core's codes are ([a code for what went
wrong](#a-code-for-what-went-wrong)). A list of tasks was the alternative,
and it would leave out a line no task builds: a front end that runs lines it
is sent would run one the list never named.

Beside it, and read the same way, is what a line takes away
(`task::Caution`, `task::cautions`): `remove --yes` deletes, `uninstall`
and `mindweaver remove --everywhere --yes` take copies out, `unlink` and
`unload` make the workspace forget a project or a load, and `--force` on an
install, an uninstall or an update told `--yes` writes over or takes out an
edited copy. A front end with no terminal asks somebody before it runs such
a line, the panel with each preview ([its changes](#its-changes)); a match
over every command with no wildcard (`Caution::of_mind`, `of_weaver`), so a
new command says what it takes away before it compiles. The TUI asks its own
questions still, worded for its rows ([open questions](#open-questions)).

The exceptions are named, not hidden: opening the install screen builds the
screen `mindweaver install` opens, as a read, rather than running a line, and
re-reading the disk is not a command, because it changes nothing. Applying
the install screen is no exception: its marks are the `install` and
`uninstall` lines they amount to (`Task::ApplyInstall`, [the
screen](#the-screen-and-why-it-is-three-files)), run as every other line is,
so the transcript says what it ran in words that run again.

### Split three ways, like the install screen

- `tui/state.rs`: what a key does. It moves cursors, fills forms and returns
  tasks; nothing in it touches a file, so a test drives it with the keys a
  person presses.
- `tui/ui.rs`: what the screen looks like, rendered into an in-memory
  terminal by the tests.
- `tui/minds.rs`: the minds screen's rows and ticks, and the task they amount
  to, which `Task::minds` builds from the marks, so another front end marking
  directories builds the same lines; like `state.rs`, it touches nothing.
- `tui.rs`: the loop, laying out the snapshot core reads ([the read
  model](#the-read-model)) and a scan as what the screen shows, and running
  tasks. `step` is everything the loop does between a key and the next frame,
  and `settle` what it does before each frame while a task runs apart; both
  are public so a test can do exactly that without a terminal.

The loop draws a box saying what is running before it runs it, because a
frozen screen reads as a hung one. Keys pressed while it ran are dropped: they
were aimed at a screen that has since changed.

`mind edit` is the one command that needs the terminal itself, because an
editor draws a screen of its own. For it the loop steps aside (raw mode off,
back to the normal screen), runs the command as it runs any other, and builds
its own screen again from scratch when the editor closes. The task says so
(`takes_terminal`), so the loop does not have to know which commands are
editors.

### A gather runs apart

A gather is the network, and the one task that runs apart
(`Task::runs_apart`). `step` starts its lines on a thread of their own and
returns at once, with a [watch](#following-a-gather-and-stopping-it) whose
steps come back over a channel. Before each frame the loop calls `settle`,
which takes in the last step and, once the task is over, shows what came of it
exactly as `step` does for a task that ran in place. Everything else is the
local disk, over before anybody could read a count, and stays on the loop's
thread rather than paying for one of its own.

The box shows the command, the last step and that `esc` cancels. The screen is
the task's until it is over, so every other key is ignored, as one pressed
while a task ran in place is dropped. `esc` cancels the watch: the gather stops
before it places anything, and the form it came from says so, as for any gather
that did not get as far as a report. Once the step is placing, which is no
longer `cancellable`, the box says a cancel is too late instead, and the gather
finishes with its report. Leaving cancels it too, and `wait` holds the screen
until it has stopped, so what it did reaches the transcript and nothing is half
done when the process ends. A gather still connecting takes a cancel only once
it is connected, so `esc` there, or leaving, waits as long as the network does;
and since the loop goes on reading the terminal, a question ssh asks there is
raced for its answer, as the [open questions](#open-questions) say under
Private repositories. The lines are still the task's command lines, as
`App::lines` gives every task (`mindweaver init` first on the [default
workspace](#the-default-made-only-by-a-write) before it is made), parsed and
run by `execute`: the thread changes where they run, not what they are.

They go through a `Runner` on the way, which is the real parsers and commands
unless `App::runner` holds a stand-in. A gather from a repository on the local
disk is over before a frame could show it, so the test of the progress and the
cancel stands in a gather that goes on only when the test says so, and drives
it through `step`, `settle` and the keys as the loop would. The stand-in is
handed the words all the same, so even then there is nothing the screen runs
that is not a command line.

### What it asks before it writes

The link form offers what `mindweaver scan` finds under the workspace, or under
the folders `mindweaver minds` was given for the session it opened
([Scanning](#scanning)), in the order `mindweaver scan --suggest` gives and with
its reasons, so the row picked when it opens is the one most worth linking.
A git repository that is not a mind project yet can be picked, but making it
one writes into it, so `mind init` is asked about, not assumed, and a typed
path that turns out to be a plain directory is asked about the same way. Unlinking asks too, and says that nothing on disk is
deleted. Deleting one of the workspace's own asks with a second yes for
removing it everywhere ([removing one of the workspace's own
everywhere](#removing-one-of-the-workspaces-own-everywhere)), the line each
answer runs shown under the question. A command that fails from a form leaves the form open with the reason
in it, so a typo is fixed where it was made rather than typed out again.

### The minds screen

`m`, or `mindweaver minds`, lists every one `mindweaver scan` finds under the
workspace that it does not link, unticked, then every project it links,
ticked, in the order `mindweaver scan --suggest` gives: what is not linked first,
most worth linking first, each row saying why ([which to link
first](#which-to-link-first)). It was one list by path, on the grounds that
the question was "which of these", not "which kind"; under a home that is
dozens of repositories in name order, and the one worth linking is wherever
its name puts it, so the screen shows core's ranking of those instead, and
sorts none of them itself. The projects it links come from the marker
rather than the scan, which may not reach them all, so the screen lists
those by their entry, as the command lists the ones it found by route. A
reason can name every agent file a repository holds, so one longer than the
pane leaves room for is cut, ending in `…`, rather than pushing what
applying would do to each row past the pane's edge; the command says it
whole. `mindweaver minds DIR` finds those under the folders it
names instead ([Scanning](#scanning)). Ticking only marks. Applying turns the
marks into lines like any other task: `mind -C <path> init` for each repository that is
not a project yet, then one `mindweaver link` and one `mindweaver unlink`, each with
every path at once. So the screen needed `link` and `unlink` to take several
paths, and they take them **all or nothing**: every path is checked before any
entry is written, so a refused one leaves the registry as it was rather than
half changed by a command that reports failure. The batch is built in the
task module (`Task::minds`), from each directory's path and what its mark
asks (`MindsChange`), and the panel's marks are built into lines by the same
function ([its changes](#its-changes)): that a directory made a project is
then linked is the batch's rule, which a front end spelling it again could
break, leaving a folder made a project and never linked. What a directory a
scan found offers to be marked for, linked as it is or made a project
first, is one function too (`MindsChange::of_found`).

### The agents and share toggles

`t` on a project's row opens a box for each agent, its share mode and
`edit_tracked`, ticked as its settings say, and applying them runs the lines
they amount to: `agents --yes <agent>...`, then `share --yes <mode>`, each
only when what it sets changes, with what they said shown in full, since an
agent change can copy and remove a skill in every project folder an agent
reads. The agents a project names keep their order, and one ticked is named
after them, so a toggle does not reorder the settings. A line cannot name no
agent at all, so unticking every one becomes `agents remove` of every one
named, which the command refuses for its own reason, in the form: the rule
that a project names an agent is core's, and the screen does not say it a
second time. Which lines the toggles amount to is said in the task module
(`Task::settings`, from what the settings said and what the toggles say,
`task::Toggles`), not on the screen, where it was born: the panel's toggles
send both, and the same function builds their lines ([its
changes](#its-changes)).

### Where a project installs skills

`T` on a project's row opens a form holding the folders its settings install
skills in ([a project's own targets](#a-projects-own-targets)), empty when
they go where its agents read, and applying it runs `targets set --yes skills
<dir>...`, or `targets reset --yes skills` once the folders are cleared, with
what it said shown in full, since it moves every copy. The folders are typed
on one line, a space between each, as a shell splits a line without quotes;
a folder whose name holds a space is named on the command line instead. A
form rather than toggles, since the folders are anybody's to name. Which of
them an install may use is core's to say, so the form takes what is typed
and shows the command's refusal in place, as the toggles do.

A project whose installs some of its agents do not see is marked on its row,
and its pane gets a strip saying so in `status`'s words
(`ProjectStatus::unreached`, and each folder no install may use), beside the
strip of what could not be read. Both strips are as tall as their lines once
wrapped, so the end of a sentence, which says what to do, is not cut off.

### Bringing a project up to date

`u` runs `update --yes` for the project under the cursor (`mind -C
<project> update --yes`), for the one artifact under it from the artifacts
column (`... update --yes <kind:name>`), and on the workspace's own row for
every project it links (`mindweaver update --all --yes`, with the artifact's
name when one is under the cursor), and shows what it said in full, as the
toggles do. It asks nothing first, unlike unlinking: an update writes over
nothing somebody changed, since the screen never forces, and what it leaves
is in the lines it shows. A project that cannot be opened is said in place
of running anything. `u` was unlinking's key until #48 gave it to the
update, the key that runs most often, so unlinking moved to `U`, which a
stray press reaches less easily and which still asks first.

The install screen marks a row whose offer has moved on since it was
installed `(outdated; mindweaver update refreshes it)`, held as any install's
row is: the screen marks what goes in and out, and the update is what
brings one up to date.

### Following a stale row

`M` on a stale project, one the scan under the screen found moved
([an entry whose project moved](#an-entry-whose-project-moved)), asks and
then runs `mindweaver relink <old> <new>`, the entry as the marker spells it and
the directory as the scan spelled it from the root; the stale row's pane says
where it went. Only where the scan found it: which directory holds the
project is the scan's to say, by the id it holds, and the screen guessing
from a name could hand what was installed to the wrong one. Where it found
nothing, under the workspace or the folders `mindweaver minds` was given, the
screen says the line that names the directory by hand. The scan's match and
the stale row are told apart by the directory each names, not by spelling,
since the entry is spelled as the marker has it and the scan's route as
`mindweaver link` would take it: by core's `scan::moved_from`, which the panel's
backend asks too, so the two never disagree on which entry moved. A key of its own rather than a second answer
to `U`'s question: unlinking drops the entry and leaves what was installed
recorded under a route nothing is at, and the pane says both keys, so the
choice is made seeing where the project went.

## The local server

`apps/server` (`mindweaver-server`) is the transport under the web panel
([D-17](#d-17-the-next-front-end-is-a-local-web-panel)): a library crate that
binds a port, checks who is asking, carries JSON between the browser and a
front end, and serves the panel's files. What it carries is an HTTP API for
scripts and tools on this machine as much as for the panel, its first
client, versioned under `/api/v1`
([D-24](#d-24-the-local-server-is-a-versioned-http-api), [its
routes](#its-routes-under-apiv1)), which [its guide](api.md) walks through
with examples a test runs ([the API's guide runs its
examples](#the-apis-guide-runs-its-examples)). `mindweaver ui` runs it, over the CLI's
backend ([launching it](#launching-it-mindweaver-ui), [what a browser may
name](#what-a-browser-may-name)). The token keeps out a process that
connects without it, and [the guards](#what-keeps-other-pages-out) keep out
other pages in the same browser: one on another port of 127.0.0.1, which the
browser counts as the same site, or one whose DNS name has been rebound to
127.0.0.1. Neither keeps out a server on another port of 127.0.0.1 that the
browser visits, because the cookie takes the token there as well (an
[open question](#open-questions)).

### Transport only

It holds no rules. It cannot tell a valid task from an invalid one, a linked
project from a path to refuse, or a skill from a rule, and it is not to learn:
each of those answers is core's, reached through the backend, so the panel
cannot disagree with the CLI and the TUI about any of them. What it knows is
HTTP (routes, the token, cache headers, a log line, the code and the status
of each request it turns away itself, and its part of the API's
description), and none of that would differ between two front ends.

What it serves is the backend's reads, each at a route of its own from a
table the backend hands over ([a table of reads](#a-table-of-reads)), and
six routes of its own, from a table of its own (`Own`):
[D-18](#d-18-the-panel-writes-by-running-commands) in HTTP, with the
switch, the transcript, the events and the API's description beside it.

### Its routes, under `/api/v1`

Every route is under `/api/v1`, the version in the path
([D-24](#d-24-the-local-server-is-a-versioned-http-api)): a route, a
parameter or a field once served is a contract, as the envelope's names are
under `ENVELOPE_VERSION`, so dropping or renaming one, or changing what it
holds, moves the API to `/api/v2`, and adding one does not. In the path
rather than in a header, so a script names the version it was written for
where it names everything else, and one written for another meets a 404
rather than an answer it misreads. What the snapshot holds keeps its own
version, `view::FORMAT`, which it carries as `format`.

The reads, each the CLI's ([what a browser may
name](#what-a-browser-may-name)) and each answered with the backend's JSON:

- `GET /api/v1/snapshot`: everything the workspace holds, as the panel draws
  it ([the read model](#the-read-model)).
- `GET /api/v1/projects`, `GET /api/v1/library` and `GET
  /api/v1/project?entry=<entry>`: the snapshot's projects, its library, and
  one project by its entry as the registry spells it, each read whole and
  cut in Rust (`Snapshot::project`), so it says what the snapshot says to
  the letter; an entry no registry holds is refused as an unlink of it is
  (`not_registered`), a 404.
- `GET /api/v1/files?reference=<reference>&project=<entry>` and `GET
  /api/v1/contents?reference=<reference>&file=<route>&project=<entry>`:
  what one artifact holds, and one file of it as text, of the project
  `project` names or, without one, of the workspace's own.
- `GET /api/v1/scan?root=<path>&root=<path>&depth=<n>`: what could be
  linked under each root, as `mindweaver scan --suggest` finds it; no root is
  the workspace's, and no depth the command's own.
- `GET /api/v1/folders?path=<path>&hidden=true`: the folders directly
  inside one, for choosing one ([choosing a folder](#choosing-a-folder)).
- `GET /api/v1/updates`: what updating every project would do, as `mindweaver
  update --all` says it without `--yes`.
- `GET /api/v1/workspaces`: every workspace a switch may go to.
- `POST /api/v1/plan` with `{"selection": [...]}`: what boxes of projects
  and offers, ticked or not, amount to, the tasks that carry them out among
  it, with nothing run.
- `POST /api/v1/validate` with `{"holder": ..., "draft": ...}`: what core
  says of an artifact a command is about to write, with nothing written.

The server's own:

- `POST /api/v1/tasks/preview`: the command lines a task is, and what of
  them the panel has somebody confirm first, with nothing run.
- `POST /api/v1/tasks`: run a task; the reply is its lines and what the
  command said, its outcome, its text and each warning with its code, and
  the state is read again afterwards, as D-24 has it, rather than answered
  with a typed result every command would have to build.
- `POST /api/v1/switch`: have the backend act on something else from now
  on, which only it can say it may; the reply is the backend's JSON for
  what it acts on then ([switching what it acts on](#switching-what-it-acts-on)).
- `GET /api/v1/transcript`: every task the session ran, with what each
  produced.
- `GET /api/v1/events`: what happens as it happens, a change on disk or a
  run starting and finishing, as server-sent events ([live
  updates](#live-updates)).
- `GET /api/v1/openapi.json`: the API described, every route above and
  below with what it takes, what it answers and every way it may be
  refused, as an OpenAPI 3.1 document the backend hands over ([the API's
  description is written from both
  tables](#the-apis-description-is-written-from-both-tables)).

Every answer that is no success carries a code beside its words, the
backend's or the server's own ([every refusal with a
code](#every-refusal-with-a-code)), so a script never has to parse the
panel's page, nor the English, to learn what it got wrong.

### Every refusal with a code

Every answer under `/api` that is no success, whoever turned the request
away, is JSON of one shape, an object of two fields and no other, with no
exception: `error`, the words a person reads of what happened, naming the
route, the parameter or the header, what was expected and what came; and
`code`, the one word a script acts on, which never changes when the words
are made clearer. One type of the server's says it, `Refused`, whichever
turned the request away, rather than an object spelled where each is
answered, and its schema is the API's ([the API's schemas come from
Rust](#the-apis-schemas-come-from-rust)):

```json
{"code":"missing_parameter","error":"/api/v1/project needs `entry`, and was sent none"}
```

A 401 carries `WWW-Authenticate: Bearer` besides, and a 405 `Allow`. Outside `/api` what answers is for a person at a browser and stays
plain text: the panel's files, the launch link's refusal of a token that
is not this server's, a 403 for a page asked for under another name, and a
405 for a method the panel's files do not take. None of them has a client
that acts on a code.

The code is of one of two kinds. A backend's refusal carries its front
end's code, core's or the CLI's ([what a browser may
name](#what-a-browser-may-name)), under a 404 or a 422 as the backend says.
Everything else the server finds itself, and each such condition has a code
of the server's own (`Condition`, `apps/server/src/code.rs`):

| Status | Code | When |
| --- | --- | --- |
| 400 | `malformed_json` | a body that is not JSON at all |
| 400 | `body_unreadable` | a body that could not be read whole, the connection failing under it |
| 400 | `body_refused` | a body axum refuses in a way the server does not know by name |
| 400 | `unknown_parameter` | a parameter the route does not take, one of the server's own routes included |
| 400 | `repeated_parameter` | a parameter that takes one value, given more than once |
| 400 | `malformed_parameter` | a value its parameter's shape does not read |
| 400 | `missing_parameter` | a parameter the route needs, left out |
| 400 | `invalid_parameters` | a GET whose parameters its route cannot make a query of |
| 401 | `token_required` | no launch token, or another server's |
| 403 | `unknown_host` | a Host the server does not answer to |
| 403 | `foreign_origin` | an Origin other than the server's own page's |
| 403 | `same_origin_required` | a change by cookie without `Sec-Fetch-Site: same-origin` |
| 403 | `x_mindflayer_required` | a change by cookie without `X-Mindweaver: 1` |
| 404 | `unknown_route` | a path under `/api` no route names, a version not served and the panel's old unversioned endpoints among them |
| 404 | the backend's | a read whose parameters name what is not there |
| 405 | `method_not_allowed` | a method the route does not take |
| 413 | `body_too_large` | a body larger than the server reads |
| 415 | `json_required` | a body not sent as `application/json` |
| 422 | `invalid_body` | JSON that is not what the route reads: a task, a switch, or a read's body its route cannot make a query of |
| 422 | the backend's | anything else the backend refuses |
| 500 | `backend_failed` | the backend panicked while it answered, previewed, ran or switched |
| 500 | `writer_stopped` | the thread every run and switch goes through has stopped |

**The server's codes are its own**, written once. Core's `codes!` would
write them, but the server depends on no mindweaver crate, so a macro of its
own mirrors that one's rules: one arm for each condition, `Self::` and its
name with its status and its code, from which a match with no wildcard
writes `code()` and `status()`, and the same arms `ALL` and `CODES`. So a
condition added does not compile without both, and none can be left off
the list, which is data each route's answers are listed from rather than
by hand: each route of either table says which conditions a request to it
may be refused for (`Route::conditions`, `Own::conditions`), by its
method, its parameters and what it hands the request on to, decided for
each condition by a match with no wildcard, so a condition added is placed
among the routes before it compiles. Named for the condition in snake_case, and never
renamed nor moved to another condition once given, as core's are. A code
reaches a client without the crate that gave it, so `invalid_body` must
mean one thing whoever says it: the server cannot see core's list or the
CLI's, and the CLI, which depends on all three crates, holds the three
apart in the test that holds its own (`error_test.rs`), as the TypeScript
`Code` holds all three. A test of the server's sends a request for every
way it turns one away and holds each answer to the shape, its code one of
`CODES` or the fake backend's, its status the one its condition names, and
its condition one its route lists.

The 415 is one condition whoever finds it, the guard on a change by cookie
or axum's JSON extractor on a request with the token as Bearer. axum's
rejection of a body is `#[non_exhaustive]`, as is what it carries when the
body could not be read, so the match over it is the one place with a
fallback: any way of not reading the body whole is `body_unreadable`, and
a rejection axum adds later is `body_refused`, a 400, a code of its own
rather than an answer without one, or with a code that says something
else.

Leaving the server's own conditions without a code, their status naming
each, was #215's first choice: a code was the backend's word for a rule of
its own, which the server has none of, and a list of the server's would be
a second to keep unique beside core's and the CLI's. The owner's review of
it undid that. Several conditions share a status, a 403 being any of four
guards and a 400 any of eight, and a script tells them apart only by
reading the words, which change whenever they are made clearer.

**A backend's refusal is a 404 or a 422 as the backend says**: each
`Refusal` carries its grounds (`Grounds::Absent` or `Grounds::Unfit`),
set where the backend builds it (`Refusal::absent`, `Refusal::unfit`), and
the server answers the first with a 404 and the second with a 422, never
working out which from the code, which is the front end's word: a server
that read it would hold a rule of the front end's, which a second front end
could disagree with. The CLI's backend says absent for a read whose
parameters name what is not there: an entry no registry holds (`/project`,
or the `project` of `/files` and `/contents`), or one whose folder holds no
project any more; an artifact nothing holds; a file or a folder that does
not exist, or that is no file or no folder. Everything else is unfit: a
path the policy refuses, which is refused for how it is spelled whether or
not it is there; a task no front end without a terminal runs; a workspace
in use that will not open, which the request names nothing of; and
whatever a body names. A GET's parameters are part of the address of what
it reads, so a value naming nothing makes the address name nothing, as a
path no route serves does. A body is sent to a route that is there, so a
task, a switch, a plan or a draft naming an entry no registry holds is a
request not to act on, `not_registered` under a 422. Each is decided by a
match with no wildcard over the core error it comes from
(`backend::Absence`), so a variant core adds is decided before it
compiles. Not core's: a status is HTTP's, and a front end with no server
has no use for one.

### A table of reads

Which reads there are is the backend's, as everything a request means is:
`Backend::routes` hands the server a table, each `Route` with its method,
its path under the version, its parameters by name with the `Shape` each is
read as and whether it is needed, what a POST's body holds and what it
answers, each by the name of the backend's type for it, and a sentence on
each. The one thing the server calls rather than reads is the route's
`query`, how what a request sent becomes the backend's `Query`, since a
parameter naming a holder, or a list of roots, means something only to the
front end. So the server matches the path, reads the query string into
JSON as each parameter's shape says, refuses what no route takes, and hands
the rest over without knowing what any of it is; and the OpenAPI document
is written by walking the very table that is served, rather than a second
description beside the handlers that could disagree with them, each body
and answer the schema of the type it names, found by that name ([the API's
description is written from both
tables](#the-apis-description-is-written-from-both-tables)). A handler per read in the server was the alternative, which would
put the CLI's reads in a crate that depends on no mindweaver crate, or a
trait method per read, which the server would have to grow with every read
the CLI adds.

- **Identifiers in parameters, never in the path.** A registry entry or a
  reference can hold a `/` (`apps/api`, `rule:git/commits`), which a path
  segment cannot carry without an escape every client spells its own way.
  A route's path is words of lowercase letters, digits, `-` and `_`, which
  the server checks as it binds (`route::check`), refusing as well a path
  it answers itself, one taken twice and a parameter named twice, so a
  table that cannot be served fails `bind` with why rather than panic in
  the router or have `{name}` in a path read as a place for a value.
- **Four shapes.** Text, a whole number, `true` or `false`, and a list, the
  parameter given once for each value (`root=/a&root=/b`), the form a
  browser's form and every HTTP library write one in, rather than one value
  split at a comma a path may hold. Each read into the JSON a body would
  hold for it, so a route reads its parameters as it would a body, into a
  struct of its own that refuses a field it does not name; one left out is
  absent, which the route reads as the panel's query leaves it.
- **A parameter no route takes is refused**, as `deny_unknown_fields`
  refuses a field: a 400 naming it and the parameters the route does take,
  so a misspelled `projet` is said rather than read as the workspace's own.
  So is a parameter that takes one value given twice, since which of the
  two a read went by would be the server's guess, a value its shape does not
  read, and one the route needs left out. The server's own routes take
  none, and refuse one sent the same way.
- **A read with a structure is a POST that changes nothing.** The boxes of
  a plan and a draft to judge hold more than a query string should: their
  route takes a body, read as a task's is, and no parameter, so a read has
  one place for what it is sent. Never a write: a change is a task, and
  every task is a command line ([D-18](#d-18-the-panel-writes-by-running-commands)).
  A POST is held to the guards on a change by cookie as any is, which costs
  the panel nothing.

The CLI's table is `routes::table` (`apps/cli/src/routes.rs`): the panel's
own reads, each the very query the panel sends for it, which a test holds
it to, and the snapshot's projects, library and one project, which the
panel takes from the snapshot it already holds. A test pins every route by
its method, path, parameters, body and answer, as a test pins a report's
JSON names, so one renamed fails it rather than reach a script written
against the old name unnoticed, and the change is made as one that moves
the API to `/api/v2`; `ui_test` calls every one over loopback with the
token as Bearer.

The server's own six routes are data too, so the description walks two
tables and describes nothing by hand: `Own::ALL`, each with its method, its path,
what its body holds and what it answers, and a sentence on each. The
server serves each from there, its handler asked by the method the table
says, so the table and what answers cannot disagree, and `route::check`
keeps a backend's route off every path in it. What a body holds and what an
answer carries are named by what they are (`Sent`, `Answer`) rather than by
a type's name, as a backend's route names its own: a body is the backend's
`Task` or `Switch`, a run's answer carries its `Outcome` and a switch's is
its JSON, all types the server never names, so the description names each
as the backend's types do, and `Answer::Events` says the stream is
server-sent events rather than JSON. What the server answers with of its
own, a `Preview`, a `Ran`, each `Event` and a `Refused`, has a schema of
the server's, where what it carries of the backend's is any JSON: which of
the backend's types fills each the description names from these words,
and from a word for each place it carries the backend's JSON (`Carried`).
A string naming each type, as a
backend's route has, was the alternative, and it would have the server
spell the CLI's names, or the trait grow one constant for each. A test pins
the table by method, path, body and answer, and another holds each route to
be served by its method alone.

### Generic over a backend

The server is generic over a `Backend` trait whose `Query`, `Task`,
`Outcome` and `Switch` are associated types. It deserializes into them,
hands them over and serializes what comes back, without ever naming one; a
`Query` it never deserializes at all, since a route of the backend's table
makes one of what a request sent ([a table of reads](#a-table-of-reads)), so
the trait asks nothing of it but that it can be sent to the pool. So the CLI
can implement `Backend` with its own `Task` and depend on the server, while
the server depends on no mindweaver crate at all, which `crate_test` checks
through `cargo metadata`. A protocol crate holding the shared types was the
alternative, and it would put a third crate between every change to a task
and the code that runs it. The server's own suites use a fake backend whose
types know nothing of mindweaver, which is the property the CLI relies on.

`query` returns JSON rather than a fourth associated type, because each query
has a shape of its own and one type standing for all of them would be an enum
whose only job is to serialize untagged. `preview` takes the task by
reference, so a task need not be `Clone` to be both shown and run, and
answers a `Preview` of the server's own: the lines, and beside them what the
panel has somebody confirm before it asks for the run (`confirm`), each item
the backend's JSON, carried unread as a watched path's scope is. In the one
answer, so the panel is told what takes something away of the very lines it
shows, rather than asking again of a disk that may have changed between.
`watched_paths` names what to watch for [live updates](#live-updates): each
path with its scope (`Watched`), a word of the backend's own carried to the
panel as JSON, as a query's answer is, so the server names no type of the
front end's here either.

`document` hands the server the API described, which `GET
/api/v1/openapi.json` answers with as it is: text the front end wrote, as
it alone knows every type a route names, with the server's part of it
([the API's description is written from both
tables](#the-apis-description-is-written-from-both-tables)). Without
`self`, as `routes` is, since a description is as fixed as the types it
describes; and text rather than a description the server would write at
each launch, so a front end writes it once, as it builds, and a server
built without schemars still serves it. An option of `Server`'s was the
other place for it, and it would make a route of the server's own one it
serves or not as a front end remembers to hand it over.

Each of `query`, `preview` and `run` may refuse what it was asked with a
`Refusal`, a code, a message and its grounds, which is how a rule of the
front end's reaches the browser without the server knowing it: what a read
names is not there, a value the path policy does not let a browser name, or
a task no front end without a terminal runs. A struct of the server's own
rather than a fourth associated type, since the server builds the reply
from it (`{"error", "code"}`, a 404 or a 422 as its grounds say, [every
refusal with a code](#every-refusal-with-a-code)) and knows nothing more of
it, and its code a `&'static str`, as a code is a fixed word. An answer of the backend's own shape that says it refused was
the alternative, and a preview, whose answer is the server's `Preview`, has
no room for one.

The server carries `confirm` and enforces none of it: a run sent without the
panel having asked anybody is run, as a shell runs `mindweaver remove --yes`
typed into it. Whoever holds the token is the person at this machine, and
[the guards](#what-keeps-other-pages-out) keep every other page from
sending a run at all; the question is for that person's slips, so it lives
where they click ([its changes](#its-changes)). A run that had to carry a
confirmation would make the run request a shape of its own around the task,
and hold a script to a question only a person needs.

### Threads

axum runs on tokio, and nothing a backend does belongs on tokio's own
threads, which are for moving bytes: one slow read there stalls every request
waiting on the same thread.

- **Queries and previews** go to tokio's blocking pool (`spawn_blocking`), as
  many at once as arrive. They only read.
- **Runs** go to one dedicated `std::thread`, fed by a channel, and so do
  switches, each taking its turn as a task does, so a switch never lands
  while a run is under way and a task sent after it runs after it. A
  channel is first in, first out, so tasks run in the order they arrived,
  which a lock does not promise whoever waits on it. One thread means two writes never run
  side by side, which is the world the commands underneath were written for:
  a CLI, where one command at a time is all there is. It orders the
  panel's own writes and nobody else's: beside a terminal or a TUI writing
  to the same workspace, the two take turns through the workspace's lock
  ([Writers at the same time](#writers-at-the-same-time)). And a long run
  holds that thread rather than one of the pool's, so the panel goes on
  reading while it waits.
- **The watcher** is a `std::thread` of its own, which owns the system's
  watches and asks the backend for its list again after each change, and
  after a switch, when the writer thread asks it to and waits until it has
  ([live updates](#live-updates)); notify and its debouncer keep a thread
  each beside it. None of them is tokio's, since asking for the list reads
  the disk.

A run's lines are worked out on the writer thread just before it, against the
disk the run itself meets after every write before it; a task whose lines
the backend refuses there is never handed over to run, and leaves the
transcript as it was, since it is no run to replay. The entry of one that
ran is in the transcript before the reply leaves, so a client that asks for the transcript
after its reply always finds its own run there. A client that stops waiting
does not take its task back: it was queued, and it runs. An outcome that will
not serialize is still a run, so it is recorded, and replied, as
`{"error": ...}` beside the lines that ran rather than turned into a 500 that
leaves the transcript a run short. A backend that panics loses that one reply,
a 500 of `backend_failed`, but not the thread, so the tasks behind it still
run.

The transcript is the TUI's, kept where the panel can ask for it at any time:
runs only, since a query or a preview changes nothing there is to replay, and
in memory only, for as long as the server runs.

### Who it answers

It binds 127.0.0.1, with no option for another address: the panel is for
the person at this machine, and beyond loopback the token would travel in
clear text. The port is 0, which lets the system pick a free one rather than
the server failing on a taken one, unless the front end names one
(`Server::port`). That is for a client that must know the port before the
server starts, the panel's dev server, whose proxy names it in its settings,
and a port somebody holds is then an error, not another port that client
would never find.

Every process on the machine can reach that port, so the address says nothing
about who is asking. A 256-bit token, issued fresh at each launch, does. The
launch link carries it once, as `/?token=...`; that request is answered with
a 303 to `/` and the token in a cookie, and the redirect is what takes the
token out of the address bar and the history. The cookie is `HttpOnly`, so the
panel's own scripts never hold it, and `SameSite=Strict`, so no other site's
page sends it, though a page on another port is the same site, which is what
[the guards](#what-keeps-other-pages-out) are for. It is not `Secure`, which a
browser that does not count 127.0.0.1 as a secure origin would drop. And it is
named for the port, `mw_<port>`, because cookies are not isolated by port:
under one name, a panel launched on a second port would overwrite the first
one's token in the browser and lock it out.

Scripts and tests send `Authorization: Bearer <token>` instead, and either is
compared in constant time. A script that did not start the panel reads the
token from the file `mindweaver ui --token-file` writes
([launching it](#launching-it-mindweaver-ui)). Only `/api` asks for it: the panel's files are the
same for everybody, and a browser that has not been through the link yet still
needs the page that will tell it so.

### What keeps other pages out

The token stops a process that connects to the port without it. It does not
stop another page in the browser that holds the cookie, and the panel is
opened in a browser full of them. Three kinds of page can reach a loopback
port, and a different check stops each:

- **A remote page**, any site open in another tab. It can make the browser
  send 127.0.0.1 a GET, a form or a `text/plain` POST without asking, though
  not read the answer, and the cookie, being `SameSite=Strict`, does not go
  with it. Where the browser says which page sent it, its Origin is refused
  before the token is even looked for; where it does not, as on a GET, there
  is no cookie to stand in for the token.
- **A rebound name.** A page on `attacker.example` has its DNS answer changed
  to 127.0.0.1 once it has loaded. To the browser it is still
  `attacker.example`, so its requests here count as same-origin and the
  same-origin policy no longer hides the answers from it. It holds neither the
  cookie, which belongs to `127.0.0.1`, nor the token, so the API would refuse
  it anyway; but it names `attacker.example:<port>` in Host, and for that
  every request it makes, the panel's files and the launch link included, is
  refused before anything else.
- **Another port on loopback**: a dev server, a notebook, a tool's sign-in
  callback, anything else listening on 127.0.0.1. Cookies are not isolated by
  port ([RFC 6265, section 8.5](https://www.rfc-editor.org/rfc/rfc6265#section-8.5)),
  and `SameSite` counts sites, not origins, so the browser sends `mw_<port>`
  with that page's requests here. It cannot read the answers, since nothing
  here grants it CORS, but a change does its harm whether or not anybody
  reads the reply. That is cross-site request forgery from the same site, and
  the checks on a change by cookie are for it.

The checks, in the order a request meets them (`src/guard.rs`, and the gate
in `src/auth.rs`):

1. **Host**, on every request: exactly one Host header, naming
   `127.0.0.1:<port>`, `localhost:<port>` or `[::1]:<port>` in any case, or a
   403 (`unknown_host`). The raw header and never `X-Forwarded-Host`, since no proxy stands in
   front to have forwarded anything. An allow-list of loopback's own names,
   because a deny-list would have to guess every name an attacker can buy.
   No browser sends `[::1]` yet: the listener binds only 127.0.0.1, so a
   connection to ::1 is refused before any Host is sent. It is on the list
   because accepting it costs nothing, and a server that also bound ::1
   would then need no change here.
2. **Origin**, on every request that sends one: `http://` and the name in
   Host, or a 403 (`foreign_origin`). That one origin, not any of the three: a page loaded as
   `localhost` that asks `127.0.0.1` is another origin to the browser, and
   `null`, from a sandboxed frame or a file, is no origin of the server's.
   Its absence proves nothing, since a browser leaves it off a same-origin GET
   and a script off everything. A front end may name exactly one more, the
   panel's dev server's ([through the dev
   server](#through-the-panels-dev-server)).
3. **The token**, under `/api` only: a 401 without it (`token_required`).
4. **A change by cookie** (any method but GET and HEAD, on a request whose
   token came as the cookie) must carry `Sec-Fetch-Site: same-origin` (or a
   403, `same_origin_required`), `X-Mindweaver: 1` (or a 403,
   `x_mindflayer_required`) and `Content-Type: application/json` (or a 415,
   `json_required`). By method rather than by endpoint, so an endpoint added later
   is covered without being listed; a POST that only reads, `POST
   /api/v1/plan` or `POST /api/v1/validate`, is held to it as well, at no
   cost, because the panel sends every POST alike.

Fetch Metadata answers the third kind of page directly: the browser writes
`Sec-Fetch-Site` itself, no page can change it, and it says `same-site` for a
page on another port. Browsers send it only to a potentially trustworthy URL,
which each of loopback's three names is, and the Host check admits no other.
The custom header and the JSON body say the same by other means. A page may
send another origin a form, `text/plain` or multipart unasked, but any other
content type, or a header of its own, needs a CORS preflight first, which
nothing here answers, so the request never leaves the browser. Either alone
would do in a browser that keeps to the rules; both are there for one that
gets a rule wrong.

A change with no `Sec-Fetch-Site` is taken only with a Bearer token. Chrome,
Firefox and Safari have all sent the header since 2023, so its absence means
a script, and a script that belongs here holds the token: the cookie is
`HttpOnly`, so not even the panel's own scripts can read it, and no page can
put an `Authorization` header on a request to another origin without the
preflight it never passes. Of these checks, a Bearer request meets only Host
and Origin. A browser too old for Fetch Metadata can read the panel but not
change anything through it, which is the right way round to fail.

So the panel (#69) fetches its own origin, letting the browser send the
cookie, and sends every POST with `Content-Type: application/json` and
`X-Mindweaver: 1`.

Every answer carries `Content-Security-Policy: default-src 'self';
frame-ancestors 'none'` and `X-Content-Type-Options: nosniff`. The policy
loads nothing from another origin and runs no inline script, so markup that
reached the page by some mistake, a gathered skill's say, could not bring a
script with it; `script-src` is left to `default-src` and never allows
`'unsafe-inline'`. No page can frame the panel to steer a click onto one of
its buttons. `nosniff` holds the browser to each answer's Content-Type, so a
page on another port that loads `/api/v1/transcript` with a `<script>` tag has
it refused rather than run. And there is no CORS layer at all: no answer
grants another origin anything, and a preflight is never passed.

Every answer under `/api`, a refusal as much as a read, carries
`Cache-Control: no-store` besides. A read is a GET, and a browser is free
to keep a GET's answer in the profile's cache on disk for as long as it
likes, the whole text of an artifact's file (`/api/v1/contents`) among
them, where a POST's was never kept. The panel loses nothing by it: what it
read stays in Vue Query's cache while the page is open, and is asked for
again whenever the disk changes. The panel's own files keep theirs
([the panel's files](#the-panels-files)).

What none of this stops is the server behind such a page. The browser sends
`mw_<port>` to every port of 127.0.0.1, since cookies are not isolated by port
and those requests are same-site, so any server there that the browser sends
a request to while the cookie is set receives the token in its Cookie header:
a page of it the user opens, or anything a page on 127.0.0.1 loads from it.
That server can then send the token as Bearer, with the Host a script writes
and no Origin, and pass every check above. So the guards keep out a benign
server's hostile pages, not a hostile server, which on a machine one person
uses is theirs, and on a shared machine may be another user's (an
[open question](#open-questions)). Such a server can also set cookies for
127.0.0.1, overwriting `mw_<port>` and locking the panel out until its link is
opened again; a cookie it adds under another path is no worse, because the
gate takes the token from whichever `mw_<port>` holds it.

### Through the panel's dev server

While the panel itself is worked on, its dev server serves it (`make
web/dev`) and proxies `/api` and the launch link to the server. A change
through that proxy still says the browser's Origin, the dev server's
(`http://127.0.0.1:5173`), which the Origin check refuses like every origin
but the server's own. So a front end may name that one origin to the server
(`Server::dev_server`), and only then is it taken: one exact origin, in any
case as a scheme and a name are, never a pattern, since every other port of
loopback is a page these checks are for. Host stays checked as it is: the
proxy sets `changeOrigin`, so it names the server rather than the dev
server. Fetch Metadata needs nothing, since the browser sees a same-origin
request to the dev server, and neither does the gate while the dev server is
opened as 127.0.0.1, where the launch link's cookie reaches it whatever its
port; opened as `localhost`, it has no cookie to forward.

The launch link then points at the dev server, where the panel is served
(`Listening::url`): its token is traded for the cookie through the proxy,
and the redirect lands on the dev server's page. What the widening lets in
is whatever answers on that port, which is no more than before: a server
there that the browser visits receives the cookie anyway, the hostile server
of the [open questions](#open-questions), and could send the token as Bearer
without the dev server's Origin.

### What a browser may name

The token and the guards decide who may ask; the backend decides what may be
asked. The CLI implements `Backend` in `backend.rs` (`PanelBackend`), and
treats every query and every task as a stranger's, because a request that
got past the guards came from a page holding the cookie, and a page on
another port of loopback holds it too ([what keeps other pages
out](#what-keeps-other-pages-out)). Which values a request may name, and what
of a file it may read, are rules, so they are core's (`policy`, `contents`);
the backend walks a task asking them (`admit::Admission`), one arm per
variant with no wildcard, so a new task does not compile until it says what
of it a browser may name.

- **One workspace at a time, pinned at launch.** `PanelBackend::launch`
  resolves the workspace once, from where the panel was launched and the
  launch's `Lookup`, as a read resolves it, and every line it runs names it
  with `-W <path>` among its words (`task::naming`, right after the binary,
  in place of the `-W .` a project consulting another workspace names),
  so what the panel previews, the transcript keeps and `mindweaver ui` prints
  once it stops says which workspace each line reached, and a line pasted
  into a shell reaches the same one whatever that shell has activated. Its
  other words are read from where it is typed, as any line's are: an
  entry `unlink` takes, or the scratch file a save names, from the
  workspace's folder, where the panel runs every line. Before the default one is made no
  line names it, since `-W` refuses a folder with no marker: the first
  change makes it, by a `mindweaver init` of its own, as the TUI does, and
  every line after that one names it. Nor does any line name a workspace
  whose root is not UTF-8, which no word of a line can hold: its lines run
  against it all the same, through the `Lookup` each runs with, which
  names it by the path as it is, as they did before any line named it,
  rather than every task on it refused for a word only shown; it can only
  be the launch's, since no switch is offered to one. Any other gone since
  it was pinned is refused, never made again by a change. So no line
  reaches another workspace because the disk changed after launch.
- **Another workspace only by a switch to one core lists**
  (`Query::Workspaces`, `Backend::switch`, `policy::switched`). The
  browser names a workspace by its path as the list of what may be
  switched to spells it, `known::switchable`: every workspace this
  machine knows, as `mindweaver workspaces` lists them, and the one the panel
  was launched on. The activated one is that one whenever there is one,
  since an activation wins over walking up and the default, and a `-W`
  given at launch switches the activation off, as for every command. The
  backend reads the list again when the switch arrives and matches the
  path character for character, so another spelling of a listed one, a
  path through its `.mindweaver`, a relative one or one starting with a
  dash, is refused (`switch_unlisted`) without anything opened at it; a
  listed one that will not open, gone since, is refused with why
  (`not_a_workspace`), never made. Nothing is switched by a refusal. Once
  taken, the home, the agent homes and everything else stay as the launch
  decided them, and the snapshot says the rule the list gives it: the
  launch's for the launch's, `-W` for every other (`Explicit`), since
  every line names it so from then on. A switch to the one in use changes
  nothing, as a load of one loaded already changes nothing.
- **No `-C`, `--home`, `-W` or agent home from the browser.** A task has no
  field for any of them, and no value it holds is read as an option ([every
  change is a command line](#every-change-is-a-command-line)). Each line
  runs through `commands::run_line`, the function the TUI's lines run
  through, from the workspace's root, with the home and agent homes the
  launch decided.
- **A project by its entry** (`policy::linked`). A holder, and each project
  an install, an uninstall or an adoption names, is an entry of the
  registry, spelled as the marker spells it and the snapshot lists it: not a
  name, which two projects can declare, nor a path the browser joined. The
  backend puts the folder the entry resolves to, absolute, in the line's one
  `-C` (or its `-p`), and the line names the workspace with `-W` as every
  line does, so it consults this workspace wherever it is read again: the
  browser's `project` or `elsewhere` is not taken, and the `-W .` the TUI
  gives a row whose project consults another is named for the workspace
  in its place. An entry `unlink`, `unload` or `relink` is given must be one the
  marker holds (`not_registered`, `not_loaded`, the workspace's own codes
  for the condition), and goes to the command as spelled, which is how those
  take one.
- **A folder not linked yet by a path that reads the same from anywhere**
  (`policy::path`). What `link`, `load`, `init` and the minds screen's marks
  name, the panel's marks among them, must be absolute (`path_relative`, or `path_url` for a `file:///etc`),
  spelled exactly as normalizing it would spell it, with no `.` or `..` part
  and no doubled or trailing separator (`path_unnormalized`), and outside
  every `.mindweaver`, as spelled and once its links are followed, as far
  as it exists, so a new folder under a link into one is refused too
  (`path_in_mindflayer`): a workspace's folder holds its marker,
  its ledger, its library and the clones it gathered, none of them a project
  to make or a workspace to load. The path goes to the command verbatim, so
  the preview shows the folder the command acts on. A mark to unlink, or
  what `mind unload` takes out, must be one an entry points at too.
- **A scan only under the folders the browser names** (`Query::Scan`), each
  held to the same rule as a folder to link, and refused with its code
  otherwise before anything is walked; none named is the workspace's root,
  as for `mindweaver scan`. A scan is a read: it lists folders and looks at a
  few names in each, opens markers and the ledger to read, and writes
  nothing, so nothing outside the workspace is reached that a command would
  not read.
- **A folder listed only where one may be named** (`Query::Folders`): the
  folder the explorer opens is held to the same rule as a folder to link,
  and refused with its code before anything is read; none named is the
  workspace's root, as for a scan. Core lists it (`browse::Folders`) and
  holds each folder inside it to the rule again, leaving out one it
  refuses, a `.mindweaver` and a link leading into one, rather than list
  what no request could name next. A listing is a read of names: each
  entry's type, and whether a marker, a `.mindweaver/weaver.toml` or a
  `.git` is there, looked for and never opened. So a page holding the token
  learns the names of folders wherever the account may list them, as a
  scan of a root it names could already, and nothing of what a file holds
  ([open questions](#open-questions)). It runs nothing: the folder chosen
  goes back to a field, and from there to a line as a typed one does
  ([choosing a folder](#choosing-a-folder)).
- **A plan only of linked projects** (`Query::Plan`): each box's project
  an entry of the registry, held by `Admission::selection` as a task's
  projects are, before core plans, which holds it to the same entries
  again. A plan is a read, and runs nothing: the tasks it answers with name
  each project by the entry its box named it by, and are admitted as any
  task's when they come back to be previewed and run. A reference or an
  origin is a value, which nothing offers when it is not one.
- **A draft only judged** (`Query::Validate`): its holder held as a read
  of an artifact's is, and nothing written; the text a save sends is a
  value, which goes to no line but as the file the backend writes it to,
  named by its hash ([authoring in the library](#authoring-in-the-library)).
- **What no front end without a terminal runs** is refused by
  `Task::refusal`, naming the line ([every change is a command
  line](#every-change-is-a-command-line)).

Values that name nothing on the disk, a skill's name, a description, a
template, an origin for `--from`, go to the command as they were sent: the
command holds each to its own rules, and the rule that no value is read as a
flag keeps each a value.

Each refusal has a code, core's or the CLI's, and the server sends it as a
404 when a read's parameters name what is not there, and as a 422
otherwise, as the backend says where it builds it ([every refusal with a
code](#every-refusal-with-a-code)). Nothing of a refused task runs: the server asks for its lines on the
writer thread first and never hands a refused task over, and the backend's
run plans the task again rather than trusting that it was asked, so it meets
the disk as it is. What does run comes back line by line
(`backend::Outcome`), each with its stdout, its stderr with each line's
code, and the code of what stopped it, a line the parsers did not take among
them (`line_unparsed`).

Reads are held the same way. A snapshot is of the pinned workspace, and so
are the parts of it served alone, its projects, its library and one project
(`Query::Project`), whose entry is matched by its spelling as a task's is
and refused as an unlink of one no registry holds is (`not_registered`, a
404). A file
(`Query::Contents`) is asked of the workspace's own or of a linked project by
its entry, its artifact by a reference, and the file by its route of plain
names from the artifact's folder, or a rule's own file name
(`contents_route_unfit`), so a `..` never reaches the disk; core's
`contents::Contents` reads it. Its links are followed and judged by
`copy::followed`, the check `copy::tree` makes of each link it meets, so a
`SKILL.md`, an asset or a folder on the way that leads out of its holder is
refused (`contents_outside`), and the file is read through the path the
check resolved, not the one asked for. A project's holds it to its root, as
an install from it holds a copy, since a skill may link to a file elsewhere
in its repository. The workspace's own holds it to the folder it keeps that
kind in, not to its root as an install from it does: the default
workspace's root is a home, where a link to `~/.ssh` would count as inside
it. UTF-8 text with no NUL, no larger
than a mebibyte (`contents::CAP`), comes back as a string inside JSON;
anything else is reported with its size, `binary` or `too_large`, and its
bytes stay where they are, one over the cap never read whole. No answer says
what type of file it is, and no artifact file is ever served as itself, so
an HTML page or an SVG holding a script reaches the panel as text to show.
Should some mistake put it on a page, the CSP runs no inline script and
`nosniff` holds the answer to `application/json`.

What an artifact holds is listed the same way (`Query::Files`, core's
`contents::Files`): asked of the same holders by the same reference, held
to the same bound, and answered with every file and folder by the route a
file is then asked for by, a folder before what it holds and each folder's
in the order of their names, and whether the listing stopped at
`contents::FILES_CAP`, a thousand. The artifact's own folder or file
leading out of its holder refuses the whole listing (`contents_outside`),
as a file of it is refused; a link further down that leads out is listed
with why and never followed, nor is one leading back into a folder it is
in (`contents_cycle`) or to nothing (`contents_missing`), so one bad link
hides nothing else and costs no walk without end. Nothing of where a
refused link leads is listed, only its path in the reason. A name no route
spells, one that is not UTF-8 or that holds a `\`, which `place` refuses as
a part of a route, is listed with why too (`contents_name_unfit`), by its
name as near as UTF-8 writes it, and a folder of one is not walked: listed
as a file, it would be a button whose answer could only be a refusal. #67
left the listing for later; it sits beside the reading, in one module, so
the two cannot hold a file to different bounds.

It is the one way a file's contents reach the browser. `show` would be a
second: it prints an artifact whole, read through every link on the way,
since a rule shared by a link is still a rule, with no cap. So it is on the
list of what no front end without a terminal runs (`show_unheld`), rather
than held to its holder for the terminal too, which would stop it showing a
rule a team shares by a link: at a terminal its reader is the person whose
disk it is.

The CLI's suite (`backend_test`) drives `PanelBackend` through the trait with
the JSON a browser would send, malicious shapes in a table, and binds no
port; the server's own suites cover the transport over a fake backend.

### Switching what it acts on

A front end may keep its backend on one thing at a time and let the panel
move it to another, as the CLI's keeps one workspace and lets the header
switch it ([what a browser may name](#what-a-browser-may-name)). The trait
says so generically: a fourth associated type, `Switch`, which the server
deserializes `POST /api/v1/switch` into and hands to `Backend::switch`, whose
answer goes back as the backend's JSON, or a `Refusal`, sent as a 422,
since what it names is in its body. Which values may be named, and what each means, are the front end's, as a
task's are: the server knows no workspace, and only carries the request.

It is carried to the backend on the writer thread, behind every run sent
before it, so it never lands while a line is running, and a task sent after
it runs against what it switched to. Once it is taken, the writer thread
has the watcher ask the backend for its paths again and waits until it has
(`Watcher::again`), which drops every watch the backend no longer names,
and only then tells every subscriber `switched` ([live
updates](#live-updates)): so whatever a tab reads on hearing it, a change to
it is watched for. A backend that panics while it switches may have
switched before it did, so it is told then too, and the thread lives on; a
refusal switched nothing, and tells nothing.

A switch is no run: it is kept in no transcript, which holds what can be
replayed, and every line the CLI's backend runs names its workspace with
`-W`, so the transcript stays true across a switch without one. A task
that switches was the other way, which would have put a run with no lines
in the transcript, and left the server to learn to watch anew after one
task among the rest. A body of JSON rather than a fourth type would have
spared the fake backend one, and left the server's suites nothing typed to
carry.

### Live updates

The panel shows a change made from a terminal, the TUI or an editor without
a reload (#74). The server watches what the backend names and tells the
browser, on one stream, when something there changed; the panel then asks
for the snapshot again, which is all it ever does about it. Nothing is
pushed but the word that something changed: what changed is read the way
every read is, so the panel cannot show a change the snapshot would not.

**What is watched is core's** (`watch::watched`), every file and folder a
snapshot reads that somebody can change, each with what it is of, the
workspace or a project by its entry. The CLI's backend hands that list
over, each scope written as the holder a query names
(`{"type": "project", "entry": "api"}`), and a desktop shell would hand
over the same one. For the workspace: its marker, its ledger, its shelf's
folders, the folders its library keeps each kind in, its templates, each
workspace it loads, by its marker and library, and the default workspace's
marker in home, whose `known` the snapshot lists. For each project: its
marker, whichever file it is in, each file an agent reads in it
(`Agent::reads_files`: `AGENTS.md`, `CLAUDE.md`, `.mcp.json`,
`.codex/config.toml` and the rest, every agent's, since the block of
`AGENTS.md` is read whichever the project names), each folder it keeps a
kind in and each its agents read one from (`Catalog::folders`), the files
Claude is given its rules in, and its templates. An entry that opens no
project is watched for its marker coming back.

- **Named, never a tree.** Each path is watched on its own
  (`RecursiveMode::NonRecursive`): a file for its own changes, a folder for
  what is directly in it. The folders named are those discovery looks in
  (`Catalog::looked_in`): a kind's folder, each skill's own folder, whose
  `SKILL.md` is read, and every folder of rules the walk goes into, since a
  rule is found at any depth. Never a project's root, which would wake for
  every file of the code beside it, and never a `.git`, a `node_modules` or
  what a skill keeps in folders of its own (a `.git` is named only while it
  is not there, for [doctor](#what-no-agent-says)'s sake): a recursive
  watch costs one of the system's watches for every folder under it. A
  test of core's list holds it to that over a fixture with all of those,
  and one of the
  server's to watching only the folders named and the one above each,
  none of them recursively, which it sees in the mode the watcher asks
  the system for: nothing on the stream would show it, since a change
  further down is no path's either way.
- **What is not there yet is named all the same**, a project's
  `.claude/skills` before its first install, and watched from the nearest
  folder above it that is, for its being made. That rule is the watcher's,
  being the same for any path: it knows files and folders, never which
  ones a project has.
- **A folder above a path named is a change to it.** Removed or renamed,
  it takes the path with it, and it is all that is told: the debouncer
  folds what a folder held into its own removal, so `rm -rf` of a linked
  project, `git clean` taking its `.claude` or a `mv` of either is told by
  that one folder, which is named nowhere. A change to a folder beside
  them, a project's `src`, is still nobody's.
- **Asked for again after every change it tells**, since a change can name
  new paths, a skill installed or a project linked: `weaver.toml` changing
  is one such change. A folder an event named as made, removed or renamed
  is watched again besides, and every folder below it, since a watch stays
  with the folder it was set on, and an install puts a copy in place by a
  rename over the folder. notify drops every watch below a folder removed,
  or renamed away from a parent it watches, and one renamed back within the
  debounce is where it was, with nothing below it watched until each is set
  again. The watches are set before the change is told, so whatever the
  panel reads on hearing it, any change after it is watched for. And after
  a switch, which no change on disk says, with every watch of the paths
  before dropped, and a watcher started then if the backend named nothing
  before.
- **Nothing a read does.** A file opened or closed without a write is no
  change, or a snapshot reading a watched file would set off the next
  snapshot, without end. For the same reason the ledger is named by its
  file and never by its folder: SQLite makes `mindweaver.db-wal` and
  `mindweaver.db-shm` beside it to read it and takes them away as the last
  connection closes, so a watch on `.mindweaver` would wake for every read.
  A write reaches the file itself when its log is checkpointed, at the
  latest as the last connection to it closes, as each command's does; a
  read that is that last connection checkpoints a write before it, which is
  that write's change told once. Tests hold both: core's, that a snapshot
  changes nothing named, and the CLI's, that reading the workspace over the
  real backend is told as no change, which fails when either rule is
  broken.

**The watcher** (`Watcher`) uses notify 8 and its full debouncer, which
waits 300 milliseconds (`DEBOUNCE`) so a burst, a command writing a dozen
files or an editor saving through a temporary one, is told as one change,
each scope it reached once. notify, rather than a poll of the files, since
the system says what changed as it changes; and its full debouncer rather
than the mini one, since it keeps what kind of change each was, which is
how a file read is told apart from a file written, where the mini one says
only that a path changed. It starts with the server, setting the first watches
before `bind` returns, so a change made as soon as the link is out is told.
A backend that names nothing costs no watcher at all.

**The events** go through a bus (`Events`), a tokio broadcast channel every
subscriber hears all of, and out at `GET /api/v1/events` as server-sent events,
one `data:` line of JSON each, tagged by `type` as a task is:

- `changed`, with `scopes`, each scope a change reached;
- `command_started` and `command_finished`, with a run's `lines`: told on
  the writer thread once its preview passed and once it stopped, however it
  stopped, a refusal or a panic included, the finish once the run is in the
  transcript;
- `switched`: the backend acts on something else since, from this tab or
  another, and what it names now is watched; everything is to be read
  again, and nothing shown before kept. With `switches`, how many the bus
  has told, this one counted (`Events::switched`);
- `resync`, first on every stream and again for one that fell behind: the
  channel keeps 64 events (`CAPACITY`) for a subscriber that has not read
  them, and one further behind is told to ask for everything again rather
  than handed a backlog. With `switches` as the latest `switched` said it,
  taken under the lock a subscription is, so a subscriber hears a switch
  or is told its count, never neither: how a tab that missed one, its
  stream let go while hidden, learns of it. A count rather than a word
  that a switch was missed, which only a bus that knew what each
  subscriber had heard could say;
- `watching` or `unwatched` with its `reason`, after `resync` once the
  watcher has said, and again whenever that changes.

Server-sent events rather than a WebSocket: the stream goes one way, a
browser's `EventSource` connects again by itself after a drop, and it is a
plain GET, which the gate holds to the token as it holds every read, the
cookie included. A comment every 15 seconds while nothing happens
(`KeepAlive`) keeps a proxy between, the panel's dev server's among them,
from taking the connection for one that died. The bus and the watcher know
nothing of axum, so a desktop shell answering the panel without HTTP
(D-17) takes events from the same two; `Listening::events` hands the bus to
a front end. Stopping the server closes the bus first, which ends every
stream: each is a request under way for as long as a browser keeps the page
open, and a graceful stop would wait for it without end.

**Six connections a browser.** The server speaks HTTP/1.1 only, and a
browser opens at most six HTTP/1.1 connections to one host and port;
HTTP/2 would carry every request on one, but browsers speak it only over
TLS, which a loopback panel has no certificate for. Each stream holds a
connection for as long as it is open, so six tabs of one panel would leave
none to ask with. So the panel keeps one stream for a tab, never one for a
view, and lets it go while the tab is hidden, taking it again when the tab
is shown, which the server answers with `resync`. Tabs of two launches are
two ports, and count apart.

**The system's limits on watches.** On Linux each watch is one of inotify's,
of which one user has `fs.inotify.max_user_watches` across every program
(8192 on older kernels, more on newer ones with memory to spare), and each
watcher one of `fs.inotify.max_user_instances`, 128 by default. A watch the
system refuses is said rather than failed on: the watcher keeps every watch
it could set, tells `unwatched` with why, and tries again with the next
change, telling `watching` once every one is set. A watcher that cannot
start at all, no instance left, is `unwatched` for as long as the server
runs. macOS's FSEvents and Windows' `ReadDirectoryChangesW` have no such
count per folder. The panel shows a banner on every page while the server
is `unwatched`, saying why, with a button that asks for everything again,
and asks again on every page and every return to the window, as it did
before there was a stream ([its views](#its-views)). A project costs about
ten watches, and one for each copy of a skill it holds, so a workspace
reaches the old 8192 only past a few hundred projects; a watch of every
folder in each repository would reach it with one `node_modules`.

### The log

The server prints nothing of its own accord. A front end that wants a log hands
`Server::log` a function, and gets a line per request once it is answered,
like `GET /api/v1/transcript 200 3ms`: the method, the path and query, the
status, and how long the answer took, in whole milliseconds, from the request
reaching the server to the answer's head, so an event stream's says how long
it took to open rather than how long it stayed open. How long, because a log
is most often read for a request that took too long, and a client's clock
cannot tell the server's part of that from its own (#240).
Never a header, since the cookie and the Bearer token travel in them, and never
a body.

The writer thread says what it did with each job in the same log, in lines
`log::Log` builds as it builds a request's: a run once it is taken off the
queue and its lines are worked out, `run: 0ms waiting its turn, 1ms working
out its 1 line`, before any of them runs, so a run that never ends is said
to have started; again once they have run, however that went, `run: 5402ms
running its 1 line`; and a switch once it is made or refused, `switch: 0ms
waiting its turn, 3ms switching`. Waiting its turn is the time behind every
job queued before it, which the run's request line, said once it is
answered, cannot tell from the run's own time or from what carrying the
answer took (#240). None of them says what a task was or which lines it
ran, which are the request's body.

The server knows a task and not its lines, so the CLI's backend says what
it can of each, in the log its front end hands it as it hands the server
one (`PanelBackend::logging`, which `mindweaver ui --log` hands both,
[launching it](#launching-it-mindweaver-ui)): once a line has run, by its place among the
task's lines, how long it took and how much of that it waited for a
workspace's lock another writer held, and which process held it when one
did and said so, `run: line 1 of 1 took 5402ms, 0ms of it waiting for a
workspace's lock`. Core counts those waits on the thread that waited
(`lock::waits`), rather than each writer handing back how long it
waited, which would carry a duration through every writer and every
caller of one for the one front end that asks. Between them the lines
say where a slow run's time went: behind another job on the writer
thread, behind another writer of the workspace, or in the line itself.
The value of a `token` parameter is replaced by `<redacted>` however its name
is encoded, decoded the way the exchange decodes it to find it; and the token
is scrubbed from anywhere else in the line, upper-cased or pasted into a path,
because there it is still the secret. axum's `tracing` feature is off, so
nothing logs on the server's behalf.

### The panel's files

The panel is embedded with rust-embed from `apps/web/dist`, spelled
`$CARGO_MANIFEST_DIR/../web/dist`, so the binary stays the one thing to
install. `allow_missing` lets the crate build, and its tests pass, on a
machine without Node where nothing built the panel; the server then answers
every page with a 503 saying `panel not built: run make web/build`, the
target that writes the folder.

So the panel is built before a binary that ships it: `make cli/release` and
`make install` build it first where Node and pnpm run, and say what they
embed instead where they do not ([in `make test`, and in
CI](#in-make-test-and-in-ci)). A release build embeds each file as it
compiles, as `include_bytes!`, so cargo sees a later build of the panel and
compiles the server again; a debug build reads the folder at run time, and
sees one at once. Either, compiled while the folder was missing, embeds
nothing and never looks again, since rust-embed resolves the folder once; so
`make web/build` touches `src/panel.rs`, the file that embeds it, a change
cargo sees, and so does `make web/build/if-node` when it builds nothing and
finds the folder filled from elsewhere, as CI fills it. A build script
watching the folder was the alternative, and it would compile the server
again on every build while the folder is missing, as it is wherever Node is
not, CI's Rust jobs among them.

Files under `assets/` are named by Vite for their contents, so they are sent
as immutable, to be kept for a year; everything else, `index.html` above all,
is `no-cache`, so a rebuild shows on the next load. A path outside `/api` that
names no file gets `index.html`, because the panel draws its own routes in the
browser, except under `assets/`, where a missing file is one an old page asks
for after a rebuild, and a 404 is easier to read than a page delivered as a
script. A test hands the server a panel of its own (`Panel::embedded`, any
folder rust-embed can embed), so the suites cover all of this without a build.

Only a name Vite could have written reaches the embed. A debug build's
rust-embed reads the folder from disk, where it turns `\` into `/`, lets an
absolute path or a drive letter replace the folder, and follows a symlink out
of it; without a check, a request with no token could read whatever a symlink
anywhere on disk leads to. So a path holding a backslash or a dot segment,
which no browser sends, is a 404, and one with a drive letter or an empty
segment is never looked up and gets `index.html`, as one of the panel's routes
may. A release build only looks names up in a table, and the check keeps the
two answering alike.

### Launching it: `mindweaver ui`

`mindweaver ui` serves the panel from the binary until Ctrl-C, and `mind ui` is
the same command reached from a project. The workspace is resolved once, as
the command starts, from its `-C`, `--home`, `-W` and agent homes
(`PanelBackend::launch`), so one that cannot be used fails the command as
any would, before anything listens, and nothing the browser sends later
changes it but a switch to one core lists, which keeps everything else the
launch decided. The server binds 127.0.0.1 on a port the system picks, or the
one `--port` names, and a port somebody holds fails the command
(`panel_unbound`) rather than move to another.

The link goes to stdout as soon as the server listens, and a token file
named is written (below), the first line there and the only one until the
transcript follows it once it stops, so a script reads it, and stderr says which workspace is served, on which address, and
that Ctrl-C stops it. That is where `ui` parts from the rule
that a command builds its output and hands it back to be printed
(`Outcome`), as the TUI's screen does: the link cannot wait for the command
to end, which it does only when it is stopped. Then the link is opened with
the `webbrowser` crate, the way each platform starts its browser, unless
`--no-open`; one that cannot be opened is a warning, since the link is there
to open by hand. It is opened on the blocking pool once the server is serving, not
before: `webbrowser` waits on a text browser until it is closed, which would
otherwise wait in turn on a server that never started.

Ctrl-C is listened for before the link is out, through the platform's own
listener, a Unix signal or the Windows console's Ctrl-C, rather than
`tokio::signal::ctrl_c`, which installs its handler only once it is first
awaited (`Stops`). On Unix a `SIGTERM` is taken as a Ctrl-C, since it is
how `kill`, `timeout` and a service manager stop a process, and so how a
script stops a panel it serves itself the API with: ended by the signal's
default, `ui` would leave its token file behind (below). Not a `SIGHUP`,
which a handler would answer even under `nohup`, replacing the ignoring
`nohup` asks for, so a panel meant to outlive its terminal would stop with
it. The first stops the server taking connections and lets every
request under way be answered, a run above all, so a change is not cut off
half way, and ends every event stream, which would otherwise be answered
for as long as a tab stays open ([live updates](#live-updates)); the
second stops at once, for whoever would rather have that than wait. What comes back once it stops is what the TUI hands back when it
closes: every line the panel ran for a task that changes something
(`Task::writes`), with what each said, laid out by the TUI's function
(`PanelBackend::transcript`), so a session in the browser can be read and
run again from a shell. The server's own transcript, the one the panel
shows, holds reads as well, and goes with the server.

`--token-file PATH` hands what the link carries to a script that did not
start the panel, and so cannot read its stdout (#216). Once the port is
taken and Ctrl-C is listened for, and before the link is printed, `ui`
writes there, as JSON, `version`, how the file is laid out (1, moved as
`ENVELOPE_VERSION` is, when a name is dropped or renamed or holds
something else, and not when one is added), `api`, where every route is
served, and `token` (`TokenFile`, in `src/commands/ui.rs`). `api` is the
server's own address under the version (`Listening::api`,
`http://127.0.0.1:<port>/api/v1`), even with `--dev`, whose link names the
dev server: a script sends the token as Bearer, which needs no page and so
no dev server to serve one. The path is typed from the directory the
command works in, as every path a command takes is, and it is one the
person names rather than one of Mindweaver's own, in its home say: several
panels may be served at once, each with a token of its own, and only
whoever starts one knows which script it is for.

The file is made with `create_new`, mode 0600 on Unix, and written in one
go: whatever is at the path already, a file, a folder or a link wherever
it leads, is refused and left as it is (`token_file_exists`), and so is a
path whose folder is not there (`token_file_folder_missing`, since
`--token-file` makes no folder) and one that cannot be written for any
other reason (`token_file_unwritable`), each before anything is served
and with no link printed. Not a temporary renamed into place, which would
replace a link or a file somebody put there in the meantime, and never
through a link. A link among the folders on the way is followed: the path
is the person's own, typed on their command line, unlike one a
repository or a browser names. Windows has no mode bits, and the file
takes the permissions its folder passes on, which is #154's to tighten.

When `ui` stops, the file is taken away, and so it is when the command
fails once it is written, but only while it still holds what was written
there: looked at as a link, never through one, by its length and then
byte for byte. Anything else at the path is somebody's, left as it is,
and said in a note, which fails nothing; one that could not be looked at
or taken away is a warning (`token_file_unremoved`), and the command exits
as one that went wrong, since a file it was to take away may still be
there. A copy of what was written, put back by somebody, goes all the
same, holding a token that opens nothing any more. Nothing else changes:
the link, the note and the transcript are as they were, the token is
printed nowhere the link was not, and `TokenFile` has no `Debug` to print
it with. Nor can the panel ask for a file: a front end with no terminal
refuses `ui` whatever its options (`Refusal::Panel`), so a browser never
names one for it to write.

`--log` hands the server and the backend one log ([the log](#the-log)),
which writes each line to stderr as it is said, after the time of day, in
UTC to the millisecond (`04:20:59.756Z`): as it is said, since it is for
watching a panel serve, or for reading beside a browser's trace of the same
minutes, and either needs it while the panel runs; the time of day alone,
which is what that reading asks for, in UTC, which needs no timezone looked
up and is what a trace and CI's log say their times in. Off unless asked
for, since a line a request is too many for a terminal somebody is using,
and on stderr rather than in a file of its own, which would be one more
path to refuse when something is there: whoever starts the panel sends
stderr where they like. Without it, stderr says what it always has. Its
help and the reference say what a line holds as the log lays it out: a
request's path and query, so a read's line names the folders and entries
it asked for, and never the token, a header or a body, so neither what a
change was sent nor which commands it ran. Not "nothing a request sent",
which a read sends in its query, and which somebody passing a log on
would take at its word.

The runtime is tokio's with one thread: every query goes to the blocking pool
and every run to the server's writer thread, so it only moves bytes. `--dev`
serves the panel through its dev server ([through the panel's dev
server](#through-the-panels-dev-server)): the server takes requests from
`http://127.0.0.1:5173` too, the link points there, and the port is 7878,
where `make web/dev`'s proxy looks, unless `--port` names another. The note
on stderr says the server's own address, which the link then does not. A
port held there is said with other advice than without `--dev`: stop what
holds it, or name another both in the proxy's `SERVER` and with `--port`.
Not another with `--port` alone, nor 0, as without it: the proxy reaches only
the port `SERVER` names (`unbound_hint`).

All of it is behind the CLI crate's `ui` feature, on by default so the one
binary somebody installs has the panel
([D-17](#d-17-the-next-front-end-is-a-local-web-panel)): the server, axum,
rust-embed, tokio's signal handling and `webbrowser`. `cargo build -p
mindweaver-cli --no-default-features` builds the commands and the TUI
without them, and `ui` is then no command; `tests/crate_test.rs` holds that
build to no server, axum, embed or `webbrowser`, `make cli/build` builds
it first, and `make cli/test` runs its unit tests, one of which finds no
`ui` there. hyper and tokio stay, which gix's HTTP transport brings to core.
What the feature costs, measured for D-17 on Linux x86_64 with 16 cores, in
the release profile, with no panel built unless said:

| | without `ui` | with `ui` |
|---|---|---|
| `mind`, as built | 24.9 MB | 27.3 MB (+2.3 MB, +9%) |
| `mind`, stripped | 19.5 MB | 21.1 MB (+1.6 MB, +8%) |
| `mind`, the panel embedded | | about 28.2 MB |
| clean build, wall clock | 1 min 22 s | 1 min 29 s (+7 s, +8%) |
| clean build, CPU | 12 min 20 s | 13 min 37 s (+10%) |

Turning the feature on in a target that holds the build without it takes
38 s, most of it compiling tokio again with its signal driver, and gix and
core above it, since a feature of a crate is one for every crate that uses
it.

## The web panel

`apps/web` is the panel [D-19](#d-19-the-panel-is-built-on-nuxt-ui) chose:
Vue 3.5 and Nuxt UI 4 on Vite 8, in TypeScript 6 checked by vue-tsc, laid out
the way Nuxt UI's dashboard-vue template lays out a dashboard, with a page
per file under `src/pages`. It holds no rules, any more than the server
does: what a valid task is, or what a project has, is core's to say, and the
panel asks. It shows an overview, the projects and a project's page, drawn
from the snapshot (#72), and the library, one of its own, the shelf and the
sources (#73), the second reading an artifact's files besides, and a page
of what could be linked, from a scan it asks for by hand (#77), in a frame
of a sidebar and a header ([its views](#its-views)). It changes things by
running tasks the server previews first, linking, relinking and unlinking
projects, changing a project's agents and share mode and migrating one an
older marker marks, and shows a transcript of every run (#76, #77, [its
changes](#its-changes)). It edits, adds and renames the library's own, and
adopts somebody's skill from a project's page, judged by core as they are
typed ([authoring in the library](#authoring-in-the-library), #82),
loads and unloads other workspaces from a page of its own (#83),
switches to another workspace this machine knows from the header of every
page ([switching workspace](#switching-workspace), #84), and goes to any
of it, or installs, updates all and links, from a command palette over
every page, its keys listed on `?` ([the command
palette](#the-command-palette), #85). It finds the folder to link, or to
scan under, in an explorer of the disk core lists a folder at a time
([choosing a folder](#choosing-a-folder), #212).

### One way out: the transport

Every request the panel makes goes through a `Transport`
(`src/api/transport.ts`): `query` for a read, `send` for a task to preview or
run, `transcript` for what the session ran, and `subscribe` for the events
the server streams ([live updates](#live-updates)). Views get it from
`useTransport`. The HTTP transport (`src/api/http.ts`) is the only file allowed
`fetch`, `EventSource`, `WebSocket` or `XMLHttpRequest`: an ESLint rule
refuses them in every other file, and a test runs the rule.

It is the versioned API's first client, under `/api/v1` ([its
routes](#its-routes-under-apiv1)): each query is asked for at the route the
backend serves it at (`route` in `http.ts`), a GET naming what it reads in
its parameters, a holder as the `project` its entry names, or, for a plan
and a draft to judge, a POST whose body holds the query's own fields. One
case for every query, so a query Rust adds is a function with no route for
it, and the panel does not compile until it is spelled. Spelling a route is
the transport's, as spelling a URL is; which routes there are, and what each
becomes, is the backend's table, which a test of the CLI's holds to the
very query the panel sends for each read, and the browser suite calls.

So authentication lives in one place. The transport uses relative URLs, so
the panel talks to whichever server sent it, on whatever port it took. The
token travels in the cookie the launch link set, which is `HttpOnly`, so the
panel's own scripts never hold it. And every request it posts is JSON and
carries `X-Mindweaver: 1`, a header no form or link can add, as the server
asks of a change by cookie ([What keeps other pages
out](#what-keeps-other-pages-out)). A desktop shell, the reason D-17 asks
for the interface, would answer the same four calls without HTTP, and no
view would change. A refusal comes back as a `TransportError` holding the
status, the words and, when the backend refused, its code, which the panel
draws a refusal by.

What the server answers lives in Vue Query's cache, which knows when an
answer is stale and asks again; a refusal is not asked again, since the
server would only refuse it again. Pinia keeps what only the browser
decides, such as whether the sidebar is collapsed. A store holding a copy of
something the server said would be a second answer that cannot tell when it
has gone wrong.

What the browser decides and a launch should not forget, the colour mode and
the sidebar's size and collapse, is kept in cookies (`src/preferences.ts`)
rather than localStorage. localStorage is kept per origin, and each launch
takes a port of its own, so every one would open on an empty store; a cookie
is kept per host, whatever the port. Neither choice can be handed a storage,
since Nuxt UI reads the colour mode through a bare `useDark()` and the
sidebar through a bare `useStorage`, so the cookies are vueuse's default
storage (its `getDefaultStorage` handler). The browser sends them to
whatever listens on 127.0.0.1, so they hold nothing about anybody's work.

### Its types come from Rust

What the panel sends and is answered with is said once, in Rust, and the
panel compiles against TypeScript written from it (#70): the snapshot and
everything it holds, an artifact's files and a file's contents, what
`list`, `show` and `validate`
print, a query, a task, what a scan, a plan and a run produced, the envelope a
reading command's `--json` prints, and every code core and the CLI give. That
TypeScript is the contract, the panel's side of [the read
model](#the-read-model): `apps/web/src/api/generated`, a file a type,
written by ts-rs, committed so a fresh clone type-checks without cargo, and
never edited by hand. The transport takes its `Query`, `Task` and `Outcome`
from there, and says what each query is answered with by its `type`
(`Answers`), so a query Rust adds leaves the panel not compiling until it
names the answer. Only the server's own replies, `{lines, confirm}` for a
preview, `{lines, outcome}` for a run and `{error, code}` for a refusal, and
the events its stream tells (`LiveEvent`), are still spelled in
`transport.ts` ([open questions](#open-questions)); a preview's items to
confirm are the generated `Confirm` and a change's scopes the generated
`Holder`, since both are the backend's words.

- **Derived beside serde.** Each type derives `ts_rs::TS` beside its
  `Serialize` or `Deserialize`, under a `ts` feature of core's and the
  CLI's (`#[cfg_attr(feature = "ts", derive(ts_rs::TS))]`), so its
  TypeScript is read off the attributes serde reads (`tag`, `content`,
  `rename_all`, `skip`): a task's `{"type": "link", "path": ...}` is a union
  of records told apart by `type` in both. Under a feature, so a plain
  build compiles none of it and a plain `cargo test` writes nothing
  anywhere: only `make web/types`, `make web/types/check` and the second
  clippy run of `make lint` build with it. A `u64` or an `i64` is a
  `number` rather than a `bigint` (`TS_RS_LARGE_INT`): a size or a time
  never comes near what a double holds, and `JSON.parse` gives a number. A
  path or a URL written through `serialize_with` is the text it is in
  TypeScript already, so ts-rs's warning that it cannot read that
  attribute, the one it would give here, is off (`no-serde-warnings`).
  What holds it is a test rather than the warning, which only scrolls past
  in a build: it serializes a field through each function a field names
  and holds what serde writes to what ts-rs says of the field's type (a
  path is a `string` whatever its bytes, a redacted URL a `string` or
  `string | null`), and refuses a function it does not hold, read off the
  source (`described::field_serializers`), core's and the CLI's, which
  names none.
- **What serde writes by hand is said from what it writes**, beside its
  serializer, never derived from the Rust variants, for the reason the
  serializer is written by hand ([the read types
  serialize](#the-read-types-serialize)). A word (a kind, an agent, a
  state, a share mode, the rule that chose a workspace, a marker's file,
  why a scan found a directory worth linking) is
  the union of its words, written by `described::words!` from the function
  the serializer calls, on every variant, matched with no wildcard so a
  variant added does not compile until it is in the union. A word whose
  type's name another's already has in TypeScript is declared under a name
  of its own (`as "ScanReason",`, for the scan's `Reason`), since both
  crates write into one folder, a file to a name. Deriving each
  with its variants renamed to their words was the other way, which spells
  every word twice and ties the TypeScript to the variants again. A
  `ValidationIssue` is a union of one record per code, `{ code:
  "name_segment_too_long", segment: string, length: number, warning:
  boolean, message: string }`, so the panel narrows an issue by comparing
  its code: what each carries is one match (`ValidationIssue::carried`)
  that the serializer writes from too, read on one issue of each code,
  which a test holds to `CODES`. A template's origin, `string | null`, and
  a skill's metadata, an object written in order, say what their
  serializers write with `ts(as)`. A test holds each to what serde writes
  (`typescript_test.rs`): every word, every issue's keys, the origin and
  the metadata as JSON and as TypeScript, and every name a query, a task, a
  holder or an agents line is read by.
- **Codes are unions too**: `CoreCode`, from `code::ALL`, and `Code`, which
  is it and the CLI's own (`CODES`), so a code the panel compares with is
  one that is given. Not the type of a `Said`'s or a warning's `code`,
  which stays a `string`: what a front end hands on holds the CLI's codes
  beside core's, and no test holds every line a command says to a code
  listed, which such a type would promise.
- **One list of roots a crate** (`typescript::roots`), which its export
  test writes, each root with every type it names, into
  `TS_RS_EXPORT_DIR`, which `.cargo/config.toml` points at the panel,
  relative to the repository, so both crates write into one folder. It
  sets every variable ts-rs reads, and forces each, since cargo otherwise
  leaves one the shell exports alone: a shell set up for another project
  would have the export write somewhere else, once the folder was emptied,
  or write other imports, integers or maps, drift CI would never see. One
  list rather than `#[ts(export)]` on each root, so that the test finding
  two types written to one file, which ts-rs would merge into one file or
  write one over the other, walks what the export writes, both crates'
  roots together. Cargo run from outside the repository reads no
  `.cargo/config.toml`, so an export test that finds no folder named is
  refused rather than writing ts-rs's default, `./bindings`, beside the
  crate.
- **Written again by cargo alone.** `make web/types`
  empties the folder, since ts-rs writes and never deletes, runs the tests
  named for it with the feature (`--lib typescript`: the exports and the
  tests holding each type to serde), and checks the panel against what they
  wrote with vue-tsc, which `make web/typecheck` runs alone. `make
  web/types/check` writes them the same way and fails when git sees them
  differ from the ones committed, a file new to git among them. It needs no
  Node, so CI runs it in a job of its own, `types drift (web)`, and a change
  to what core or the CLI hands the panel fails there unless it was written
  again; the vue-tsc half is `test (web)`'s ([in `make test`, and in
  CI](#in-make-test-and-in-ci)).

The panel compiles against no OpenAPI. #70 left a description of the
routes out, a schema between the Rust and the TypeScript that ts-rs does
without; since [D-24](#d-24-the-local-server-is-a-versioned-http-api) the
API is described for every other client, its types as JSON Schema ([the
API's schemas come from Rust](#the-apis-schemas-come-from-rust)) and its
routes as an OpenAPI 3.1 document written from the tables that serve them
([the API's description is written from both
tables](#the-apis-description-is-written-from-both-tables)), both beside
the TypeScript rather than in its place: the panel's types are still read
off serde's own attributes, and its transport still spells each route ([open
questions](#open-questions)). A protocol crate holding the types, which both
the CLI and the server would depend on, was rejected with
[D-17](#d-17-the-next-front-end-is-a-local-web-panel)'s B; the types stay
where serde's are, and the feature only reads them.

### The API's schemas come from Rust

What the API takes and gives is described as JSON Schema 2020-12, which
OpenAPI 3.1 takes as it is, written from the Rust types (#217,
[D-24](#d-24-the-local-server-is-a-versioned-http-api)) as the panel's
TypeScript is: `docs/api/schemas`, a file a root, each holding the root's
schema at the top and every type it names in its `$defs`, committed and
never edited by hand. Every type the TypeScript describes is described
there too, and beside them what the API takes or gives that the panel's
types leave to `transport.ts` or never needed: the bodies of the two reads
that take one (`Selected`, `Drafted`), the snapshot's projects, one project
and its library, which three routes answer with alone, the server's own
answers, a preview, a run, each event and a refusal (`Preview`, `Ran`,
`Event`, `Refused`), and the token file a script reads first
(`TokenFile`).

- **Derived beside serde, as the TypeScript is.** Each type derives
  `schemars::JsonSchema` beside its serde derive and its TypeScript, under
  a `schema` feature of core's, the CLI's and the server's
  (`#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]`), so a
  plain build compiles none of it and a plain `cargo test` writes nothing.
  schemars reads the attributes serde reads (`tag`, `content`, `flatten`,
  `rename_all`, `skip`, `deny_unknown_fields`, `default`), so a schema
  says what serde writes or reads; schemars 1.x, the current major, whose
  default is 2020-12. The server derives its own under its own feature: it
  may depend on schemars, which is no mindweaver crate, and what it carries
  of the backend's unread, an item of a preview's `confirm`, a run's
  `outcome`, a change's scope, is any JSON in its schemas, which backend
  type fills it being the front end's to say, as a route's answer is.
- **Each the way the API uses it.** serde's derive writes a field that is
  an `Option` every time, `null` when it is none, and reads one left out
  as none, so a schema of one way for both would call a field written
  every time optional, or refuse a request that leaves out what serde
  reads as none. A root the API answers with is given, described as serde
  writes it, every field in `required`; one it is sent, a query, a task, a
  switch and the two bodies, is taken, described as serde reads it
  (schemars' contract). Each crate's `schema::roots` names every root and
  which way it goes through a `Visitor`, as `typescript::roots` names the
  TypeScript's; the server's names its own through a visitor of its own,
  since it depends on no mindweaver crate, and the CLI lists them beside
  its own, as the API they are part of is the one it serves.
- **What serde writes by hand is said from what it writes**, beside its
  serializer, as its TypeScript is, for the same reason. A word is a
  string, one of its words, written by the same `described::words!` that
  writes its TypeScript, from the function the serializer calls, so the
  two descriptions cannot differ on a word; its schema's id is its Rust
  path, so the two `Reason`s, named apart, are never taken for one. A
  `ValidationIssue` is one object of each code, its code a constant, read
  off the match its serializer writes from (`oneOf`); a project's id is the
  text of its own spelling (a `pattern`); a template's origin and a skill's
  metadata say what they are written as with `schemars(with)`, as their
  TypeScript says it with `ts(as)`; and `CoreCode` and `Code` are the
  strings every code is, `Code` naming `CoreCode`'s definition. A path or
  a URL written through a function of its own is a string, which is what
  schemars says of the type it takes, held so by a test as the
  TypeScript's is, and the finder of such functions
  (`described::field_serializers`) passes over a `with` of schemars',
  which names a type rather than a function.
- **Named as the routes name them.** A root's file is its type's name,
  as a route names what it is sent and answers with
  ([a table of reads](#a-table-of-reads)) and as the TypeScript names its
  file, and `Schemas::get` finds a root's schema by it, as the API's
  description finds the schema of each body and answer by the same name
  among the roots ([the API's description is written from both
  tables](#the-apis-description-is-written-from-both-tables)). A test holds every type a route of the CLI's names to be a root,
  another every root of the panel's types to have a schema of its name,
  and a third no two roots of the three crates to share a file. schemars
  names each type once in a document and gives a second type of a name
  already taken a number after it, without a word, so a test visits every
  root of every crate into one document for each way, as a description of
  the whole API would hold them (`Together`), and fails on a name taken
  twice.
- **Open where they are answers.** An answer's objects allow a field they
  do not name, so a field added, which moves no version, does not make a
  client that checks answers refuse it; what the API is sent is closed
  where serde refuses a field it does not know (`deny_unknown_fields`).
  schemars closes such a type whichever way it describes it, and a type
  sent can be answered too (a task, a holder and a box are in what a plan
  answers), so a root given is described with a transform that opens what
  schemars closed (`schema::given_settings`, which `Schemas` and
  `Together` both use), and a test holds no root given of any crate to
  close an object. A change to what a root holds shows as a diff of its
  file, and one that drops or renames a property, or changes what it
  holds, is one that moves the API to `/api/v2`, as the names it
  describes are.
- **Held to real answers.** A test holds each schema written by hand to
  what serde writes of every variant, and to refusing what serde never
  writes: a word that is none, an issue missing what its code carries, an
  id of another spelling, the list of pairs a skill's metadata is in Rust.
  And every fixture the panel's views render, which the real backend and
  server answer ([its fixtures come from
  Rust](#its-fixtures-come-from-rust)), is held to the schema of what it is
  an answer to, or was sent as: a snapshot to `Snapshot`, a read's query to
  `Query` and its answer to what its route answers, a task, its preview and
  its run to the CLI's and the server's types and what each carries of the
  backend's, a refusal to `Refused`; a fixture the test does not know
  fails it. Against the schemas closed, each object refusing a field it
  does not name, so a schema that names less than serde writes fails as
  surely as one that says other, and as written too, since an object
  opened could match two members of a `oneOf` that closed match one. Validated with boon, a 2020-12 validator
  that passes the suite of tests JSON Schema publishes and builds nothing
  that reaches the network, its one loader, which reads a file, taken away
  in the tests; jsonschema was the other, whose default features build an
  HTTP client for a remote `$ref`, and which brings some thirty crates to
  boon's six.
- **Written again by cargo alone.** `make api/schemas` empties the folder,
  since an export writes and never deletes, and runs the tests named for
  it with the three crates' `schema` feature (`--lib schema`), and with
  `ts` beside it, so the test holding every root of the panel's types to a
  schema runs too. `make api/schemas/check` writes them the same way and
  fails when git sees them differ from the ones committed, a file new to
  git among them; CI runs it in `types drift (web)`, after the fixtures'
  check, rather than in a job of its own, whose name the rulesets would
  have to be changed by hand to require ([in `make test`, and in
  CI](#in-make-test-and-in-ci)).

Replacing ts-rs with a TypeScript written from these schemas was the other
way, one description where there are two, and it would have the panel
compile against what a generator makes of a schema rather than what ts-rs
reads off serde's own attributes, and change the panel to describe the
API. The two are written from the same attributes and the same
hand-written words, and a test holds the TypeScript's roots within the
schemas', so they describe one set of types.

### The API's description is written from both tables

The API is described, routes and all, as an OpenAPI 3.1 document (#218,
[D-24](#d-24-the-local-server-is-a-versioned-http-api)):
`docs/api/openapi.json`, written from Rust, committed and never edited by
hand, and served at `GET /api/v1/openapi.json` behind the token, as every
route is. It holds every route of the backend's table and of the server's
own, with its method, its parameters (each name, whether it is needed and
the schema its shape is read into, a list given once for each value), its
body, what it answers and every status it may be refused with, the codes of
each listed; the token's two ways in, as a Bearer token and as the launch
link's cookie, as security schemes; the events as `text/event-stream`, with
what each holds; and the API's version, in its server's URL, `/api/v1`,
relative to where the document is served, and as `info.version`.

- **Put together where every type is known, from the very tables served.**
  The CLI writes it, under its `schema` feature (`openapi::document`), as
  only the CLI knows every type a route names. The server writes its part,
  what is HTTP and the same whatever the backend (`schema::paths`), walking
  `Backend::routes` and `Own::ALL` as it serves them, and asks the CLI for
  each type a route names through a trait of its own (`schema::Describe`):
  a backend's type by the name its route gives it, found among the roots
  of core, the CLI and the server as a file of `docs/api/schemas` is, the
  way the route uses it; a body of the server's own by `Sent`; and what the
  server carries of the backend's by a word of the server's (`Carried`). So
  the server names no type of the front end's and depends on no mindweaver
  crate still, and the CLI spells none of HTTP's rules: the statuses, the
  headers beside a 401 and a 405, the media types and the security schemes
  are the server's. A document written by hand beside the handlers was the
  alternative, a second description that could disagree with them; and one
  the server wrote whole at each launch, which would compile schemars into
  every build.
- **What the server carries is said of the CLI's types, on the route that
  answers with it.** The server's own schemas say any JSON where it carries
  the backend's ([the API's schemas come from
  Rust](#the-apis-schemas-come-from-rust)): an item of a preview's
  `confirm`, a run's `outcome`, a change's scope and a switch's answer.
  Its definitions stay that, one whatever the backend, and each route's
  answer is an `allOf` of the server's type and what fills each place, from
  the CLI: a `Confirm`, an `Outcome`, or the `{"error"}` the writer thread
  answers in its place when it would not serialize, a `Holder`, and a
  `Switchable` or `null`. Filled in the definitions instead, the server's
  own types would be described for one backend.
- **A type sent and answered, described otherwise each way, is two
  definitions.** schemars describes a type as serde writes it, every field
  required and its objects open, or as serde reads it, a field read as none
  left out of `required` and closed where serde refuses one it does not
  know. A task, and the holders, boxes, marks and toggles it holds, are
  sent, and are in what a plan answers too, so `components.schemas` holds
  each way: answered under the type's name, and sent with `Sent`
  after it (`TaskSent`), every reference to it named to match. A type both
  ways describe alike, a word, is one definition under its own name, found
  by narrowing the names taken alike until none is dropped, since a
  definition naming one named apart differs too. Describing both as serde
  reads them, which what serde writes holds to as well, was the other way,
  and it would close what a plan answers to a field added to a task, which
  moves no version.
- **Every way a route may be refused, with its codes.** Every refusal is a
  `Refused` ([every refusal with a code](#every-refusal-with-a-code)), each
  status an answer of its own, its `code` one of an enum: the server's own
  conditions of that status the route may meet, which its table says
  (`Route::conditions`, `Own::conditions`), and, where the request reaches
  the backend, what the CLI says its backend refuses with on the grounds
  that status is (`Describe::refused`): core's codes and the CLI's own
  (`CoreCode`, `CliCode`), as absent, a 404, only for a read named by its
  parameters, and as unfit, a 422, anything it reaches. A 405 is listed
  under each route, though it is the answer to another method there, since
  OpenAPI has no place for a path's other methods; a path no route names, a
  404 of `unknown_route`, is said in the document's description, as it is
  no route's.
- **The stream's events beside it.** OpenAPI 3.1 has no place for what each
  item of a stream holds, so the events' schema is under `x-itemSchema`,
  OpenAPI 3.2's `itemSchema` as a 3.1 document may carry it, beside a
  `string` for the stream whole. A document of 3.2, which few tools read
  yet, was the other way.
- **Served as it is committed.** The CLI's backend hands the server the
  committed file, embedded with `include_str!` (`Backend::document`), so a
  plain build serves it without compiling schemars, and what it serves is
  what the check holds to what Rust writes. Behind the token, though it
  says nothing of a workspace: one rule for every route is simpler to hold
  than a list of what may be read without it.
- **Held to OpenAPI 3.1, offline.** A test reads the committed document with
  oas3, a reader of OpenAPI 3.1 alone, into types of its own whose required
  fields and closed words it must have, every operation's parameters, body
  and answers resolved; holds every Schema Object in it to JSON Schema
  2020-12's own meta-schema, which boon carries, and compiles each from the
  document, so every reference names a definition it holds; and walks both
  tables, every route served in the document and every route in it served.
  oas3 is handed the text, and nothing is fetched. roas was the other
  reader, which models every keyword of a schema and refuses the `uint64`
  format schemars writes, which 3.1 allows; openapi-spec-validator-rs
  carries 3.1's own meta-schema, but builds jsonschema, which boon was
  chosen over, and turns on serde_json's `preserve_order` for every crate a
  test builds, as oas3's default feature would, which it is taken without:
  every map schemars and the commands write would come out in another
  order.
- **Held to every answer the tests get.** Every answer `ui_test` gets over
  loopback, every event the stream tells it, and every answer the backend's
  suite gets from the backend, as the server would send it, is held to the
  schema the document gives that route for that status, with boon
  (`common::contract`): a route answering outside its document, or with a
  status it does not list, fails whichever test met it. The server's own
  suite holds every refusal it brings about to a condition its route lists.
- **Written again by cargo alone.** `make api/openapi` runs the CLI's export
  test (`--lib openapi`) with the features `make api/schemas` builds with,
  so the two share one build, and writes the one file over; `make
  api/openapi/check` writes it the same way and fails when git sees it
  differ from the one committed. CI runs it in `types drift (web)`, after
  the schemas' check ([in `make test`, and in
  CI](#in-make-test-and-in-ci)).

### Its views

Ten pages draw the snapshot ([the read model](#the-read-model)), an
eleventh a scan beside it and a twelfth what an update would do, each a
file under `src/pages`:

- **The overview** (`/`) names the workspace, where it is and why that one,
  counts what it holds from the snapshot's `summary`, a card for every kind
  in `kinds`, lists every failure the snapshot collected under what it is
  about, and names the commands that would fill what is empty.
- **The projects** (`/projects`) are a table of every entry in the
  registry's order, with a column for every kind, and one badging how many
  of what each was given are `outdated`, from its `tally`, a link to the
  page of updates; sorted and filtered.
- **A project's page** (`/project?entry=...`) groups what one holds by
  kind, each artifact with its state, a badge in a slot for each of the
  project's agents, its origin, its targets, what does not read and what
  `validate` says, and what its agents never read above; below, its agents
  and share toggles (`SettingsForm.vue`), or, for one an older marker
  marks, its migration (`MigrateOffer.vue`).
- **What could be linked** (`/discover?root=...&depth=...`) scans the
  folders typed, one a line, or added from the explorer ([choosing a
  folder](#choosing-a-folder)), and lists what `mindweaver scan --suggest` finds
  there in its order: what a relink would follow, then what is not linked,
  each with its reason, the command's words for why and a box to mark it,
  then what is linked already, and the entries no scan can follow, with the
  `mindweaver relink` that would. The folders and the depth are the address's,
  so a reload scans the same again; with none, the page starts from the
  workspace's own folder, and scans nothing until asked.
- **Assign** (`/assign?kind=...&origin=...&agent=...&q=...&by=project`) is
  the matrix (#79): every offer in `offers` against every project, a row an
  offer under a heading for its origin and a column a project, or turned
  round by project, a row a project and the origins in a band over the
  offers' columns. Each box is ticked where `boxes` says, with the icon of
  its name's state over its corner, each state in `box_states` said in the
  legend below, and one of a kind its project can take none of is closed,
  saying why. Kind, origin and text filter the offers and the agent the
  projects, whichever way round, in the address. The boxes marked are
  planned, and applied ([its changes](#its-changes)).
- **Updates** (`/updates`) draws core's survey of what updating every
  project would do (`Query::Updates`): a section for each state it
  counts, in its order, listing each installation in that state, project
  by project, with its origin, what an update does with it, the copies
  that names, and why it leaves one it leaves, by its code; above them,
  each project with an update of its own where an update writes
  something, and each entry that opens no project. Which state a row is
  under is a filter of what core listed; what each step word is called,
  and which change it offers, is drawn, as a state's look is, a step core
  adds shown by its word with none. A state's section is reached by its
  hash (`#state-outdated`, the projects table's badge), which the page
  scrolls to itself once the section is drawn, since the router scrolls to
  none and the section waits on two queries: once for each hash, so a live
  update drawing it again leaves where somebody scrolled since.
- **The library** (`/library?kind=...`) has a page for each kind in `kinds`,
  the first when none is named, listing the workspace's own of it, filtered
  by name or summary, each with the projects holding a copy and how each
  copy stands, and what `validate` says.
- **One of the library's own** (`/artifact?kind=...&name=...`) shows its
  front matter as a table, its body rendered, its files, each opening as
  text (`&file=...`), what `validate` says of it, and the projects holding a
  copy, each with its state, agents and targets, and below, its removal
  everywhere, dry run first ([its changes](#its-changes)). Named in the
  query, as a project is, since a rule's name is a route.
- **The MCP page** (`/mcp?edit=...`), beside the library in the sidebar,
  draws each of the library's servers as the snapshot carries it
  (`library.servers`): for each agent whether it is given it, in words, an
  icon and a colour (`given whole`, `given in part` for what its entry is
  written without, `refused`), each refusal and each thing dropped by its
  code and in core's words, what it would run or reach as its entry would
  spell it, and what it asks before it starts one, the word core says
  (`Starts`) put in the words the install's line says it in, as a state's
  look is the panel's; and for each project holding it (`library.holders`)
  how its copy stands and what doctor finds there (`doctor`, by the
  project's entry), Codex's trust among it. A filter of the snapshot's
  lists, as the library's holders are. Below, its form adds a server, or
  changes how the one the address names is started
  (`ServerForm.vue`, [its changes](#its-changes)), started from what core
  read it back as (`given`); one whose definition no form can say offers
  its file's editor instead.
- **Loads** (`/loads`), beside the library and the shelf in the sidebar,
  the third place an offer comes from, lists every workspace it loads, with
  its entry, where it is, its `origin`, why it does not open, and what it
  offers, each with a link to the matrix showing that origin alone; and
  below, every workspace this machine knows (`known`), each saying in words
  whether it is the default, this one, loaded and by which entry
  (`loaded_as`), or one that does not open, with the projects it links as
  core counts them. It had been a table at the foot of every page of the
  library, which is a page a kind, where a list of workspaces is of none.
- **The shelf** (`/shelf`) lists what was gathered, filtered by kind, by
  name, summary or origin, each with its `origin`, the commit it was taken
  at and when.
- **The sources** (`/sources`) lists where the shelf is filled from, each
  with its `origin`, branch, folder, commit and what it supplied, and the
  latest actions the ledger logged (`history`), newest first.

A project holds a copy of one of the library's own when core names it
among `library.holders`, and the copy is its artifact of that kind and
name: so a project's own of the same name, or one installed from the
shelf, is never taken for it, and an install recorded before origins were
kept, whose copy names no origin, is not left out, which comparing a
copy's `origin` with `library.origin` in the panel would do. What a source
supplied is the shelf's rows that name its folder, which no two sources
share, and what a load offers the offers of its `origin`, which a second
spelling of one workspace shares with the first, as it shares its count.
Each is a filter of what the snapshot lists, as a health filter is; which
known workspace can be loaded is core's (`loadable`), not the panel's
reading of `in_use`, `stale` and the loads' paths, which would take two
spellings of one folder for two. An origin is shown as core says it,
never put together from an address and a branch, which would be the rule
for `--from` in TypeScript ([the read model](#the-read-model)).

An artifact's page reads more than the snapshot: its files
(`Query::Files`), whose answer names the file it is read from, then that
file (`Query::Contents`), which core hands back cut at its front matter
(`contents::Manifest`): each key at its top, in the order written, every
value a string, a list or a mapping as YAML writes it, and the body after.
Cut in core, by the readers that decide where front matter ends and
whether a rule's fences hold any, rather than by the panel from the text,
which could show as declared what every agent reads as content. The
reference it sends is `kind:name`, as a command line takes it, so one name
of two kinds never means either. Each is a Vue Query of its own
(`src/api/artifact.ts`), asked once the one before it is answered.

What a skill says is somebody else's, so its markdown is drawn from
marked's tokens, an element at a time (`src/markdown.ts`), never handed to
the browser as HTML to parse: raw HTML is shown as the markup it is, in a
code span or block, no element gets a style or an event, a table's
alignment is a class rather than the inline style marked's own renderer
writes, an image is named rather than loaded, and a link keeps its address
only to another site or a mail (`https:`, `http:`, `mailto:`), opened in a
tab of its own with no referrer. Any other, to a file beside the skill above
all, is its text alone, since the panel never links to an artifact's file,
and a `javascript:` or `data:` URL never reaches an attribute. An entity
or character reference (`&amp;`, `&#169;`) is decoded from the string,
in text and in a link's address before it is judged, and left as written
in code, as CommonMark reads them: marked leaves them for the browser to
decode in the HTML its renderer writes, so text drawn as text would show
them as written. The decoder is `entities`' strict one, which takes only
a reference ended by `;`. marked's renderer with v-html, behind a
sanitizer, was the other way, which trusts the sanitizer to know every way
HTML runs something. Its headings start a level below the section they
are drawn in and never go down more than one level at a time, so the
page's outline holds whatever the document skips.
A file opened from the listing is shown as text in a `<pre>`, whatever it
holds, and a listed file is a button asking for it, never a link.

Every page reads one Vue Query of the snapshot (`src/api/snapshot.ts`), so
moving between them draws at once from what is cached. The server's events
say when to ask again ([live updates](#live-updates)), and `src/api/live.ts`
turns each into what Vue Query is to ask for again, never a read of its own
and never a scan: `changed` marks the snapshot stale, and what was read of
an artifact in a holder among its scopes; `resync` and `command_finished`
mark all of those, since a command can change what nothing watches. Those
queries, and the matrix's plan and the preview of its lines, which are of
the disk as it stands (`src/api/plan.ts`, keyed `plan` and `preview`), and
what an update would do, which is too (`src/api/updates.ts`, keyed
`updates`), are named by the first word of their key (`markStale`), rather
than every query marked, for one asked for by hand: a scan (`src/api/scan.ts`,
keyed `scan`) reads the disk far past what is watched, and an event asking
for it again would run one on every change, every command and every tab
shown again. A scan is never stale on its own either, so neither a page
opened again nor the window coming back into focus walks the disk: the page
asks again when it is told to scan, and after a run of its own, which
changed what the scan found linked. Told to scan folders a kept answer
holds, from an earlier visit or an earlier set of folders, it drops that
answer before the address changes to them (`forget`), or Vue Query would
draw it and ask nothing; a run marks every kept answer stale (`stale`),
the one on the page asked for again at once. A `gcTime` of 0 was the other
way, and would walk the disk again for a page only opened again. While the
server says it is `watching`, an answer stays fresh until one of those
marks it (`staleTime` of `Infinity`), so a page asks for nothing on
opening. Otherwise, before the server has said and while it is
`unwatched`, an answer is stale as soon as it comes, as before there was a
stream: each page asks on opening, and so does the window when it comes
back into focus, and `SnapshotGate` shows the banner that says why above
the page, with a button that marks stale what `resync` would. How the server
watches is what the stream said, kept beside the subscription and provided
to the views, rather than in Pinia, which holds only what the browser
decides. `SnapshotGate` draws a page once the snapshot is answered, says it
is waiting until then, and says why when the server refused.

- **The panel lays out, groups and filters; it works nothing out.** Every
  count it shows is one the snapshot carries, the `summary` and each
  project's `tally` and `health`, rather than the length of a list it
  draws, and every order is the snapshot's: the registry's for the rows,
  `kinds` for the columns and groups, `healths` for a sort by health.
  Grouping a project's artifacts by kind, or keeping the rows a filter
  keeps, only chooses what to draw of what the snapshot lists. A sort by
  name or path compares the words as the browser collates them, and one by
  a kind's count compares numbers.
- **The matrix is drawn a screen at a time.** Its rows and its columns
  are both virtualized (`AssignGrid.vue`, with `@tanstack/vue-virtual`,
  which Nuxt UI's table virtualizes with already, so no package came with
  it), each laid out from the one size every row and every column has, so
  nothing is measured: a matrix of hundreds of offers by hundreds of
  projects holds a screen's worth of boxes, and a scroller nothing has laid
  out, as in a test, counts as the window. Each icon a box shows is drawn
  once from the bundle and shown in every box by an SVG `<use>`, and one
  listener on the grid serves every box's press, focus and key, since a
  component or a listener for each of a screen's boxes was most of what
  drawing one cost. `tests/matrix.test.ts` holds a matrix of 50 projects by
  200 offers to at most 100 rows in the document, and its first render to
  300 ms, run by vitest after every other suite (`test.projects`, by
  `groupOrder`), since beside them it timed whatever else the runner was
  busy with. It is a grid from the keyboard: `role="grid"` with each row
  and cell saying its place among all of them, the arrows moving a box at a
  time over the groups' headings, Home, End and the page keys further, and
  one box in the tab order, the last moved to.
- **A kind core adds reaches every view untouched**
  ([D-21](#d-21-the-panels-views-follow-the-capabilities-they-show)): a
  card, a column and a group each come from `kinds`, and a test adds one
  the build does not have to a fixture and finds it on every page. The
  colour and the icon of a state or a health are the panel's, kept for
  each word it knows and plain for one it does not: the word itself, which
  every badge says, is core's.
- **The address is the state.** The table's text filter, its health, the
  state the overview's counts link to, and its sort are the query of
  `/projects`, so a link or a reload opens it as it was left. A project is
  named by its entry in the query, never the path: an entry is a route from
  the workspace, which can be `.` or `..`, and a browser resolves such a
  segment in a path before the router sees it, escaped or not, which would
  turn `/projects/..` into `/`. The server answers any path outside `/api`
  with the page, so a reload of any of them is the panel's
  ([The panel's files](#the-panels-files)).
- **Usable from the keyboard, and never by colour alone.** Every control is
  a link, a button (with `aria-pressed` for a filter and `aria-sort` on a
  sorted column) or a field; every state and health is said as its word
  beside its colour and icon, an agent that does not read an artifact keeps
  its slot, crossed out, and says so in words, and a failure says its code.
  The sidebar is a landmark of its own and each page the main one, so
  nothing is outside a landmark. The command palette reaches every page,
  and installs, updates all and links, from the keyboard alone, and `?`
  lists its keys and every other's ([the command
  palette](#the-command-palette)). `tests/a11y.test.ts` runs axe on every
  page, of both fixtures, with every WCAG A and AA rule and the best
  practices but contrast, which happy-dom lays nothing out to measure.
- **The commands are the empty states.** Whatever is empty names the
  command that would fill it, as somebody would type it, since every change
  is a command ([D-18](#d-18-the-panel-writes-by-running-commands)), for a
  shell rather than for the panel, which runs only the tasks its pages offer
  ([its changes](#its-changes)). Each is a `mindweaver` line naming the
  workspace with `-W` and its path, and a project with `-p` and its entry,
  rather than a `mind` line typed from the project's folder: which workspace
  `mind` consults there depends on the shell's own `-W`, activation and
  home and on what is above the project ([the read
  model](#the-read-model)), so for a project linked from beside the
  workspace, or a panel launched with `-W` or on an activated workspace, it
  would act on another one or none. A workspace not made yet names `mindweaver
  init` first, since `-W` never makes the one it names.

### Its changes

Every change the panel makes is a task the server previews, the panel
shows, and runs only once somebody says so
([D-18](#d-18-the-panel-writes-by-running-commands)). One runner does it
for every page (`src/api/commands.ts`), made once in `CommandRunner.vue`,
which wraps the pages and draws what the runner shows beside them; a page
hands it a task (`ask`) and is answered once that task's run is over, or
was let go.

- **The lines are the server's.** The preview (`POST
  /api/v1/tasks/preview`) answers the lines the backend planned, and the
  dialog (`CommandDialog.vue`) shows exactly those, each as `$ line`: the
  panel puts no line together itself, so what it shows is what will run,
  quoted for a shell by the CLI. A refused preview runs nothing and says
  why, with its code, in a toast.
- **Which lines take something away is the CLI's to say.** The backend reads
  each planned line with the parsers that run it, as it reads a refusal,
  through one list every front end shares (`task::Caution`, a match over
  every command with no wildcard): `remove --yes`, `uninstall`, `mindweaver
  remove --everywhere --yes`, `unlink`, `unload`, and `--force` on an
  install, an uninstall, or an update told `--yes`. A dry run takes nothing
  away, nor does a line that adds, or carries what is installed to where
  the agents now read. The preview names each such line, by its place
  among the lines, with the caution's word and the CLI's words for it
  (`backend::Confirm`), and the dialog is then a confirmation: titled for
  what it takes away, each line's cautions under it, its button the error
  colour, and the focus on Cancel, so an Enter pressed by habit runs
  nothing. A list in TypeScript was the alternative, and it would be a
  second place for a command to be forgotten, as the TUI's own questions
  are ([open questions](#open-questions)). Its icons are the panel's, for
  each word it knows, and plain for one it does not.
- **A run is over once its views are drawn from what it changed.** The run
  goes through the live updates' `settle` (`src/api/live.ts`), which waits
  for the stream to tell that run of those lines finished, a finish the
  server tells just before it answers, so the stream may tell it first, and
  for what that marked stale to be read again. When no word comes within
  two seconds (`PATIENCE`), the stream let go while the tab was hidden, or
  lost, it marks the same stale itself and waits for that. Nothing the run
  answered is written into a view: an update made beforehand would be a
  second guess at what core says, and could disagree with it.
- **How it went is a toast, and what it said a drawer.** A toast says what
  ran, or which line failed, and what the lines printed. When a run failed,
  or said a line with a code, the warnings drawer (`WarningsDrawer.vue`)
  opens, every warning, note and error under the line that said it, each
  with its code beside its words and an icon chosen by that code: a plain
  warning's or error's for a code it knows no icon for, and a note's for a
  line with none. The toast offers it for a run that said only notes. A
  refused run, or one whose answer never came, is a toast too, the second
  naming the transcript, which says what did run. Either leaves the drawer
  on the last run that answered, so an earlier toast still opens what that
  run said: the drawer reads the last answered run in Vue Query's mutation
  cache, kept while the tab is open, since the run mutation's own answer is
  cleared as the next run starts and stays cleared when it fails.
- **The transcript is the server's.** Every page's header opens it
  (`TranscriptDrawer.vue`): every task the session ran, from any tab or
  script holding the token, each line with what it printed, read from `GET
  /api/v1/transcript` only while the drawer is open and again whenever a run
  finishes, since a finish and a resync mark it stale too. **Copy as
  script** copies the lines each task ran, exactly as the server previewed
  and ran them, one to a line, and nothing of the panel's own: pasted into a
  shell in the workspace's folder, where the lines run from, they do it
  again. Across a switch, each line reaches the workspace it names with
  `-W` wherever it is pasted, but reads an entry or a scratch file from
  the folder it is typed in, so the lines of each workspace are pasted in
  its own folder. A `cd` or a shebang in front was the alternative, and it
  would be a line the panel wrote; so was a `-C` beside every `-W`, which
  would make each line longer for the one case of a transcript spanning a
  switch. It holds reads too, a `validate`, where what
  `mindweaver ui` prints when it stops holds only what changed something.

What the pages offer:

- **A link, from the projects page**, of the folder typed above its table,
  or chosen in the explorer, sent as typed for the path policy to hold.
- **An unlink, from a project's page**, which is a confirmation, in the
  CLI's words that nothing on disk is deleted, then goes back to the
  projects.
- **The folders marked on the page of what could be linked**, sent as
  their marks (`Task::Marks`: each folder's path and what the scan said it
  offers), never as the batch: the task module builds the minds screen's
  batch from them (`Task::minds`), a `mind -C <path> init` for each
  repository that is not a project yet and one `mindweaver link` of them all,
  so the same marks are the same batch on either front end, which a test
  of the backend holds the screen and the panel to. The words differ for a
  folder inside the workspace, which the panel names by its absolute path,
  all the path policy takes from a browser, and the minds screen by the
  scan's route from the root, where its lines run: the same folders, which
  the test runs both on and finds linked alike, and the same words for one
  outside it, where both name it by its path. Turning the panel's path into
  the route was the other way, and would have shown a preview naming a
  folder by what reads right from one directory only. That `mind init`
  writes into each such repository first is said on the page before
  anything is asked, and its line is shown before it runs. The scan is
  asked for again once the run is over.
- **A relink of what the scan found moved**, by the entry it moved from, as
  the marker spells it, which the backend names beside it (`moved_from`),
  matching the scan's `moved` to the entry by the directory each names
  through the function the minds screen matches by (`scan::moved_from`):
  the scan spells `moved` as `mindweaver link` would take it, which a path
  outside the workspace spells differently from the entry.
- **A project's agents and share mode**, from its toggles, sent as they
  stand beside what its settings said (`Task::Toggled`), never as the
  lines' agents: `Task::settings` turns the two into the `agents` and
  `share` lines, as for the TUI's toggles, so the order a project names its
  agents in, and that unticking every one asks `agents remove` of them all,
  is said once. Offered only where `migrates` is false: a project an older
  marker marks changes no setting until it is moved.
- **The boxes marked on the matrix**, each as the plan takes it (`Cell`),
  kept in Pinia while they are not applied (`src/stores/marks.ts`), since
  only the browser knows them, and asked of the plan whenever they change
  (`usePlan`): the page draws the lines its tasks are as the server
  previews them (`usePreview`, the preview the runner asks for again once
  Apply is pressed), every lock and clash on its box and listed with its
  code, and nothing it works out itself ([a batch is planned in
  core](#a-batch-is-planned-in-core)). **Apply** sends the tasks back as one
  `apply_install`, as they came, and is offered only while the plan is of
  the boxes marked now and runs something; once it ran, the marks are done
  with, and the boxes are drawn again from what it changed.
- **An update, from the page of updates**: of every project, `mindweaver
  update --all --yes`; of one, where an update writes something, `mind -C
  <project> update --yes`; and of one installation, the same line naming
  it, each the `update` task the TUI's `u` sends. What nothing offers any
  more offers its uninstall, `mindweaver uninstall -p <project> <kind:name>`,
  and the page all of them at once (`Task::UninstallOrphaned`, `mindweaver
  uninstall --all-projects --orphaned`, never forced), both asked about as
  an uninstall is. Which change a row offers is read off the step core
  gave it, a word at a time, and whether it is forced off core's `forced`
  (`update::Pending`): an edited copy, which an update not forced leaves
  and, once nothing offers it, an uninstall not forced leaves too, offers
  either with `force`, the line's `--force`, which the preview makes a
  confirmation by `Caution::Forces`, never a question of the panel's. The
  command still refuses whatever the disk says by the time it runs.
- **A removal, from the page of one of the library's own**, everywhere and
  never the library's copy alone, which would leave each copy installed
  from it orphaned: the dry run first, `mindweaver remove --everywhere
  <kind:name>` (`Task::Remove` without `yes`), whose report the page shows
  once it ran, then the same line with `--yes`, offered only then and
  asked about for what it removes and uninstalls, after which the page
  goes back to the library. As a migration is, below.
- **A save, an addition and a rename of one of the library's own, and an
  adoption from a project's page**, each a task the server previews like
  any other ([authoring in the library](#authoring-in-the-library)).
- **An MCP server added, or how one is started changed, from the MCP
  page's form**: `Task::Add` with its `server`, `mindweaver add [options]
  mcp <name> <description>`, or `Task::EditServer`, `mindweaver edit
  [options] mcp:<name>`, the form's transport and each of its fields an
  option's value as an `mcp::Given` holds it, a row of the form a value,
  nothing judged or left out in TypeScript. Judged as it is typed by
  core, a quarter of a second after the last key (`Draft::Add`'s `server`,
  `Draft::EditServer`), so what the line would refuse is said in its words
  and by its code before it is sent, and again by the line when it runs. A
  secret is the name of the variable an agent reads it from, each field in
  a secret's place labelled so, with a note that no value is stored: a
  `Given` holds a secret as that name alone, so no field in a secret's
  place sends a value, which a test of the request holds it to. A setting
  of the environment and a header are values `add mcp` writes as typed
  (`--env`, `--header`), which the form says no secret goes in: core
  refuses one that looks like a token, and nothing else tells a secret
  typed there from a setting, as the options' own note says.
- **An MCP server that asks first, confirmed on the matrix**, as the
  install screen confirms it ([installing an MCP
  server](#installing-an-mcp-server)). A plan answers, beside its tasks,
  each install that waits for a yes (`Planned::asks`, `Asking`): which of
  the tasks it is, what it would give each agent of each project, in the
  lines the install screen's confirmation shows (`install::foreseen` of
  core's `install::mcp::preview`, under a heading a project), and the task
  carrying the yes (`Task::confirmed`). The page draws the lines under
  what runs, with a box to say yes once they are read; until it is ticked
  the plan's own task goes, whose line installs nothing and says what it
  would, and once ticked the confirmation's task goes in its place
  (`batchOf`, given what was said yes to), its line `--yes`, previewed and
  asked like any other. A yes is to that task as its lines were shown
  (`sameAsking`): a plan is asked for again whenever the disk changes, a
  loaded workspace's definition among it, so lines that changed since ask
  again, and a yes the plan no longer asks for, a box unmarked, is let
  go. `Task::Install`'s `yes` is JSON for that alone: a
  plan's task carries `false`, the palette's install too, and the panel
  sets it on no task of its own, sending only what a plan handed it. A yes
  sent with any other install is refused by the path policy before a line
  is built (`policy::confirmed`, `install_yes_unasked`): the panel's
  backend holds a request to the server's own rule of which install asks
  first (`install::asks_first`), as every value a browser sends is held,
  and one left out is none, as before a yes could be sent. Handing the yes
  over beside the lines rather than letting the panel set it was the
  choice: which install waits for one is core's, and the confirmation is
  the one place a yes may come from.
- **A load and an unload, from the page of loads**: a workspace this
  machine knows by the path the snapshot lists it by, offered only where
  core says the workspace can load it (`loadable`), or any other by the
  path typed, sent without the white space around it, which the TUI's form
  takes off too, for the path policy to hold; and an unload of
  one it loads by the entry the marker spells it by (`policy::loaded`),
  one that does not open among them, asked about by `Caution::Unloads`.
  Each is the task the TUI's load form sends for the same workspace, typed
  or picked (`Task::Load`, `Task::Unload`), which a test of the backend
  holds the panel to, line for line and marker for marker. Neither
  remembers anything: what this machine knows is `mindweaver init`'s to add,
  so a workspace loaded by its path joins the list only once `mindweaver init`
  runs on it, and a load from the panel never writes in the default
  workspace's marker ([the workspaces this machine
  knows](#the-workspaces-this-machine-knows)). `mind load`, into the
  workspace above a project (`Task::LoadHere`), is no page's: the panel is
  launched on a workspace, and loads into that one
  ([open questions](#open-questions)).
- **An install, an update of every project and a link, from the command
  palette**, each the task of the page offering the same change: one box's
  plan applied as the matrix applies its marks, **Update all**'s, and the
  link form's ([the command palette](#the-command-palette)).
- **A migration, for a project an older marker marks**, the dry run first,
  `mindweaver migrate -p <project>`, whose report the page shows once it ran,
  then the migration, the same line with `--yes`, offered only then. The
  workspace's command rather than `mind migrate` in the project, so what it
  carries is what the workspace on screen records; the agents and share
  mode are the command's own, every agent and shared for a `mind.toml`,
  for the toggles to change once it has moved.

The preview and the run are Vue Query mutations, and the transcript, the
scan, the plan and the matrix's preview of its lines queries, so what the
server answered lives where every answer does, the palette's plan of one
box among them, under the plan's key; which drawer is open is Pinia's,
beside the sidebar's state, and so are the boxes marked on the matrix and
whether the palette or the sheet of shortcuts is open.

### Authoring in the library

The library's own are written from the panel as from a terminal: by `mindweaver
edit --contents`, `mindweaver add`, `mindweaver rename` and `mindweaver adopt`, each a
task the server previews and runs, and judged before they are sent by what
core says of them, never by a rule of the panel's (#82).

- **Judged by core, as it is typed.** A draft, the text in the editor or
  what the form adding one holds, is sent as a query (`Query::Validate`,
  with a `Draft`), once somebody stops typing for a quarter of a second,
  and answered with core's `draft::Judged`: why the command would write
  nothing, by the code and in the words it would refuse with, and what
  `validate` would say of the artifact once written, each issue in its own
  words. Core calls the command's checks rather than copies of them: the
  template found as `add` finds it and every check `add` makes before it
  writes, worked out apart from the write (`create::plan`, `Scaffold`), and
  every rule `edit --contents` holds contents to before it reads the file
  (`lifecycle::replaceable`), the hash aside, which only the write can tell.
  What `validate` would say is the artifact read from the text as discovery
  reads one from the disk (`Artifact::skill_holding`, `rule_holding`) and
  judged by `Artifact::validate`, so the panel's list is word for word what
  `mindweaver validate` then prints, which a test of the CLI writes beside the
  panel's fixtures and the panel's test holds it to. A front end checking a
  name or front matter itself was the other way, and would be a second
  place for the Agent Skills rules to be spelled.
- **The text, never a path.** A save is `Task::Save`: the holder, the
  artifact by its kind and name, the text, and the hash core gave of the
  file when it was read (`Contents::hash`, `hash::bytes`), never one worked
  out in TypeScript, where a line ending could tell the text from the file.
  Its line is `mindweaver edit [-p <project>] --contents <file> --if-unchanged
  <hash> <kind:name>`, the file the workspace's scratch file for the text:
  `.mindweaver/tmp/edit-<the first sixteen hex digits of its SHA-256>.md`,
  a route from the root the lines run from (`task::scratch_route`, the
  folder's route core's, `WeaverWorkspace::scratch_route`, the one
  `WeaverWorkspace::scratch_dir` is made of, where the backend writes). The
  workspace's command at either level, `-p` naming a project, so the route
  reads from where the lines run. Named by what it holds rather than drawn
  for the run, so the line the preview shows is the line that runs, a
  preview and a run being two requests. The backend writes the text there
  before the line runs and takes the file away once the lines have run,
  whatever came of them (`backend::Scratch`): made with `create_new`, in a
  folder made with `create_dir` inside a `.mindweaver` that is there, so no
  `.mindweaver` is made, and looked at once it is there, so nothing is
  written through a link in its place, which a cloned workspace can bring
  to an agent's own folder: `create_dir` says a link is there as it says a
  folder is, and `create_new` follows one anywhere but at the file itself.
  A file already there under the name is taken when it holds the text
  exactly, as an interrupted run would have left it, and anything else
  there, or a link, refuses the save (`scratch_unwritable`). In the
  workspace's `.mindweaver` (`WeaverWorkspace::scratch_dir`), where no agent
  looks: not the system's temporary folder, where another user could read
  what is about to be written, nor a project's `.mind/tmp`, which a cloned
  repository can bring with it, or a link in its place.
- **Only over what was read.** The editor keeps the hash of the text it
  started from, and takes the file in again when it changes on disk only
  while nothing typed would be lost, or once the file holds what the editor
  does, as after a save. Otherwise it says the file changed and offers to
  read it again, and a save is refused by the line, `edit_stale`, in core's
  words, with nothing written over the change. A file is taken in as it
  is, and stays what was read until something is typed: one whose every
  line ends in CRLF is read and given back with CRLF, each time it is taken
  in, and one mixing endings is read a line at any of them and given back
  with LF once typed in, which CodeMirror's one separator cannot keep.
- **CodeMirror, held to the CSP.** The editor is CodeMirror 6, drawn in a
  shadow root of its own (`MarkdownEditor.vue`). Its styles come from
  style-mod, which writes a `<style>` element into a document, refused by
  `default-src 'self'`, and adopts a constructed sheet into a shadow root
  (`adoptedStyleSheets`), which the CSP has no say over: it is built through
  the CSSOM, as an element's `style` is, and so is every style CodeMirror
  sets of its own, never a `style` attribute. A nonce was the other way, and
  would have had the server write one into every page it serves. The
  panel's colours and font reach it through the custom properties a shadow
  root inherits; Tab is left to move the focus on, and the text is named for
  whoever cannot see it. happy-dom adopts the sheet as a browser does, and
  the panel's test holds it to being there and to no `<style>` element in
  the shadow root or the page; the end-to-end suite loads the editor under
  the server's header in both engines.
- **Adding, from a template or none**, from a page of its own (`/new`), the
  kind's templates as the snapshot lists them; **renaming**, from the page
  of one of the library's own; and **adopting**, from the row of a project's
  page that reads `foreign`, sent as `Task::Adopt` and refused by the line,
  in core's words, for whatever core does not adopt, a rule among them.
  Each goes to the page of the name it then has.

### Switching workspace

The header of every page names the workspace in use and the rule that
chose it, in the TUI's words for `Reason` (`named with -W`, `activated`,
`found above`, `default workspace`), from the snapshot's `workspace`
(`WorkspaceSwitcher.vue`). It opens a menu of what the panel may switch to,
the backend's `Query::Workspaces`, core's `known::switchable`, asked for
each time it opens (`src/api/workspaces.ts`) rather than with the snapshot:
which one the panel was launched on is the launch's, and a snapshot is of a
workspace, the same whoever reads it. Each row is an item checked for the
one in use, saying in words where it is, whether it is the default, the one
launched on and by which rule, how many projects it links, or why it does
not open, the one in use included, gone since it was switched to, so the
menu says why every page is refused rather than list it as holding
nothing; one core offers no switch to (`switchable`), the one in use, one
that does not open and one whose path is no word of a command line, cannot
be chosen. So a state is said by the check, the words and whether it can be
chosen, never by colour.

A choice is sent by its path as the list spells it (`Transport.switch`,
`POST /api/v1/switch`), and a refusal, `switch_unlisted` for a list read
before it changed or `not_a_workspace` for one gone since, is a toast with
its code and changes nothing. The server tells every tab `switched` once it
is taken, and each lets go of the workspace before then: it resets every
answer rather than marking it stale (`src/api/live.ts`), so no view draws
one workspace's projects under the other's name while the new ones come,
lets a task it shows go, whose lines name the workspace before with `-W`
(the runner, `src/api/commands.ts`), and forgets the boxes marked on the
matrix (`src/stores/marks.ts`), which may name projects the next one has
not. The tab that asked waits for that word as a run waits for its finish
(`settleSwitch`, beside `settle`), letting go itself after `PATIENCE`
without it, so it lets go once rather than once for the answer and again
for the word, then draws the overview of the new workspace; a page on
another tab stays where it is, and draws what the new workspace has
there. A tab that missed the word, its stream let go while it was hidden
or fallen behind, learns of the switch from its next `resync`, which
carries how many switches the server has taken (`switches`, as every
`switched` does): a count other than the last it heard is a switch, let
go of as on hearing the word, and a `switched` the stream still held
behind a resync that counted it is not let go of twice. Without the
count, a task shown in a tab hidden through a switch was still shown
once the tab came back, under lines naming the workspace before, and Run
ran it in the new one. The menu is drawn where the switcher is rather
than at the end of the body, so it is inside the page's landmark, which
axe asks of everything a page holds outside a dialog. The switcher
cannot be opened while a task is shown or runs.

After a switch, the rule is `-W` for every workspace but the launch's,
which keeps the rule that chose it at launch: every line the panel runs
names the workspace so from then on, and nothing else chose it. A line
the panel previews, the transcript it keeps and the lines `mindweaver ui`
prints once it stops each name the workspace they ran against, so the
transcript stays true across a switch and needs no entry for one.

### The command palette

Ctrl+K, or Cmd+K where the browser says it is a Mac, opens Nuxt UI's
`UCommandPalette` in a modal over any page (`CommandPalette.vue`), and so
does **Search** at the top of the sidebar. It is another way to the pages
and their changes, and holds no rule of its own:

- **What it lists is the snapshot's**, read while it is open and only
  then (`useSnapshot`, told so), asked again only as a page would ask it,
  when the server is not watching: the pages, the sidebar's own list
  (`VIEWS`, spelled once in `AppSidebar.vue`, where Nuxt UI bundles the
  icons it names, which it does for no `.ts` file), each project with its
  health, the library's own and the shelf with their kind, and the
  sources. A choice goes where the link on a page goes, once the palette
  has closed and Reka has given the focus back, so whatever a choice opens
  next takes the focus from there and gives it back there. Opened again
  while that close is still drawn, the modal is kept and Reka never says
  the close ended, so the choice is let go of rather than done at the next
  close, and every opening draws the palette anew (its key), nothing typed
  and its field focused.
- **Its search is Fuse's**, the one Nuxt UI's palette runs, over each
  entry's label, the word beside it and its description: a token search,
  every word typed to be found somewhere (`tokenMatch: 'all'`), each
  within a threshold of 0.2, which forgives a letter out of place
  (`revew`) and not a word only half like another (0.3 found `came` for
  `acme`). Searched, it lists the twelve nearest entries in all, Nuxt
  UI's default (`resultLimit`), each under its group, the groups in the
  order of their best match; with nothing typed, each group lists its
  first twelve, an offer's projects to install into among them. So a long
  list is found by typing rather than scrolled through, and nothing says
  more are there ([open questions](#open-questions)). It filters and
  orders what the snapshot lists, as a page's filter does, and no count or
  verdict depends on it. Nuxt UI would draw what matched as HTML, each
  match marked; every entry gives that HTML as empty, so its words,
  somebody's summary among them, are drawn as text, as a skill's markdown
  is ([nothing the CSP refuses](#nothing-the-csp-refuses)).
- **Its changes are the pages' tasks**, through the one runner, so each is
  previewed by the server, shown, and confirmed where the CLI says it takes
  something away, as any page's is. **Update all** is the page of updates'
  task (`UPDATE_ALL`, spelled once in `src/api/updates.ts`). **Link** is
  the link form's, the path as typed, offered last for whatever is typed,
  since telling a path from a word is the path policy's. **Install** lists
  each offer, then the projects whose box of it the snapshot draws
  neither ticked nor closed, looked up as the matrix looks them up
  (`src/boxes.ts`), and plans the one box chosen (`Query::Plan`, through
  `usePlanOnce`, under the matrix's key), whose tasks go to the runner as
  one `apply_install` (`batchOf`, the matrix's own): the offer to install
  from, what it replaces and what holds it back are core's, a box held back
  is said with its code, and one whose plan runs nothing, which a change
  made since the snapshot was read leaves, says that. A task of the
  palette's own, an `install` spelled here, was the alternative, and would
  have been the install screen's batch rules written again.
- **Its keys are passed over while somebody types.** Nuxt UI's
  `defineShortcuts`, told nothing otherwise, passes over a shortcut while
  an input, a textarea or anything editable has the focus, CodeMirror's
  content among it, which vueuse finds through the editor's shadow root;
  and the panel gives it none while a task is shown or runs, whose
  question is answered first. Ctrl+K alone is told otherwise
  (`usingInput`), and only while the palette is open: its modal holds the
  focus, so the field Ctrl+K is pressed in is the palette's own search,
  where Nuxt UI would pass it to the browser, which in Chromium puts the
  focus in its own search bar and leaves the palette open. vueuse, which
  Nuxt UI asks what has the focus, hears of the focus moving, and not of
  the element holding it being taken away, which a browser tells nobody;
  the palette's search field goes with the palette, so the palette lets go
  of the focus as it closes, before Reka gives it back, or every shortcut
  would be passed over as typed into a field that is gone.
- **`?` opens the sheet of shortcuts** (`ShortcutSheet.vue`): every key the
  panel answers, the palette's, the dialog's, the matrix's and the
  editor's, under where each applies, its keys spelled out beside them for
  a screen reader. Which key does what is each component's; the sheet lists
  them and changes none, so a key a component gains is added to it by hand,
  as it is to the README's table and the reference's. The palette and the
  sheet each close the other.

Both are modals, which trap the focus and give it back where it was when
they close, named by the title Reka keeps for assistive technology; the
palette's field and its list carry names of their own. `tests/palette.test.ts`
holds the palette to its keys, its groups and its search, and each change
to the task the page offering it sends, with every way a page could reach
the network stubbed to fail; `tests/views.test.ts` opens both, the
palette through an offer's projects to what holds an install back, and
holds every icon they draw, which no page draws, to the bundle, with no
`<style>` added ([nothing the CSP refuses](#nothing-the-csp-refuses));
`e2e/palette.spec.ts` installs a skill from it with the keyboard alone
([end to end](#end-to-end-in-a-browser)).

### Choosing a folder

The Link form and Discover name a folder by its absolute path, as typed
(#77, #76), and a browser cannot walk the disk to help somebody find it.
So each has a **Browse** button opening the explorer (`FolderPicker.vue`,
#212), a modal showing one folder at a time as core lists it
(`Query::Folders`, core's `browse::Folders`, [what a browser may
name](#what-a-browser-may-name)):

- **What it shows is core's.** The folder, its trail from the root as
  buttons, and the folders directly inside it, in the order of their
  names, each with an icon and a word for what core says it is
  (`browse::Is`: a mind project by any marker, a weaver workspace, a git
  repository or any other folder, the strongest when it is more than one),
  whether the workspace links it and whether it is a link, and the same of
  the folder open, so it can be chosen too. The panel puts no path
  together: up is a crumb's path, down a subfolder's, home the path the
  answer names (the launch's, normalized as core takes it everywhere,
  when the path policy takes it), and the workspace's root no path at
  all. Which folder is a project and which the workspace links are rules
  (`MindProject::is_marked`, `WeaverWorkspace::is_linked`), and a listing
  put together in TypeScript would be a second copy of them.
- **What it leaves out is core's too.** A hidden folder is listed only
  once its switch is on, which says how many there are, counted by core
  whether listed or not; a `.mindweaver`, and a link leading into one,
  never, since the path policy refuses to name them; and no more than a
  thousand (`browse::FOLDERS_CAP`), saying so, past which a path is
  typed. A subfolder whose type will not read, a link that loops say, or
  whose path is not UTF-8, which no request could send back, is said with
  its code beside the rest (`browse_entry_unread`, `browse_name_unfit`),
  a hidden one only once the switch is on, so a switch left off says
  nothing of what it hides; and a folder that cannot be listed at all is refused with one
  (`browse_missing`, `browse_not_a_folder`, `browse_unlisted`). A link is
  listed and opened like any folder, since a repository is often reached
  through one, and cannot make a listing of one folder loop.
- **A path typed goes as typed**, as the Link form's does, for the server
  to hold to the path policy. A refusal is said with its code above the
  last folder listed, which stays, so a typo leaves the explorer where it
  was.
- **Choosing runs nothing.** A folder, or the one open, is chosen with a
  button, which puts its path in the field the explorer was opened from:
  the Link form's, which the link previews and runs as a typed one, or
  Discover's folders to look under, added once, which a scan reads only
  once Scan is pressed. A task that chose and linked in one step was the
  other way, a second way to link a folder, with a preview of its own to
  keep in step with the form's.
- **It opens where the field says**: at the folder the Link form's field
  holds, or the last one Discover's names, sent as typed, and at the
  workspace's root when it holds none. Its own path field holds what was
  sent until a listing answers, so a start the server refuses is there to
  be corrected, never the folder an earlier opening listed.
- **Read again each time it is opened.** Its key (`src/api/folders.ts`)
  is one `src/api/live.ts` does not follow, since the server watches none
  of the folders it lists, and carries which opening it is, and a listing
  is dropped as soon as nothing shows it, the explorer moving on to
  another folder included, as what the header may switch to is read each
  time its menu opens. So every folder is read again whenever it is
  opened, one walked back to within an opening too, the last folder
  listed staying on screen until it answers.

No command line gains anything: a shell has `ls` and completion, and the
listing is what a front end with no terminal lacks ([the read
model](#the-read-model)). Nor does the TUI's link form, which keeps its
text field ([open questions](#open-questions)).

`tests/folders.test.ts` walks the explorer over what a test of the CLI was
answered with as it walked the candidates fixture's folders
(`tests/fixtures/folders.json`): down by a name, up by the trail, home and
back, hidden folders asked for, a path refused with its code; and holds
what each choice puts in a field, and the link that previews, to the
commands' fixtures. `tests/a11y.test.ts` and `tests/views.test.ts` open it
from the Link form and from Discover, a modal like the palette;
`e2e/explorer.spec.ts` walks the suite's fixture in a browser, chooses a
project the Link form previews, cancelled, and adds a folder typed, on
Enter, to Discover's, which a scan then reads ([end to
end](#end-to-end-in-a-browser)).

### Its fixtures come from Rust

The views' tests render the snapshots in `apps/web/tests/fixtures`, which a
test of the CLI writes (`apps/cli/tests/fixtures_test.rs`): it builds a
workspace by running the commands, installing, editing a copy, changing
the library, removing a linked folder, and reads it through the panel's
backend as the server answers it, beside a home where nothing is made, and
beside the same workspace with more to find (`candidates.json`): projects
and repositories nobody linked, a project moved from its entry, and one a
`mind.toml` marks, linked, whose page offers the migration. A project like
that one drawn by hand from another in the snapshot was the other way,
whose counts said what its emptied lists did not hold. Beside them, `library.json` holds every query an artifact's page asks of
the library's own, its files and each file of them, with what the backend
answers, which the test transport answers from (`tests/panel.ts`), and
`commands.json` what previewing and running a few tasks in turn is
answered with, a link, an unlink to confirm, a `validate` that fails with a
code, a removal everywhere, then what the page of what could be linked and
a project's page send, folders marked, a relink, toggles and a migration's
dry run and run, which the runner's, the transcript's and the pages' tests
answer their tasks from (`commanding`), and `scan.json` what scanning the
folder of the workspace `candidates.json` is a snapshot of is answered
with, which the page of what could be linked is drawn from, beside that
snapshot, so the two describe one workspace. `folders.json` holds what
the explorer is answered with as it walks that workspace's folders,
beside a relative path refused with its code ([choosing a
folder](#choosing-a-folder)). `assigning.json` is a
workspace built for the matrix, one name offered from the library and a
loaded workspace, the shelf, a copy current, outdated, edited and
somebody's, a project nothing can go into and an entry that opens none;
`plans.json` what the backend plans for a few selections of its boxes, as
the matrix sends them, a change, a replacement, and a clash and two locks
beside a change; and `applied.json` what applying the first is previewed
and run as. `updating.json` is a workspace built to update, an installation
outdated, edited, missing and orphaned, a rule among them, and an entry that
opens none; `updates.json` what asking what an update would do there is
answered with; and `updated.json` each change its page offers, previewed and
run each on the workspace as it was built, so each is answered as the first
thing run there. `commands.json` ends with a removal everywhere of one of
the library's own, its dry run first, which that page sends. `drafts.json`
holds what judging drafts of the first workspace is answered with, the
library's `review` as it is, with two things `validate` warns of and with
its description emptied, and skills to add, one `add` writes and two it
refuses; `validated.json` what `mindweaver validate` says of the library once
`review` holds the draft it warns of, which the editor's verdict is held to
word for word; and `authored.json` each change authoring sends, previewed
and run each on the workspace as it was built, a save, a save once the file
changed on disk since it was read, an addition from a template, a rename
and an adoption. `serving.json` is a workspace keeping MCP servers, a
program, an address and one reached over server-sent events, loading a
team's server that waits for a yes, and linking a project for every agent
that received one; `servers.json` what its MCP page and its matrix ask of
it, a server to add judged fit and one whose secret is read from a
variable no agent can refer to, `github` given another transport, and
plans of the team's server and of one of the library's own; and
`served.json` a server added, one changed, and the team's server
installed by the plan's task, which writes nothing, then by its
confirmation's. No token is in any of them: a fixture is a file of the
repository, so what core refuses there is a variable's name. So
the panel's tests render what core says rather than what a person drew,
and a field the snapshot adds, renames or fills differently reaches them.
What differs between runs is pinned: the temporary folder reads as
`/home`, each project's id is a fixed one, and every time the ledger keeps
is set from one instant.

They are kept as the types are ([its types come from
Rust](#its-types-come-from-rust)): committed, so the panel's tests run
without cargo; written under the `ts` feature, so a plain `cargo test`
writes nothing; written again by `make web/fixtures`, which empties the
folder first, with cargo alone; and checked by `make web/fixtures/check`,
which fails when git sees them differ from the ones committed, in CI's
`types drift (web)`. A fixture drawn by hand was the other way, which
would pass its tests while the snapshot moved under it. Each is held to
the schema of what it is an answer to as well, which holds the API's
schemas to what the real backend answers ([the API's schemas come from
Rust](#the-apis-schemas-come-from-rust)).

### Nothing the CSP refuses

The server's CSP is `default-src 'self'` ([What keeps other pages
out](#what-keeps-other-pages-out)): nothing from another host, no inline
script or style, no `data:` URLs. The panel is built to need nothing more,
though four of its libraries assume otherwise:

- Geist and Geist Mono come from Fontsource, as files of the build.
- Icons are Lucide's, bundled: Nuxt UI's own and every one a component
  names. Iconify's online build fetches from its API any icon it was not
  handed, so `@iconify/vue` resolves to `src/overrides/iconify.ts`, over
  its offline build, which has no code that could ask. An icon left out of
  the bundle draws as nothing, which the shell test checks for.
- Nuxt UI's colours plugin writes the palette's variables into a `<style>`
  element at run time. The plugin resolves to a stub that does nothing, and
  `main.css` declares the same variables, from the mapping in `src/theme.ts`:
  zinc for the neutrals, and violet as the one accent.
- vueuse adds a `<style>` for every colour mode switch, to stop transitions
  while the classes change, and runs a switch as the page starts.
  `src/color-mode.ts` takes over the `updateHTMLAttrs` handler vueuse lets
  a program replace, and stops them with a class `main.css` holds instead.
  It also makes dark the default: `index.html` carries the class before any
  script runs, and dark is stored when this browser has chosen nothing.
- CodeMirror's style-mod writes a `<style>` element into a document, and
  adopts a constructed sheet into a shadow root, which is where the editor
  is drawn ([authoring in the library](#authoring-in-the-library)).
- Vite inlines nothing as a `data:` URL (`assetsInlineLimit: 0`).
- A skill's markdown is drawn from its tokens, so nothing in it reaches the
  page as HTML, a style or a `data:` URL ([its views](#its-views)).
- Nuxt UI's command palette draws what a search matched as HTML, each match
  marked: every entry gives that HTML as empty, so it draws the words as
  text ([the command palette](#the-command-palette)).

A nonce per launch would have let the style elements in, at the price of the
server rewriting the page each time and the panel handing the nonce to each
library that writes one. `tests/dist.test.ts` builds the panel as `make
web/build` does, in a process of its own, and fails on a URL naming another
host anywhere in the page, the scripts or the styles (namespace names, the
address marked writes into the message of an error its parser throws, which
the panel never parses with, and the links Vue prints beside a production
error, aside), a reference not to
the panel's own files, an inline script or style in the page, a palette
variable missing, or Iconify's API or the injected palette back in the
scripts. The shell test fails on any `<style>` element added while the panel
starts and switches colour mode.

### End to end, in a browser

Every suite above stops at a boundary: the server's suites run against a
fake backend, the CLI's backend suite binds no port, `ui_test.rs` speaks
HTTP without a browser, and the panel's suites draw fixtures in happy-dom
over a fake transport. `apps/web/e2e` (#75) crosses all of them at once: the real
`mindweaver ui`, the CLI's backend over core, the server and the panel it
embeds, driven by Playwright in Chromium and in WebKit, the engine a
desktop shell would draw the panel with.

`make web/e2e` builds the panel and the debug binaries, a debug build
reading `dist/` as requests come in, and runs `e2e/run.ts`, a driver, with
Node. A driver because Playwright's order is the wrong one twice over: its
webServer starts before any globalSetup runs, and the fixture has to be on
disk before `mindweaver ui` resolves its workspace; and it stops the server
only after every teardown, when the home is to be found untouched. So the
driver makes a folder in the system's temporary folder, refusing one with a
`.mind` or a `.mindweaver` above it, since resolving walks up before it
looks at a home; makes an empty `home` there; builds the fixture workspace
beside it (`e2e/fixture.ts`) with the binaries under test, a command at a
time, each held to exit 0 with no warning, and nothing written by hand but
the folders `mind init` runs in; runs `playwright test` with the options it
was given; and once Playwright, and the server with it, has stopped, fails
unless the home is as it was made, each file's size and time and the
folder's own time, so a file written and removed again shows too. Then it
removes the folder. `playwright test` run alone is refused, as nothing
would build the fixture or check the home.

What the suite starts can reach no workspace or agent home of the
developer's, by three means, as the CLI's own tests do it. Playwright runs
without `MINDWEAVER_WORKSPACE`, `MINDWEAVER_HOME`, `CLAUDE_CONFIG_DIR`,
`CODEX_HOME` and `OPENCODE_CONFIG_DIR`, so neither the server nor any worker
inherits them, and the config refuses to load with one set. The server and
every command get the temporary home as `HOME`, never an empty one, with
which Rust falls back to the account's. And every line names its home with
`--home`, its workspace with `--workspace` (empty for `mindweaver init`, which
names none and ignores the variable too) and `-C`, starts in a folder under
the temporary one, and gives every agent-home option empty
(`e2e/environment.ts`, which the driver, the config and the specs all start
processes through).

The webServer is `mindweaver ui --no-open --port 0`, `exec`ed by the shell
Playwright starts it with, so the Ctrl-C it is stopped with (`SIGINT`)
reaches the server alone and lets it close its event streams. Port 0, so a
run never meets another, which leaves the link the server prints first as
the only way to know the port: the config reads it with a regex whose
named groups Playwright puts in the environment of every worker
(`MINDWEAVER_E2E_URL`, `MINDWEAVER_E2E_TOKEN`). The server's stdout is not
printed, so the token stays out of the log, as the server keeps it out of
its own; its stderr is. Every test that opens a page goes in through the
launch link first (`panel.open`), so the token is traded for the cookie as
a person's browser trades it. The security smoke tests send requests of
their own: one with the cookie the link set, one with the token as
Bearer, and one with neither.

A page a click leads to is acted on through what that page alone draws.
The router loads a page's code the first time it is opened, so a click on
a link returns while the page it leaves is still drawn, and Playwright
matches a label or a name in any case and anywhere in it unless told
`exact`. The add page's `Name`, looked for anywhere, was once the
library's filter ("Filter each skill by name or summary"), and the name
typed into it went into the filter, or nowhere once the library made way
for the add page (#238). So a spec finds the fields it fills inside what
the page draws once it is ready, the add page's form by its name, and a
spec that only reads waits for a test id or an address only the page it
arrives at has; never for a fixed time, which a slower runner outlasts.

The specs hold the panel to the CLI: the overview's cards to the rows
`mindweaver list --json` prints, counted from them rather than read from a
count beside them; each project's page to what the listing says of that
project, kind by kind, in its order; each of the library's own to its row
and to the rows of the projects holding a copy. A security smoke test sends
the panel's own query, with the cookie and every header the panel sends,
from the panel's origin and from another, and only the second is refused,
as is one carrying the token as Bearer, and the page itself. The explorer
walks the fixture's folders, up by its trail and down by a name, and the
project it chooses is the folder the Link form's preview links, cancelled;
a folder typed in it, on Enter, is added to Discover's and scanned
(`e2e/explorer.spec.ts`), neither changing anything.

What the panel changes is held to the CLI byte for byte. A test that
changes something takes a workspace of its own (`own`, in `e2e/panel.ts`):
built under the run's folder by the commands, `mindweaver init` and two `mind
init`s with one of them linked (`changeable`), copied whole beside itself
before anything else touches it, and served by a `mindweaver ui` the test
starts as the config starts the suite's (`serve`, in
`e2e/environment.ts`), with the same home and options, and stops with the
Ctrl-C a person would. A project linked from the panel, its line shown
first, has to leave `weaver.toml` exactly as `mindweaver link` leaves the
copy's; an unlink has to ask, focused on Cancel, leave the marker as it
was when told no, and then leave it as `mindweaver unlink` leaves the copy's;
and the project a scan of the workspace's folder suggests, marked on the
page of what could be linked, has to be linked by the line it shows, scanned
again as linked, appear in the projects table, and leave the marker as
`mindweaver link` leaves the copy's (`e2e/discover.spec.ts`). Of its own, since the suite runs its tests side by side and in two
browsers over one server: a change to the shared fixture would move what
the overview counts under a test reading it, and the second browser would
find its project linked already.

`e2e/updates.spec.ts` takes `updatable`: the library's `lint`, installed
into two projects, one of which authors its skills in Claude's folder and
had its copy edited with `mind edit`, an `--editor` copying a draft in, as
an editor saves one. The library's `lint` changed the same way while the
projects table is open has to show the other project outdated, by the
stream rather than a reload; the page of updates has to list it and the
edited copy with why; **Update all** has to run `mindweaver update --all --yes`,
shown first, after which the badge is gone and the edited copy still
listed with its reason, which the run's own warning says by its code too;
and removing `lint` has to show its dry run, then take it out of the one
project and leave the edited copy, confirmed first. Each leaves the
projects as the same lines leave them in the twin. The edit is made with
the commands too: `mind edit` reaches only what a project authors, so the
project authors where Claude reads, and the edit keeps the copy's record,
against which it then stands as edited. The drafts the `--editor` copies in
are the one thing a builder or a spec writes by hand, beside the workspace
rather than in it, and never in a home.

A spec says what its own workspace is built as (`building`): the matrix's
(`e2e/assign.spec.ts`) takes `assignable`, a workspace loading `team`
beside it, whose own skill it offers, and linking two projects for Claude
and Codex. The skill, ticked into both, has to be copied where each agent
reads it, in each; unticked out of one, confirmed, gone from there alone;
each batch's line shown before it runs; and the same lines, typed into the
copy, have to leave both projects holding the same files. From a load
rather than the shelf, which only a gather from a git source fills, and the
suite builds nothing a git source makes; an offer from the shelf is
installed with `--from` as one from a load is, and the views' suites draw
one from their fixture. That the ledger a batch leaves is the one its
lines leave run in the CLI, every hash and action logged, is the backend
suite's to hold (`backend_test.rs`), which reads it.

`e2e/mcp.spec.ts` takes `serveable`, a workspace linking one project for
Claude, Codex and OpenCode, and serves it with a variable set in its
`mindweaver ui`'s environment alone (`variables`, handed to `serve`): an
MCP server's secret, its value drawn as the run starts, never a token
written in the repository. A server added from the MCP page's form, its
secret that variable's name, and ticked into the project on the matrix,
each line shown first, has to be in `.mcp.json`, `.codex/config.toml` and
`opencode.json`, each referring to the variable in its agent's own way,
and the value in none of them nor in the library; the run's own warning
says Codex reads it only in a project it trusts, by its code; and the twin
given the same lines has to hold the same files.

`e2e/palette.spec.ts` takes `assignable` too, and drives the palette with
the keyboard alone once the sidebar's **Search** has the focus: opened by
its keys, it has to hold the focus through Tab and Shift+Tab,
name its search field and its list, and give the focus back on Escape,
and on its keys pressed again in its search; then the team's skill, typed and chosen, and one project, typed and
chosen, have to show the line core's plan of that box runs, Run focused
since it takes nothing away, which Enter runs. The copies have to be where
each agent reads them in that project alone, and the copy given the same
line on the command line has to hold the same files, and be listed by
`mindweaver list --json` as the workspace is, its folder aside.

`e2e/loads.spec.ts` takes `loadable`, the one builder whose workspace is
served with a home of its own beside it rather than the run's: only a
default workspace remembers the workspaces this machine knows, and the
run's home is held to stay empty. The builder makes that default first, so
each `mindweaver init` after it remembers the workspace it makes, `team` with a
skill of its own, then the workspace, holding one and linking a project.
`team`, loaded from the page's list of what this machine knows by the line
it shows, has to list its skill, which the matrix then offers; unloaded,
confirmed first, it has to be offered no more; and the twin, given the
same two lines, has to hold the same marker after each. A home of a test's
own is held to stay as it was made once its server has stopped, by the look
the driver holds the run's with (`look`, in `e2e/environment.ts`), so a
load that wrote into the default workspace would fail the test that ran
it.

`e2e/switch.spec.ts` takes the same builder, for the workspaces its home
knows: the header has to name the workspace and say it was named with
`-W`, as the server is launched with one, and list the default, `team` and
the workspace, the last checked; switched to `team`, the overview has to
draw it, the library its skill and not the workspace's, and the line a
load previews has to name `team` with `-W`; switched back, the line has to
name the workspace again. A switch writes nothing, which the home's look
holds too.

No page may log a violation of the CSP. Each engine fires
`securitypolicyviolation` and words its own console messages, and a
message the browser rather than a script logs may not reach Playwright, so
a script Playwright adds beside the page's own says each event in the
console in one form, and every test fails on any console line naming the
policy, and on any error a page throws. One spec visits every page and
switches the colour mode; another writes a style into the page, which the
policy refuses, so a watch that saw nothing could not pass the rest.

The browsers are Playwright's builds for its version, which `make
web/e2e/browsers` downloads into `apps/web/node_modules`
(`PLAYWRIGHT_BROWSERS_PATH`) rather than `~/.cache`, so what the suite
installs stays with the clone and goes with `node_modules`. WebKit's build
links against libraries Playwright installs only on Debian and Ubuntu
(`WITH_DEPS=1`, with sudo), so elsewhere the suite runs in Chromium
(`BROWSERS=chromium`), and WebKit in CI. A failed test keeps its trace,
every action with the page and the requests at each step, in
`test-results/`, and CI adds the HTML report.

A trace times each request from the browser, so it says a run took
seconds, and not where they went: an unload once took five and a half on
CI, which its trace could say no more of (#240). So a test's own
`mindweaver ui` is started with `--log` (`serve`, in `e2e/environment.ts`),
its stderr kept for as long as it runs, and a test that fails keeps it
beside its trace, once the server has stopped and said all it will, as
`mindweaver-ui.log` in its folder of `test-results/`, attached to the
report too (`close`, in `e2e/panel.ts`): every request with how long its
answer took, and every run with how long it waited its turn, how long
each of its lines took and how much of that it waited for a workspace's
lock ([the log](#the-log)), each after the time of day, to read beside the
trace. Not the config's server, whose stderr Playwright prints, which
would put a line for every request of every test in the run's output; no
spec runs a change on it. `e2e/log.spec.ts` holds a run from the panel to
being said there, as it starts, as its line runs and as it ends, with its
request, and never the token or the body the run was sent.

The log says where a run's time went among the server's parts, and not
whether the machine kept all of them waiting: the unload's line had
written its marker a second in, and took four more to let go of the lock
and finish, where every read beside it took its usual milliseconds, as
waiting on a disk shared by a whole suite and two browsers would look.
So every test that fails keeps how long the machine stalled while it ran
(`stalls`, in `e2e/panel.ts`), as `pressure.txt` beside the trace: for its
processors, its memory and its disk, how long at least one task was kept
waiting for each, and how long every task that would run was, from the
totals Linux keeps in `/proc/pressure` read as the test's first fixture
starts and again after its last one stops, so it is the test's own time
rather than an average of the minutes around it (`stalled`), then each file
as it read the second time. Nothing on a system without them, where there
is nothing to read.

Both are kept only for a test that ended other than it was expected to,
decided in one place for both (`keep`), and held by tests that pass: one
made to fail would fail the suite, and `test.fail()` makes none that keeps
anything, a test marked so that fails having ended as expected. So
`e2e/log.spec.ts` hands `keep` and both teardowns (`close`, `keepStalls`)
a stand-in for a test's `TestInfo` that failed (`ending`), and finds the
server's log kept whole once it has stopped, the token redacted where the
launch link carried it, and the pressure kept for each file Linux has;
and nothing kept, nor anything asked of what would be, for a test that
passed, or one marked to fail that failed.
Rather than a nested run of Playwright over a spec made to fail, which
would need a config and a fixture of its own beside the driver's.

### The README's screenshots

The README's pictures of the panel are taken, never drawn or captured by
hand: a capture by hand drifts from the panel unnoticed, and shows whatever
the machine it was taken on holds, a home's path and an account's name
among it. `make docs/screenshots` builds the panel and the debug binaries
as `make web/e2e` does and runs `e2e/screenshots.ts`, which reuses the
suite's harness (`environment.ts`): every process without the variables
that lead to a real workspace or agent home, a temporary home holding only
the default workspace the demo makes there, which has to be as the demo
made it once the server has stopped, every command naming its home, and
every agent home empty.

The workspace it shows is `e2e/demo.ts`'s rather than the suite's fixture,
which is made to be compared and looks it: six projects with plausible
names, each a repository `git init` made and `mind init` made a project,
some local and some shared, for different agents; a library of two skills
and three rules, with bodies to render, and two MCP servers, a program
started with a secret, installed into `billing-api`, and an address
reached with a bearer token, for the MCP page; a shelf gathered from a repository
beside the workspace, made with the git binary under a fixed author and
date and no configuration of the user's; installs from both, three of them
outdated, two by a second gather and one by an edit of the library's own,
and one edited, `infra`'s copy of that same skill, which `infra` keeps
where Claude reads its skills and changed with `mind edit` before;
a rule stating `paths`, so a project's page warns that two of its agents
never read it; two repositories nobody linked, for Discover; and a home
made the default workspace before anything else, so it knows the demo's
workspace and `platform` beside it, a workspace of skills of its own that
nothing loads, for the tour to load. Every
change is a command; the files written by hand are the repositories'
contents, as a person's work is, and each body of the library's own is
written by `mindweaver edit --editor 'cp <draft>'`, as an editor saves it.

It lives in `mindweaver-demo` under the system's temporary folder, always
the same, rather than a folder made fresh for each run as the suite's is:
every page shows the workspace's path, so a fresh name would change every
image at every run, and a reader is better served by
`/tmp/mindweaver-demo/Projects` than by a random suffix. A fixed name in a
shared folder could be somebody else's, so a run removes it only when it
carries the mark an earlier run left, and refuses it otherwise; the
`.mind` or `.mindweaver` check above the temporary folder is the suite's.
`mindweaver ui` is started there naming no workspace, so it finds the one it
is in, as a person's does, and the overview says `above` rather than that
`-W` named it.

The tour is Playwright's library driving Chromium, not a spec: Playwright's
runner would run the pages side by side, while the tour visits them once,
in order: the command palette over the overview, a word typed and
nothing chosen; the explorer over the projects page, at the workspace's
root, nothing chosen; the matrix with a box ticked and another unticked, the
plan's lines below them, never applied; the page of updates, its edited copy left
with why; the preview of linking what Discover marked, then, once that has
run, an unlink asked about and cancelled, and the transcript; and last the
dry run of removing one of the library's own everywhere, which changes
nothing, taken after the transcript it would otherwise join, once the toast
saying it ran has gone from over what it said; and `platform` loaded from
the page of loads, after every other page is taken without it, so no other
picture changes with it. Chromium
alone, since the images are the same in WebKit, which does not run
everywhere. The viewport is 1440 by 900, wide enough for a project's table
with every column beside the sidebar (1280 cut its targets short), at a
device scale of 1.25: one image pixel to a CSS pixel blurs once GitHub
scales the image to its column and a dense screen scales it up again, and
two doubled what each weighs, the busiest close to the budget. Dates are
written in US English and in UTC: the panel writes them in the browser's
language and time zone (`src/time.ts`), which Playwright takes from the
machine, so a run under another language would put its words into an
English README, and every run would tell its taker's time zone. The colour
mode is picked through the cookie the panel keeps it in (`src/preferences.ts`):
dark for the whole tour, as the owner prefers the README's images, one
image a page rather than a light and a dark one for a `<picture>` to pick
between.

Before an image is written, the page is read, its URL, its markup, its
text and what each of its fields holds (a value bound into a field is in
neither the markup nor the text, and Discover's folders start as the
workspace's own), for this account's home, a path under any other home
but `/home/you` (the reference's stand-in, which the panel's own
placeholders use), the account's name as a folder of a path, and the
launch token, and any of them fails the run: the name only as a folder,
since as a bare word it would be found in a page that says `root` or
`admin` of something else. A
page that threw, could not read the workspace, or says it cannot watch it
fails too, as does an image over 300 KB, which regenerating would add to
the history each time; each is near half that. Neither CI nor an aggregate
runs it, since it needs Chromium and changes committed files: a change to
what the panel shows runs it, looks at the images, and commits them.

### In `make test`, and in CI

`web` is in the root `Makefile`'s `APPS` like any app, and first, so `make
test` and `make build` run the panel's targets before the crates': a machine
without Node is told at once by `make test` rather than after every Rust
suite, and `make build` compiles the crates after the panel, so the server
takes it in within the same run rather than the next. An app's own Rust
targets need no Node, and the server builds without the panel through
`allow_missing`, so CI's Rust jobs set none up. The panel had stayed out of
the aggregates, in a `NODE_APPS` of its own, until CI had Node (#71):
before, `make test` would have failed on every runner for want of pnpm, and
said nothing about the code.

CI gives the panel jobs of its own:

- **`test (web)`** sets up pnpm at the version `packageManager` pins, which
  pnpm/action-setup reads from `apps/web/package.json` so a bump stays one
  line, then Node from `.node-version` with pnpm's store cached, and runs
  `make web/lint`, `make web/typecheck`, `make web/test` and `make
  web/build`. It uploads `dist/`.
- **`types drift (web)`** runs `make web/types/check`, with cargo and git
  and no Node. With `test (web)`'s vue-tsc it is `make web/types` in two
  halves: what is committed is what Rust writes, and the panel compiles
  against what is committed. A job of its own rather than the step of `test
  (cli)` it was, so a drift is a check named for it, required on its own.
  It runs `make web/fixtures/check` too, for the snapshots the views' tests
  render, which drift the same way ([its fixtures come from
  Rust](#its-fixtures-come-from-rust)), and then `make api/schemas/check`,
  for the API's schemas, which holds every fixture, as Rust has just
  written it again, to its schema ([the API's schemas come from
  Rust](#the-apis-schemas-come-from-rust)), and `make api/openapi/check`,
  for the API's OpenAPI document, which holds those schemas ([the API's
  description is written from both
  tables](#the-apis-description-is-written-from-both-tables)): steps of a
  job the rulesets require already, rather than a job of their own, whose
  name they would have to be changed by hand to require.
- **`build (cli)`** waits for `test (web)`, downloads that `dist/` before
  anything compiles, and builds the release for the runner's own triple,
  which embeds it. No step of the job sets up pnpm, so on a runner with
  none of its own `make cli/release` takes the downloaded panel rather than
  building another, and the panel a
  binary ships is the one that passed. Then it removes `apps/web/dist` and
  runs `make cli/release/check`, which starts the release's `mindweaver ui`
  (`scripts/serves-panel.sh`) from a folder of its own, with a home of its
  own and every agent home empty, reads the link, and fails unless `/`
  answers with the panel's page, not the 503 a build without it answers,
  and every file under `/assets/` the page names is served. The folder has
  to be gone, since a debug build reads it as requests come in and would
  pass; the script refuses to run while it is there rather than delete what
  somebody built. A script rather than a test: only a release with the
  panel embedded can pass it, and `cargo test` builds neither. Then it
  uploads the release's `mindweaver` and `mind`.
- **`e2e (web)`** waits for `build (cli)`, downloads those binaries outside
  the checkout, installs Chromium and WebKit with the libraries they need,
  and runs `make web/e2e/run` on them ([end to end, in a
  browser](#end-to-end-in-a-browser)), uploading the traces and the report
  when a test fails. It builds nothing: the binaries and the panel it drives
  are the ones a release ships, the panel `test (web)` built and the
  release `build (cli)` checked, rather than a third build of either.
- **test-cross**, the macOS and Windows jobs, sets up Node and pnpm as
  `test (web)` does, since `make test` runs the panel's suite there, and
  runs it with `-k`, so one app failing on a platform does not hide whether
  the others pass there.

The rulesets for `dev` and `main` require every check that runs on every
pull request, the panel's three among them; AGENTS.md lists them by name.

Every Linux job, `Gitflow`'s two among them, runs on the repository's own
runner, a self-hosted one asked for by its labels (`runs-on: [self-hosted,
Linux, X64]`), and only test-cross on GitHub's: GitHub's Linux runners
stopped starting jobs once the account was locked over billing, and a check
that never starts is one the rulesets wait on for ever. By its labels
rather than its name, so a second runner given them takes jobs beside the
first: one runner runs one job at a time, so the jobs GitHub ran side by
side wait for each other there. A variable naming the runner was rejected:
it would switch back without a commit, but the workflow would no longer say
where a job runs. A self-hosted runner's image is not GitHub's, and it keeps
what a job installed, so a step that needs what GitHub's image held
installs it only where it is missing, as `test (cli)` does zsh and the
python3 its virtualenv is made with. What every job needs is the image's:
`make` and a C compiler (`build-essential`), which links every binary and
compiles the SQLite `rusqlite` bundles, and a user of its own with sudo,
never root. Root
writes in a folder whose write permission a test took away, which is how
the suites make a write fail to see what is put back after it
(`migrate_names_test`, `writers_test`, `everywhere_test`), so under root
those fail, and the ones that probe first pass without holding anything;
`fmt and clippy` refuses a runner that is root before anything compiles,
rather than leave it to the first of them a quarter of an hour into `test
(core)`. Skipping them under root, as four others already skip when the
folder can be written all the same, was rejected: CI would then never hold
a failed write to what it puts back.

The repository is public, and such a runner runs whatever a pull request's
code says, on a machine that keeps what a job leaves. A first-time
contributor's approval, GitHub's default, is not enough there: one merged
change is all it takes for a stranger's next pull request to run unasked.
So the repository's Actions settings hold a pull request from a fork for a
maintainer's approval from every outside contributor
(`all_external_contributors`); collaborators work in branches of the
repository, which no setting holds.

The tests are split by app and by nothing else. A crate's unit tests, its
value objects and its integration suites run in its one `test (<app>)`,
as `make <app>/test` runs them: a workflow of jobs by layer (`unit`,
`value objects`, `integration`) ran beside it for a while, and ran the
same tests a second time on every push, compiling each crate twice to
say by a check's name which layer broke, which the failing test's path
says already. Each `build (<app>)` then builds its app in debug and, as a
step after it, the release (`make core/release`, `make server/release`,
the CLI's `make cli/release`): the optimized build is the one that ships,
so it is built on every run rather than first when a release is cut, and
after the debug build, which fails sooner and cheaper. A step rather than a
job of its own, so no check name changes and the rulesets stay as they are.

`make install` and `make cli/release` build the panel first through `make
web/build/if-node`. Where Node and pnpm run it is `make web/build`, and a
build that fails fails them. Where either is missing, a warning says what
the binaries embed instead, the panel `dist/` already holds or none, and
they build anyway, since a binary without the panel still has every
command; failing there, as both did before #71, left `cargo install` the
one way to the binaries without Node. Both tools are looked for, since pnpm
runs the panel's tools on the `node` the PATH gives, and GitHub's runners,
which CI ran on until it moved to its own, had a Node and no pnpm.

Node is 24, the LTS line (`.node-version`), and pnpm is pinned through
`packageManager`; `PNPM` names another way to run it, such as `npx`, where
corepack is not set up. Every target that runs a tool first installs from
the lockfile, frozen, when `node_modules` is older than it, `package.json` or
`pnpm-workspace.yaml`. pnpm holds back any version published less than a
day ago (`pnpm-workspace.yaml`), and runs no dependency's install script:
the two that ask, esbuild's and vue-demi's, have nothing to do here. A test
reads the licence of every package installed, development tools included,
and fails on one a GPL-3.0-only program could not take in, or one that
declares none and is not named with what its source says.

Nuxt UI's plugins declare its components and composables for vue-tsc in
`src/components.d.ts` and `src/auto-imports.d.ts`, committed so a fresh
clone type-checks. They name each file by its path in pnpm's store, whose
folder for `@nuxt/ui` carries a hash of its peers, so a bump of one leaves
every path naming nothing, and each type `any` without a word from vue-tsc.
A test fails on such a path, and `make web/build` writes the files again; no
test run writes them, or it would mend a stale one before the test saw it.

`make web/dev` serves the panel from Vite on 127.0.0.1:5173 and proxies
`/api`, and the launch link's `/?token=`, to `mindweaver ui --dev`, which listens
on 7878 unless `--port` names another and prints a link to the dev server.
Through the proxy the token is traded for its cookie where the panel runs.
`changeOrigin`, so the Host the server checks is its own address; the Origin
stays the dev server's, which `--dev` accepts ([through the panel's dev
server](#through-the-panels-dev-server)).

## Which workspace a command uses

One function in core decides, `resolve`
([D-08](#d-08-an-explicit-or-activated-workspace-wins)), and every front end
feeds it: each command, and the TUI's screens. In order:

1. the workspace `-W DIR` names, by its root or by its `.mindweaver`;
2. the one `$MINDWEAVER_WORKSPACE` activates, which `mindweaver activate` sets
   (see [activating a workspace](#activating-a-workspace));
3. the first one found walking up from where the command starts: the
   directory for a `mindweaver` command, the project's root for a `mind` one;
4. the default one, in home.

It hands back the workspace with the `Reason` that chose it (`Explicit`,
`Activated`, `Above`, `Home`), so a front end that says why it is showing a
workspace asks the answer rather than working it out from paths. `mindweaver
which` puts it into words beside the root, and the TUI's header names it for
every workspace it shows (`named with -W`, `activated`, `found above`,
`default workspace`), as the panel's header does. A switch from the panel's
header makes it `Explicit` for every workspace but the one the panel was
launched on ([switching workspace](#switching-workspace)), since each line
the panel runs names it with `-W` from then on. Only for `Home` does the TUI also say there is no
workspace here or above: the directory home is can also be named with `-W`,
activated, or reached walking up from below it. Which workspace a command
changed is another question, about the workspace rather than the rule, and
`load` and `unload` answer it from the path (`WeaverWorkspace::is_default`).
They call the one in home the default one however it was reached, so their
line reads the same on the run that makes it, from anywhere, as on every run
after, which from below home finds it walking up.

The first two are choices somebody made on purpose, so they win over where
somebody happens to stand. Activating a workspace is meant to override
location, as activating a Python virtualenv does; with the walk up first, an
activated workspace would lose, silently, to any workspace above the current
directory. A choice that does not open as a workspace is an error and never a
reason to carry on down the list, which would act on a workspace nobody chose
and say nothing about it. Nor is a chosen directory made into a workspace for
the occasion, a write included: `mindweaver init` makes workspaces, where it runs
or in the directory it is given.

`-W` given at all is the whole explicit answer. Given empty, it names none and
also keeps the activated one out, as `--home=` keeps the default out; that is
what lets the test helpers pin `--workspace=`, so no test picks up what the
developer's shell activated. An empty `$MINDWEAVER_WORKSPACE` names none, as
an empty `$MINDWEAVER_HOME` does. Both are read by the front end, as home is
(`Lookup::given` in the CLI), and handed to core in `Inputs`, so core never
reads the environment. `--home` and `$MINDWEAVER_HOME` keep meaning only
where the default lives: reusing them to choose a workspace would confuse
where the default is with which workspace to use. A relative `-W` is taken
from where the command was run, as `--home` is, rather than from `-C`, which
moves where a command works and not what it was told.

### Activating a workspace

`mindweaver activate [DIR]` prints shell code that sets `$MINDWEAVER_WORKSPACE`
for the rest of the shell, puts `(weaver:<name>) ` in front of the prompt,
and defines `mindweaver-deactivate`, which puts both back; it is run as `eval
"$(mindweaver activate DIR)"`. Printed for the shell to run rather than written
to a file to be sourced: activation lasts as long as the shell, as a
virtualenv's does, and D-08 rejected an activation file as hidden state in
repositories meant to stay minimal. Activating writes nothing at all, so it
does not remember the workspace among [the ones this machine
knows](#the-workspaces-this-machine-knows) either: `mindweaver init` does. It
opens `DIR` as `-W` opens a workspace (`resolve::named`), by its root or its
`.mindweaver`, and without `DIR` it activates what a read resolves from
where it runs, so `-W` and an earlier activation count, and the default one
only once it is made. What cannot be activated is an error with nothing on
stdout, so `eval` runs nothing. `mindweaver init PATH` makes the workspace to
activate, wherever it is to go. A relative `DIR` or `PATH` is taken from
where the command works, `-C` included, as `link`'s and `load`'s paths are:
each is one of the command's own words, unlike `-W`, which is taken from
where the command was run, as said above.

The code is core's (`activate::script`): what it sets is what `resolve` is
fed from, and how a path is quoted for a shell is a safety rule, so no front
end that offers to activate a workspace can write it differently. So is the
variable's name (`activate::WORKSPACE_VARIABLE`), by which the CLI reads it,
so the code that sets it and the front end that reads it cannot spell it two
ways; the agents' variables, which nothing here writes, stay the CLI's. The
CLI only decides the shell, from `--shell` or else from `$SHELL`, which it
reads because core never reads the environment. `$SHELL` names the login shell,
which is the running one more often than not; asking the system which
process started this one would be right more often, for much more
machinery, and `--shell` covers the rest.

bash and zsh share one code, since everything it uses means the same in
both. The prefix is put in front of `PS1` and taken out again by its text,
wherever it is by then, rather than by restoring a copy saved on the way in,
as a Python virtualenv does: restoring would also take out a virtualenv's
prefix added since. The virtualenv's own copy, `_OLD_VIRTUAL_PS1`, gets the
prefix and loses it the same way, so the virtualenv's `deactivate` neither
drops the prefix while a workspace is active nor brings it back once none
is. Between them, either deactivate undoes only its own activate, in either
order, which `tests/shells_test.rs` checks beside a real virtualenv, in bash
and in zsh. Each of those runs its shell with no startup file and an
environment holding only a `PATH`, a temporary `HOME` and what the test sets,
so no developer's shell or activation gets in. A shell that is not installed
is skipped, which a passing test does not show; under `$CI` that fails
instead, and the `test (cli)` job installs the zsh and the python3 its
runner lacks, where it lacks them. The
function is never called `deactivate`: defining that would take the
virtualenv's away. Activating again runs an existing `mindweaver-deactivate`
first, so one workspace is swapped for the other, and what the variable held
before the first is what the last puts back.

fish and PowerShell keep the prompt in a function, which the code wraps once
per shell. The wrapper prints the prefix only while a workspace is active,
and deactivating leaves it where it is. Unwrapping, as a virtualenv does,
would break the prompt of whatever wrapped it since: a virtualenv activated
afterwards keeps a copy of the wrapper, which calls the function unwrapping
takes away. Those two are asserted as text, in core, until something runs
them.

The name in the prompt is the marker's, and a marker comes with whatever
directory holds it, a cloned one included. A prompt is expanded every time
it is drawn: bash, and zsh with `PROMPT_SUBST`, run a `$(…)` or a backquote
in it, `\` and `%` are escapes, and `!` is a history number. Each of those,
and every control character, is shown as `_` (`activate::prompt`). Escaping
them instead would take one escaper per shell and per option (`promptvars`,
`PROMPT_SUBST`, `PROMPT_BANG`), none of which the code can see when it is
written. The path is only ever one single-quoted word, in each shell's own
quoting, and one that is not UTF-8 is refused rather than spelled wrong.

### The default, made only by a write

When nothing chooses a workspace and nothing up the tree has one, a command
falls back to the one at the default home. Home sits above most of what a
person works on, so it is the one directory where a workspace can manage
projects anywhere, and making it on demand means `mindweaver link ~/code/x` works
before anyone has chosen a directory.

It is made by the first command that **writes** to it, and by nothing that
only reads, and the caller says which with an `Intent`. `mindweaver link`, `add`,
`gather git`, `load`, `install` and `uninstall` resolve with `Intent::Write`,
which makes `~/.mindweaver` when nothing up the tree has one. A read resolves
with `Intent::Read`, which falls back instead to `WeaverWorkspace::unmade`:
the workspace `init` would make there, parsed from the same template rather
than written. That is `mindweaver list`, `show`, `validate`, `scan`, `templates`,
`gather list`, and `load` and `install` on their own.

Made, it puts one thing in the home directory, `.mindweaver`. Its own
artifacts go in its library ([The workspace's own
artifacts](#the-workspaces-own-artifacts)), so no `~/skills` or `~/rules` sits
above every project under home, and the workspace writes no `AGENTS.md` or
`CLAUDE.md` there. What an install into a project at home writes there is
that project's, as for any project
([D-03](#d-03-rules-reach-every-agent-through-agentsmd)).

A read of the unmade workspace answers as a read of one made a moment before
would, because it is the same marker: `mindweaver list` says it manages no
projects yet, `mindweaver show` looks for its own in the library the made one
keeps them in, `mindweaver scan` lists what is under home, all of it unlinked,
`mindweaver gather list` says nothing is gathered. Then one `hint:` line on stderr
says the workspace is not made yet and where the first command that writes
will make it. A hint, not a `warning:`: nothing is wrong with a workspace
nobody has needed yet, so the exit is whatever the read alone makes it. The
alternative was an empty answer written for each command, which is one more
thing to keep in step with what the made workspace says, and every front end
would have to write its own.

The TUI and the minds screen open on it the same way: empty, under a header
saying it is not made yet. The first change made from either runs `mindweaver
init` before its own lines (`App::lines`), so the workspace is made where the
screen shows it and the transcript says so. Left to the change, the command
that writes would make it, as it does from the shell; but the screen's lines
run from the workspace root and walk up from there, and before it is made no
marker at the root stops them, so with a workspace above home the change would
land in that one. `-W` cannot name it instead: a directory with no marker is
refused, not made. For the same reason the minds screen is built from where
`mindweaver minds` was run, as the TUI is, and the TUI opens its install screen on
the root it shows rather than walking up from it. The install screen itself
does not open on an unmade workspace: like any workspace with no projects, it
says there are none to install into.

Nothing is written through an unmade workspace. An edit of its marker is
refused as it is for any directory that is not a workspace, and so is opening
its ledger to write to, which would otherwise leave a `.mindweaver` holding a
database and no marker. A command that writes asks for the real one.

`unload`, which only takes away, resolves as a read and wants a made
workspace, since an unmade one has loaded nothing.

### The workspaces this machine knows

The default workspace keeps a list of the others on the machine, a `known`
array in its `weaver.toml` beside `projects` and `loaded`, so switching
between several weavers, in the CLI, the TUI and the panel's header (#84),
has somewhere to read them from. `mindweaver workspaces` lists it, `mindweaver
workspaces forget PATH` takes one out, and `w` in the TUI runs `mindweaver
workspaces` and shows what it prints, as `s` shows `mindweaver gather list`. What
is known, in which order, and which one is in use are core's (`known::known`,
fed by `resolve`), so the panel's switcher reads the answer the CLI prints,
with how each stands with the panel beside it: whether it is the one the
panel was launched on, which is listed whether anything remembers it or
not, the rule it is chosen by once switched to, and whether a switch to it
is offered (`known::switchable`), the list a switch is held to
([what a browser may name](#what-a-browser-may-name)). Its page of loads reads it already (#83), to offer loading one,
and whether the workspace on screen loads each and could load it is the
snapshot's, beside the row the list prints (`view::KnownHere`), since it
is of that workspace rather than of the machine.

In the default workspace's marker rather than in a file of its own or in the
ledger. Home is the one place every weaver on a machine can find; its marker
is already edited one array at a time (`edit_array`), under its lock, with
every comment kept; and the list is configuration a person may read and
prune by hand, like the registry. A file of its own would be one more format
to version and lock, and the ledger would hide the list from that person, and
would have to be made to hold it. The key is not in the template, since only
the default has a use for it and every other marker would carry a line about
nothing: the edit that first makes it gives it a comment saying what it is
(`explained`), and an edit that makes a key and puts nothing in it, as a
refused `forget` would, takes it out again, so the file is left as it was
found.

`mindweaver init` remembers: each workspace it makes, and one already there,
which it otherwise leaves as it was, so a workspace made before the default,
or by a build from before the list, is one idempotent command from being on
it. conda's `environments.txt` gains an environment the same way, when it is
made and not when it is activated. #56 asked `mindweaver activate` to remember
too, and it does not. Activating writes nothing ([activating a
workspace](#activating-a-workspace)), which #55 was accepted on and the
reference promises; writing there would take the default workspace's lock at
every new shell whose startup file runs `eval "$(mindweaver activate DIR)"`, wait
behind a gather holding it for up to `lock::WAIT`, and turn a default a newer
build wrote, or a home nobody can write to, into a warning at every prompt.
And nothing is lost by it: the workspace in use is listed whether it is
remembered or not, so an activated one is never missing from the list
somebody is looking at. Whether activating should remember after all is an
[open question](#open-questions).

Never made for the occasion. A default nothing has made yet remembers
nothing and lists nothing, and `mindweaver init` does not make one to remember
in, the rule every read follows; with `--home ""` there is no default, and
only the workspace in use is listed. The default is never in its own list:
it comes first, for being the default, whatever `known` says. An entry is
stored as `link` stores a project ([how entries are
stored](#how-entries-are-stored)), a route from home when one reaches and an
absolute path otherwise, and matched by where it points. So remembering one
already known by another spelling changes nothing and says the spelling the
file holds, and `forget` takes out every spelling, of a workspace that has
gone too, since that is the one most worth forgetting. One nothing remembers
is refused, as `unlink` refuses what is not linked.

The list is the default, then each entry in the order `known` holds them,
each directory once, then the one in use when it is none of those, said to
be not remembered and given the line that would remember it. The one in use
is marked `*`, as `git branch` marks the branch checked out, and each says
how many projects it links and how many of those are stale. A remembered
workspace that has gone is a row that cannot be opened and a `warning:` that
fails the exit, as a stale project is in every report: shown, never raised.
The stale entries of a workspace that opens are counted rather than warned
of, since they are that workspace's to report. `--json` prints the same list
in the envelope ([JSON, in one envelope](#json-in-one-envelope)), each field
by its name: `name`, `path`, `default`, `remembered`, `in_use`, `projects`,
`stale_projects` and `stale`.

`known` moves no `WEAVER_FORMAT_VERSION`. A build from before it reads past
the key, which says nothing about the workspace holding it, so there is
nothing for it to misread, and keeps it when it edits the marker, since every
edit goes through `toml_edit` ([formats are
versioned](#formats-are-versioned)).

### A project's workspace

A `mind` command resolves from the project's root, not from the directory it
was run in, because a project's workspace is the one above the project: core
moves the start there itself (`resolve::managing` and `resolve::consulted`).
`mind link` and `unlink` register the project with the workspace resolved;
`install` and `uninstall` need that workspace to link the project, and are
refused (`NotManaged`) when it does not (`managing`); `add`, `edit`, `rename`,
`remove` and `templates` consult it, to see to the record of what they change
and to lend templates, and only if it is made (`consulted`): a change to a
project is no reason to write a marker into somebody's home, and a workspace
made just now would hold nothing to see to or lend. One found above that
cannot be read is a `warning:` after the change, since the change is the
project's and the workspace was only to be told; one named or activated that
will not open refuses it, as it refuses every other command, because whoever
chose it meant that one to be told.

With `-W` or an activation, that is the chosen workspace wherever it sits,
which is how a project linked from beside its workspace is managed from
inside the project: walking up never reaches such a workspace. Which of a
workspace's projects `-p` names is core's too (`WeaverWorkspace::pick`, and
`holder` for the workspace's own when nothing is named).

`load` resolves with `resolve_except`, which passes over the workspaces being
loaded on the walk up and at home (see
[Loading](#loading-a-third-place-read-live)). A workspace named or activated
is never passed over: it was chosen, and a workspace loading itself is
refused where loads are.

### The TUI looks where it was told

The TUI resolves through the same function. `tui::load` and `load_project`
ask `resolve` and `consulted` with the `Lookup` the screen was opened with,
and `tui::execute` hands that lookup, home and agents' folders and all, to
every line the screen runs, rather than letting each line read the
environment again: a screen opened with `-W` or an activation runs every line
against that workspace. A `-W` among a line's own words wins, and is typed
from the root the lines run from, as a `-C` is.

A project's row runs `mind -C <entry>`, the line somebody in the project would
type, which consults the project's own workspace. Where that is not the
workspace on screen, because the project is linked from outside its tree or
from above a nearer workspace, the row runs `mind -W . -C <entry>` instead
(`Holder::Elsewhere`), so a change sees to the record of the workspace on screen
and the add form offers that workspace's templates, which are the ones its
line finds. The `-W` is there only where leaving it out would consult another
workspace, so every other row still teaches the plain project command; a
`mindweaver <cmd> -p <entry>` line would have consulted the same workspace, at the
price of a project's row no longer teaching a project's command.

### A read leaves the ledger as it finds it

`mindweaver.db` belongs to the workspace like its marker, and a read does not
make it either. `WeaverWorkspace::ledger_for_reading` opens it only when it
exists (`Ledger::open_existing`, without SQLite's create flag) and otherwise
answers from an empty ledger in memory, which says what a ledger nothing was
ever written to would. So `mindweaver gather list`, the snapshot every front end
reads and the install screen leave a workspace that has never gathered or
installed anything without one. The install screen is a read until something on it is applied;
applying runs lines that open the ledger to write (`ledger_to_write`,
through `assign`), which makes it, and never the stand-in, whose records
would be gone when it was dropped. Nor does a read scrub an existing ledger of
what an older build stored with a credential, which waits for the next
command that writes ([A URL is kept without its
credentials](#a-url-is-kept-without-its-credentials)).

A read does write to one thing: an existing ledger from an older schema is
migrated to this build's, as every open migrates it. A build reads its own
schema and no other, so declining would leave the file unreadable rather than
untouched; that step is upkeep of Mindweaver's own file, not a change anybody
asked for. Every migration copies the database aside before its first step
([Why the ledger is SQLite](#why-the-ledger-is-sqlite)), and a read migrates
through the same migration (`Sqlite::migrate`), so it leaves that copy too: the first
`mindweaver gather list` after an upgrade puts `mindweaver.db.v2.bak` beside a
ledger version 2 wrote.

## Agents, and where each kind reaches them

Mindweaver puts what it manages where Claude Code, Codex and OpenCode read
it, and which folder each one reads is recorded in
[docs/agents.md](agents.md): each fact verified on the binary by
`scripts/agent-probe`, or marked docs-only with a fallback. Core turns those
facts into two things in `agent.rs`, so no front end ever spells an agent's
folder.

`Agent` is `Claude`, `Codex` or `OpenCode`, named on a command line and in a
project's settings by the agent's binary: `claude`, `codex`, `opencode`. It is
taken by that exact name, as a shell is, and anything else is
refused with every name there is (`UnknownAgent`), so a settings file never
holds two spellings of one agent. It is a closed enum for the reason `Kind` is:
what an agent reads, and where, is a fact about that agent, so a new one is a
new arm in every match. An agent named at runtime would meet a table with no
entry for it and quietly reach nothing.

`TargetTable` says where each kind goes in a project for each agent, every
location relative to the project's root. `TargetTable::standard` is
[D-07](#d-07-one-copy-per-agent-folder)'s layout, and the facts under it are
the ones docs/agents.md lists for #28:

| Kind | Claude | Codex | OpenCode |
|---|---|---|---|
| skill | `.claude/skills` | `.agents/skills` | `.agents/skills` |
| rule | none yet | none yet | none yet |

- **Claude needs a copy of its own.** It reads `.claude/skills` and not
  `.agents/skills`, verified on Claude Code 2.1.309.
- **Codex reads `.agents/skills`**, docs-only. Its `.codex/skills` is read only
  in trusted projects and `$CODEX_HOME/skills` is deprecated, so neither is
  ever targeted.
- **OpenCode reads both**, docs-only, and is given `.agents/skills` rather than
  leaning on Claude's copy. V1 can switch its `.claude/skills` off
  (`OPENCODE_DISABLE_CLAUDE_CODE`), and a project that targets OpenCode alone
  should not need a Claude folder for it.
- **Nothing at user scope.** `~/.claude/skills`, `~/.codex/skills` and
  `~/.agents/skills` are [D-22](#d-22-user-scope-installs-come-later)'s, later.
  A test pins the table: every location it gives stays inside the project,
  and none is `.codex/skills`.

Where each agent reads is another thing, and core keeps it apart:
`Agent::reads(kind)` is every folder in a project the agent reads a kind
from, whoever put what is there, from the same facts. It can be more than the
table sends: OpenCode reads `.claude/skills` and `.opencode/skills` as well as
`.agents/skills`, and is sent a copy only in the last. A listing asks it which
agents see a skill where it sits ([read where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)); an
install asks the table where to write. One table for both was turned down: it
would either send OpenCode a second copy it does not need, or have a listing
say OpenCode misses what sits in Claude's folder. A test holds the two
together: every folder the standard table sends an agent is one it reads.
`.codex/skills`, read only in a trusted project, is not counted, and no folder
of rules is said to be read, though Claude reads `.claude/rules`: what
Mindweaver writes there is listed from the ledger as its rule's target
([installing rules](#installing-rules)), not found by discovery.

Rules have no entry, by design rather than by omission.
[D-03](#d-03-rules-reach-every-agent-through-agentsmd) gives a rule a section
of a block in the project's `AGENTS.md`, with Claude's fallback in
`.claude/rules/mindweaver` chosen by `claude_md::reach` ([how Claude reaches
the block](#how-claude-reaches-the-block)). The block is no folder
a copy goes into, one file every agent shares, so it is rendered outside the
table ([how a rule reaches the agents](#how-a-rule-reaches-the-agents)), and
so are Claude's files, which follow how Claude reaches the block rather than
an agent alone ([installing rules](#installing-rules)).

Nor have MCP servers, which reach an agent as one entry in its own config
file ([D-10](#d-10-an-mcp-server-is-one-owned-entry-per-agent)). Which file
that is, `Agent::mcp_file` says, the table's MCP targets, asked of the
renderer that writes the file rather than spelled a second time, so the
file the ledger records an entry under is the one it is written into:

| Kind | Claude | Codex | OpenCode |
|---|---|---|---|
| MCP server | `.mcp.json` | `.codex/config.toml` | the first of `opencode.json`, `opencode.jsonc`, `.opencode/opencode.json`, `.opencode/opencode.jsonc` there is, or `opencode.json` |

A function of the project's root rather than a row of the table, which is
pure: OpenCode's is whichever of its four files the project holds
([OpenCode's `opencode.json`](#opencodes-opencodejson)), looked for on disk,
and only that one can fail. `Agent::reads_files` names the same files, all
four of OpenCode's among them, so a front end watching a project watches
whichever one a snapshot reads.

`TargetTable::targets(kind, agents)` gives one target (`AgentTarget`) per
location, each with the agents it is for: all three agents take a skill in two
places, Codex and OpenCode share one copy, and an empty list takes none. An
agent named twice is served once, and an agent the kind does not reach adds
nothing. The targets are sorted by location and their agents in report order,
in core, so two front ends list them alike. Installing a copy into each
starts from this list (`install::targets`).

The table is a value a caller is handed, not a function it reaches for,
because the standard layout is only the default: the owner's amendment to
D-07 lets a project's `.mind/settings.toml` put a kind somewhere else, one
`.skills` folder for every agent for instance ([a project's own
targets](#a-projects-own-targets)). A caller that takes the table follows
the project's without changing, and two agents the override sends to one
folder get one target, as Codex and OpenCode do in the standard layout.
The project hands it on, in core: discovery, and an install, take the
targets from the project (`MindProject::targets`), which gives the standard
table overridden by what its settings name, and no front end calls
`TargetTable::standard`, which would pick a table the project's settings
could contradict. Rejected: a free `targets`
over the standard layout, which every caller would hard-wire and the override
would have to find again; and a folder per agent alone, which cannot say that
a skill and a rule reach Claude in different places.

A target's agents are the ones its copy is for, not proof that they read it:
the standard layout is built from what each agent reads, but a folder an
override names reaches an agent only once that agent is pointed at it, which
a project's status warns of (`ProjectStatus::unreached`). The table's shape
is private, a list of locations per kind and agent: the standard layout
needs one, and `TargetTable::overriding` sends a kind to the list of folders
an override names, for every agent alike, without a caller noticing.

It is pure: nothing is looked for on disk, and locations are joined a folder
at a time, so each has one kind of separator on Windows too. Whether a folder
exists, holds foreign content or is tracked is the install's to find out.

### A project's own targets

A project's settings may name, per kind, the folders what is installed into
it lands in, in place of the ones its agents read: a `[targets]` table
holding `skills = [".skills"]` sends every skill installed into the project
to `.skills`, one copy for every agent it names (the owner's amendment to
[D-07](#d-07-one-copy-per-agent-folder)). `MindProject::targets` overrides
the standard table for each kind the settings name with the folders they
give (`TargetTable::overriding`), so discovery reads there, an install writes
there and records that one target, and a change of agents carries copies to
and from there, none of them knowing an override was read.

- **The standard layout is the default**, and stays so: it is the one place
  each agent finds a skill without being told, by the facts in
  docs/agents.md, so a project made without a thought for it reaches every
  agent it names. An override is for somebody who points the agents
  elsewhere on purpose, and says so in the file.
- **For every agent alike**, rather than a folder per agent: one shared
  folder is what the amendment asks for, and a table per agent written by
  hand would be the standard one again, with every fact in it to keep in
  step with the agents by somebody who is not reading docs/agents.md. Several
  folders are each every agent's, and a folder named twice is one target.
- **Each folder is held to what an install may use** (`misplaced_target`):
  inside the project, as a folder it authors in is, so a copy is that
  project's and no other's; not in `.mind`, which no agent reads and a local
  project keeps out of git; not in a `.mindweaver`, which no agent reads
  either, and whose shelf and cache gathering replaces by names of its own,
  which a workspace at the project's root keeps there; not in a `.git`,
  which git owns, where a skill named `logs` would be written among its own
  files, in every clone of a shared project; and only for a kind
  installed as a folder an artifact, so a rule, which reaches agents through
  `AGENTS.md` ([D-03](#d-03-rules-reach-every-agent-through-agentsmd)), has
  none: its block and Claude's files are rendered outside the table. What
  an install refuses of any target, an agent's own folder and one a loaded
  workspace keeps as its own, it refuses of these too, since only it knows
  the homes and the loads.
- **Strict when writing, lenient when reading.** A change that names a folder
  no install may use is refused before anything is written, and so is a kind
  named with no folder at all, which would install it nowhere: an
  uninstall's to do, by name. A folder written into the file by hand is read,
  passed over and said (`MindProject::misplaced_targets`, `MisplacedTarget`):
  a warning wherever the project is reported or installed into, and no copy
  there, rather than a reason to refuse everything else the settings say. A
  kind every folder of which is passed over goes nowhere, rather than back to
  the standard folders the settings said not to use, and an install into it
  is refused (`InstallError::NoUsableTarget`).
- **Read by no agent until one is pointed there.** Whether an agent reads a
  folder is told by where it reads (`Agent::reads`), matched on disk, so a
  `.claude/skills` linked to `.skills` is Claude reading it.
  `ProjectStatus::unreached` names, for each kind, the agents that read none
  of its targets; `status` warns of them, with a non-zero exit, since what is
  installed there reaches those agents no more than if it had gone, and
  `targets set` notes it when it is set (`Plan::unreached`). An agent pointed
  there by a setting of its own is beyond what docs/agents.md says it reads,
  and is still warned of ([an open question](#open-questions)).
- **Changed through `reapply`**, as the agents are: `mind targets set <kind>
  <dir>...` and `mind targets reset <kind>`, and `mindweaver targets -p`, a dry
  run without `--yes` ([when a project's agents, targets or share mode
  change](#when-a-projects-agents-targets-or-share-mode-change)).
- **With a version a build from before them refuses** (`TARGETS_VERSION`).
  A key the format does not define is otherwise read past, and a build from
  before this one would read past `targets` too and install into the
  standard folders without a word, recording copies the newer build does not
  read. A `settings.json` that set them was written at version 4, which such
  a build refuses, and one that did not stayed at 3, which every build since
  settings reads: the version such a file was written with was the lowest
  that read it as it meant, and a change of `targets` brought it along,
  back down once no kind was named. Bumping every file to 4 was turned down:
  a project that never set an override would have been refused by every
  older build for nothing. A `settings.toml` is 5 whatever it sets, since
  every build that reads one reads `targets`, and one from before it finds no
  project there at all, so setting or resetting them moves no version. A
  version 3 `settings.json` given `targets` by hand is read with them all the
  same: somebody wrote them to be used, and only an older build could miss
  them ([an open question](#open-questions)). One whose `targets` do not
  read as them is read past, as before them, so that no project an older
  build opened stops opening.

### Adding an agent

Record what it reads first, in docs/agents.md, verified or docs-only with a
fallback. Then a variant on `Agent` and its name, and its arm for each kind in
the table's `standard`, and in `read`, behind `Agent::reads`, which both match
every pair with no wildcard: neither a new agent nor a new kind compiles until
somebody has said where it goes and where it reads. Its arm in `files`,
behind `Agent::reads_files`, says which files in a project it reads, which
`Agent::reads_at_root` names by the name at the root each sits under, for a
suggestion to rank by ([which to link first](#which-to-link-first)), and
the panel watches ([live updates](#live-updates)). Its arms in `mcp::name`'s
`reserved` and `claimed` say which MCP servers' names it keeps for itself,
and its arm in `McpDefinition::inexpressible` what it cannot be given of a
definition, each from the "MCP servers" table in
[docs/agents.md](agents.md#mcp-servers). The
compiler lists every match on `Agent`, since each is exhaustive, but not what
is not a match:

- **`Agent::ALL`**, an array. A variant left out of it still compiles, parses
  as no agent, and gets no entry in the standard table, which is built from
  it, whatever its arm in `standard` says.
- **Its home, if it keeps a folder of its own for every project.** A field on
  `AgentHomes` and its folder in `AgentHomes::defaults_under`, its option in
  `AgentHomeOptions` and its variable in `homes::agent_homes`, and both in the
  tests' `common::PINNED` and `common::CLEARED`, or a test could read the
  developer's real folder. AGENTS.md's "So do the agent homes" and
  [the agents' own folders](#the-agents-own-folders) list them too.

## The agents' own folders

Which folders each agent reads, for skills and for rules, at project and user
scope, is recorded in [docs/agents.md](agents.md), each fact either verified
on the binary by `scripts/agent-probe` or marked docs-only with a fallback.
A change to an agent path cites it; this section is about how Mindweaver
finds the agents' own folders, not what the agents do in them.

Some checks look at what an agent reads for every project, not only at what
a project holds: Claude's user memory, which `claude_md` leaves out of what
keeps Claude from reading `AGENTS.md` (#44), and Codex's `config.toml`,
which `codex_mcp::render` reads (#91), and what doctor reads (#96): Codex's
`config.toml` again, OpenCode's global config files and every folder of
skills an agent reads at user scope, `~/.agents/skills` among them
(`AgentHomes::skills_read_by`, [what no agent says](#what-no-agent-says)).
Those folders come in the way home does. `--claude-config-dir`, `--codex-home`,
`--opencode-config-dir` and `--agents-dir` are global options on both parsers,
and `homes::agent_homes` takes each from its option, then from the variable
the agent itself reads (`$CLAUDE_CONFIG_DIR`, `$CODEX_HOME`,
`$OPENCODE_CONFIG_DIR`), then from its folder in the user's home (`~/.claude`,
`~/.codex`, `~/.config/opencode`, `~/.agents`). `~/.agents` has no variable:
it belongs to no one agent, so none of their variables moves it. Core takes
the answer as an `AgentHomes` and never resolves one.

For OpenCode that order is a simplification: its documentation reads
`$OPENCODE_CONFIG_DIR` as a folder loaded on top of `~/.config/opencode`, not
one that replaces it, and names `~/.claude/skills` whatever
`$CLAUDE_CONFIG_DIR` says. Doctor is the first to read OpenCode's folder, so
`AgentHomes` takes both, as docs/agents.md's fallback says it would then:
while a variable names a folder of its own, the one in the user's home is
kept beside it, `~/.config/opencode` beneath OpenCode's
(`opencode_dir_in_home`) and `~/.claude` for the skills OpenCode reads
there (`claude_dir_in_home`), and read, watched and kept free of copies as
OpenCode's. An option names the one folder outright, for every agent
reading it, so a test that pins every option reaches no folder in the home
through these either. #17 ran only Claude Code, so both readings are still
docs-only; the [open questions](#open-questions) say what changes if a
check on the binary disagrees.

Those folder names are core's all the same (`AgentHomes::defaults_under`),
the way `.mindweaver` is: the CLI finds the user's home and core says which
folder in it each agent reads. Which folder an agent reads is a fact about
the agent, and spelling it in the CLI would leave a second front end to copy
it and, one day, to disagree. The variable names stay in the CLI, the only
one that reads them.

They are options rather than variables read where a check needs them, for the
reasons `--home` is one: core reads no environment, and a test cannot safely
set a variable, because every other test in the process would see it. An
option is pinned per command line, so `common::mind` and `common::weaver` give
all four empty and no test can read a developer's real `~/.claude`. A test that
spawns a binary has no parser to pin through, so it clears the variables and
points `HOME` at a temp dir instead.

Empty means none, not "unset, so use the default", because that is what makes
pinning work: an empty option has to switch the lookup off rather than fall
through to the environment running the tests. An empty variable means none
too, as an empty `$MINDWEAVER_HOME` does; whether the agents agree is an [open
question](#open-questions).

Installing is the first to take them, to refuse a copy inside one ([never
into an agent's own folder](#never-into-an-agents-own-folder)), and
rendering a project's rules takes them to refuse a block of them there ([how
a rule reaches the agents](#how-a-rule-reaches-the-agents)), as rendering an
MCP server into Codex's `.codex/config.toml` takes them to refuse a file
inside one (`codex_mcp_user_scope`), and into OpenCode's config too
(`opencode_mcp_user_scope`). They are
decided once per command line, with the workspace to use, into the same
`Lookup` (`Lookup::given`), which the TUI keeps and hands to every line it
runs (`tui::execute`), its install screen's among them: a line the TUI runs
carries none of the four options, so deciding them again would read the
environment the TUI was opened in, and a test that drives the TUI would reach
the real folders. None is written in. `codex_mcp::render` reads `config.toml` in the Codex
home `AgentHomes` gives, through a link and up to 2 MiB, to refuse a
server's name it holds already and to warn of a project it does not mark
trusted ([Codex's `.codex/config.toml`](#codexs-codexconfigtoml)), and
doctor reads it the same way, through the same reader, with OpenCode's
global config files beside it (`config_entry::read_through`, one function
for every such file), and each folder of skills an agent reads at user
scope. With no folder given for an agent, nothing of it is read.

## How a rule reaches the agents

A rule reaches a project's agents as a section of a block in the project's
root `AGENTS.md` ([D-03](#d-03-rules-reach-every-agent-through-agentsmd)):
the one file of a project that Claude Code, Codex and OpenCode all read, and
Codex has no way to include another ([docs/agents.md](agents.md)).
`agents_md::render` writes it, and `claude_rules::render` the rules Claude
alone is given ([Claude's rule files](#claudes-rule-files)). Which rules a
project is given, when, and the record of what was written are
`install::rules`' ([installing rules](#installing-rules)); whether Claude
reads the file at all, or needs an `@AGENTS.md` import in its `CLAUDE.md`,
is `claude_md`'s ([how Claude reaches the
block](#how-claude-reaches-the-block)).

```markdown
# Working here

Run `make test` before every push.

<!-- mindweaver:begin -->
<!-- mindweaver:rule commit-style -->
Write commit messages as `<area>: <imperative summary>`.

<!-- mindweaver:rule git/no-force-push -->
Never force-push a branch somebody else may have pulled.
<!-- mindweaver:end -->
```

- **HTML comments for markers.** Claude strips a block-level comment before
  the file reaches the model and keeps the text between two of them,
  verified on 2.1.309 with the file read natively and imported. Codex and
  OpenCode read the file as it is, where a comment is a line of markup that
  says nothing. A heading of Mindweaver's own was turned down: every agent
  would read it as one more instruction.
- **One section a rule, in listing order.** A section is the rule's marker,
  naming its route, then its body: `Artifact::contents`, the file less its
  front matter, without the blank lines it opens and closes with, so the
  block is laid out the same however each file spaces its fences or ends.
  One blank line separates two sections, which keeps two rules two
  paragraphs once Claude has stripped the markers. They come in the order
  `list` gives them (`catalog::report_order`), by name. A rule that states
  `paths` has no section: it goes to `.claude/rules/mindweaver` alone
  ([Claude's rule files](#claudes-rule-files)), where Claude loads it only
  once one of those files is read, while in the block every agent would read
  it for every file. A `paths` in a shape nothing reads says nothing, as
  everywhere else, so that rule has its section. Both renderers ask
  `RuleManifest::is_scoped`, so they never disagree on which rules those
  are.
- **Strict when writing.** A route is held to the rules every name is
  (`name_issues`), which also keeps its marker one that reads back as
  itself: lowercase words joined by single hyphens and slashes hold no space
  and no `--`. A body line that would read back as a marker is refused, and
  so are two rules of one name, a rule that cannot be read and anything that
  is not a rule. Each refuses the whole render, with nothing written: a
  section left out would take its rule away from every agent until the next
  render.
- **Every byte outside the block is kept.** The file is read as lines that
  keep their endings (`lines`, shared with git's exclude file), and only what
  lies between the markers is replaced, whole, a line somebody wrote there
  included. A new block goes after what the file holds, on lines of its own,
  with the ending the file already uses, so a CRLF file stays CRLF; a last
  line without a newline gets one, and the end marker goes without. An empty
  line sets it apart from what is above, so that once Claude strips the
  markers the file's last paragraph and the first rule's are still two, the
  layout the probe verified ([docs/agents.md](agents.md)). That line is the
  block's: an empty line just above the begin marker is taken out with it,
  one somebody wrote there by hand included, since nothing tells the two
  apart. So dropping the block gives the file back exactly as it was before
  the block was written, and a render that changes nothing writes nothing.
- **Refused, never repaired.** A block left open, an end marker closing
  nothing, a second block, a rule's marker outside the block, one route
  twice, and a line written like a marker but none of them (indented, spaced
  otherwise, naming a mark there is not) are each refused with the line they
  are on, and nothing is written (`agents_md::Malformed`), as in the exclude
  file. Read as text, a hand-indented begin marker would have the next render
  write a second block below the first. The file is read a line at a time,
  not as markdown, so a marker quoted in somebody's code fence counts:
  telling fences apart would need a markdown parser, for a mistake that is
  refused and never acted on.
- **Made, changed, taken away.** A file that is not there is made with
  `create_new`, and only then is the render said to have made it
  (`Written::Created`); the ledger keeps that (`created_files`) and the
  next render is handed it back as `created`, so that once no rule is left
  the block goes, and the file with it when nothing but blank lines is
  left. Mindweaver takes away only a file it
  made, and a link somebody put in its place is emptied, never removed. A
  file that is there is replaced through `replace_bytes`, a temporary and a
  rename, which follows a link to the file it names: an `AGENTS.md` linked
  to the `CLAUDE.md` beside it, as some teams keep the two, is written
  through, and Claude then reads the block as that `CLAUDE.md`, with no
  import beside it (`Reach::Same`, [how Claude reaches the
  block](#how-claude-reaches-the-block)). A link leading out of the project is refused
  (`AgentsMdError::LinkedOut`), since a repository can hold one to the
  user's own `~/.codex/AGENTS.md`, and replacing it with a file of the
  project's own would undo what somebody linked on purpose. It takes no
  lock, as `ConfigFile` takes none: its caller holds the workspace's.
- **Never above other projects.** Claude reads every `AGENTS.md` from where
  it starts up toward `/`, and OpenCode V2 up to the home directory, so rules
  rendered into a folder above a project the workspace links would reach
  that project too, beside its own: a monorepo's root, above its packages.
  A block is refused in such a folder, a stale entry's included, and in the
  workspace's own root unless a project it links is there. The workspace
  puts nothing in its own directory (`WeaverWorkspace::own` is a holder at
  that root, not a project), while a project at the root is a project like
  any other, and its `AGENTS.md` its own. A render that leaves no block goes
  ahead in either: a block written before a project was linked below it, or
  before the one at the root was unlinked, is reaching those agents already,
  and refusing to take it out would keep the leak the refusal is there to
  stop.
- **Never into an agent's own folder.** A block is refused, the same way,
  in a project at or inside one of the agents' own folders
  (`AgentsMdError::UserScope`), which the front end hands over as it does
  for an install ([never into an agent's own
  folder](#never-into-an-agents-own-folder)): a project at `$CODEX_HOME`
  would write the `AGENTS.md` Codex reads in every project, one at
  `~/.config/opencode` OpenCode's, and one at `~/.claude` the
  `.claude/AGENTS.md` Claude finds above every project in the user's home.
- **What Codex would miss is said.** Codex reads one file a folder,
  `AGENTS.override.md` before `AGENTS.md`, from the git root down to where it
  starts, and cuts the whole off at 32 KiB (`project_doc_max_bytes`); with no
  git root it reads only the folder it starts in (docs-only, each with its
  fallback in docs/agents.md). So a render that leaves a block warns
  (`agents_md::Warning`) of an override beside the file, of a project in no
  git work tree, and of the files from the git root down coming within an
  eighth of the budget or past it, each worked out for Codex started at the
  project's root. Warnings, not refusals: the block still reaches Claude and
  OpenCode, and what to change is in files that are not Mindweaver's. A
  repository or a file that cannot be looked at is said the same way, rather
  than turning a render already written into a failure.

### Claude's rule files

Only Claude reads a project's `.claude/rules`, at any depth, and only Claude
honours a rule's `paths`, loading such a rule once it reads a file they
match (verified on 2.1.309, [docs/agents.md](agents.md)). So a rule about
some files only reaches its project as a file there, which
`claude_rules::render` writes, and has no section in the block. The same
renderer gives Claude the rules about every file when it reads no block.

```text
.claude/rules/mindweaver/rust/errors.md          a rule that states paths
.claude/rules/mindweaver/git/no-force-push.md    one that does not, in fallback mode
```

- **Two modes, chosen by the caller.** `Mode::Scoped` writes only the rules
  that state `paths`, for a project whose Claude reads the block, natively or
  through the import in its `CLAUDE.md`. `Mode::Fallback` writes every rule,
  for one whose Claude reads no block. Which applies is `Reach::mode`, from
  how Claude reaches the block ([how Claude reaches the
  block](#how-claude-reaches-the-block)), never from whether a block was
  written alone: in fallback mode the block may still be written for Codex
  and OpenCode, and Claude, reading none of it, reads each rule once and
  misses none. A file left from the other mode goes
  once its rule is no longer taken, unless somebody changed it since: that
  one stays, and is said, like any edited file.
- **The rule as written.** A file holds the rule's bytes, front matter
  included, where the block holds `Artifact::contents`: Claude takes `paths`
  from it and strips the rest before the rule reaches the model (docs-only),
  and a file that is the rule byte for byte is one somebody can open and
  recognise. A `paths` in a shape nothing reads makes a rule about every
  file here as in the block, and its front matter is still kept as written.
- **A folder of Mindweaver's own.** `mindweaver` keeps the files apart from
  rules somebody keeps in `.claude/rules` by hand, so no route names one of
  theirs. Below it a file is filed by its route, folders and all, as in the
  library; a route is held to the name rules (`name_issues`), so it never
  climbs out, and neither does a recorded one.
- **What is Mindweaver's is what the caller recorded.** The renderer is
  handed what the ledger records of each file ([installing
  rules](#installing-rules)), as an earlier render returned it
  (`Rendered::recorded`): each file's route, and the hash of what it held
  (`hash::bytes`, the SHA-256 of the bytes, beside `hash::tree` for a
  folder). A file is Mindweaver's while it holds exactly that: replaced when
  its rule changes, taken away when its rule is no longer asked for.
  Anything else at a rule's place is somebody's, never written over or taken
  away. A file nothing recorded is `Written::Foreign`, and its rule is not
  written, even when it holds the rule byte for byte, since taking it for
  Mindweaver's would remove it with the rule. One changed since it was
  written is `Written::Edited`, left as it is with its record kept, as
  [D-04](#d-04-an-edited-copy-is-detected-not-disowned) has an edited copy;
  edited back into the rule as it is now, it is current again. Only a forced
  install or uninstall of its own rule replaces or takes it away, handing
  the hash of what it holds now as the one recorded. A link at a rule's
  place is somebody's too, since Mindweaver writes none. Rejected: telling
  them by route alone, which would write over a teammate's change committed
  into a shared project, as the block once replaced a hand-edited section;
  by the folder, which would take somebody's file there for Mindweaver's;
  and by a mark in the file, which would no longer be the rule as written,
  and which a copy made by hand carries along.
- **Made, replaced, taken away.** A file that is not there is made with
  `create_new`, so one somebody made after the look is never written over,
  and only then is it said to be made (`Written::Created`). One that is
  Mindweaver's is replaced through a temporary beside it and a rename, never
  through a link (`replace_bytes_unresolved`): the temporary's name ends in
  `.tmp`, which Claude's `**/*.md` does not load, so it is no rule staged
  where Claude looks. One that already holds the rule is not written at all.
  Every file is written before any is taken away, so a renamed rule is never
  missing from the folder between its old file and its new. Taking away the
  last file takes the folders it leaves empty, up to and including
  `mindweaver`, and not `.claude/rules` or `.claude`, which may be
  somebody's even when empty; a recorded file found gone (`Written::Gone`)
  leaves its folders to go the same way.
- **Refused whole, or failed file by file.** A rule that is not a rule, one
  that cannot be read or is named no name, two rules of one name and a
  recorded route that is no name refuse the whole render before anything is
  written, as the block's render refuses them. Two of one name are refused
  among every rule handed in, whichever the mode takes: in scoped mode the
  block compares only the rules about every file, so a pair of which one
  states `paths` is this render's to refuse. A file that cannot be looked
  at, written or taken away is only its rule's failure
  (`claude_rules::Warning::Failed`): each file stands on its own, and the
  caller has to hear of every file that was written, to record it.
- **Kept out of git in a local project.** One exclude entry a file, in the
  project's group keyed by its id ([what keeps an install out of
  git](#what-keeps-an-install-out-of-git)), all written before any file
  lands; a block that cannot be edited refuses the render with nothing
  written. The entries of files taken away, found gone, or that failed to
  land come out last, and a block that cannot be edited then is said
  (`Rendered::still_excluded`), not raised. Somebody's file at a rule's
  place gets no entry, and keeps whatever entry it had. An entry names the
  route through `.claude`, not the folder on disk, and git follows no link,
  so a file reached through a link inside the project is not kept out
  ([open question](#open-questions)).
- **Never where the block is refused.** A render asked for a rule is refused
  wherever `agents_md::misplaced` refuses the block: above a project the
  workspace links, at the workspace's root where it links none, and inside
  an agent's own folder. For a project at the user's home that folder is
  `~/.claude` itself, whose `rules` Claude reads in every project. A render
  that only takes files away goes ahead, so rules left there can come out.
  Whether Claude reads a `.claude/rules` in a folder above where it starts is
  not recorded, so above a project is refused as for `AGENTS.md`, which at
  worst is too cautious (an [open question](#open-questions)).
- **Never out through a link.** A link on the way to a rule's file that
  leads out of the project is refused, writing or taking away
  (`ClaudeRulesError::LinkedOut`): `.claude` linked to the user's own
  `~/.claude` would give every project these rules, and a removal through
  it would take a file out of somebody else's folder. A link that stays
  inside the project is followed, though in a local project the file's
  exclude entry then misses it (above).
- **What misses a rule is said.** A rule that states `paths` reaches Claude
  alone, in either mode, and `Warning::ClaudeOnly` names the project's
  agents that read no `.claude/rules`: Codex and OpenCode, unless the
  project names fewer, and both for a project a `mind.toml` marks, which
  names none (docs-only for both agents; were either to read the folder,
  the warning would be too cautious, never wrong about Claude). Which agents
  read the folder is a match with an arm an agent and no wildcard, so a new
  agent says whether it does before it compiles. `Agent::reads` still names
  no folder of rules, so discovery reads none of these files, and a listing
  of the project takes none for somebody's: status lists each as a target
  of its rule, from the ledger ([installing rules](#installing-rules)).
- **No lock of its own.** As `agents_md::render`, it takes none: its caller
  holds the workspace's.

### How Claude reaches the block

Claude Code reads `AGENTS.md` natively only when no `CLAUDE.md`,
`.claude/CLAUDE.md` or `CLAUDE.local.md` is in the folder it starts in or
any folder above it (verified on 2.1.309, [docs/agents.md](agents.md)). So
the block reaches Codex and OpenCode wherever it is, and Claude only as
`claude_md` decides: natively, through an `@AGENTS.md` import Mindweaver
adds to the project's own `CLAUDE.md`, or not at all, when Claude's rule
files carry every rule instead ([Claude's rule
files](#claudes-rule-files)).

```markdown
<!-- mindweaver:begin -->
@AGENTS.md
<!-- mindweaver:end -->

# Working here
```

- **One predicate.** `claude_md::reads_agents_md` looks for the three names
  in the project's root and every folder above it, past the git root, since
  a `CLAUDE.md` in the folder somebody keeps their repositories in stops the
  read too (verified). It stops below `/`, as Claude's own check does by the
  directory counts in its debug log: counting `/` would choose the fallback
  where Claude reads the block natively, and give Claude every rule about
  every file twice (D-03 records the choice). The user memory file,
  `CLAUDE.md` in the Claude folder the front end hands over in
  `AgentHomes`, never counts, also where the walk from a project under the
  home finds it as the home's `.claude/CLAUDE.md` (verified); it is told by
  the file it is, both paths resolved, so a spelling through a link is still
  that file. The walk starts from the project's root resolved, as Claude,
  started there, has its directory from the system. Anything at one of the
  names counts, a link followed, a link to nothing not.
- **Five reaches.** `claude_md::reach` takes the predicate, the project's
  settings and whether the block was written, and says how Claude gets the
  block. `Reach::Native` when nothing is in the way. `Reach::Same` when one
  of Claude's own names in the project is the `AGENTS.md`, one file through
  a link, as some teams keep the two: Claude reads the block as that file,
  and an import there would bring the file into itself, between markers the
  block already holds. `Reach::Imported` when one of the project's own
  files already imports `AGENTS.md` with a line somebody wrote, as Claude's
  docs show a `CLAUDE.md` sharing it. `Reach::Import` when the project's
  own `CLAUDE.md` or `.claude/CLAUDE.md` may take Mindweaver's import, or
  already holds it. `Reach::Fallback`, with its
  reason, otherwise: no block written, only a `CLAUDE.local.md`, a
  `CLAUDE.md` only above the project, the project's own one tracked by git
  in a local project without `edit_tracked`, one reached through a link, or
  one of which git cannot say whether it tracks it. `Same` is the case #42
  left to this predicate: the block is written through such a link, and an
  import in the same file would have each render take the other's block
  for its own.
- **Never read twice.** `Reach::mode` is the mode Claude's rule files are
  rendered in: `Mode::Scoped` for `Native`, `Same`, `Imported` and `Import`,
  where Claude reads the block once, natively, as its own file, or through
  an import, which loads a file once and turns the native read off by being
  in a `CLAUDE.md` (verified); `Mode::Fallback` for every reason Claude
  reads no block, where its rules folder holds every rule and the block none
  of them for Claude. No block written is a fallback too, whatever the
  predicate says: Claude cannot read a block that is not there, and the
  rules folder then holds the rules about every file that a refused or
  skipped block would have. So Claude reads a rule in exactly one place.
  An import somebody wrote counts as Mindweaver's does, in a file Mindweaver
  may not edit too: a team's tracked `CLAUDE.md` holding `@AGENTS.md`, in a
  local project, would otherwise give the fallback, and Claude would read
  every rule about every file in the block through that line and again in
  its rules folder. So does one in a `CLAUDE.local.md`, when no file of the
  project's own can take Mindweaver's, since Claude reads that file as it
  reads `CLAUDE.md`. Such an import is a line of its own, `@AGENTS.md`, or
  `@../AGENTS.md` in `.claude/CLAUDE.md`, the line Mindweaver would write,
  outside a fenced code block, whose imports Claude skips (docs-only), and
  outside an HTML comment over several lines, which Claude strips. Looking
  no further is on purpose: a line taken for an import Claude does not
  follow would keep those rules from Claude altogether, while one missed
  has Claude read them twice at worst. An `@AGENTS.md` in the middle of a
  sentence, which Claude follows too, is one missed.
- **Which file takes the import.** `CLAUDE.md` first, the file the probe
  verified the import in, as `@AGENTS.md`; then `.claude/CLAUDE.md`, as
  `@../AGENTS.md`, since Claude resolves an import from the file holding it
  (docs-only, with its fallback in docs/agents.md). Never
  `CLAUDE.local.md`, a person's own file, kept out of the team's view. A
  file may take it in a shared project, in one a `mind.toml` marks, which
  was committed by design, and in a local one while git does not track the
  file (`exclude::tracks_file`, from the index with gix) or its settings say
  `edit_tracked` (D-02). A file already holding the import keeps it, so it
  never moves from one file to the other, and counts as how Claude reaches
  the block even once it may no longer be edited, a tracked `CLAUDE.md`
  committed with the import in it say, since Claude reads it there anyway:
  taking the fallback beside it would have Claude read every rule twice.
  An import somebody wrote in either file comes before Mindweaver's, which
  is then wanted nowhere and taken out where it may be, so nothing is
  written where a team already shares `AGENTS.md` with Claude. One in a
  `CLAUDE.local.md` comes after every file of the project's own that may
  take Mindweaver's: it reaches only its owner, and a teammate cloning a
  shared project needs the import in the file they share.
- **A block at the top.** The import sits between the markers the block of
  `AGENTS.md` has, read by the same search (`agents_md::locate`) through
  the same lines (`lines`), at the top of the file, an empty line below it,
  the layout the probe verified. The empty line is the block's, taken out
  with it, so the file comes back as it was. Everything outside the block
  is kept byte for byte, CRLF and a missing final newline included, and a
  byte order mark stays first, where the block would otherwise push it into
  the middle of the file. A malformed block is refused with nothing
  written, as in `AGENTS.md`, and so is a rule's marker in the block: it
  marks a copy of `AGENTS.md`, and an import beside its rules would give
  them twice.
- **What is never touched.** `claude_md::render` edits only the project's
  own `CLAUDE.md` and `.claude/CLAUDE.md`, each only while it is there. It
  never makes one, as it never makes a `CLAUDE.local.md`: a `CLAUDE.md`
  Mindweaver made would itself turn Claude's native read off. It never edits
  a file above the project, which is reported as the fallback's reason
  (`Fallback::Above`) instead: Claude reads it in every project below, and
  it is nobody's to edit for one of them. Nor a file reached through a
  link, whose import would mean whatever the link leads to; nor the user
  memory file in a project at the home; nor the `AGENTS.md` itself.
- **Added once, gone with the last rule.** The import goes into the file
  the reach names, and comes out of the project's other `CLAUDE.md`, so it
  is in one file at most, however many rules there are; once no block is
  written it comes out of both, and each file is as it was before.
  Replaced through a temporary and a rename, never through a link
  (`replace_bytes_unresolved`), and a file whose text a change leaves as it
  was is not written. Every file is read before any is written, so a
  malformed block refuses the whole render. An import no longer wanted in a
  file that may no longer be edited stays there, and is said
  (`claude_md::Warning`), never taken out behind the setting that keeps
  Mindweaver out of the file ([an open question](#open-questions)).
- **No lock of its own.** As the two renderers, it takes none: its caller
  holds the workspace's. Which rules a project is given and when to render
  are `install::rules`', as for the block.

### Installing rules

A rule is installed, and taken out, through the same `install::install`
and `install::uninstall` a skill is (`install::rules`), offered from the
workspace's own rules and from a load's, by the route it is filed under:
held to the rules every name is, as the block holds it (`name_issues`),
rather than to one folder name, and two places offering one route are
refused as for a skill (`OfferedTwice`), `--from` saying which. A rule is
never gathered (#111), so the shelf offers none. The command line, the
install screen and what a load is said to offer take rules as they take
skills ([which kind a name is](#which-kind-a-name-is)).

- **One render for every rule.** The block is one file, written whole, so
  a project marked by its settings is given its rules together: installing
  one, or taking one out, renders every rule the ledger records for the
  project, each from the offer it was installed from, as it is there now,
  into the block ([how a rule reaches the
  agents](#how-a-rule-reaches-the-agents)), then the import or none in its
  `CLAUDE.md` ([how Claude reaches the block](#how-claude-reaches-the-block)),
  then Claude's rule files in the mode that reach says ([Claude's rule
  files](#claudes-rule-files)). `install::rules::render` does the same with
  nothing installed or taken out, for a front end to bring the three in
  line with a project as it is now. Rendering only the rule asked for was
  turned down: the block has no part of its own to write, and leaving the
  other sections as they stand would need each read back from the file,
  which is what an edit there changes. A section somebody edited is the one
  read back and kept (below).
- **One installation a rule, a hash a target.** Its targets are its
  section, recorded by the file it is in, `AGENTS.md`, and its file for
  Claude, each with the hash of what it holds (schema 4, [why the ledger is
  SQLite](#why-the-ledger-is-sqlite)): the section's is `Section::hash`, of
  its lines whatever ending the file uses, and the file's `hash::bytes`. A
  rule given neither, one about some files in a project that does not name
  Claude, is recorded with none, and said to reach no agent
  (`Warning::Unreached`). The installation's own hash is of the rule as it
  was rendered, what its standing compares the offer with: outdated once
  the two differ ([an edited copy is left
  alone](#an-edited-copy-is-left-alone)).
- **`AGENTS.md` goes only if Mindweaver made it.** Whether the render made
  the file is noted in the ledger (`created_files`) and handed back to the
  next render, so the last rule out takes away a file Mindweaver made and
  leaves one somebody wrote, as it was before the block. A file Mindweaver
  made that somebody then wrote in is theirs once no block is left in it,
  and so is one git tracks: somebody committed it.
- **Local mode, and a file the team owns.** In a local project
  ([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)) an
  `AGENTS.md` Mindweaver makes is kept out of git before it is made, an
  entry in the project's group of the exclude block, taken out once the
  file is no longer Mindweaver's; one somebody had is theirs to commit, and
  gets none. One git tracks is written only in a shared project or with
  `edit_tracked`. Otherwise no block is written there: Claude has every
  rule from its rule files, since the reach of no block is the fallback,
  and the agents that read rules only in the block, Codex and OpenCode
  among those the project names, are said to read none of them
  (`Warning::Withheld`), while the project has a rule the block would have
  held: with none left, or only rules about some files, which reach Claude
  alone anyway, nothing is missed. A block written there before, by a
  project shared then, is left as it is, and said (`Warning::Left`), as an
  import left in a tracked `CLAUDE.md` is. A rule whose section it holds is
  not uninstalled meanwhile (`InstallError::Withheld`): its section would
  stay, with no record, and the next render that may edit the file would
  take this workspace's own section for somebody's. Sharing the project,
  setting `edit_tracked`, or taking the section out by hand lets it go.
- **Somebody's section is never written over.** A section in the block
  that the ledger does not record was written by another workspace that
  links the project, or by a hand that copied a marker, and rendering the
  block whole would take it out. So it makes its rule `Standing::Foreign`,
  as a file at a rule's place in Claude's folder that nothing records does,
  and refuses every render of the project's rules until it is gone
  (`InstallError::NotRecorded`). Taking it for this workspace's was turned
  down: two workspaces, each with its own ledger, would take turns taking
  each other's rules out.
- **An edited section is kept.** A section this workspace rendered whose
  hash is not the one recorded was edited since, and a render keeps it as
  the block holds it (`AgentsMd::section`), with its record, so it reads as
  edited still, and says so (`Warning::Kept`), as Claude's renderer leaves an
  edited file. What the rule was rendered from stays recorded too, so once
  the edit is undone it reads as outdated rather than current. Installing or
  uninstalling that rule is refused, as for any edited copy, unless forced:
  then its section is rendered from its offer, or taken out, and its file
  for Claude is written over or taken away. A section that cannot be read
  back, a line in it that is not UTF-8, refuses the render
  (`InstallError::Unread`) rather than go with it. Replacing the whole block,
  as every render did before #47, lost the edit the next time any rule of
  the project was installed.
- **A rule no longer offered refuses the render.** A rule whose origin
  offers it no more (deleted from the library, a load gone) cannot be
  rendered, and leaving its section out would take it from every agent,
  which is what the block refuses a rule it cannot read for. So the render
  is refused (`NoLongerOffered`) until it is put back, or uninstalled,
  which renders the rest without it. An uninstall of another rule goes
  ahead past it, keeping it as the project holds it: its section as the
  block holds it (`AgentsMd::section`), an edit included, its file for
  Claude where it is, and its record as it was. Taking one rule out is no
  reason to take another, and refusing would leave two rules no longer
  offered each keeping the other in, with nothing else that drops a rule's
  record. An update goes past it the same way (`rules::update_held`),
  saying it is orphaned rather than refusing to bring the rest up to date
  ([bringing what was installed up to
  date](#bringing-what-was-installed-up-to-date)).
- **Refused before anything is written.** What can refuse is asked first:
  a block that cannot be read or holds somebody's section, a place no rule
  is written (`agents_md::misplaced`), an `AGENTS.md` linked out of the
  project, a `CLAUDE.md` whose block cannot be read (`claude_md::reach`), a
  rule linked out of the workspace offering it (`InstallError::Outside`).
  The block is written before the import and the files; one of those that
  fails after it records what was written, then is raised, and the next
  render finishes it.
- **Recorded before it is written.** Before the first write the ledger
  holds the rule being installed, and, for each rule's file for Claude that
  nothing is at, the hash of what the render would put there (and whether
  the render will make `AGENTS.md`). A render killed part way then leaves
  files the next one knows for Mindweaver's, rather than taking each for
  somebody's and leaving it where Claude reads it. A record of a file that
  was not made says nothing wrong: the next render makes it, or forgets it.
  A file replaced and not yet recorded reads as edited, and is left, until
  a render finds it holding its rule as written again, which makes it
  current. An uninstall keeps its record until the render is done, so one
  killed part way is run again.
- **A project a `mind.toml` marks gets a copy.** It names no agents and
  has no block, so a rule is copied into the folder its marker names, the
  file as written, filed by its route with its folders made, and recorded
  as a skill's copy is, its target with the same hash as the installation.
  An unrecorded file there is somebody's, and refuses the install, as for a
  skill; an uninstall removes the file its record names, and the folders
  its route was filed in that this leaves empty, as removing a rule does.
  Moved to its settings, the project keeps that copy and its record until
  the next render of its rules, an uninstall's included, which takes the
  copy out of the folder the project authors in while it holds what was
  copied, as a change of agents takes out an untouched copy, and leaves one
  changed since where it is, its author's, its record gone and said
  (`Warning::Edited`). One that cannot be taken out stays recorded for the
  render after (`Warning::Unremoved`).
- **Held by the workspace's lock**, from before the records are read until
  the last is written, as an install of a skill holds it; none of the three
  renderers takes it.
- **A change says what it found, once.** What the three renderers and the
  render as a whole have to warn of is gathered in core
  (`Rendered::warnings_of`), so no front end says one renderer's and forgets
  another's. Not what holds of another rule than the one installed or taken
  out whatever the render, that it reaches Claude alone or no agent: that
  was said when the rule went in, `status` says it whenever asked, and saying
  it on every render made installing one rule warn, and fail, for another.

## One entry of a config file somebody else owns

An MCP server reaches an agent as one entry in that agent's own config file
([D-10](#d-10-an-mcp-server-is-one-owned-entry-per-agent)): `.mcp.json`,
`opencode.json`, `.codex/config.toml`. Those files are the team's, the user's
and the agents' own CLIs', so `config_entry` changes the one entry it is asked
to and keeps every other byte. It knows no agent: which file, which key path
and what an entry holds are the caller's, and the agents' shapes are #90 to
#92's: Claude's `.mcp.json` is the first to call it ([Claude Code's
`.mcp.json`](#claude-codes-mcpjson)), Codex's `.codex/config.toml` the
second ([Codex's `.codex/config.toml`](#codexs-codexconfigtoml)), OpenCode's
`opencode.json` the third ([OpenCode's `opencode.json`](#opencodes-opencodejson)),
and an install of a server writes and takes out each through its agent's
renderer ([installing an MCP server](#installing-an-mcp-server)).

An entry is found by its key path, the keys of the objects that lead to it
(`["mcpServers", "github"]`), and holds a `config_entry::Value`. Its own type
rather than `serde_json::Value` or toml_edit's, because one entry is rendered
into both formats and neither can stand for the other: JSON has a null that
TOML lacks, and TOML keeps integers and floats apart where JSON does not. A
value a format cannot write is refused there, not bent into something it was
not. Arrays are not walked into: an entry Mindweaver owns is a member of an
object, which is what lets it be told apart by name from everybody else's.

### Edited in a tree that holds every byte

JSON and JSONC go through jsonc-parser's CST (its `cst` feature), TOML through
`toml_edit`, the crate the markers are edited with. Both hold the whole text,
comments and whitespace included, and change one node of it. Parsing into a
value and serializing it again, as `serde_json` would, writes the whole file in
the library's layout and drops every comment. The agents' CLIs are no way
round it either: D-10 turned them down, since `codex mcp add` writes only the
user's config and re-serializes every entry in it.

The format is the caller's to say, never guessed from a file's name: OpenCode
reads `opencode.json` as JSONC. JSON is held strict, because Claude reads
`.mcp.json` with a strict parser, so a file with a comment or a trailing comma
is refused rather than edited, and one without stays without. JSONC allows
comments and trailing commas and nothing more: jsonc-parser's defaults take
every JSON5 extension (single quotes, bare keys, hexadecimal numbers), which
the agents' parsers refuse. The text is parsed once without the CST to hold it
to that, because in 0.34 the CST's own parse lets a comment through
`allow_comments: false`. Two things no option of its covers are refused in
JSON by a scan of the text once it has parsed: a control character written as
itself in a string, and whitespace other than space, tab, LF and CR between
tokens, both of which `JSON.parse` refuses. `loose_json` holds a text to the
same scan outside its comments, once it reads as JSONC, for a caller that
says a file JSON but for comments and trailing commas apart from one no
reader takes.

Where a library keeps less than every byte, the module makes up the
difference, and the tests pin what is left:

- toml_edit files the comments above an entry with the entry, so removing it
  would take them, where jsonc-parser leaves them. A TOML entry with comments
  above it is first replaced by a one-line stand-in that keeps them, marked by
  a comment nothing else in the file holds, and that line is cut from what
  toml_edit writes. Dotted keys file them with the last key of their first
  line, which a replace carries over to the new first line.
- toml_edit writes every line ending as LF. A file whose lines all end in CRLF
  has them put back, and one that mixes the two is refused, since which line
  had which would be lost. jsonc-parser keeps the file's own. toml_edit also
  ends the last line and drops a byte-order mark: a file whose last line had
  no ending is left without one, and a mark is put back.
- A string with a line break is written on one line, the break escaped, where
  toml_edit would open a multi-line string: in a CRLF file its breaks would
  then be the file's line endings to the file and part of the value to its
  reader. A float is written as the fingerprint writes it, in both formats,
  where the libraries write every digit of `1e300`.
- A TOML object is laid out as the objects already beside it are: a `[table]`
  of its own after theirs, inline, or dotted keys. Anything else is a key after
  the keys beside it, indented as the one before it.
- jsonc-parser's own `append` lays the object an entry goes into out again
  first: it drops the blank lines just inside the braces and breaks an object
  written on one line onto several. So a JSON insert takes only its place from
  the CST, after the last member and the comment on that member's line, and is
  spliced into the text there, laid out as jsonc-parser lays out what it is
  given: on a line of its own, indented as the last member, in an object
  written on several lines, and on the same line in one written on one. A
  replace and a remove go through the CST, which changes nothing outside the
  member.

### What an edit is held to

What an edit wrote is read back before it is handed over: it must parse, the
entry must hold what was asked (nothing, for a remove), and everything else
must hold what it held before, compared as values (`config_entry::kept`). A
library that changed more than the entry is then a refusal, not a file somebody
has to repair. The objects on the entry's way that it leaves empty are left out
of that comparison, because the formats differ there: jsonc-parser keeps
`"mcpServers": {}` when its last entry goes, and toml_edit drops a
`[mcp_servers]` that was never written, only implied by `[mcp_servers.github]`.

A key written twice on the way to an entry, or as the entry, is refused rather
than one copy picked, since which one an agent reads is its parser's. A replace
with what the entry already means hands the text back untouched, so installing
again over an entry somebody reformatted leaves their layout alone.

### The fingerprint

An entry can carry no mark of its own: OpenCode refuses keys it does not know,
`codex --strict-config` fails on them, and `.mcp.json` allows no comments. So
what the ledger keeps of an entry Mindweaver wrote (#93) is its fingerprint,
`hash::fingerprint`, the recipe's one definition beside a copy's `hash::tree`
and a file's `hash::bytes`: the SHA-256 of the entry's canonical JSON form,
which each renderer's `Entry::fingerprint` takes of the entry it makes and
`Value::fingerprint` of one a file holds, as
[RFC 8785](https://www.rfc-editor.org/rfc/rfc8785), the JSON Canonicalization
Scheme, writes it. Members sorted by UTF-16 code units at every depth, no
whitespace, strings escaped only where JSON must be, and numbers written as
ECMAScript writes a double, so `1`, `1.0` and `1e0` are one value. A published
scheme rather than one of ours, and one that keeps no integer apart from a
float: a file Claude rewrites goes through `JSON.stringify`, which keeps
neither, so anything stricter would call an entry nobody changed edited. It
departs from the scheme only for what JSON cannot spell. An integer past 2^53
is written digit for digit, since TOML's are 64-bit and two of them must not
read as one, and a float that is not finite as ECMAScript names it.

The fingerprint stays the same whatever a file does to an entry's layout (its
indentation, the order of its keys, a table or an inline table, its quotes) and
changes with any value. So a reformatted file still holds Mindweaver's entry,
and an edited one holds its editor's. A digest rather than the canonical text,
so every row is as long as every other, as a copy's `hash::tree` is.

### The file around the entry

`ConfigFile` reads the text from a file and writes it back. The file is named
by a route of plain folder names from a folder the caller trusts, a project's
root, and the route is checked by its spelling before anything is read, as a
project's artifact directories are kept inside it. A symbolic link anywhere on
that route, the file included, is refused, for a read as for a write. An
agent's folder, `.claude/` or `.codex/`, is somebody's to point anywhere, and a
gathered repository can hold one that points at the user's own `~/.claude`: an
entry written through it would land outside the project, in a file of the
user's, which the project's ledger row would then claim. The root itself may
be reached through a link, as a project often is. The route is looked at when
the file is read and again just before it is written, since a folder on it may
have become a link in between, and the write goes to the path as spelled,
never resolved: `replace_bytes`, which the markers go through, resolves it
first, to follow a config somebody shared by a link
([open questions](#open-questions)).

A file that does not parse is refused and left as it is: what it was meant to
say is its owner's to settle. A changed file is written whole through
`replace_bytes_unresolved`, a temporary of its own and a rename, with its
permissions kept, so a crash leaves the old file or the new one. A file not
there yet is
made, with the folders on its route, by `create_dir` and `create_new`, where a
rename would put it over one an agent's CLI made after the read found none. A
replace that changes nothing writes nothing, modification time included.

A caller may bound what a file is read for (`ConfigFile::capped`), and a
file past it is refused before it is held in memory (`config_too_large`),
counted as it is read rather than by the size the file says it has. The
bound is the caller's, as the format is, since how large a file of its
agent's can be is the caller's to know.

A file is taken away only through `ConfigFile::take`, and only when its
caller says what it holds is nothing it keeps: which documents those are
is the caller's to say, as the key path is, since `{"mcpServers": {}}` is
nothing in `.mcp.json` and something in a file that keeps other settings
beside it. `config_entry::holds_only` answers it of the document for a
caller whose file keeps nothing of Mindweaver's but entries: each object on
the way to the entry holding the next key alone. The document holds no
comment, so `take` hands its caller the text as well, and a caller whose
format has comments asks the text: one somebody wrote is something.
`codex_mcp` takes `.codex/config.toml` only when taking the entry out of
its text leaves it blank, and `opencode_mcp` its JSONC only when what that
leaves is the objects on the way, empty, and no comment
(`config_entry::jsonc_commented`, read where the parser finds comments,
so a `//` in a string is the string's). It is read as an edit reads it, so a file that does not parse is
refused rather than judged, and the route is looked at again before the
file goes, so one that has become a link is refused and what it points at
kept.

The caller holds the workspace's lock, as an install does, so two of
Mindweaver's writers take turns over one file. `ConfigFile` takes none: the
lock is not re-entrant, and nothing here knows about workspaces.

### Claude Code's `.mcp.json`

`claude_mcp` (#90) gives Claude a server as one entry of the project's
`.mcp.json`, `mcpServers.<name>`, written and taken out through
`ConfigFile`, every byte outside it kept. What it builds on is #18's runs
on Claude Code 2.1.318 ([docs/agents.md, Claude Code's
`.mcp.json`](agents.md#claude-codes-mcpjson)), which it takes as they
came out rather than deciding again.

The entry (`claude_mcp::entry`) is a program's `{type, command, args, env}`
or an address's `{type, url, headers}`, every key written whether or not it
holds anything, so a definition has one entry and one canonical form, the
form whose digest the ledger will keep (#93). `type` is always written:
Claude drops an http entry without one, alone, and one somebody else wrote
so is no reason to refuse the file. Each input is written where its
placeholder stands as `${VAR}`, the variable it is read from (D-11), which
Claude expands in `args`, `env`, `url` and `headers`, inside a longer value
too, in one pass from the left, so a reference is never read again as a
placeholder: `${A}` holds `{A}`, which another input may be named. A brace
naming no input stays as it is written, and a `${` written as it is, which
Claude would read as a reference, refuses the entry (`text_expanded`,
[what each agent cannot be given](#what-each-agent-cannot-be-given-of-a-server)).
The command is written as the definition holds it: no input's kind puts
one there, and that Claude expands a reference in it as well is its docs'
word and its binary's, which no run tried. Claude's extras are passed on
beside the entry, never over a key it writes (`extra_repeated`, a shape
of the report, since `claude_mcp::keys` says which keys those are to
`McpDefinition::inexpressible` too), and a TOML date and time as its
text, as `config_entry` reads one.

What Claude cannot be given is `McpDefinition::inexpressible`'s to say,
asked before the file is read, and never a second copy of its rule: a
refusing shape refuses the entry with the shape's own code, and a program's
`cwd` is left out and said. So is sse, written with a warning that Claude's
docs call it deprecated (`claude_mcp_sse_deprecated`). A definition
`validate` calls invalid is refused before anything else
(`claude_mcp_unfit`), so a token written as it is never reaches a file, and
so is an input read from no variable (`claude_mcp_input_unread`), which an
entry could be given only as its value, and which `show` does not say:
it is no agent's limit, but a value no install hands the renderer yet.

The file is read as strict JSON, as Claude reads it, and up to 2 MiB
(`claude_mcp::MAX_BYTES`): a project's servers come to a few KiB. A file
holding a comment or a trailing comma loses Claude every server in it, so
it is refused saying so (`mcp_json_not_strict`) and left as it is, rather
than given an entry Claude would never load or rewritten into one it would,
which is its owner's to do; it is told from a file no reader takes, which
keeps `config_malformed`, by being read again as JSONC and held, outside
its comments, to the scan strict JSON is held to
(`config_entry::loose_json`): a raw control character in a string or a
no-break space between tokens is no looser reader's either, and a JSONC
read alone would take both. No approval is
written: a person approves a project's servers when Claude asks, and a `-p`
run starts every server the file names, approved or not.

`render` replaces an entry of the name where it stands or adds it after
the servers there, making the file with `create_new` when it is not there
yet (`Written::Created`, which the caller keeps), and leaves a file that
already holds the entry as it is. `remove` takes the entry out, and the
file with it when the caller says Mindweaver made it and nothing else is
left, no other server and nothing beside them, in one step rather than a
write and then a removal, as `agents_md` takes away an `AGENTS.md` it made.
Whose entry it is, and whether Mindweaver made the file, are the ledger's
to say (#93), and the install's to ask ([installing an MCP
server](#installing-an-mcp-server)): nothing here reads either.

### Codex's `.codex/config.toml`

`codex_mcp` (#91) gives Codex a server as one table of the project's
`.codex/config.toml`, `[mcp_servers.<name>]`, written and taken out
through `ConfigFile` in TOML, every byte outside it kept: the team's
settings, their tables and servers, and the comments above and beside
each. What it builds on is Codex's documentation, docs-only, since Codex
was never run here ([docs/agents.md, Codex's
`.codex/config.toml`](agents.md#codexs-codexconfigtoml)): every
`codex-mcp-*` result stands pending, and the renderer takes the facts as
they stand rather than waiting on a run.

The entry (`codex_mcp::entry`) is a program's `command`, `args`, `env`,
`env_vars` and `cwd`, or an address's `url`, `http_headers`,
`env_http_headers` and `bearer_token_env_var`, each written only when it
holds something, where Claude's writes every key: a `cwd` or a bearer token
has no empty value to write, and an empty `env` would be a table of its own
holding nothing. A definition still has one entry and one canonical form
(`Entry::canonical`), its members sorted at every depth, as every
fingerprint is: `codex mcp add` writes every entry it keeps again, and a
person may write one inline or as tables, so an entry laid out otherwise is
still the one Mindweaver wrote, and a replace with what it already means
leaves their layout alone. Codex's extras are passed on beside it.

Codex has no interpolation, so an input is written only where a key names
its variable: a program's environment variable that is the input whole,
under the variable's own name, goes to `env_vars`, a header that is the
input whole to `env_http_headers`, and the first `Authorization` header
holding `Bearer` and the input to `bearer_token_env_var`. Every other value
is written as the definition holds it, `${VAR}` among it, which means there
what it says. What Codex cannot be given is `McpDefinition::inexpressible`'s
to say, asked before anything else, never a second copy of its rule: a
secret anywhere else, sse, and extras over a key the entry may write or
naming the environment Codex runs a server in refuse the entry, each by
the shape's own code ([what each agent cannot be
given](#what-each-agent-cannot-be-given-of-a-server)). A definition
`validate` calls invalid is refused first (`codex_mcp_unfit`), and so is an
input read from no variable (`codex_mcp_input_unread`) or, no secret,
written where Codex reads none (`codex_mcp_input_in_text`), which an entry
could be given only as its value, and which `show` does not say.

The user's own servers, in `$CODEX_HOME/config.toml`, are the layer a
project's is merged over, key by key, so a name both hold would be read as
one server made of both. That file is read and never written, from the
Codex home `AgentHomes` gives and nowhere else, through a link, as a
person's settings kept in a repository of theirs are linked in, and up to
the 2 MiB the project's file is; a name it holds under `mcp_servers`
refuses the entry (`codex_mcp_collision`), before the project's file is
read. Refused rather than warned of, as #91 planned: which it should be is
what `codex-mcp-collision` would record, never run, and the outcomes that
call for a refusal break more than the one server, so until a run finds the
merge leaving the rest of the configuration working, a refusal is the side
to be wrong on ([open questions](#open-questions)). With no Codex home
given there is no user's config, and nothing collides, as in a home holding
none.

Codex reads a project's file only in a project the user's config marks
`trusted` (`[projects."<path>"] trust_level`), so an entry written into one
it does not is written all the same, and said (`codex_mcp_untrusted`, in
`Rendered::warnings`, apart from what is said of the entry): trusting a
project is the user's to do, and Codex asks them, where a refusal would
leave the project without a server Codex reads once it is trusted. The path
looked up is the project's root as it is handed and as its links resolve
to, either counting, since the folder a program starts in is reported to
it with its links resolved; which path Codex keys a project by, the folder
it starts in or the repository's root above it, is docs-only, so a project
trusted under another is warned of, a warning wrong at no cost to the
entry. With no Codex home given nothing marks it trusted, and that is
said.

`render` makes the file with `create_new`, `.codex` with it, when it is not
there yet (`Written::Created`, which the caller keeps), and refuses a
project whose `.codex/config.toml` is inside an agent's own folder
(`codex_mcp_user_scope`), one at the user's home while Codex's is
`~/.codex`, where the file is the one Codex reads for every project, as an
install refuses a copy there. `remove` takes the entry out, and the file
with it when the caller says Mindweaver made it and nothing else is left,
as `claude_mcp::remove` does. That is asked of the text, not of the
document `config_entry::holds_only` reads, which holds no comment: what
taking the entry out leaves must be blank, so a comment somebody wrote
anywhere outside the entry keeps the file. `.codex` stays,
whatever is left in it: it is Codex's own folder, which may hold what else
Codex keeps for the project, and no record says Mindweaver made it. Whose
entry it is, and whether Mindweaver made the file, are the ledger's to say
(#93), and the install's to ask ([installing an MCP
server](#installing-an-mcp-server)): nothing here reads either.

### OpenCode's `opencode.json`

`opencode_mcp` (#92) gives OpenCode a server as one entry under `mcp` of
the project's config, written and taken out through `ConfigFile` in
JSONC, every byte outside it kept, the team's comments and trailing
commas among them. What it builds on is #18's runs on OpenCode 2.2.5,
2.1.3 and 1.19.4 ([docs/agents.md, OpenCode's
`opencode.json`](agents.md#opencodes-opencodejson)), which it takes as
they came out, and OpenCode's docs where no run looked.

The file is the first of `opencode.json`, `opencode.jsonc`,
`.opencode/opencode.json` and `.opencode/opencode.jsonc` there is at the
project's root, in #92's order, and `opencode.json`, made with
`create_new`, when none is. Whatever stands at one of the four routes
counts, a link or a folder too, so that `ConfigFile` refuses it rather
than a server being written into another file beside it; a `.opencode`
that is no folder holds none of them. Each is read as `Format::Jsonc`,
the `.json` files too, rather than as the format its name would say:
OpenCode's docs say it reads JSON and JSONC alike, so a `.json` holding a
comment is a file OpenCode loads, which strict JSON, as Claude's
`.mcp.json` is held to, would refuse to touch. What Mindweaver makes is
strict JSON all the same, holding no comment. Up to 2 MiB
(`opencode_mcp::MAX_BYTES`), as the other two are read.

The entry (`opencode_mcp::entry`) is D-12's minimal V2 shape,
`mcp.servers.<name>`: a program's `{type: "local", command: [command,
...args], environment, cwd}` or an address's `{type: "remote", url,
headers}`, each key after the command or the address written only when
it holds something, as Codex's are, and no `codemode`, `protocol` or
`timeout.startup`, on which V1 warns, nor a key of Mindweaver's, since an
entry takes none OpenCode does not know. Nor `disabled`, which #92 gives
a program's V2 entry: a definition has no switch for it, so the least
entry leaves it out, and an extra giving it is refused, as one giving
`enabled` is (`extra_repeated`). V1's shape, `mcp.<name>` with
`enabled`, is written only into a file that keeps a server so already:
`mcp` holding a member other than `servers` and `timeout`, the two keys it
holds itself, which `mcp::name` refuses a server's name for, so neither is
ever taken for a server of V1's. A server of the name is replaced in the
shape it stands in, where it stands, rather than written beside itself in
the other, and one written in both is refused (`opencode_mcp_twice`), by
`render` and `remove` alike: which of the two OpenCode reads cannot be
told. Each shape has one canonical form (`Entry::canonical`), its
members sorted, and `Entry::shape` says which, for the ledger's record
(#93) to find it again. V1's shape has no key for a program's folder, by
V1's docs, so `cwd` is left out of an entry written in it and said
(`opencode_mcp_v1_cwd`), a warning of the renderer's rather than the
report's, since the report is of V2's shape, which takes it, and a
definition is given V1's only for the file it goes into.

Each input is written where its placeholder stands as `{env:VAR}`, the
variable it is read from (D-11), through the pass Claude's `${VAR}` is
written in (`mcp::referring`), since OpenCode fills it in in every
place, the command's first word and inside a longer value included. The
first word and the folder are written as the definition holds them, no
input's kind being written there. What OpenCode cannot be given is
`McpDefinition::inexpressible`'s to say, asked before anything else:
sse, which V2's shape does not take, text it would fill in where no input
stands, `{env:` or `{file:` written as it is (`text_substituted`), and
extras over a key either shape may write (`extra_repeated`), each by the
shape's own code. A definition `validate` calls invalid is refused first
(`opencode_mcp_unfit`), and an input read from no variable after the
report (`opencode_mcp_input_unread`), as the other renderers refuse them.
Nothing is said of whose environment V2's background server fills the
references from: that is the owner's open question ([open
questions](#open-questions)).

`render` makes the entry first, in V2's shape, so a definition refused
reads nothing, and again in V1's when the file keeps its servers so, and
writes it in place of a server of the name or after the servers there,
leaving a file that already holds it as it is (`Written::Unchanged`). It
refuses a project whose file is inside an agent's own folder
(`opencode_mcp_user_scope`), before reading it, as `codex_mcp::render`
does: one whose root is `~/.config/opencode` while that is OpenCode's,
where `opencode.json` is the config OpenCode reads for every project,
which #92 leaves out, as an install refuses a copy there.
`remove` takes the server out of whichever shape it stands in, and the
file with it when the caller says Mindweaver made it and nothing else is
left, asked of what taking the server out of the text leaves: the objects
on the way to it, empty, and no comment (`config_entry::jsonc_commented`),
since the document `config_entry::holds_only` reads holds none, and one
somebody wrote is something. Only `opencode.json` is ever one Mindweaver
made, so another of the four chosen once it has gone stays whatever it
holds, and `.opencode` is never taken away. `remove_from` takes a server
out of the one of the four its record names, rather than the first there
is, which another may have come before since: taken out of the first, it
would be a server of the name somebody keeps there, and the one written
left behind. Whose entry it is, and whether Mindweaver made the file, are
the ledger's to say (#93), and the install's to ask ([installing an MCP
server](#installing-an-mcp-server)): nothing here reads either.

### How an entry stands

The ledger records each entry an MCP server's installation writes, one an
agent: the agent, its file as a route from the project (`Agent::mcp_file`
when it was written), the server's name it is keyed by, and its
fingerprint ([why the ledger is SQLite](#why-the-ledger-is-sqlite), schema
6). Since an entry carries no mark, that record is the one thing that makes
it Mindweaver's (D-10), and how it stands is told by three things together
(`install::entries`): the record, what the agent's file holds under the
name, by its fingerprint, and what the place it came from offers now,
rendered for that agent by its own renderer (`Entry::fingerprint`), in the
shape OpenCode's file keeps the server in. Neither the file alone nor the
offer alone tells it, as for a copy (D-04): an edited entry can be what is
offered now, and an untouched one behind it.

It stands in `install::Standing`'s words, said in the same order, edited
first, then orphaned, missing and outdated, and each means of an entry
what it means of a copy. Six of them are the issue's (#93): absent,
current, outdated, edited, foreign and missing; orphaned is the seventh,
as for a copy, since an entry whose definition nothing offers any more has
nothing to be current or outdated against, and an update nothing to write
it from. What differs is how each is told:

- **Foreign** by the server's name, where a copy is told by its path: an
  entry the agent's file holds under a name nothing records an entry of in
  that file for that agent. A file holds every server of the project, so
  its path tells no entry from another.
- **Edited** by the fingerprint of what the name holds ([the
  fingerprint](#the-fingerprint)), where a copy is told by its tree's hash:
  an entry laid out again by Codex or by hand is current still, and one
  whose value changed is somebody's work.
- **Missing** while the file the record names holds nothing under the
  name, is not there, or cannot be read, which is said beside it with the
  renderer's code (`install::ConfigUnread`), as a block that cannot be read
  holds none of a project's rules and the next render says why.
- **Outdated** while what is offered now is given to the agent as another
  entry than the one written, by its fingerprint, or as none, which an
  install of it would then be refused for. Told an agent at a time, so a
  change to what one agent alone is given, its extras, leaves the others
  current.

An installation stands as the first of those its entries do, edited,
orphaned, missing, outdated, and current otherwise
(`install::standings`), which is what an update's survey reads of it. Each
file is read as its renderer reads it to write into it
(`claude_mcp::servers`, `codex_mcp::servers`, `opencode_mcp::servers`):
Claude's strictly, OpenCode's as JSONC, each up to 2 MiB, so a file an
install would refuse to write into is refused here too, with the same
code, and a recorded entry is looked for in the file its record names,
OpenCode's whichever of its four that was. `install::entry_standings` says
how one server stands for each of a project's agents, absent and foreign
among them, which is what an install meets in each file ([installing an
MCP server](#installing-an-mcp-server)).

## Creating and changing artifacts

`mind add` writes the least that passes `validate`: a skill's front matter and
a heading, a rule's opening line, and an MCP server's definition from how a
command line says it is started or reached ([adding
one](#adding-an-mcp-server)). The name is checked by
`artifact::name_issues`, the function `validate` itself uses, or a server's
by `artifact::mcp_name_strict_issues`, so a name the tool creates is one
the tool accepts. The front matter is serialized rather than
formatted, because a description is free text, and one containing `: ` would
otherwise be written as YAML that does not parse back into what was typed.

Nothing is written over. A skill's folder is made with `create_dir`, so the
check that it is not already there and the making of it are one step, and a
folder that exists but is not a skill counts as there: it is somebody's. What
was written is read back rather than trusted.

A created artifact is its author's work, so it is not recorded in the ledger.
To `install` it is foreign: never overwritten by a shelf entry of the same
name, never removed by unticking one.

### Templates live inside the markers

`.mind/templates/` and `.mindweaver/templates/`, not beside the code: the
artifacts are beside the code because agents read them, and nothing but
Mindweaver reads a template. A workspace's templates count for the projects it
manages and for its own artifacts; of two templates of one kind and name, the
closer wins.

Which workspace that is, the command says. Core takes it as a parameter
(`template::templates`, `template::find`) rather than walking up from the
project, because a walk misses a workspace that links the project from beside
it, and `mindweaver add -p` run there would not find the workspace's own
templates. `mindweaver` passes the workspace it runs in. `mind` passes what
`resolve::consulted` resolves from the project: the one `-W` names, then the
activated one, then the one above the project, then the default one in your
home if it has been made, without making anything. `-W` is how a workspace
beside the project lends it templates from inside it. The TUI's add form
offers the templates its line will find. On a workspace's screen those are
the workspace on screen's: a row whose project would consult another runs
`mind -W . -C <entry>` to reach it (see [the TUI looks where it was
told](#the-tui-looks-where-it-was-told)). Core still checks that the workspace manages the project, so one
that does not lends it nothing.

A workspace `mind` cannot read refuses `mind add --template`, since passing it
over would let a farther template of the same name, a built-in one say, stand
in for its own. For `edit`, `rename`, `remove`, an add from nothing and `mind
templates`, one found above that cannot be read is a warning after the
change, since there the workspace is only told, and `mind templates` lists
what it could read. One named or activated that will not open refuses all of
them, as it refuses every other command (see [a project's
workspace](#a-projects-workspace)).

A skill template's front matter keeps whatever it says except `name` and
`description`, which `mind add` always sets: the template's lines for those
two are dropped, continuation lines and all, before anything parses the rest,
so a template may write `name: {{name}}`, which is not YAML anybody could read,
and still work. The result is parsed back before the first file is written, so
a template with broken front matter costs nothing but the error.

A rule template is filled in throughout, front matter included, so it may
scope its rule to `{{name}}/**`. Whether its fences hold front matter is
decided as for any rule, but on what they hold once filled in, since
`{{name}}/**` is a YAML flow mapping until it is. A `description` the front
matter declares is set as a skill's is, dropped and serialized afresh: pasted
in as text, a description breaks the YAML at its first `: `, and one opening
with `#` is a comment that declares nothing. The result is read back before
anything is written and refused for whatever `validate` would warn about
(`artifact::rule_issues`): front matter that does not parse, a `paths` or a
`description` it cannot read. Reading a rule takes those as content or passes
over them, because a rule somebody wrote still says what it says; writing one
is the moment to find out, and a template's `paths:` that became content would
scope nothing and say so nowhere.

A rule template that does not say where `{{description}}` goes gets it as a
heading on top of its body, below any front matter it declares. On top of the
file, the heading would push the fences down a line, where they stop being
front matter.

An MCP server's template is a definition, `<template>.toml`, found as a rule
template is but by its own extension (`Layout::Flat`), whose `name` and
`description` `add` sets through `toml_edit` and nothing else, as `rename`
sets a name ([adding one](#adding-an-mcp-server)).

### Finding what to change

`edit`, `rename` and `remove` find an artifact by what it declares, and then by
where its name would put it on disk (`lifecycle::find`). The second is what
lets a skill whose front matter does not parse be opened and fixed: it is left
out of every listing, and it is the one most in need of opening. Only a name
made of plain folder names (`install::is_folder_name`, segment by segment) is
turned into a place, so `../x` cannot reach out of the project. Not the naming
rules: they are for what Mindweaver writes, and a folder from before they were
tightened (`a--b`) or one made by hand is exactly what has to be found to be
fixed. A template is looked up by one folder name for the same reason, since
`mind templates` lists whatever is there. More than one match is an error: changing the wrong artifact is
not something to guess about. Both looks are only in the folders the project
authors in (`Catalog::discover_authored`): a listing also shows what sits in
a project's agents' folders, and none of that is the project's to change, so
a name found only there is refused (`NotAuthored`). So is a rule the project
only received, a section of its block, never authored in place
([D-23](#d-23-what-a-project-received-is-an-installed-view)): a front end
finds what to change through `lifecycle::find_to_change`, which refuses one
the ledger records as received (`Received`), saying how to take it out. And
so is an MCP server the project received, an entry in each of its agents'
config files written from the library's definition (`ReceivedServer`,
`change_received_mcp`), saying to uninstall it, or to change its definition
in the library it came from and update: a change made in a file is read as
an edit (D-04), never one the next install keeps. The hint names it as an
uninstall finds it, however it was typed: bare (`github`) while the ledger
records the name as no other kind a bare name could be, and qualified as a
server's (`mcp:github`) where it records a skill or a rule of the name
too, which a bare name would be refused for as two. A rule is asked of first,
and a name received as both is one a qualifier settles. The TUI lists unreadable files in the same way, by the name their place gives
them (`lifecycle::identify`), so they can be reached with a key rather than
only read about in a warning.

### Rename rewrites one line

A skill's name is in two places, its folder and its front matter, and they have
to move together or `validate` fails. The front matter is somebody's writing,
so it is not re-serialized: the one `name:` line is replaced and every other
byte kept (`frontmatter::with_key`). A name spread over several lines is not
touched, and neither is a manifest where the replacement would change anything
but the name: the result is parsed back and compared, field by field, before
anything is written. The folder moves first and the manifest is replaced
through a temporary file; if the second fails, the folder moves back.

### Remove is a dry run until it is told

`mind remove` without `--yes` deletes nothing and says what it would have, the
way `git clean` refuses without `-f`. It is the one command here that cannot be
taken back, so it is asked for twice: once by naming what, once by saying yes.
The TUI asks with a question and then runs the command with `--yes`, so the
transcript shows the yes that was given. A `mindweaver remove` without `--yes`
resolves its workspace as a read, as the dry runs of `update` and `migrate`
do, so a dry run in a home nothing has made a workspace in leaves none
behind.

### Removing one of the workspace's own everywhere

One of the workspace's own reaches a project by being installed there, so
removing it from the library alone leaves every copy installed from it with
a record nothing offers any more: orphaned, never updated again, and taken
out only by an uninstall somebody remembers to run. `mindweaver remove`
therefore names, beside a removal of one of the workspace's own, each
project that has a copy of it, in the dry run with what `--everywhere` would
do about it and after the removal with what takes them out now ([taking out
what nothing offers](#taking-out-what-nothing-offers)), and a warning, since
the copies left were not asked about. A removal that fails names none of
them: it may have left the library copy whole, offering it still, or a
skill's folder half deleted, offering nothing, and saying them orphaned, or
sending somebody to `--everywhere` against a folder no longer found, would
each be wrong half the time. `mindweaver remove --everywhere` is that
done for you: core's `everywhere::plan` says what it would do, reading the
ledger without making it, and `everywhere::apply` does it.

- **The copies first, then the library.** Each project the workspace links
  is surveyed for the artifact's name (`install::surveyed_one`), and each
  copy nobody has changed since it was installed is uninstalled as `mindweaver
  uninstall` would (`install::uninstall_with`, never forced): current,
  outdated, missing one of its copies, or orphaned already. The library copy
  goes last (`lifecycle::remove`), because a record from before hashes were
  kept is told from an edit only against its offer, which the library copy
  is.
- **D-04 decides what stays.** An edited copy is somebody's work: left, every
  copy of it, with its record, and from then on offered by nothing, which a
  note says with the uninstall that takes it out once somebody has looked.
  Somebody's own of that name, which nothing records (`install::somebodys`),
  is never Mindweaver's to take out, and is listed as foreign so it is not
  taken for a copy left behind.
- **Only what came from the library.** A record naming the workspace's own as
  its origin came from it. One installed from a loaded workspace or the
  shelf under the same name stays, offered still by where it came from. One
  from before origins were kept may have come from the workspace's own or
  from any loaded workspace, and counts as the library's only while no
  loaded workspace offers its name (`offered_for` matched against the
  library alone and against everything else): taking out what may be
  another workspace's would be a guess. The library copy is matched by its
  place rather than found among the offers, so one whose front matter no
  longer reads still finds what was installed from it.
- **Only the projects it links.** A project another workspace links, or none,
  keeps what was installed there, since the ledger that could say so is not
  this one; a project linked but unreadable is a warning, as for `update
  --all`.
- **The library copy goes whatever the projects came to.** A copy whose
  uninstall fails is an error of its project, said beside the rest, and the
  library copy is removed all the same: what is left is orphaned, as a copy
  left because it was edited is, and `uninstall --orphaned` finishes it.
  Keeping the library copy back would ask for the whole removal again to
  finish one project, and would make a failed project and an edited one two
  different leftovers of one command.
- **Each project's uninstall takes the lock for itself,** as `mindweaver
  uninstall --all-projects` does ([one artifact into many
  projects](#one-artifact-into-many-projects)), and looks again under it at
  what the plan saw: a copy edited between the two is left all the same,
  and said as left.
- **A rule goes the same way.** Its uninstall renders the project's block
  without it, or takes its copy out of a project a `mind.toml` marks.

The TUI's confirmation for `d` on one of the workspace's own offers a second
yes, `e`, which runs `mindweaver remove --everywhere --yes`, and shows the line
each key runs; on a project's own, which was installed nowhere, it keeps the
one yes. The panel removes one of the workspace's own only everywhere, and
shows the dry run first, `Task::Remove` without `yes` being the line
without `--yes` ([its changes](#its-changes)).

### A record must not outlive the copy it is about

The ledger's record of an installation is what gives an install leave to
overwrite and an uninstall leave to delete. Remove an installed skill and the
copy it was about is gone; a record left behind would be inherited by
whatever is written there next, and the install screen would then delete
somebody's work because a box looked untidy, or, now that a copy gone reads
as missing, put back what was removed on purpose. So `lifecycle::remove`
uninstalls the copy it reached, dropping its record, and `lifecycle::add`
drops any stale one under the name it writes, as `lifecycle::rename` does
under the new name.

Edit or rename an installed copy and it is still there. Its record stays
([D-04](#d-04-an-edited-copy-is-detected-not-disowned)), and the hash it
holds is what tells an edit apart ([an edited copy is left
alone](#an-edited-copy-is-left-alone)): edited, status says so, and no
install writes over it, nor uninstall takes it out, unless forced.
`finish_edit` leaves the record as it is and only reads the ledger to say
so (`Record::Kept`); `rename` carries it to the new name and place
(`install::carry`, `Ledger::renamed_copy`), from where it came and with the
hash of what was written. A skill's rename rewrites its `name` line, which
reads as the edit it is; a rule's moves its file as it is, so it reads as
it did, edited only if it was, and otherwise as what was installed, which
an uninstall takes out unforced. Which of the two it is is told as an
install tells it, and handed back (`Record::Moved`'s `edited`), so what
`mind rename` says of the copy is what an install and an uninstall will
find. Dropping the record, as both once did, made the
copy its author's: safe, but never updated again and never taken out by the
uninstall that put it there. Where another installation holds the new name
already, the renamed copy cannot join it, a name being one installation a
project, and its record goes instead, as it did (`Record::Dropped`).

Only a record that names the copy the change reached, by its path
(`install::disown`'s and `install::carry`'s `at`), not every record of that
name. A project marked by its settings authors in a folder of its own,
beside the copies installed for its agents: adding, editing, renaming or
removing its own `deploy` changes nothing in `.claude/skills/deploy`, and
seeing to that copy's record by name would drop or carry a record over a
change to something else. In a project a `mind.toml` marks, the folder it
authors in is the one it was installed into, so a change there reaches the
copy. A skill's record naming no target, which only a build from before
targets wrote, is dropped by name as records were. A rule's naming none is
one a build writes, for a rule that reaches no agent the project names
([installing rules](#installing-rules)), and a change to the project's own
rule of that route reaches nothing it installed, so it stays. So does an MCP
server's, which records an entry in each agent's config file and never a
copy ([D-10](#d-10-an-mcp-server-is-one-owned-entry-per-agent)): adding,
renaming or removing the project's own server of that name reaches none of
them, and dropping the record would leave what Mindweaver wrote there
somebody's, never updated or taken out again.

And only that copy's part of the record (`Ledger::uninstalled_copy`,
`Ledger::renamed_copy`), in one transaction: settings can make the folder a
project authors in one of its targets, `"skills": ".claude/skills"`, and a
change there reaches the copy an install put there and no other. The copy in
`.agents/skills` is still what was installed, so its target stays recorded,
for `mindweaver install` to update and `mindweaver uninstall` to remove; the
installation goes with its last copy, or takes the new name whole when a
rename reached its last. A renamed copy that leaves others behind is an
installation of its own under the new name, from the same place. In a local
project the place a removal reached also leaves the exclude block, after the
record, as an uninstall takes its entries out after its copies: what is
written there next is its author's, and a line left behind would hide it
from `git status`. A rename moves the entry to the copy's new place, which is
still an install's and kept out of git. A block that cannot be edited is
reported beside the change as a record that cannot be changed is
(`InstallError::StillExcluded`, `InstallError::NotExcluded`).

Core applies that rule inside each change. Dropping the record in the CLI
after the change, as it once did, put a rule outside core, and
[D-18](#d-18-the-panel-writes-by-running-commands) asks for the orchestration
left in the CLI to move into core before the panel runs its writes through
the CLI. Installed rules and MCP entries, when they come, would each have
needed calls of their own there, and each could have got them wrong. Each
change returns what it did to the record (`lifecycle::Record`), with what
opening the ledger to do it scrubbed out ([A URL is kept without its
credentials](#a-url-is-kept-without-its-credentials)), and the CLI only says
so; `install::disown` and `install::carry`, which change records, are
private to core so that nothing else can. That keeps a front end from
applying the rule differently, not from skipping it: `create` and
`create_from`, the writers `lifecycle::add` is built on, see to no record and
stay public for core's tests, so a front end adds through `add`. A record
that cannot be changed is reported beside the change rather than instead of
it, and the CLI prints it as a warning: the change has been made, and saying
it was not would be false.

Editing is split in two, because the editor is not core's to run: which one
comes from `$VISUAL` or `$EDITOR`, and core never reads the environment.
`lifecycle::start_edit` notes what the file holds, the CLI runs the editor,
and `lifecycle::finish_edit` decides whether anything changed and says
whether an install of it stays recorded. The CLI finishes the edit even when
the editor fails: one that writes the file and then exits non-zero, as vim
does on `:cq`, has changed the copy all the same. Nothing is written to the
ledger either way, so nothing is scrubbed out of it for the error to carry.
An edit with its contents given whole sees to the record the same way
([contents given whole](#contents-given-whole)).

The record seen to is the one held by the workspace the command resolved,
the one its templates come from, and each change is given it for the same
reason: a workspace beside the project, which a walk up from the project
never reaches, would keep a record of a copy removed. A change takes that
workspace rather than its ledger, because opening a ledger creates one, and a
workspace that never installed anything must not get a database because
something in a project changed; core opens it only when it is there. Another
workspace that links the same project keeps its own, an [open
question](#open-questions).

### Contents given whole

The panel has no editor to run, and a script has the text already, so
`edit` also takes what the artifact is to hold: `mind edit deploy
--contents draft.md`, or `-` for standard input, at either level. The CLI
reads the bytes, a file typed from the directory the command works in as
every path a command takes is, and `lifecycle::replace_contents` decides
what may be written. Core never reads standard input, as it never reads the
environment, and taking the bytes rather than a path keeps whatever reads
them (the CLI, and through it the panel's backend, by a file of its own,
[#82](#authoring-in-the-library)) from deciding anything about them.

- **Held to the rules a write is, before a byte is written.** An editor's
  edit is checked after the fact, since the editor has written the file by
  then; contents given whole are checked first, so what does not pass is
  never on disk. A skill's must be one Mindweaver would write
  (`artifact::strict_issues`): front matter that reads, a name and a
  description that pass. A rule's must say something, and its front matter
  must read whole (`artifact::rule_text_issues`), as a template's must.
  The rest of the file is the writer's, written as given; what `validate`
  only warns about is said after, as for an editor's edit. Not a list of
  its own: a rule that changes for one write changes for every one.
- **A name stays where its folder is.** A skill's manifest naming it other
  than its folder is refused (`edit_name_changed`) with the command that
  changes both, `rename`, rather than written: a name changed in one place
  is a skill an agent finds by one and not the other, and moving the folder
  to match would be a rename nobody asked for by that name. Compared with
  the folder, not with what the manifest said before, so contents that put
  right a manifest whose name had drifted from its folder are taken.
- **Only over what was read.** `--if-unchanged <sha256>` names what the file
  held when the contents were made from it, by `hash::bytes`, the recipe
  every hash of a file is. A file that holds anything else now was changed
  by somebody else since, and is not written over (`edit_stale`). A value
  that is no SHA-256 at all, 64 hex digits in either case
  (`hash::is_digest`), is refused as one before anything is read
  (`edit_hash_malformed`): compared, it could only be called stale, which
  sends its writer to read again a file nobody changed, and the line
  `sha256sum` prints, the file's name after the digits, is one such value.
  The panel's editor sends the one core gave with what it read
  (`Contents::hash`) with every save, so a change made in an editor while
  the page was open is refused rather than lost.
  Without it the write crosses whatever came first, as an editor's does.
- **Where the artifact is kept, and nowhere else.** The file is followed
  through its links once, as `copy::followed` follows them for a copy and a
  read for the panel, and written at the path it resolves to, so a link
  swapped in after the check is not written through, only when that lies
  inside each of three folders (`lifecycle::written_at`,
  `edit_leads_outside` otherwise). The folder its holder keeps the kind
  in: a repository can link a skill's `SKILL.md` to `~/.bashrc`, and the
  panel would otherwise replace it with a skill. A skill's own folder, so
  a manifest linked to a sibling's is not rewritten under a name that
  folder does not have. And a project's root, which a read of it for the
  panel is held to: a kind's folder, or `.claude` above it, linked out of
  the repository, to the user's own `~/.claude` say, would otherwise carry
  the write along. Not the root of the workspace's own, which may be a
  home: what is in the folder the workspace keeps its own of a kind in
  (`WeaverWorkspace::own`, of a kind `own_kinds` gives it) is held to that
  folder alone, as a read of it is. Told by the folder, since the holder
  is a `MindProject` either way, so whatever reaches the library's folder
  is held as the library is, and a project's folder anywhere else to the
  project's root. A project's skill linked to a file elsewhere in its
  repository is refused too, and edited where it leads: a write reaches
  nothing a read of it for the panel does not, and less.
- **Through a temporary renamed onto it**, as every file Mindweaver replaces
  is (`replace_bytes_unresolved`), named for the run and ending in `.tmp`:
  an agent reading the file finds the old contents or the new, and a skill's
  folder holds nothing new an agent would load as one. Contents the same as
  the file's write nothing, and say so.
- **The record as an edit leaves it.** An installed copy replaced holds
  something other than was installed, and reads as edited (D-04), from the
  same function `finish_edit` asks (`Record::Kept`). Like an edit, a rename
  or a removal, it takes no lock: what changes is the artifact's file, not
  anything the workspace keeps, and its ledger is only read ([open
  questions](#open-questions)).
- **Taken by the CLI, judged by core.** `--contents` names a file or `-`
  (`parse::Editing::given`), conflicts with `--editor`, and
  `--if-unchanged` needs it; one struct for both binaries' `edit`, so the
  two levels cannot come to take them differently.

`edit` without `--contents` stays on the list of lines a front end with no
terminal refuses (`opens_editor`), and so does `--contents -`, which reads
standard input (`reads_stdin`): the server's is the terminal `mindweaver ui` was
started from, and the TUI's is the screen's own keys. A file runs, from
either ([every change is a command
line](#every-change-is-a-command-line)). The one task that builds such a
line, `Task::Save`, names a file the backend writes the text into, never one
a browser names ([authoring in the library](#authoring-in-the-library)).

## Scanning

`mindweaver scan` walks down from the workspace root, or from each folder it is
given, looking for mind projects and git repositories, and says which of them
the registry already points at. It links nothing (a workspace manages what it
was told to); it only makes the telling easier, which is what the TUI's link
form and the minds screen are built on.

It had to be pointed somewhere else once the default workspace moved to
`~/.mindweaver`: a workspace in a home directory has nothing under it but the
home, and the projects are wherever somebody keeps them, say `~/Work`. So
core's `scan` takes a `Request`: the roots, none meaning the workspace root,
and the depth, `DEFAULT_DEPTH` (three) unless told otherwise. A front end
makes the folders it was typed absolute from the directory the command works
in, as it does the paths `link` is given, and each root is taken once
however it was spelled. A root named that is not there is a failure beside
what the others hold, said even at depth 0, so a typo is never an empty
answer.

The walk is bounded, because a root can sit at the top of a home directory.
It goes three levels down unless told (`repo`, `org/repo`,
`github.com/org/repo`), breadth first, so a budget of listed folders
(`MAX_LISTED`) runs out on the deepest rather than on a whole neighbouring
tree, and never into hidden folders, symlinks, `node_modules` or `target`. A
repository is a leaf, since its folders are its own. A root is the exception:
it is a candidate itself and is looked inside even when it is a repository,
whatever it is called and however it is reached, because somebody named it.
Every root starts the walk at once rather than one after another, so the one
budget, however deep the scan, is shared breadth first across them: when it
runs out it is the deepest folders that went unseen, never a whole second
root behind a first that filled it, and a folder two roots reach is looked at
once. A scan that runs out of budget says so (`truncated`) rather than
passing for complete. A folder that cannot be listed is a failure reported
beside what was found, except the workspace root when it is not there at
all: that holds nothing, and only the default workspace before it is made
can have one, in a home nobody has created yet.

What it reports is spelled to be typed. Each one found carries the route
`mindweaver link` takes for it from where the request says (`Request::from`):
the directory `mindweaver scan` runs in, and the workspace root for the TUI
and the panel, whose lines run there; the panel links by each one's path
all the same, which is what its path policy takes. A directory inside gets the route from there; one
outside, its path, rather than a route that climbs out with `..`, which reads
right from one directory only. A project marked by its settings carries the
id they name it by ([D-09](#d-09-project-settings-live-in-the-project)),
which is what names it for good whatever it is called and wherever it is; a
git repository, a project a `mind.toml` still marks and one whose marker will
not open have none, and the last is listed all the same, with a failure
beside it, since its marker still says it is a project.

A project found and not linked that holds the id the ledger last saw at an
entry where no project is any more is that entry's, moved, and carries the
entry (`moved`), spelled as `route` is, so the line it is listed with,
`moved: <old> -> <new>`, is the `mindweaver relink` to type from there. Only an
entry nothing marks (`MindProject::is_vacant`), not every stale one: one
whose marker is there and would not open still holds its project, whose id
nothing read, so a copy of it would pass for it moved; and one that opens
holding another id holds another project, which the ledger follows the next
time it is written. The ledger is read only when
something found could match, as a read ([a read leaves the ledger as it
finds it](#a-read-leaves-the-ledger-as-it-finds-it)), and one that cannot be
read is a failure beside what was found, listed with no move. An id two
directories hold, among those found and those the workspace links, is a
failure naming both, and neither is a move: a copy, which only
`mind init --new-id` tells apart.

The ledger learns an id only when it is written, so a project linked and
moved before anything wrote it is found by nobody, though the scan may list
it. While something found and not linked holds an id no entry was matched
to, each entry nothing marks whose id the ledger never learned is named
(`untraced`), and the text says to `mindweaver relink` it: the hint to
`mindweaver link` alone would leave the entry stale beside a second one for the
same project, which is what a relink is for. Which of them went where is
the person's to say, never a guess.

`mindweaver scan --json` prints the scan in [the envelope](#json-in-one-envelope),
each field by its name (`roots`, `depth`, `found`, each with `path`, `route`,
`id`, `mind`, `linked`, `moved`, `reason` and `agent_files`, `untraced` and
`truncated`); its failures, and that it stopped early, are the envelope's
warnings. `moved` and `untraced` came after the others, and `reason` and
`agent_files` after them, names added, so the envelope's version stayed.
The table leaves the id out: the path already tells its rows apart, and the
id is for a script to match a project by.

`mindweaver minds DIR` opens the minds screen looking under the folders it is
given rather than the workspace root, and the TUI keeps them and looks there
each time it reads the disk again, so a change made from the screen does not
send it back to the workspace root. It still lists every project the
workspace links: a folder says where to look for more, not which of its own
to show. What the scan could not read, and that it stopped early, the screen
says in a strip below the list, in the words `mindweaver scan` warns with, so a
folder typed wrong is said there too rather than drawn as one with nothing
in it.

### Which to link first

A scan of a home lists dozens of repositories, most of which nobody wants
managed, so `mindweaver scan --suggest` ranks them. What it finds carries its
reason (`scan::Reason`), the strongest of what it holds, best first: a mind
project its settings name by an id (`id`), which a workspace follows when it
moves; one with no id (`mind`), which a `mind.toml` marks or whose marker
would not open; a git repository holding something an agent reads at its
root (`agents`); and any other repository (`git`). A project ranks above a
repository because it is linked as it is, where a repository needs
`mind init` first, a write into somebody's tree; a repository an agent works
in ranks above one nobody does because that is where skills and rules would
be read. Each goes out as that word, by hand rather than derived from the
variant, as a kind or an agent does.

What counts as an agent's is a name at the root, never what a file holds:
`Agent::reads_at_root` gives, for each agent, the folder each place it reads
a kind from sits in, taken from `Agent::reads` (`.claude` for
`.claude/skills`), its instruction files, and each file
[D-10](#d-10-an-mcp-server-is-one-owned-entry-per-agent) may put its
project's MCP servers in, each resting on a fact in
[docs/agents.md](agents.md#what-says-an-agent-works-in-a-project). It lives
with the agent rather than in the scan or a front end, behind a match on
every agent with no wildcard, so an agent added, or a folder an agent is
found to read, reaches the ranking without anyone remembering it. The names
are looked at with `symlink_metadata`, a few in each directory already
found: nothing more is listed, so the walk stays inside the bounds above,
and a link counts as what it is called without being followed. Every
directory found lists the ones it holds (`agent_files`), a project too, so
a script sees what made a repository's reason.

The order is core's too: a `Request` names it (`scan::Order`), by route
unless suggested, and a suggestion lists what is not linked first, best
reason first and by route within one, then what is linked, by route, whose
reason says why it was worth linking once and nothing now. The minds screen
and the link form read their scan in that order and show what is not linked
unsorted (the screen lists every project linked after it, by its entry),
each found row with the words `mindweaver scan --suggest` uses (one helper in
the CLI says them for the command, both screens and the panel), so the
screen cannot rank differently from the command it shows. The panel's page
of what could be linked reads the same scan, through its backend, in the
same order ([its views](#its-views)). A project found moved says
`moved` on the screens as in the command: its reason is its id, which ranks
it first, but what it wants is a relink, and a row offering it as the
project most worth linking would have it linked beside the entry it left
stale; the panel offers it nothing to mark, only the relink. A suggestion
links nothing, makes nothing a project, and reads as any scan does: no
marker, no ledger and no workspace written.

## Writers at the same time

The commands were written for a person at one terminal, running one at a time.
The TUI runs them while another terminal runs its own, and the server under the
panel runs them while both do. Three things keep two writers from undoing each
other, and none of them makes a reader wait for the workspace.

- **A temporary of its own.** A file Mindweaver replaces (a marker, the
  `SKILL.md` a rename rewrites, an artifact's file `edit --contents`
  replaces, git's exclude file) is written whole into a
  temporary beside it and renamed into place (`replace_file`). The temporary
  was `weaver.toml.tmp`, one name for every writer: two at once wrote into one
  file, either could rename the other's half into place, and the second rename
  found nothing to move. It is named for the run now,
  `weaver.toml.<pid>-<suffix>.tmp` as [a stage](#built-where-no-agent-looks)
  is, made with `create_new`, and taken away when the write or the rename
  fails.
- **A ledger that waits, and keeps a log.** `Ledger::open` gives its
  connection SQLite's busy timeout, as long as a writer waits for the
  workspace, so a second writer waits for the first rather than failing with
  "database is locked". rusqlite sets five seconds of its own accord, but as a
  library's default; set there, the wait is Mindweaver's. It also turns on
  write-ahead logging: a reader goes on reading what the last write left while
  the next is made, and a writer does not wait for readers. The mode is kept in
  the file's header, so a ledger switches once, at the first open to write,
  and SQLite keeps `mindweaver.db-wal` and `mindweaver.db-shm` beside it while
  it is open. It switches only once `migrate` has accepted the version, since
  switching first would change a ledger a newer build wrote before refusing
  it, and a switch that fails is never a reason to refuse the ledger: one that
  finds another connection writing fails at once, because SQLite will not wait
  where waiting could deadlock, and is left to the next open. `open_existing`
  sets the timeout and not the mode, because switching is a write, and a read
  makes none but a migration.

  A ledger that keeps its log needs those two files made beside it even to be
  read, and SQLite takes them away when the last connection closes, so in a
  folder the reader may read and not write (a sandboxed agent's view of the
  workspace, a read-only mount, another user's workspace), the first read
  fails, where the journal the ledger had before let it be read.
  `open_existing` then opens it as SQLite's `immutable`, which needs neither
  file and takes no lock, but only when there is no log: then everything is
  in the file itself. Somebody who can write there writes into a log the read
  does not see, so what it reads is the ledger as their last checkpoint left
  it, and a checkpoint made between two of its reads is the one thing that
  could change the file under it.
- **A lock on the workspace.** Unique names keep each file whole, not each
  entry in it: two `mindweaver link`s that both read `weaver.toml` before either
  writes each put back a marker with only their own entry, and the second
  rename wins. So a writer holds `.mindweaver/lock` from before it reads what
  it will change until the change is written: an edit of the marker (`link`,
  `unlink`, `load`, `unload`, and the default workspace's, which `mindweaver
  init` and `workspaces forget` change), an install or an uninstall (an
  MCP server's too, whose renderers take none), a
  gather (clone and all, since two gathers of one source clone into one
  folder) and the scrub that opening the ledger to write runs
  (`ledger_to_write`), while it rewrites clones an older build left a
  credential in and removes the folders of a source made one with another,
  which a gather beside it would be filling.
  A migration of the workspace's own into its library holds it from its plan
  to its rewrite of the marker, and a project's move from its `mind.toml`
  from before it reads the project again until its ledger is written
  (`migrate::project`); a move from a `settings.json` changes nothing of the
  workspace's, and takes none.
  Core takes it inside each of them (`WeaverWorkspace::lock`), so no front end
  has to remember to. An install looks again at what the project holds once
  it has the lock: the standing it is handed comes from a survey made before,
  minutes before on the install screen, and a copy somebody has written or
  edited since is left alone rather than written over and recorded again.
  A change to a project's agents or share mode holds it too, from before it
  reads the project's settings until it has written them and carried the
  installs along (`reapply::apply`): the settings are the project's, but
  what changing them moves is the workspace's, and two changes at once in
  one workspace would otherwise each keep only their own. An update holds
  it from the plan it makes again until its last installation is written
  (`update::apply`), writing each through the body an install hands to once
  it holds the lock (`install::install_held`), since asking for the lock
  again, as `install_with` does, would wait for itself. A relink holds it
  from before it reads the ledger for its checks until the entry and the
  records it moves are written (`relink::relink`), and a new id from before
  it reads the project's settings again until the ledger knows it
  (`relink::new_id`), as a change to the agents does. An adoption holds it
  from the plan it makes again until its record is written
  (`adopt::apply`), so what the ledger says was adopted is what the library
  holds. Two workspaces
  linking one project are kept apart by neither's lock
  ([open questions](#open-questions)).
- **Opening the ledger takes no lock**, though it is when a ledger is made or
  migrated. A migration is one transaction that reads the version again once
  it holds SQLite's write lock (`Sqlite::migrate`), so two processes opening
  one an older build left take turns there and the second finds nothing to
  do (a read's migration included, which the workspace's lock could not keep
  apart, since readers never take it). Taking that lock as well made every
  change to an artifact wait behind a gather to see to its record, once the
  change was made, and fail as busy when the gather ran long, leaving the
  record as it was. The scrub takes it only when it has clones to rewrite,
  the first write after an upgrade from a build that stored a credential,
  and a record dropped or carried then waits for it without a deadline: the
  change it follows is made, and trying it again would find nothing to
  change. Telling the ledger which id each linked project holds, which
  opening it to write does every time (`Ledger::knew`), takes none either:
  it writes rows, in a transaction that takes SQLite's write lock before it
  reads, so a relink at the same moment comes wholly before or after it.

How the lock behaves, and why:

- **The system's lock, as a stage's is.** `std`'s `File::lock`, which is
  `flock` on Linux and macOS and `LockFileEx` on Windows. The system drops it
  when its process dies, however it dies, so a writer that was killed leaves
  nothing to clear, and nothing has to decide whether a process named in a
  file is still running. The file itself is never deleted, not even by the
  writer that made it: a writer waiting on the file it opened would then lock
  one nobody else can see, beside the one the next writer makes, and the two
  would write at once.
- **A bounded wait.** A writer that finds the lock held tries again, every
  few milliseconds at first and every tenth of a second at most, for
  `lock::WAIT`: polled, because `std` cannot wait for a lock with a deadline,
  and a thread blocked in `lock` could not be told to stop. Then it gives up
  with `LockError::Busy`, which names whoever holds it:

  ```
  /home/you/Projects/.mindweaver/lock: workspace busy: waited 10s for process 3234405 to finish writing to it; try again once it has
  ```

  Ten seconds is longer than anything but a gather takes, and short enough
  that a command kept waiting by a process that has stopped answering says
  so rather than looks hung. The error is collected like any other: the CLI
  prints it, and the install screen prints it under the line it was running
  and goes on to the next. A wait that ends in the lock is said nowhere,
  but counted on the thread that waited, with the holder it found, for a
  front end that asks what some work it ran waited (`lock::waits`), as
  the panel's log does ([the log](#the-log)).
- **Who holds it, in a file of its own.** The writer holding the lock writes
  its process id to `.mindweaver/lock.pid`, and takes it away while it still
  holds the lock, so it never names a writer that has let go. Not in `lock`
  itself, because a lock is mandatory on Windows: while one process holds a
  file locked, no other can read a byte of it, and the id is wanted by exactly
  the processes that cannot have the lock. Writing it is best effort, and a
  waiting writer that finds none says "another process".
- **Readers never take it.** A marker is replaced by a rename, and a read of
  the ledger sees only what was committed, so each file a read opens is
  whole, as it was before a write or after it. A change made in several steps
  is not seen whole: a read between an install's copy and its record finds the
  copy without the record, and calls it somebody's, and a read during a gather
  finds some shelf copies replaced and others not yet. With its log the ledger
  never keeps a read waiting for a write; in the journal a filesystem without
  one leaves it in, a read waits for a write to be committed, as readers and
  writers take turns there.
  `ledger_for_reading` migrates an older ledger as `ledger` does, in the same
  transaction, so the most a read waits for is another process's migration,
  once after an upgrade.
- **Not re-entrant.** Two handles on the file are two writers even in one
  process, which is what lets threads stand in for processes in the tests,
  from a link on each of two threads at once to every writer waiting for a
  lock the test holds. It also means a writer that asks for the lock while it
  holds it waits for itself, and fails as busy. Core's writers each take it
  once and call none of the others while they hold it, and a front end does
  not take it at all.
- **A filesystem without locks lets writers through unlocked**, as it lets
  installs through without a sweep: refusing would refuse every change there.
  Any error taking the lock counts, for the reason a stage gives: NFS
  without its lock daemon says "no locks available". The unique names still
  keep every file whole.

Writers on two machines are out of reach, and not attempted. On a network
filesystem `flock` is emulated with locks that belong to a process rather than
to a handle, and the write-ahead log needs every process using the ledger to
share memory on one machine. A workspace on a network share is safe used from
one machine at a time.

## Decisions

The roadmap in [#15](https://github.com/otsobide/mindweaver/issues/15) rests on
twenty-six decisions: twenty-two validated by the owner on 2026-10-26,
[D-24](#d-24-the-local-server-is-a-versioned-http-api) on 2026-11-03 with the
milestone it opens, [D-25](#d-25-core-is-values-and-ports), asked for by the
owner on the same day, [D-26](#d-26-the-product-is-mindweaver), asked for on
2026-11-04, and [D-20](#d-20-opencode-subagents-pending), still
pending. They are written down
here so that the PRs carrying them out do not reopen them: a change that
disagrees with one starts by changing the decision, here, with the owner. Each
says what was chosen, why, what was turned down, and which issues it unblocks;
the pending one says instead what holds until the owner chooses.

They describe what is to be built, not what is. Until the issues a decision
unblocks have landed, the sections above still describe the code as it stands,
and each of those issues rewrites the sections it changes.

### D-01: A project's `.mind` ignores itself

`mind init` writes a `.mind/.gitignore` holding `*`, as Python's venv does, and
`mind init --shared` leaves it out for a team that wants its marker committed.
The rule lives inside the folder Mindweaver owns, so it needs no git repository
at init time, survives a copy and goes when `.mind` does. Rejected: keeping
`.mind` committed, as a `mind.toml` project's was, against the owner's ask for
an ignored one; an
anchored `/.mind/` line in `$GIT_COMMON_DIR/info/exclude`, which stays the
mechanism for installed copies (D-02) because they have no Mindweaver folder to
hold a `.gitignore`; and a line in the repository's `.gitignore`, which edits a
team file. A fresh clone has no `.mind`, and `scan` already offers plain
repositories. Unblocks #29, #35 and #38.

Carrying it out, #29 writes the `.gitignore` (`MindProject::init_settings`),
and #35 makes it what `mind init` does, with `--shared` to leave it out (see
[a project's settings](#a-projects-settings)). #38 writes it for a project
moved from its `mind.toml` with `--local`, and says how to stop git tracking
a `.mind` committed before, which the `.gitignore` does not ([moving a
project to its settings](#moving-a-project-to-its-settings)).

### D-02: Installs stay local unless a project shares them

A project's settings name a share mode, `local` or `shared`. New projects are
local; projects upgraded from v2 are shared, because they were committed by
design. Local mode keeps installed copies and entries out of git through
anchored entries in a Mindweaver block of `info/exclude`, and edits a tracked
`AGENTS.md`, `CLAUDE.md` or MCP config only when the project sets
`edit_tracked`; otherwise Claude gets its rules through ignored
`.claude/rules/mindweaver` files, and the agents left unreached are reported.
That gives quiet projects, keeps the team case one flag away, and changes no
committed team file behind anyone's back. Rejected: always shared, which leaves
every install for the team to commit; and always local with marked edits to
tracked files, which puts Mindweaver's block in a team's diffs. It rests on
agents reading git-ignored files, which none documents. #17 confirmed it on
Claude Code 2.1.309 for skills, `AGENTS.md`, `.claude/rules` and
`CLAUDE.local.md`; for Codex and OpenCode it rests on their source, with
shared mode as the fallback ([docs/agents.md](agents.md)). Local copies never
reach teammates or cloud runs, and reach another worktree only for Claude, and
only when that worktree has no `.claude/skills` of its own. Unblocks #17, #32, #34, #37, #38, #43, #44, #45 and #94.

Carrying it out, #34 keeps a local project's installed skills out of git
with one entry per copy in its group of the exclude block, keyed by the
project's id, written before the copies land and taken out after they go;
a shared project's exclude file is left alone ([What keeps an install out of
git](#what-keeps-an-install-out-of-git)). #37 lets the share mode change,
with `mind share`: a project turning shared loses its `.gitignore` and its
whole group, one turning local gains both ([when a project's agents, targets
or share mode change](#when-a-projects-agents-targets-or-share-mode-change)).
#38 moves
a v2 project to settings shared unless `--local` ([moving a project to its
settings](#moving-a-project-to-its-settings)). #43 keeps Claude's rule files
out of git the same way, an entry a file ([Claude's rule
files](#claudes-rule-files)). #44 adds the `@AGENTS.md` import to a
project's `CLAUDE.md` that git tracks only in a shared project or with
`edit_tracked`, and otherwise gives Claude every rule through those files
([how Claude reaches the block](#how-claude-reaches-the-block)). #45 keeps
an `AGENTS.md` Mindweaver made out of git, and writes no block into one git
tracks in a local project without `edit_tracked`, saying which agents then
miss the rules ([installing rules](#installing-rules)). #59 writes no line
for a copy git tracks, as an adopted one often is, whether an install, an
update or a project turning local would have written one, since it keeps
none of the copy out ([what keeps an install out of
git](#what-keeps-an-install-out-of-git)). #94 keeps out of git only the
config files an install of an MCP server makes, a line each, and writes no
entry into one git tracks in a local project without `edit_tracked`,
saying which agent then misses the server, nor takes one out of it
([installing an MCP server](#installing-an-mcp-server)).

### D-03: Rules reach every agent through `AGENTS.md`

Assigned rules are rendered between `<!-- mindweaver:begin -->` and
`<!-- mindweaver:end -->` in the project's root `AGENTS.md`, one section per
route. Where Claude would not read that file, a managed `@AGENTS.md` import
goes into the project's own `CLAUDE.md`, or
`.claude/rules/mindweaver/<route>.md` when no file may be edited (D-02); a rule
that declares `paths:` goes only there. One core predicate (#44) decides,
walking every ancestor directory but `/` (see below) and ignoring the user
memory file. The root
`AGENTS.md` is the only per-project file all three agents read, and Codex has
no include mechanism. Claude 2.1.277+ reads it natively only when no
`CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` sits in the start
directory or above it; the import covers the other case, and Claude before
2.1.277. Rejected: per-agent files, under which Claude reads every rule twice
when there is no `CLAUDE.md`; and OpenCode's `instructions` globs with Codex's
`developer_instructions`, unverified and honoured only in trusted projects.
Mindweaver never creates a `CLAUDE.local.md`, which silently disables
`AGENTS.md`, and warns when an `AGENTS.override.md` hides the file from Codex
or the chain nears Codex's 32 KiB budget. #17 confirmed these Claude behaviours
on Claude Code 2.1.309: a `CLAUDE.md` above the git root or a `CLAUDE.local.md`
stops `AGENTS.md` being read, the user memory file and `.claude/rules` do not,
the block's text reaches Claude without its markers, the import loads it once,
and a rule with `paths:` loads only when a matching file is read
([docs/agents.md](agents.md)). Two it did not try: that a `.claude/CLAUDE.md`
counts as well, seen only among the names in Claude's debug output, and the
2.1.277 floor, which is docs-only. One corner differs from what this said
when validated: Claude's check most likely stops below `/`, so a `CLAUDE.md`
at `/` alone does not stop it reading `AGENTS.md` (read from the directory
counts in its debug log, not tried). #44's walk stops below `/` too: every
ancestor directory but the filesystem's root, so the predicate agrees with
Claude, where walking up to and including `/` would choose Fallback while
Claude still read the block, and give Claude each unscoped rule twice.
Unblocks #17, #42, #43, #44 and #45.

Carrying it out, #42 renders the block, between those markers and under a
`<!-- mindweaver:rule <route> -->` for each rule, leaving out a rule that
states `paths:`. It leaves no block in a folder above a project the
workspace links, nor in the workspace's own root where no project it links
is, nor inside an agent's own folder, since the agents of other projects
would read an `AGENTS.md` there too, and takes out one it finds there; a
project at the workspace's root has its rules in its own. It warns of the override, the
budget, and a project in no git work tree, where Codex reads only the
folder it starts in ([how a rule reaches the
agents](#how-a-rule-reaches-the-agents)).

#43 writes a rule that states `paths:` as
`.claude/rules/mindweaver/<route>.md`, front matter kept, and every rule there
in the fallback mode #44 chooses, never because a block was written alone.
It says that such a rule reaches Claude alone, naming the project's agents
it misses. What is Mindweaver's there is what its caller recorded of an
earlier render, each file's hash among it, so a file nothing recorded, or
one changed since, is never written over or taken away. It is refused where
the block is, and through a link out of the project ([Claude's rule
files](#claudes-rule-files)).

#44 is the predicate, `claude_md::reads_agents_md`: the three names in the
project's root and every folder above it but `/`, the user memory file left
out by the file it is. `claude_md::reach` turns it, the project's settings
and whether the block was written into Native, the import in the project's
own `CLAUDE.md` or `.claude/CLAUDE.md`, or the fallback with its reason, and
two more: a `CLAUDE.md` that is the `AGENTS.md` through a link, which Claude
reads the block as, and an `@AGENTS.md` line somebody already wrote in one
of Claude's files, which Claude reads the block through, so that a team's
own import, in a file Mindweaver may not edit too, never leaves Claude
reading a rule twice. `claude_md::render` adds the import once, between the
block's markers at the top of the file, and takes it out with the last rule;
it never makes a `CLAUDE.md` or a `CLAUDE.local.md`, and never edits a file
above the project, reporting it as the fallback's reason instead ([how
Claude reaches the block](#how-claude-reaches-the-block)).

#45 wires the three to installing: a rule installed into a project marked by
its settings, or taken out of it, renders every rule the project has, the
block, then the import, then Claude's rule files in the mode the reach says,
and records each rule with a hash of its section and of its file, and
whether Mindweaver made `AGENTS.md`. A project a `mind.toml` marks is given a
copy of the rule's file in its rules folder instead ([installing
rules](#installing-rules)).

### D-04: An edited copy is detected, not disowned

The ledger keeps a content hash per target. An untouched copy stays managed and
can be updated; one whose hash differs is Edited, reported, and never
overwritten or deleted without `--force`. `edit` and `rename` keep the record,
and removing an installed copy uninstalls it. A record from before hashes
counts as current only when the copy matches its offer byte for byte, and as
Edited otherwise. This keeps "never overwrite what somebody wrote" while
updates still reach the copies nobody touched, and it catches a hand edit made
outside Mindweaver, which today goes unseen. Rejected: today's rule, which
stops updates at the first edit; overwriting on update, which loses the edit;
and a three-way merge, too complex for skills that ship scripts. Unblocks #23,
#30, #47, #48, #50 and #93.

Carrying it out, #30 keeps the hash once per installation, on
`installations`, rather than once per target: every copy D-07 writes is
staged from one tree, so each is compared with the same hash, and an edited
copy is the one whose own hash no longer matches it. A target whose content
differs from its installation's in form (a rule's section of the managed
block in `AGENTS.md` beside its `.claude/rules/mindweaver/<route>.md` file,
as D-03 has it) shares no one tree with the others, and would need a hash of
its own on `installation_targets`, added by the schema step that first
records one, which #45 did for rules. #38 holds a v2 project's records from
before hashes to their offers when it moves the project to settings, and
gives a record whose copy matches the hash every copy then holds ([moving a
project to its settings](#moving-a-project-to-its-settings)).

#47 reads the hashes: `Standing` tells an installation current, outdated,
edited, missing or orphaned ([an edited copy is left
alone](#an-edited-copy-is-left-alone)), for a skill's copies, a rule's
section and its file for Claude, and a rule's copy in a project a
`mind.toml` marks. `install` and `uninstall` refuse an edited copy, the whole
change with it, unless `--force`; a render keeps an edited section as the
block holds it, as Claude's rule files were kept; `status` says `edited`;
and the lifecycle keeps the record through an edit, carries it through a
rename, and drops it with a removal, which uninstalls the copy ([a record
must not outlive the copy it is
about](#a-record-must-not-outlive-the-copy-it-is-about)). A record with no
hash is held to its offer in code, with no schema step, and the first
install over a copy that matches stores its hash. A change of agents, or a
move to settings, still drops the record of an edited copy it leaves in a
folder no agent the project names is sent copies to: no install writes
there again, and no listing reads it ([when a project's agents, targets or
share mode
change](#when-a-projects-agents-targets-or-share-mode-change)).

#48 acts on them: `mindweaver update` and `mind update` write every outdated or
missing installation again from where it came from, leave an edited one,
every copy of it, unless `--force`, and say an orphaned one without failing
([bringing what was installed up to
date](#bringing-what-was-installed-up-to-date)); and `status` says all five,
`current`, `outdated`, `missing` and `orphaned` taking over from `installed`
and `partial` ([what each project has](#what-each-project-has)).

### D-05: A workspace's library lives in `.mindflayer`

New workspaces keep their own artifacts in `.mindweaver/library/<kind>`.
Existing ones keep the explicit `[directories]` in their `weaver.toml` until
`mindweaver migrate --library` moves them, and a kind added later (MCP) defaults to
`.mindweaver/library/<kind>` in every workspace, whatever its `weaver.toml`
says. `weaver.toml` already spells its directories out, so the new default
needs no format bump; no agent scans `.mindweaver`, which keeps
instruction-shaped files out of the directories above projects; and a
venv-style weaver becomes one folder. Rejected: beside the projects, as today,
which puts `~/skills` and `~/rules` in the home directory; and moving only the
home workspace's library, which leaves two layouts to explain. Unblocks #52,
#53 and #86.

#86 carried out the part about a kind added later: MCP servers are kept in
`library/mcp` in every workspace's folder, a marker naming no folder for
them, as every marker written before them does, and a new one too ([the
workspace's own artifacts](#the-workspaces-own-artifacts)). A marker naming
`mcp` by hand is followed, as one naming any kind is, which "whatever its
`weaver.toml` says" was not meant to undo.

### D-06: Projects keep authoring beside the code

Amended by the owner. Projects keep authoring their own skills and rules beside
the code, and `mind init` still creates `skills/` and `rules/` by default. The
v3 project marker was a JSON file, `.mind/settings.json`, which names those
folders (`directories`) beside the project's agents, share mode and id, so any
tool or editor can read and change it. Amended again by the owner
(2026-10-27): the settings are TOML, `.mind/settings.toml` at version 5, like
`weaver.toml`, so both markers are written from a template whose comments say
what each key does and edited through toml_edit alike ([the
markers](#why-the-markers-are-toml-written-from-a-template)); a
`settings.json` is still read as written, and migrated.
Installs go to the agent folders the project targets (D-07), not to the
authored ones. That keeps every existing workflow, makes the workspace the
default source, and keeps the marker minimal. Rejected: receiving only, which
would strand what v2 projects authored; and today's model unchanged, against
vision points 2 and 8 (the weaver as the source, a minimal `.mind`). The plan
made the folders opt-in (`--skills`, `--rules`); the owner prefers them to
exist from the start. Unblocks #29, #33, #35 and #59.

Carrying it out, #35 makes `mind init` write settings that name `skills/` and
`rules/`, or the folders `--skills` and `--rules` give, and make them. #166
carries out the second amendment: `mind init` writes a `settings.toml`, a
change edits it a key at a time through toml_edit, a `settings.json` is read
as written and changed by nothing but `mind migrate`, which moves it, and a
`mind.toml` straight, to a `settings.toml` ([a project's
settings](#a-projects-settings), [moving a project to its
settings](#moving-a-project-to-its-settings)). #59 brings what a project's
agents read and nothing installed, a skill somebody keeps in
`.claude/skills` by hand, into the workspace's library when asked, its
copies recorded as installed from there and left as they are; what a
project authors is never adopted ([adopting what a project already
keeps](#adopting-what-a-project-already-keeps)).

### D-07: One copy per agent folder

Amended by the owner. An installed skill is a real copy in
`.agents/skills/<name>`, which Codex and OpenCode read, and an identical one in
`.claude/skills/<name>` for Claude, staged and written in one step; the ledger
records one installation with several targets. That layout is the default, and
a project's settings (`.mind/settings.toml` since D-06's second amendment)
may override it per kind, for example with one `.skills` folder for every
agent (#40). Copies fit today's install model
and the Windows CI, survive the hub moving, and keep gathered content
contained. OpenCode sees the skill twice, which is harmless only while the
copies are identical, so an install is refused if either target holds foreign
content. Rejected for now: a relative symlink in `.claude/skills`, which may
come later as a Unix opt-in, since #17 showed Claude 2.1.309 loads a symlinked
skill once (its exclude entry has to drop the trailing `/`, because git never
follows a symlink and sees a file);
and symlinks into the hub, which break projects when `~/.mindweaver` moves and
leak its path into repositories. `.codex/skills` and `~/.codex/skills`,
deprecated or undocumented, are never targeted. Unblocks #17, #28, #34 and #40.

Carrying it out, #28 puts the layout in core's target table
(`TargetTable::standard`, see [Agents, and where each kind reaches
them](#agents-and-where-each-kind-reaches-them)), a value every caller is
handed so the override can take its place. #34 installs into it:
every copy is built from one tree in the project's stage and put in place
with the others or not at all, the installation is recorded once with a
target for each, and somebody's work in either folder refuses it, in both
([Installing](#installing)). #40 reads the override from a project's
settings, `targets`, which sends a kind to the folders it names for every
agent, at version 4 of a `settings.json` and in any `settings.toml`, and warns
of agents that read none of them ([a project's own
targets](#a-projects-own-targets)).

### D-08: An explicit or activated workspace wins

The workspace is `-W/--workspace DIR`, then `$MINDWEAVER_WORKSPACE` (set by
`mindweaver activate`), then the first one walking up, then the home default. An
explicit `-W` wins even when empty, and empty means none, following the
`--home=` pattern so test helpers can pin it; `--home` and `$MINDWEAVER_HOME`
keep meaning where the default lives. One core function decides, fed by the
front ends, and an explicit or activated path that is not a workspace is an
error, never a silent fallback. Activation exists to override location, as a
virtualenv does. Rejected: walking up first, under which an activated weaver
silently loses to any workspace above the current directory; reusing `--home`,
which confuses where the default lives with which workspace to use; and a
direnv-style activation file, hidden state in repositories meant to stay
minimal. Unblocks #54, #55 and #56. #54 carried it out: core's `resolve`,
with `-W/--workspace` on both binaries (see [which workspace a command
uses](#which-workspace-a-command-uses)). #55 added what sets the variable,
`mindweaver activate` and `mindweaver-deactivate`, and `mindweaver which`, which says the
reason (see [activating a workspace](#activating-a-workspace)). #56 lists the
workspaces this machine knows, with the one `resolve` chooses marked (see
[the workspaces this machine knows](#the-workspaces-this-machine-knows)).

### D-09: Project settings live in the project

A project's agents, share mode and `edit_tracked` live in its own settings,
the v3 marker `.mind/settings.json` and from version 5 `.mind/settings.toml`
(D-06), ignored by default (D-01). They describe the
files physically in the project, and several workspaces may link one project,
so keeping them beside it means every workspace sees the same targets.
Rejected: `[[project]]` tables in `weaver.toml`, which let two workspaces
disagree about one project's files; and the ledger, which hides editable
configuration in a database. Unblocks #29 and #37.

Carrying it out, #29 adds the format, `Settings` in `settings.rs`, which
`MindProject` reads from a settings file before a `mind.toml`, and
`MindProject::init_settings`, which writes it with D-01's `.gitignore` (see
[a project's settings](#a-projects-settings)), and #35 makes `mind init`
write one, naming the agents `--agents` gives and the share mode `--shared`
chooses. #37 changes them, with `mind agents` and `mind share`, and carries
what is installed into the project along ([when a project's agents, targets
or share mode change](#when-a-projects-agents-targets-or-share-mode-change)).
#40 adds `targets`, changed with `mind targets` through the same path ([a
project's own targets](#a-projects-own-targets)).

### D-10: An MCP server is one owned entry per agent

Each server is one versioned, neutral definition in the library,
`mcp/<name>.toml`, shaped as a subset of the registry's `server.json` with
typed inputs. Installing renders it as exactly one entry in each target agent's
project config, and ownership is a ledger row holding a semantic fingerprint of
that entry; project scope only, for now. This reuses the catalog and the
create, rename, remove and validate lifecycle, maps registry entries easily,
and touches one key per agent file. A marker key inside the entry cannot work,
since OpenCode rejects unknown keys, `codex --strict-config` fails on them and
`.mcp.json` allows no comments, so the fingerprint is the only ownership
signal. Rejected: storing `server.json` verbatim; copying whole config files,
which overwrites the user's other servers; and delegating to the agents' CLIs,
since `codex mcp add` writes only Codex's user config, never a project's, and
re-serializes every entry there, though they may return for user scope (D-22).
Unblocks #18, #86, #89, #93 and #94.

#86 added the definitions: `Kind::Mcp`, one `<name>.toml` a server in
`format = 1`, its inputs written between braces where their value goes, as
the registry writes a variable ([what an MCP server's definition
declares](#what-an-mcp-servers-definition-declares)), listed, shown,
validated, renamed and removed by the lifecycle every kind goes through,
and installed by nothing (`not_installable`) until #94, nor stepped
through by any front end until #95. #88 added one from the
command line, from how an agent starts or reaches it or from a template
([adding one](#adding-an-mcp-server)), and laid out what `show` says of
its transport and inputs. #90 renders one into Claude's `.mcp.json` as
its one entry and takes it out again ([Claude Code's
`.mcp.json`](#claude-codes-mcpjson)), handing back the entry's canonical
form for the fingerprint, #91 into Codex's `.codex/config.toml` the
same way ([Codex's `.codex/config.toml`](#codexs-codexconfigtoml)), its
canonical form its members sorted, since `codex mcp add` writes every entry
again, and #92 into OpenCode's `opencode.json`, or the first of its four
config files there is, in the shape the file keeps its servers in
([OpenCode's `opencode.json`](#opencodes-opencodejson)). #93 records each
entry an installation writes by its fingerprint, and tells how it stands
([how an entry stands](#how-an-entry-stands)), and #94 installs a server
into the file of each agent the project names, all of them or none, and
takes out only the entries it recorded that nobody changed since
([installing an MCP server](#installing-an-mcp-server)). #95 gave it the
commands' words: a server named bare as every kind is, what each agent
will run said from its renderer's own entry, and one from elsewhere than
the library written only with `--yes`, on the command line and from the
install screen alike.

Checking it, #18 added MCP checks to `scripts/agent-probe`, run by hand.
Claude Code's and OpenCode's ran on 2026-11-05; Codex's were not run, since
the owner has no Codex subscription, so what they bear on stands as
expected ([docs/agents.md, MCP servers](agents.md#mcp-servers)). What each
result bears on, and what came of it:

- **An approved `.mcp.json` entry with a `type` loads in Claude**
  (`claude-mcp`): verified, so #90's shape stands. A `-p` run also started
  the entry nobody approved, so approval gates nothing there, and an entry
  #90 writes runs in a `-p` session as soon as the file holds it.
- **An http entry with no `type`** (`claude-mcp-untyped`, recorded): Claude
  drops it alone and never reaches its URL, and the file's other servers
  load. #90 writes `type` whatever comes of it, and need refuse no file for
  holding such an entry; #96 may report it.
- **A comment or a trailing comma in `.mcp.json`** (`claude-mcp-comment`,
  `claude-mcp-comma`, recorded): Claude refuses the file whole ("MCP config
  is not valid JSON"), none of its servers loading. So #90 reads
  `.mcp.json` as strict JSON, and one holding either is refused, saying
  why, rather than given an entry Claude will never load.
- **A trusted project's `.codex/config.toml` loads, an untrusted one's does
  not** (`codex-mcp-trusted`, `codex-mcp-untrusted`): not run, taken as
  expected. Were an untrusted project's servers started, #91's trust
  warning would go. Were a trusted one's not, Codex cannot be reached at
  project scope, and this decision comes back for Codex, with D-22's user
  scope. #94 counts a server installed into an untrusted project as
  reaching no Codex, as #91 warns, its entry written all the same.
- **A server of the same name in the user's `config.toml`**
  (`codex-mcp-collision`, recorded): not run, so it settles nothing, and
  #91's open choice is #91's to make without it: refusing a name a lower
  layer holds is the reading that is safe whatever Codex does, and #91
  refuses it (`codex_mcp_collision`) until a run says a warning would do.
- **V2's `mcp.servers` loads in V2 and in 1.19.4, and V1's `mcp.<name>` in
  V2** (`opencode-mcp-v2`, `opencode-mcp-1-19`, `opencode-mcp-v1-shape`):
  verified on 2.2.5, 2.1.3 and 1.19.4, so #92 writes D-12's minimal V2
  entry, and keeps V1's shape in a file that already uses it.
- **Each agent loads its servers from the file the target table names**
  (every run): verified for `.mcp.json` and `opencode.json`, taken as
  expected for `.codex/config.toml`. #93's MCP targets stand
  (`Agent::mcp_file`, [agents, and where each kind reaches
  them](#agents-and-where-each-kind-reaches-them)).

### D-11: MCP secrets are environment references

A secret is written as a reference in each agent's own syntax (Claude `${VAR}`;
Codex `env_vars`, `env_http_headers` and `bearer_token_env_var`; OpenCode
`{env:VAR}`), and its value is never stored. A literal in a secret input is
refused, and so, for that agent, is a shape it cannot express: in Codex, secret
arguments, a secret inside a URL, and composite secret headers other than
`Authorization: Bearer`. References work in all three agents without Mindweaver
running and keep committed files safe, and refusing rather than writing a
literal keeps the guarantee where Codex has no interpolation. Rejected: values
under `~/.mindweaver/secrets` read through OpenCode's `{file:...}`, which
Claude and Codex cannot match; and, for now, a `mind mcp run` wrapper reading
the OS keychain, which may come later as an opt-in but makes every teammate
depend on the binary and does not cover remote headers. Unblocks #18, #86, #87,
#90, #91 and #92.

#87 carried out what a definition can hold of it ([a secret is an
environment reference](#a-secret-is-an-environment-reference)): a secret
input names the variable it is read from, and a token written as it is in
a transport is refused, said by where it is and what it starts with. What
each agent cannot express is core's report, asked by its renderer before it
writes ([what each agent cannot be given](#what-each-agent-cannot-be-given-of-a-server)),
and never a reason to call a definition invalid, which the other agents are
given whole. #88's `add mcp` takes a secret only as the name of the
variable it is read from (`--secret-env`, `--secret-header`,
`--bearer-env`), each in a shape every agent, Codex included, can be given,
and refuses a value shaped like a token where a value is written as typed.
#90 writes each input as `${VAR}` in Claude's entry, and finds in
Claude's binary a shape it cannot be given: an input of an address read
from a variable Claude reads as empty toward a remote server, a credential
of its own or of a tool on the machine, which #87's report now says
(`variable_blanked`), so the renderer refuses it. #91 writes Codex's entry
with `env_vars`, `env_http_headers` and `bearer_token_env_var`, and says a
shape D-11 left unsaid: a secret in a program's environment beside other
text, or under a name other than its variable's, since `env_vars` passes a
variable on under its own name (`secret_env_composite`,
`secret_env_renamed`), which a registry's server often is and `add
--secret-env` never writes. #92 writes each input as `{env:VAR}` in
OpenCode's entry, and says the shape OpenCode's reference leaves: text
it would fill in where no input stands, `{env:` or `{file:` written as it
is, which the report now says (`text_substituted`), as #90 said Claude's
`${`.

Checking it, #18's checks put a reference in every place a renderer would,
naming a variable with a value of its own, and read what a stand-in server
was started with or sent ([docs/agents.md, MCP
servers](agents.md#mcp-servers)). Claude Code's and OpenCode's ran on
2026-11-05, Codex's not. What each bears on, and what came of it:

- **Claude expands `${VAR}` in `args`, `env`, `url` and `headers`, also
  inside a longer value** (`claude-mcp`): verified, so #87's report gives
  Claude every place, and #90 writes the reference in each.
- **Codex passes `${VAR}` on as written, in `args`, `env` and `url`**
  (`codex-mcp-trusted`): not run, taken as expected. This is why Codex
  cannot express a secret argument or a secret inside a URL. Were Codex to
  expand one after all, #87 would drop that shape from Codex's list, and
  #91 would write the reference.
- **`env_vars`, `env_http_headers` and `bearer_token_env_var` fill a value
  from a variable** (`codex-mcp-trusted`): not run, taken as expected. One
  that does not would leave its shape inexpressible in Codex: #87 would
  list it and #91 refuse it, where it means to write it.
- **OpenCode expands `{env:}` in the command, the environment, the URL and
  the headers, in both lines** (`opencode-mcp-v2`, `opencode-mcp-v1-shape`,
  `opencode-mcp-1-19`): verified on 2.2.5, 2.1.3 and 1.19.4, the command's
  first word and a reference inside a longer value included.
- **What V2's background server fills `{env:}` from**
  (`opencode-mcp-service`, recorded): its own environment, on 2.2.5 and
  2.1.3. A server started apart, without the variables, filled none of the
  five from the list that asked; one the list started itself filled all
  five. So in V2 a reference holds only once its variable has reached the
  server, started from a shell that has it or given it with `opencode
  service set env`, and "works without Mindweaver running" needs saying
  again for V2: #92 would warn of it, and #96 would check the server's
  variables. That change is the owner's, here, and stays open until made.

### D-12: OpenCode gets the V2 MCP shape

The owner runs OpenCode 2.1.3, so Mindweaver writes a minimal V2
`mcp.servers.<name>` entry, which V1 also reads since 1.18.24, and the V1
`mcp.<name>` shape only when the target file already uses it. `codemode`,
`protocol` and `timeout.startup` are left out unless asked for, because V1
warns on them. Following a file's existing shape avoids mixing two in one file.
Rejected: always the V1 shape; and running `opencode --version` on every write,
which would tie core to a binary. Unblocks #92.

#92 carried it out ([OpenCode's `opencode.json`](#opencodes-opencodejson)),
on #18's runs, which loaded V2's shape in 2.2.5, 2.1.3 and 1.19.4 and V1's
in V2: V2's minimal entry, and V1's only in a file whose `mcp` holds a
server so already, a server of the name replaced in the shape it stands
in and one written in both refused (`opencode_mcp_twice`), so no entry
mixes the two. V1's shape has no key for a program's folder, by its docs,
so `cwd` is left out of an entry written in it and said
(`opencode_mcp_v1_cwd`).

### D-13: Sources have names, and a marketplace is a parent

Every gathered source (closed issue #7) gets a unique, editable name in
`[A-Za-z0-9._-]`, `owner-repo` for git and the catalog's own name for a
marketplace by default, and `--from <name>` takes it. Its identity is a
canonical, credential-free URL plus a non-null reference key. A marketplace is
a parent source with one pinned child per plugin entry, gathered only when
selected (`--plugin`, or a confirmed `--all`), and refreshing or removing the
parent cascades. What a listing shows is then exactly what a command takes;
each child keeps its own revision and its own failure, since entries point at
different repositories and pins; a token in a URL never reaches
`mindweaver.db`; and the duplicate hole where SQLite's UNIQUE ignores a NULL
reference closes. Rejected: a `name@source` spelling, when `@` already means
plugin@marketplace in Claude and skill@version in `gh`; and keeping URL
strings. claude-community lists 2283 entries, so gathering a marketplace whole
by default would clone thousands of repositories. Unblocks #98, #100 and #103.

### D-14: Core owns HTTP, behind a seam

Core owns catalog URLs, paging, caching and parsing behind an injectable `Http`
trait. The default is a blocking client reusing `reqwest` with rustls, already
in `Cargo.lock`, and tests pass a fake. Paging, retries and mapping are rules,
so they belong where the CLI, the TUI and the server cannot disagree about
them, and a synchronous client fits a synchronous core. Rejected: front ends
fetching the bytes for core, which spreads those rules across them; and calling
HTTP with no seam, which takes the tests online. Carrying it out amends [the
line between core and a front
end](#where-the-line-between-core-and-a-front-end-falls) and AGENTS.md's "only
I/O is files". Unblocks #105, #106 and #108.

### D-15: Skill search is GitHub plus opt-in finders

`mindweaver gather search` queries GitHub code search when a token is supplied,
plus opt-in finders, marked unstable, for skills.sh's `/api/search` and
api.claude-plugins.dev; the owner reads those two services' terms first.
Services that need a key or attribution (SkillsMP, Glama, PulseMCP, skills.sh
v1) are excluded, because they bring terms and attribution obligations. A
finder only returns addresses that resolve to git sources, so an endpoint that
changes breaks nothing already gathered, and its failure is a warning.
Rejected: GitHub alone, and no finders at all, both of which leave the skill
directories out (vision point 5). Unblocks #108.

### D-16: Credentials come from the front end, never from a prompt

The front end supplies a credential provider, `--token-env VAR` or
`gh auth token`, handed to `gix` through a credentials callback that never
prompts. Failing that, the git-config helpers that `gix` runs itself are tried,
then the system ssh binary for ssh URLs. A token is never persisted. This
copies Claude Code's own non-interactive model, and a prompt would hang the TUI
and the server, so the never-prompting callback lands early (#26) and the
providers later (#110), which also documents the ssh-binary and git-helper
dependencies in [Why `gix`](#why-gix-rather-than-running-git). Rejected: HTTPS
tokens only, which leaves ssh remotes out; and shelling out to `git`, which
gives up what `gix` was chosen for. Unblocks #26 and #110.

### D-17: The next front end is a local web panel

`mindweaver ui` and `mind ui` serve a Vue single-page app embedded in the same
binary, behind a default-on cargo feature, `ui`. A library crate,
`apps/server`, runs axum on 127.0.0.1 and handles transport only: it is generic
over a `Backend` trait with associated `Query`, `Task` and `Outcome` types, so
it only moves JSON, and the CLI implements the trait. There is one binary to
install, it works in any browser, the associated types keep the Cargo graph
free of a cli/server cycle without an extra crate, and the server's tests use a
fake `Backend`. Rejected: a separate protocol crate for `Task` and the outcome
types, which the associated types make unnecessary; a Tauri desktop app first,
while Tauri 3 is alpha, though a `Transport` interface in the frontend keeps a
later Tauri shell cheap; and a separate server binary or daemon, the re-exec
wrapper [the line between core and a front
end](#where-the-line-between-core-and-a-front-end-falls) already rejects. #68
measured the feature's binary size and compile time ([launching
it](#launching-it-mindweaver-ui)), about 2.3 MB on a release binary of 25 and 7 s
on a clean release build of 82, and it stays on by default. Unblocks #65, #68
and #69.

### D-18: The panel writes by running commands

As in the TUI, the panel's reads are typed queries that core answers, and its
writes are typed Tasks turned into CLI command lines and run through the same
parser on one worker thread, each line shown before it runs. That extends
[every change is a command line](#every-change-is-a-command-line) to a second
front end, so the three cannot disagree, and gives the panel a transcript the
user can replay. Four things come first: the remaining CLI-side orchestration
moves into core (#23, #54), CLI warnings carry core error codes (#63), batch
rules for assignment are planned in core (#78) rather than reimplemented in
TypeScript, and every Task from the browser passes a path policy (#67, [what
a browser may name](#what-a-browser-may-name)).
Rejected: a core service layer the server calls directly, which duplicates
every command's entry point; and shelling out to the `mindweaver` binary, which
needs the argument forwarding already rejected for the two binaries. Unblocks
#63, #67, #76 and #78.

### D-19: The panel is built on Nuxt UI

The panel uses Nuxt UI 4: MIT, built on Reka UI and Tailwind v4, with a
dashboard layout, a command palette, a virtualized data table and colour mode,
and an official dashboard-vue template on Vue 3.5, Vite 8 and TypeScript 6. It
covers the control-panel pieces out of the box under a GPL-compatible licence,
so it needs the least custom code. Rejected: shadcn-vue, the same base with
component source copied into the repository, which can still be compared on one
screen of the #69 scaffold during M1 to M3; PrimeVue 5, whose proprietary,
keyed licence is incompatible with GPL-3.0-only; and Naive UI, with
accessibility gaps open since 2021. Unblocks #69.

### D-20: OpenCode subagents (pending)

Not decided. Whether "agentes de OpenCode" means the OpenCode tool or also its
subagent definitions is open, and the owner, who mainly uses Claude Code, has
not chosen. Until then OpenCode is supported as a tool, receiving skills, rules
and MCP servers as Codex does, and no subagent-definition work is planned. The
other option would manage subagents as one kind per agent
(`.claude/agents/*.md`, `.opencode/agents/*.md`, `.codex/agents/*.toml`),
because the three formats are incompatible and share no location; skills
already act as slash commands in all three agents. Unblocks nothing yet;
choosing subagents would add kinds, through [Adding a third
kind](#adding-a-third-kind).

### D-21: The panel's views follow the capabilities they show

The views are built after the core capabilities they reuse: agents (M1), rules
and updates (M2) and the hub (M3), so they sit on the final data model and need
no rework. The server shell, its guards and the scaffold can be built in
parallel during M1 to M3, so the owner can review the look (D-19) early, and
MCP and connectors then ship in core, CLI, TUI and web together. The snapshot
lists kinds from `Kind::ALL`, so MCP reaches the panel without frontend rework.
Rejected: views right after M0, on today's data model, which are reworked when
the hub and the agent targets land; and the panel last, after MCP and
connectors, which leaves the owner's top wish to the end. Unblocks #65, #66 and
#69, which need nothing from M1 to M3, and #61, which needs only #36; the views
wait for the snapshot (#62).

### D-22: User-scope installs come later

Installing into user scope (`~/.agents/skills`, `~/.claude/skills`, user MCP
config) comes after M7, as a follow-up milestone, behind an explicit `--user`.
It writes `~/.agents/skills/<name>` and `~/.claude/skills/<name>`, never a
`manifest.json` in `~/.claude/skills`, and never anything under
`~/.claude/skills/synced` or `.trash`. Meanwhile `mindweaver doctor` (#96) reports
project skills that shadow user-scope skills, or are shadowed by them, with
different contents. One name at both scopes resolves differently in each agent
(Claude prefers the personal one, OpenCode V2 lets `~/.agents` beat a project's
`.claude`, Codex shows both), `~/.agents/skills` already holds other tools'
skills, and Claude before 2.1.280 moved skills listed in
`~/.claude/skills/manifest.json` to `.trash`. Project scope keeps ownership per
project, where the ledger can track it. Rejected: project scope forever, which
gives up the hub's promise of skills for every project; and user scope now, as
the home workspace's default. Unblocks #17, #28, #94 and #96.

Until then, #34 refuses an install whose copy would land inside one of the
agents' own folders, which a project at the user's home would otherwise
reach by accident ([never into an agent's own
folder](#never-into-an-agents-own-folder)), and #94 an MCP server's entry
in a config file there, before anything is written ([installing an MCP
server](#installing-an-mcp-server)). #96's doctor reports a project skill
whose name a skill at user scope that the same agent reads holds too, with
something else in it, in `~/.claude/skills`, `~/.agents/skills` or
OpenCode's own `skills`, by the folders `AgentHomes` gives, writing in none
([what no agent says](#what-no-agent-says)).

### D-23: What a project received is an installed view

The catalog stays about authored payloads: the library's and a project's own
files and folders, MCP definition files included. What a project has received
through an `AGENTS.md` block or an agent config entry is an installed view,
built from the ledger plus the target files, which status and the snapshot
show. `edit`, `rename` and `remove` aimed at an installed section or entry are
refused, with a hint to uninstall, or to change the library copy and update,
and foreign entries found in agent configs are listed as foreign. "Discovery
matches on `Layout` and nothing else has to know" then stays true for authored
files: a section or an entry is never authored in place but rendered from a
library artifact, and editing one in place is exactly what the Edited state
(D-04) detects. Rejected: new `Layout` variants, which would make discovery
parse `AGENTS.md` blocks and agent configs; and treating installed sections and
entries as artifacts editable in place, which lets three agents' config formats
leak into the lifecycle code. Unblocks #33, #45, #62, #93, #94 and #95.

Carrying it out, #45 lists the rules a project marked by its settings
received in `status`, from the ledger and the block of its `AGENTS.md`, and
never through discovery ([what each project has](#what-each-project-has)).
A lifecycle change finds no section, since it looks only where the project
authors, and #46 says so with a hint to uninstall: the front ends find what
to change through `lifecycle::find_to_change`, which refuses a name the
project authors nothing of but the ledger records as a rule it received
(`LifecycleError::Received`), where "nothing named" would deny what `status`
just listed. #94 refuses an MCP server a project received the same way
(`LifecycleError::ReceivedServer`), with a hint to uninstall it, or to
change its definition in the library it came from and update ([finding
what to change](#finding-what-to-change)).

### D-24: The local server is a versioned HTTP API

Validated by the owner on 2026-11-03, opening M8 (#215 to #219). The server's
HTTP is an API for scripts and tools on this machine, and the panel is its
first client: it stays bound to 127.0.0.1 behind the launch token, with every
guard it has ([who it answers](#who-it-answers), [what keeps other pages
out](#what-keeps-other-pages-out)). Reads are resources, each a `GET` with its
identifiers as query parameters, since a registry entry or a reference can
hold a `/`; a read that takes a structured body, a plan or a draft to judge,
is a `POST` that changes nothing. Writes stay typed tasks, previewed and then
run ([D-18](#d-18-the-panel-writes-by-running-commands)): every write is
still a command line run through the real parser, so the API can do nothing
the CLI cannot, and the transcript keeps it. A write answers with what the
command said, its outcome, its text, each warning with its code and whether
it succeeded, and the state is read again afterwards. Everything is under
`/api/v1`, and a route, a parameter or a field once published is a contract,
as the envelope's names are under `ENVELOPE_VERSION`: dropping or renaming
one, or changing what it holds, moves the API to `/api/v2`, and adding one
does not, while the snapshot's own `view::FORMAT` keeps versioning what the
snapshot holds. It is described by JSON Schema written from the Rust types
with `schemars`, and an OpenAPI 3.1 document written from the routes and
those schemas, committed and checked for drift in CI (#217, #218);
`schemars` because JSON Schema is also how an MCP tool describes its input,
so a later MCP front end would reuse the schemas, though none is started
until the owner says so. Rejected: keeping the private protocol and only
documenting it, which leaves every read behind one endpoint and a tagged
body; a REST resource for every write, which would put a second way to
change things beside the command lines; a typed result for each write, which
every command would have to build; reaching it from another machine, with
TLS and accounts; and an API server or daemon of its own, which
[D-17](#d-17-the-next-front-end-is-a-local-web-panel) already rejected.
Unblocks #215 to #219.

Carrying it out, #215 serves the reads from a table the backend hands the
server, each route's method, path, parameters and answer as data, so the
server matches and refuses by it without a rule of its own and the OpenAPI
document can be written from it, and moves the tasks, the switch, the
transcript and the events under the version, served from a table of the
server's own in the same way ([its routes](#its-routes-under-apiv1), [a
table of reads](#a-table-of-reads)). The panel's unversioned endpoints went in
the same change, since the panel ships in the same binary and was their only
client. Its follow-up, asked for in the owner's review, gives every answer
under `/api` that is no success a code beside its words, the server's own
conditions codes of the server's own (`Condition`), and has the backend say
whether a refusal names what is not there, a 404, or is unfit, a 422
([every refusal with a code](#every-refusal-with-a-code)). #216 hands the
launch token, and where the API is, to a script that did not start the
panel, through a file only the account reads (`mindweaver ui --token-file`,
[launching it](#launching-it-mindweaver-ui)). #217 describes what the API takes
and gives as JSON Schema written from the Rust types, the server's own
answers and the token file among them, a file a root in `docs/api/schemas`,
each found by the name a route gives its type, and checked for drift with
the panel's types ([the API's schemas come from
Rust](#the-apis-schemas-come-from-rust)). #218 describes the API as an
OpenAPI 3.1 document written from both tables and those schemas, the
server writing what is HTTP and the CLI each type a route names, served at
`/api/v1/openapi.json` as it is committed, and holds every answer the
tests get to it ([the API's description is written from both
tables](#the-apis-description-is-written-from-both-tables)). #219 writes
the guide somebody calling the API reads first, [docs/api.md](api.md),
every example in it a `curl` line and its answer, which a test sends to
the server and holds to what it answers ([the API's guide runs its
examples](#the-apis-guide-runs-its-examples)). The guide says no MCP front
end is there, and nothing more: none is started until the owner says so.

### D-25: Core is values and ports

Asked for by the owner on 2026-11-03: every entity core builds is an
aggregate of value objects, and core is hexagonal, so that infrastructure
can change, and interaction modules be added, with the least change to the
rules. Carried out on the branch that asked for it ([values, ports and
adapters](#values-ports-and-adapters)): `value` holds a hash, a redacted
text, an origin, an artifact's name, a revision and a project's id, each
going out as the text it is, so no contract moves; `port` holds a `Remote`,
a `LedgerStore` and a `Clock`, and `adapter` holds `gix`, SQLite and the
system clock as their defaults, injected as
[D-14](#d-14-core-owns-http-behind-a-seam) injects HTTP, the ledger a
facade over its store so no caller changed. The driving side is the command line every front end already runs,
so a new front end stays one more driver of it
([D-18](#d-18-the-panel-writes-by-running-commands)). Rejected: a port over
the filesystem, whose promises are the operating system's own calls, and a
second crate for each adapter, which would split one engine's rules from
the code that keeps them for no reader that needs the split yet. Still to
carry out: the values not named yet (a project's name, a registry entry),
an open question below.

### D-26: The product is Mindweaver

Asked for by the owner on 2026-11-04, to keep clear of intellectual
property that is not ours: "Mind Flayer" is a name from a published game.
The product is Mindweaver, and so is everything named after it: the crates
`mindweaver-core`, `mindweaver-cli` and `mindweaver-server`
(`mindweaver_core`...), the panel's package `mindweaver-web`, the second
binary `mindweaver` (`apps/cli/src/bin/mindweaver.rs`), and `mind weaver
<cmd>`, the same as `mindweaver <cmd>`. What was a flayer workspace is a
weaver workspace, and every identifier, test name and comment built from
the old name follows it: `WeaverWorkspace`, `WeaverCli`, `WeaverCommand`,
`WeaverConfig`, `run_weaver_with`, `WEAVER_FORMAT_VERSION`. `mind` keeps
its name, and so does a mind project and its `.mind`. The binary is
`mindweaver` rather than `weaver`, which OpenTelemetry's CLI already is on
many PATHs; the concept and the `mind` subcommand are the short `weaver`,
and so are the prefix an activated prompt shows, `(weaver:<name>)`, and the
alias `make dev/link` makes, `mw`, as `mf` was.

In two steps, so that no change of a name and of a file a user has go out
together. #228 renamed the product, the code and the commands, and changed
nothing on the disk or on the wire: the names Mindflayer writes outside its
own code stayed byte for byte, each held in the one constant that names it
where the code does (`WEAVER_DIR` for `.mindflayer/`, `WEAVER_CONFIG` for
`flayer.toml`, `LEDGER_FILE` for `mindflayer.db` and its backups, the
markers of the blocks in `AGENTS.md` and in git's exclude file, Claude's
rules folder `.claude/rules/mindflayer/`, the staging suffix, the variables
`MINDFLAYER_WORKSPACE` and `MINDFLAYER_HOME`, the shell function
`flayer-deactivate`, the panel's header `X-Mindflayer` and the cookie
`mf_<port>`). #229 renames those, reading the old ones and moving them only
when asked, as every format change does ([formats are
versioned](#formats-are-versioned)). The GitHub repository is renamed,
otsobide/mindflayer to otsobide/mindweaver, once the code says Mindweaver:
the owner's step, not an issue's. Rejected: one change for both, which
would have moved every user's workspace in the same PR as every
identifier; and `weaver` for the binary. Unblocks #228 and #229. The
repository was renamed on 2026-11-04, once #228 and #229 reached `main`
(#232); GitHub sends the old address on to the new one.

Carrying it out, #228 kept a few more names as they were, each for #229 to
decide: the codes `path_in_mindflayer` and `x_mindflayer_required`, since a
code once given is never renamed ([a code for what went
wrong](#a-code-for-what-went-wrong)), though the variants that give them
are `PolicyError::InWeaverDir` and `Condition::PanelHeaderRequired`; the
`.gitignore` a local project's `.mind` is given (`MIND_IGNORE`), whose text
says the old name and is matched byte for byte before it is removed; the
prefix of the panel's own cookies, `mindflayer-`, which keeps a browser's
colour mode and sidebar; and what an older build wrote that the tests read
back as it was, the ledger dumps and the markers under
`apps/core/tests/fixtures` and the version 2 `mind.toml` that only tests
still write. A new marker's comments name the new product and commands,
and so do the examples' (`examples/.mindflayer/flayer.toml`, whose path
alone is #229's), which changes no format. The agent probe was renamed with #18,
whose branch was changing it at the same time. `make install` and `make dev/link`
leave a `flayer` or an `mf` an older build put on the PATH where it is,
the user's to remove ([the reference](reference.md#getting-started) says
how).

#229 renamed them: `.mindweaver/weaver.toml` and `mindweaver.db`, the
blocks' `mindweaver:` markers, `.claude/rules/mindweaver`, the staging
suffix `.mindweaver-`, `$MINDWEAVER_WORKSPACE`, `$MINDWEAVER_HOME`,
`mindweaver-deactivate`, `X-Mindweaver` and `mw_<port>`, each held in the
constant that names it, the old spelling beside it (`FLAYER_DIR`,
`FLAYER_CONFIG`, `FLAYER_LEDGER_FILE`, `agents_md::FLAYER_BEGIN`...), and
the pair chosen by one word, `Names`. What a build makes is spelled the new
way; what it finds keeps the names it was found under, and is moved only
when asked, as a format is ([formats are versioned](#formats-are-versioned)):

- **A workspace** a Mindflayer build made is a `.mindflayer` holding
  `flayer.toml` and `mindflayer.db`, found walking up, named, activated,
  and as the default in home while no `~/.mindweaver` is there, and read
  and written under those names, its ledger migrated and copied aside under
  its own (`mindflayer.db.v<N>.bak`). Nothing makes a `.mindweaver` beside
  it: `init` takes it for the workspace there. A folder holding both is
  the new one, the old one left over, which the line typed says, as a
  project's older marker is named. Every guard that looks for a workspace,
  the path policy's, an install's targets, the shelf's reserve and the
  browser suite's, knows both names.
- **A block** in `AGENTS.md`, `CLAUDE.md` or git's exclude file is read
  under either spelling of its markers and rendered again under the
  spelling it was found with, a section or a group added to it included:
  one spelled two ways would be read whole by neither build, and spelling
  it the new way at the next render would move a name nobody asked to
  move. Only a block made anew says `mindweaver:`. A marker spelled
  otherwise than its block's begin marker is refused, as a second block is
  (`markdown_markers_mixed`, `exclude_markers_mixed`).
- **Claude's rule files** are looked for where the ledger records each, a
  file a Mindflayer build recorded staying in `.claude/rules/mindflayer`,
  written over and taken away there, while a rule recorded nowhere gets a
  file in `.claude/rules/mindweaver`, unless something is at its place in
  either folder, which is somebody's: per file rather than per project,
  since a file is recorded by itself where a block is one unit, so a
  project's rules can sit in both folders until it is moved, each read by
  Claude once.
- **The staging suffix** is the new one whatever a workspace is, being
  gone once an install ends, and the next install clears what an
  interrupted one left under either. **`MIND_IGNORE`** says Mindweaver,
  and a `.gitignore` holding the text a Mindflayer build wrote is taken
  for Mindweaver's too when a project turning shared drops it
  (`FLAYER_MIND_IGNORE`), so none is stranded.
- **The variables** are read new first, and the old only where the new is
  not set at all, said in a note: a warning would fail every script run in
  a shell an older `activate` set up, and reach its envelope, for nothing
  the script did. `activate` runs the old `flayer-deactivate` before it
  activates, so the old activation is undone by its own code.
- **The wire** changed outright, with nothing of the old read: the panel is
  embedded in the binary that checks its header, and a cookie holds one
  launch's token. The panel's own preference cookies are `mindweaver-`
  from now on, and one under `mindflayer-` is read once, where none is kept
  under the new name, and moved: ten lines keep a browser's colour mode and
  sidebar across the rename, where renaming outright would forget them
  once, and reading the old name on every load would keep it forever. The
  codes `path_in_mindflayer` and `x_mindflayer_required` stay, as every
  code given does; the condition each names is the one it always named.

The move is `mindweaver migrate --names` (`migrate::names_plan`,
`migrate::names`), and `mind migrate --names` on the workspace a project's
commands use: named for what moves, the names written on the disk, beside
`--library`, named for where the workspace's own go, rather than
`--mindweaver`, which names the product and would be wrong again after
another rename, or `--rename`, which reads as the command that renames an
artifact. A plan first, a dry run without `--yes`, as `migrate::library`
and `migrate::project` have it, and under the workspace's lock: each
project's blocks spelled the new way (but one git tracks in a local project
without `edit_tracked`, left under the old markers as a render leaves it,
`install::rules::withheld_from`, since D-02 keeps a team's file out of reach
for a name as for a rule) and each rule file it recorded
carried to the new folder with its record and its exclude line (one
changed since left where it is, its record naming it there, and one not
there pointed at the new folder all the same, since left naming the old
one it would be written there again by the update that puts it back), then
the
ledger's shelf paths, the marker's folders inside the old one, the marker,
the ledger and every copy kept of either renamed, and the folder last. A
name taken where anything would go, a block that cannot be read, a project
whose marker cannot and a ledger another process has open, whose log a
rename would leave behind, refuse the whole move; a step that fails puts
back every step before it. The ledger is renamed, never migrated: it is the
same schema, and rows naming the old places are rewritten by the move, as
`relink` rewrites a project's. A workspace under the new names is planned
too, its projects alone: what a first move could not take (a project it
could not find, a rule file changed since and put back as it was written)
is taken by a second, where a plan that looked no further than the
workspace's own folder would leave it under the old names for good, and
what is left as it is is said each time, as the reason a name is still
there. No copy of the marker is kept, since only
the values of its folders change, each by its first component. Git is
never run: each rule file git tracks that the move carries is said with
the `git rm --cached` and `git add` that take the move in, by its route
rather than the folder's, since a `git rm -r --cached` of the old folder
would take a file the move left there out of the team's repository too. A line typed on a workspace still
under the old names ends with a hint naming the move, once a line, read or
write, since whoever typed it is whoever can move it; the TUI's and the
panel's lines read no environment and say nothing, their own launch saying
it once as it closes.

## Open questions

- **The ledger's port is shaped by the store it came from.** `LedgerStore`
  is the SQLite store's public methods as they were
  ([values, ports and adapters](#values-ports-and-adapters)), so a second
  store implements what only SQLite's past needs: `tainted_sources`,
  `scrub_source` and `scrub_log` repair rows an older build wrote with a
  credential in them, which a store made today never holds, and `path`
  names a file a store kept elsewhere may not have. Splitting the repairs
  off into a port of their own waits for a second store to say which of
  them it can answer, rather than guessing now.
- **Values not named yet.** A project's name, a registry entry and a path
  inside a project are still `String`s, and so are an error's fields, as
  the read types that hold them were. Each joins `value` when a rule is
  found that it holds, as `ProjectId` did; one with no rule of its own, a
  name named apart, joins when a mix-up between two of them is found.
- **What no run has moved of an MCP server.** Every name an agent keeps
  for itself and every shape it cannot express is docs-only
  ([docs/agents.md](agents.md#mcp-servers)), since none of #18's checks
  asks it: which spelling of `Claude Preview` Claude compares, whether
  Codex keeps a name of its own, whether OpenCode's V1 line takes sse where
  its V2 shape does not, and whether it fills `{file:}` in as it does
  `{env:}`. A fact a later run moves changes one table in `mcp::name` or
  one arm of `McpDefinition::inexpressible`. So does V1's shape having no
  key for a program's folder, read from its docs, which `opencode_mcp`
  leaves out of an entry it writes in that shape (`opencode_mcp_v1_cwd`):
  a run of an entry with `cwd` in V1's shape would move it.
- **OpenCode's other config files.** `opencode_mcp` writes into the first
  of a project's four config files there is and reads none of the others
  ([OpenCode's `opencode.json`](#opencodes-opencodejson)), as #92 orders
  them. OpenCode's docs say it merges every config it finds, the
  project's files among them and those of the folders above it up to the
  repository's root, so a server of the name in another of them would be
  read beside Mindweaver's, merged with it key by key as Codex's user
  config is. Refusing a name another of them holds, as
  `codex_mcp_collision` does, waits for a run that says what the merge
  makes of two entries of one name; until then a server is written into
  the file #92's order chooses whatever the others hold.
- **A server Codex's user config names too.** `codex_mcp` refuses a name
  `$CODEX_HOME/config.toml` holds under `mcp_servers`, since Codex merges
  the two key by key ([Codex's `.codex/config.toml`](#codexs-codexconfigtoml)).
  Whether that breaks more than the one server, the whole configuration or
  the others, is what `codex-mcp-collision` records, and it was never run:
  a run finding the project's layer or the user's winning, the name
  dropped, or both started, makes it a warning, and a refusal stays for the
  rest.
- **Which path Codex trusts a project by.** The trust warning looks up the
  project's root as it is linked and as its links resolve to. Codex may key
  one by the repository's root above a project linked below it, or a linked
  worktree by its main checkout's, and the warning is then said of a
  project Codex trusts. A run of `codex-mcp-untrusted` with the project
  trusted under another path, or a reading of Codex's source, makes the
  lookup exact; the entry is written either way.
- **Tokens of other shapes.** The check for a literal knows the shapes #87
  named and nothing else: an AWS key, a Slack token, or a password in an
  address's userinfo shaped like none of them, passes, the last kept out of
  the definition as every address's userinfo is, but not out of its file. A
  shape added is one line of `mcp/token.rs`'s table. Nor does it read a
  key of a program's `env` or of an address's `headers`, which names a
  variable or a header and is written into an entry as such, nor a
  definition's description or provenance, which say what a server is and
  are written into no entry yet: a renderer that writes either (#90 to #92)
  adds it to `McpDefinition::literals`, where the transport's values and
  the extras are looked through.
- **Twins the panel cannot repair.** Two definitions that clash by case and
  each hold another problem are repaired in an editor or by hand
  ([which names a server is given](#which-names-an-mcp-server-is-given)),
  and the panel has neither: `edit --contents` and `rename` refuse each,
  so removing one is all it can do. The MCP page's form (#97) changes a
  server's transport through the same rules (`lifecycle::retransport`), so
  it refuses such a pair too, by the clash. Whether a change of contents
  passes over a problem those contents cannot change (a clash, or a
  reserved name a hand-made file is filed under), as a rule's contents are
  never refused for its route, is still open.
- **Names that clash by case across holders.** Two definitions clash by
  case only in one folder: a project's own `github` and the library's
  `GitHub` are told nothing of each other until an install (#93) would put
  both in one agent's config, which is then the install's to refuse.
- **What a command line cannot say of a server.** `add mcp` puts every
  `--arg-env` after the `--arg`s, since clap hands each option's values
  apart and their order between two options is lost; an argument read
  from a variable in the middle is written by hand, or `add` reads the
  options in the order typed (`ArgMatches::indices_of`) and `Given` keeps
  one list of both. An input is named after the variable it is read from,
  so one variable read in two places (`--secret-env TOKEN --arg-env
  TOKEN`) is two inputs of one name, which `validate` refuses
  (`input_repeated`), where a definition written by hand names them apart.
  Nor does `add` say an input is `required`, a secret argument or one in
  an address, a program's `cwd`, provenance or extras: each is written by
  hand. The MCP page's form (#97) says what the options say and nothing
  more, so a definition holding any of them is not one it starts from
  (`Given::of`), and is changed in its file; a form saying more needs the
  options to say it first.
- **What the panel draws of an MCP server elsewhere.** The MCP page (#97)
  draws each server as each agent would be given it and adds and changes
  one from a form, but three pages still draw it as they draw the other
  kinds where it is not one: an artifact's page puts its definition, which
  is TOML, through the markdown renderer, as the body a skill's front
  matter is cut from; the page adding one of the library's own offers
  `mcp` among the kinds, writes one from a template of the kind, and
  otherwise says core's refusal (`add_mcp_unscaffolded`) once a name is
  typed, where the MCP page's form would; and the projects table gains a
  column, which at the README's width pushes its `stale` column out of
  view. Each wants a word from core (whether a kind's file is markdown,
  which kinds `add` writes from a name and a description alone,
  `create::scaffolds`) rather than a special case of the kind's name in
  TypeScript.
- **An imported value holding its own placeholder as text.** Format 1 has
  no escape, and needs none while a definition's author chooses its inputs'
  names ([what an MCP server's definition
  declares](#what-an-mcp-servers-definition-declares)). A registry entry
  (#106) whose value holds `{name}` as text while it declares a variable of
  that name is the one case an author does not choose: the import renames
  the input, or the format gains an escape, which an older build would read
  as text.

- **A known workspace that has gone is offered nothing.** The page of
  loads and the header's switcher list it, with why it does not open, and
  offer neither a load nor a switch, which would be refused, nor `mindweaver
  workspaces forget`, which would take it off the list. Forgetting changes
  what the machine knows rather than what the workspace on screen offers,
  and is a write to the default workspace's marker, which a task of its own
  would carry; the switcher, whose list is the machine's, may be the place
  for it.
- **A task shown before a switch made in another tab.** A tab lets go of
  the task it shows once the stream tells it `switched`, or, having missed
  that, once its next `resync` counts more switches than it heard, as on
  being shown again; but one sent to run in the moment between the switch
  being taken and that word arriving runs against the new workspace, under
  lines its preview showed naming the old one: the server previews it
  again on the writer thread, so the transcript and the run's answer say
  what really ran, but the person agreed to other lines. A preview carrying the workspace it was of, which
  the run would be held to, would close it, at the price of the server
  carrying one more thing it cannot read.
- **The activated workspace beside a `-W` at launch.** A panel launched
  with `-W` does not read `$MINDWEAVER_WORKSPACE`, as no command does once
  `-W` is given, so the shell's activated workspace is listed among what it
  may switch to only when this machine knows it. Listing it would mean
  reading the variable the launch was told to pass over.
- **The palette offers Update all whatever an update would do.** Whether
  one writes anything is core's survey (`Query::Updates`), which reads
  every origin again, and which the page of updates asks for and greys its
  button by; the palette would ask for it each time it opened. Asked only
  while the palette is open, it could say what the button says, at the
  price of that read on every opening; until then, choosing it shows the
  line, and its run says what it did.
- **The palette lists twelve, and says nothing of the rest.** Nuxt UI's
  palette cuts a search to its twelve nearest entries in all, and each
  group, with nothing typed, to its first twelve: a workspace with more
  projects than that lists only twelve under **Projects**, or under an
  offer to install, until enough is typed to narrow it. A larger
  `resultLimit` would draw every entry the snapshot lists each time the
  palette opens, the whole shelf among them, which a long list would want
  virtualized; saying how many are left out would be a count the snapshot
  does not carry. Until a workspace that large asks for one or the other,
  typing is the way to the rest, as the reference says.
- **The palette offers to link whatever is typed.** Telling a path from a
  word is the path policy's, on the server, so `Link deploy` is listed
  under what a search for `deploy` finds, and refused with why if chosen.
  Asking the policy as the text changes would take a query of its own, and
  guessing from the text, a separator say, would be a second policy.
- **Ctrl+K is the browser's too.** Some browsers put the focus in their
  search on Ctrl+K; the panel takes it while its page has the focus outside
  a field, or the palette is open, as many web applications do. A key of
  one's own would leave the browser its own, but keys a person chooses are
  for later.
- **A field taken away with the focus in it leaves the keys off.** vueuse,
  which Nuxt UI asks what has the focus, is not told when a browser takes
  away the element holding it, so the shortcuts stay passed over as typed
  into a field until something else is focused. The palette lets go of the
  focus as it closes, its field going with it; any other overlay, or a page
  that takes away a field while it has the focus, would have to do the
  same, and nothing yet holds one to it.
- **The explorer lists any folder the account may list.** The path policy
  holds what a request may name, not where on the disk it is, so a page
  holding the token learns the names of folders anywhere the account may
  read, as a scan of a root it names could already, though nothing a file
  holds. Holding the explorer to the home, or to the workspace's folder,
  would refuse the repository kept elsewhere that somebody browses to
  find; the token and the guards are what keep other pages out.
- **The explorer orders names by their bytes.** Core sorts a folder's
  names as `ls` does with no locale, so `Zeta` comes before `alpha`. The
  reader's order is the browser's to know, and sorting there would be a
  second order of what core listed; a listing told the locale would be
  core reading something of the front end's it has never needed.
- **On Windows the trail ends at a drive.** The trail goes up to the root
  of the folder listed, `C:\` say, and nothing lists the other drives,
  which only a path typed reaches. Development is Linux's for now (#154).
- **The TUI's link form has no explorer.** It keeps its text field, where
  the shell's completion is not to hand either; the minds screen's scan is
  how it finds a folder, and a list of folders on a screen of its own
  would be the TUI's version of the panel's explorer, over the same core
  listing.
- **`mind load` is no page's.** The panel is launched on a workspace and
  loads into that one; a project's own load into the workspace above it
  (`Task::LoadHere`), which the TUI offers when it is opened on a project,
  is admitted by the backend and offered by no page, since no page shows a
  project no workspace links.
- **A save's line names a file gone once it ran.** The transcript keeps
  `mindweaver edit --contents .mindweaver/tmp/edit-<hash>.md ...` as it ran, and
  the backend takes the file away after, so the line pasted into a shell
  reads nothing. Keeping the text beside the transcript, or a line that
  hands it on standard input, would let it run again, the one at the price
  of a folder of every text saved, the other of a line no front end without
  a terminal runs.
- **The workspace's scratch folder is not reserved.** `.mindweaver/tmp` holds
  the text a save hands `edit --contents` for as long as the line runs, and
  nothing stops a workspace's own `directories` naming it, as they may name
  any folder of `.mindweaver` but the shelf and the cache
  (`gathered_overlap`): a rules folder there would list the text as a rule
  for that moment. Refusing it as the shelf is refused would need words for
  a refusal that is not about gathering.
- **Two servers saving the same text at once share a file.** The scratch
  file is named by what it holds, so two `mindweaver ui` on one workspace saving
  the same text into it at once write, and take away, one file; the second
  line then reads nothing, and writes nothing. Each server's runs take
  turns, so one panel never meets it.
- **Adopting is offered on every foreign row.** Core adopts only a skill an
  install could have written, and refuses the rest when the line runs, a
  rule among them; whether a row can be adopted is not in the snapshot,
  since telling it means hashing every copy of every foreign row on every
  change. A word for it, worked out only for a row somebody points at, would
  let the panel offer adopting only where it can be done.

- **Nothing says when the screenshots are stale.** `make docs/screenshots`
  takes them again, and AGENTS.md asks a change to what the panel shows to
  run it, but no check fails when a page has changed and its image has
  not: the images differ at every run (the port in the sidebar, when the
  shelf was gathered), so a byte-for-byte check in CI would always fail,
  and comparing pixels within a tolerance would need Chromium in a job,
  and a tolerance somebody tunes.

- **What of the snapshot no command prints.** #64 asked that whatever the
  panel shows, a script or an agent can print. `list --json` prints the
  snapshot's holders, `templates --json` their templates, `gather list
  --json` what the shelf and the loaded workspaces offer and `workspaces
  --json` the workspaces this machine knows, but no `--json` prints the
  rest of it: the workspace it is of and why that one (`workspace`), the
  workspaces it loads, each with what it offers or why it cannot
  (`loaded`, which the TUI draws), the sources the shelf is filled from
  (`sources`), the latest actions (`history`), what it says belongs to
  no holder (`warnings`), or what the overview counts (`summary`). Which
  command prints each, a `--json` of its
  own, a history command, or the snapshot whole, is left for the issues
  whose views show them, so that each is printed as the view that needs it
  reads it. #73 draws the loads, the sources and the history from the
  snapshot, and prints none of them: a `--json` for each was no part of it.
- **A plan says no agent a change reaches.** The issue that brought plans
  into core (#78) counted which agents a change reaches among a batch's
  rules, but the install screen held none: an install reaches every
  folder `install::targets` gives when it runs, and a project's agents are
  the snapshot's. A plan saying it of each change would be a second
  reading of the targets beside the install's own, which could come to
  disagree with it. The matrix (#79) went without: each project names its
  agents beside its column, and filters by them, and once a batch ran, a
  project's page says which of them read each copy. A line of the plan
  naming the agents it reaches is still the one thing the matrix shows
  only after a run.
- **An installation nothing offers any more holds the box of its name's
  offer.** A name the project holds ticks one of its offers' boxes
  (`install::holding`), so one installed from a place that no longer
  offers it ticks the box of another place offering the name, as the
  screen always has, so that unticking it takes it out. Asking for that
  offer alone is then asking for what is so, and changes nothing; asked
  for with the origin it came from asked away, by the box the snapshot's
  `origin` gives it, it is installed in its place. The matrix (#79) gives
  such an installation no box of its own: its rows are what an install
  offers (`offers`), and one whose name nothing offers is the project's
  page's and the page of updates', which lists it under `orphaned` with
  its uninstall, and all of them with `uninstall --orphaned` (#80), rather
  than a box of the matrix's: a box is an offer to tick, and nothing
  offers it.
- **The page of updates reads the plan again on every change.** It is a
  query of its own, as the matrix's plan is, followed by the live updates
  while the page is open, so a change anywhere reads where every
  installation came from again. Folding it into the snapshot was the
  other way, every page paying for it; one change of a large workspace
  reading it twice, once in the snapshot's states and once here, is the
  price, and the two can disagree for the moment between their reads,
  which a row reading one state on the projects table and another here
  would show.
- **A row is forced by core's word, not told from its state.** An edited
  copy nothing offers any more steps as `orphaned` and stands as `edited`,
  and only a forced uninstall takes it out, as removing one of the
  library's own everywhere leaves one. The panel could have told that from
  the two words, which would be a rule of core's, which refusal an
  uninstall meets, spelled again in TypeScript; core says it instead
  (`Pending::forced`), and the uninstall task takes `force` as the update
  task does, behind the same confirmation. `--orphaned` stays unforced,
  leaving such a copy to its row, where it is read before it goes.
- **The panel removes one of the library's own everywhere or not at
  all.** Removing the library's copy alone, which the CLI and the TUI
  still offer, would leave every copy installed from it orphaned, which
  the page of updates would then list; the dry run says what removing it
  everywhere leaves, and offering the other removal beside it would be a
  second button that leaves more behind.
- **Every snapshot surveys every offer in every project.** The boxes are
  told as the install screen tells them, an offer's place looked at in
  each folder a project installs in, so a live update of a workspace of
  hundreds of offers and projects reads that many places again, on every
  page and not only the matrix's. A query of its own, asked by the matrix
  alone as the scan is asked by its page, was the other way, at the price
  of a second read the live updates follow and a second answer that could
  disagree with the snapshot drawn beside it; a workspace that makes the
  survey slow would move it there.
- **The benchmark times happy-dom.** The matrix's first render is held to
  300 ms in the panel's own suite, which lays nothing out and builds its
  document in JavaScript, so it is slower than a browser at the same work
  and says nothing of painting. The browser suite times nothing; a bound
  there would be one more number a runner's load moves.
- **An unload's run once took five and a half seconds.** In one run of
  CI's Chromium, `e2e/loads.spec.ts` found the unload's command dialog
  still drawn five seconds after Run was clicked (#238). The trace CI kept
  says the dialog was the unload's own, its buttons held as they are while
  a run goes on, and the server answered the run 5508 ms after it was
  sent, half a second after the spec stopped waiting, where the load
  before it, a change to the same marker, took 952 ms, and a snapshot
  asked meanwhile 9 ms. Read further (#240), it says where in the run the
  time went. The event stream told the unload had started and that the
  workspace had changed, and never that it finished before it closed; the
  snapshot asked on hearing the change, 1.6 s after the run was sent, was
  byte for byte the one from before the load, so the marker had been
  renamed into place about 1.2 s in. The answer came four seconds after
  that, written while the server stopped on the Ctrl-C the test's teardown
  sent it half a second before (`Connection: close`). All the line had
  left to do then was read the marker back, let go of the lock, taking
  `lock.pid` away, and say what it did, and the writer thread to keep the
  run and tell the stream: none of it waits on anything of the panel's.
  Not the workspace's lock, which it held already, and which nothing else
  in that test takes, its twin being a copy elsewhere; not the writer's
  queue, which held it alone; not the event bus, a broadcast that never
  blocks a sender, whose lock the stop took at once; not a subprocess, of
  which an unload runs none; nor resolving the workspace, which `-W` names
  and the preview had resolved in 1.5 ms. In that run writes were slow,
  the load taking 952 ms where it takes a few milliseconds here, and
  reads were not, which
  is how waiting on a disk looks: CI's runner keeps its temporary folder,
  where the suite builds its workspaces, on a disk two browsers recording
  their traces, the other worker's install and its ledger's commits share,
  where this machine's is a RAM disk. The likeliest cause, and not shown:
  300 runs of the spec here, on the release binaries, the whole Chromium
  suite on two workers held to four processors with `taskset`, every test
  traced as CI traces it, never took longer than 15 ms to answer an
  unload, half of them 2.4 ms or less, nor 217 ms to answer any of the
  5060 runs of every spec beside them. So nothing is changed for it, and
  the spec is left as it is, since a longer wait there would hide a run a
  person waits for too. A test that fails keeps instead its own server's
  log, which says how long each run waited its turn, ran and waited for a
  workspace's lock, and how long the machine kept tasks waiting for its
  disk while it ran ([end to end, in a
  browser](#end-to-end-in-a-browser)). Should the next say the disk, what
  to change is where the suite works on CI, not a timeout.
- **Only the library's own open.** An artifact's page reads the
  workspace's own, the one holder of the snapshot an install takes from
  that `Query::Files` and `Query::Contents` read. What is on the shelf has
  no holder there, and a loaded workspace's own are counted by the
  snapshot, not listed, so neither opens yet, nor does a project's own from
  its page, though both queries read a linked project. A shelf holder would
  be a holder no task takes, which `task::Holder` is shared with; a query's
  holder of its own, with the shelf among its places, would open them.
- **A skill's links to other sites stay links.** Its markdown keeps an
  `https:`, `http:` or `mailto:` address as a link, opened with no referrer
  in a tab of its own, so somebody reading a gathered skill can follow what
  it cites. A skill could point at a page made to look like the panel; the
  link's title says its address. Showing every address as text was the
  other way, at the price of every reference a skill cites.
- **The TUI still counts a row's problems itself.** Its project rows show
  how many of a project's artifacts do not read or have something
  `validate` says, and its failures, counted in the CLI from the
  snapshot's rows (`Member::problems`), from before the snapshot said a
  project's `health`. Showing the health instead would make the two front
  ends agree on which projects want looking at, at the price of a row that
  no longer says how many things do.
- **The panel sorts names and paths as the browser collates them.** The
  projects table sorts every other column by what the snapshot says, its
  counts and the order it lists healths in, but text by the browser's
  collation, which no command shares: `mindweaver list` keeps the registry's
  order. Core naming an order for text, as it names one for healths, would
  make the two agree the day a command sorts by name.
- **The commands an empty state names are not quoted.** Each names the
  workspace's path after `-W`, and a project's page its entry, as the
  registry spells it, after `-p`, `relink` or `unlink`; one a shell would
  split, a space in it, is shown unquoted: quoting is the CLI's
  (`task::shell`). A line the server previews names `-W` too now, quoted,
  but these are for a shell rather than tasks the panel offers, with `<PATH>`
  and the like in them, so no preview stands in for them.
- **The server's replies are spelled twice.** A preview's `{lines,
  confirm}` is the server's `Preview`, a run's `{lines, outcome}` its
  `Ran`, a refusal's `{error, code}` its `Refused`, and the stream's events
  are its `Event`, each with a schema of the API's under the server's
  `schema` feature, and all are spelled again in the panel's
  `transport.ts`, outside what is written from Rust ([its types come from
  Rust](#its-types-come-from-rust)): the server has no `ts` feature, and its
  own tests hold what it answers and tells. `TS` derived beside their
  schemas under a feature would bring them under the drift check, at the
  price of ts-rs in the one crate that depends on no mindweaver crate.
- **The TUI asks its own questions.** It asks before an unlink, a removal,
  a relink, an adoption and applying the install screen, whatever its marks
  hold, in words of its own for what they are about; the panel asks before
  every line `task::Caution` names, before an unload too, and before no
  relink or adoption, which take nothing away. The TUI building its
  questions from the same list, worded for its rows, would make the two
  ask about the same lines.
- **A confirmation is of the lines previewed.** The run is planned again on
  the writer thread, and runs the lines the disk then gives, which the
  transcript says; a line that changed in between, an entry pointed
  elsewhere, a default workspace made meanwhile, runs without being asked
  about again. Sending the previewed lines with the run, to be refused
  when they differ, would close it, at the price of a run request shaped
  around the task.
- **Nothing says when the panel's run waits on the stream.** A run whose
  finish the stream never tells waits two seconds (`PATIENCE`) before the
  panel reads again for itself, which a tab whose stream was let go, or a
  server that stopped telling, costs on every run; the runner says nothing
  of it while it waits.
- **What each query is answered with is the panel's to name.** `Answers`
  pairs each query's `type` with the type it is answered with, and vue-tsc
  holds it to name every query, but nothing holds the pairing to what the
  backend answers, since `Backend::query` answers any JSON. A query type
  that says its answer in Rust, written into the TypeScript with it, would.
- **The panel spells each route again.** `route` in `http.ts` turns each
  query into its route and parameters by hand, beside the CLI's table: a
  route the table renames is a 404 the panel meets only once it asks,
  which the CLI's test of each route's query and the browser suite catch
  between them, but no type does. The table written into the panel, as its
  types are, would let the transport build each request from it; the
  OpenAPI document written from the table ([the API's description is
  written from both tables](#the-apis-description-is-written-from-both-tables))
  is one way there, which the panel does not read yet.
- **A token file can be read before it is whole.** It is made empty and
  written at once, before the link is printed, so a script that waits for
  the link finds it whole, but one that waits for the file to appear may
  read it in between, and has to read it again until it parses. A
  temporary linked into place with `link`, which never replaces anything,
  would have it appear whole, at the price of a second file holding the
  token beside it for a moment, and of file systems that make no hard
  links.
- **A panel killed leaves its token file.** Only a stop `ui` sees takes it
  away, a Ctrl-C or, on Unix, a `SIGTERM`: a `SIGKILL`, a `SIGHUP` (left
  to its default for `nohup`'s sake), a Windows console closed, an abort or
  a machine that goes down leaves the file, holding a token that opens
  nothing, and the next launch naming that path is refused until somebody
  takes it away. A file that said which process wrote it would let a
  launch tell one left behind from a live panel's, but a process id is
  reused, and telling the two apart is then a guess.
- **On Windows the token file is as private as its folder.** It takes the
  permissions its folder passes on, which in the account's own folders
  keep other accounts out but in a shared one do not; an ACL of its own,
  for the account alone, is #154's, with the rest of Windows.
- **A refusal lists every code core and the CLI give.** The 404 and the
  422 a backend's refusal is answered with list every code of core's and
  the CLI's own, not those the route's query can meet, which nothing says
  but the code each query runs; and a 404 is listed for every read named
  by its parameters, `/scan` among them, whose roots are refused for how
  they are spelled and never found missing. Each route naming the codes it
  can give would hold the document closer, at the price of a list beside
  each query that no type holds to the errors it returns.
- **The cookie's scheme names no cookie a tool can send.** The launch
  link's cookie is `mw_` and the server's port, one for each port so two
  panels open at once keep their tokens apart, and OpenAPI's security
  scheme for a cookie names one name: the document says `mw_<port>`, which
  a client generated from it would send as it is. A script sends the
  token as Bearer, which the document says too, and the cookie is the
  panel's page's.
- **The events' schema is under an extension.** OpenAPI 3.1 has no place
  for what each item of a stream holds, so the document says it under
  `x-itemSchema`, 3.2's `itemSchema` as a 3.1 document may carry it, which
  no tool of 3.1 reads. Moving the document to 3.2, once the tools that
  read OpenAPI read it, would put it where they look.
- **Nothing builds the schemas without the TypeScript.** `make
  api/schemas` builds the `schema` feature with `ts`, for the test holding
  every root of the panel's types to a schema, and `make lint` lints the
  two together, so code under `schema` that leaned on `ts` would compile
  everywhere it is built, and fail only for whoever builds `schema` alone.
  A run of clippy with each feature on its own would catch it, at the
  price of another check of the workspace in every lint.
- **WebKit where Playwright builds none.** Playwright's WebKit is built
  against Ubuntu's libraries, which its `--with-deps` installs there and on
  Debian only, so on Arch and most other systems the browser suite runs in
  Chromium alone, and a page WebKit draws differently is found in CI. A
  WebKit of the system's own (WebKitGTK, which a Tauri shell on Linux
  would use) is no browser Playwright drives.
- **The browser suite reads from one fixture, and changes its own.** A
  test that changes something builds and serves a workspace of its own,
  which costs a `mindweaver ui` a test; one server for every writing test,
  taking turns, would cost less, at the price of tests that wait on each
  other in two browsers. And its fixture holds only
  what commands make: an install changed since, a copy edited by hand, a
  linked folder gone or a shelf, which take a hand edit, a removal or a git
  source and a `git` binary to make, are drawn by the views' suites from
  the snapshots a test of the CLI writes, never end to end. So is a rule
  installed, whose block in an `AGENTS.md` outside a work tree is a
  warning.
- **The browsers keep the developer's home.** Only the server and the
  commands get the temporary `HOME`. Playwright, and the browsers it
  starts, each with a profile of its own, keep the developer's: they reach
  nothing of Mindweaver's but the server, and a run that names no
  `PLAYWRIGHT_BROWSERS_PATH` finds its browsers through it. Nor does the
  suite run on macOS or Windows: the webServer's command is a POSIX
  shell's line.
- **What no view reads has no TypeScript.** `status` and `workspaces`
  print a `Status` and a `Known`, and `gather list` its `Offer`s, which no
  root names, so the panel has no type for them yet, nor the API a
  schema; nor does `scan --json`
  have one for its envelope's `Scan`, the panel reading each `Found` inside
  the backend's own answer (`Scanned`), nor core's `install::Plan` and its
  `Change`, a plan reaching the panel as the backend's `Planned`, its
  changes the tasks that run them. Each joins a crate's
  `typescript::roots` when a view reads it, so that the folder holds what
  the panel compiles against, and no more.
- **The page of what could be linked links, and unlinks nothing.** The
  minds screen unticks a linked project to unlink it in the same batch as
  what it links; the page lists those linked as linked and nothing more, so
  several are unlinked one at a time, from each project's page. The marks
  it sends could hold an unlink, which the backend admits as the screen's,
  so a box on a linked row is all it would take.
- **A migration from the panel takes the command's agents and share mode.**
  `--agents` and `--local` are not offered, so a project a `mind.toml` marks
  moves with every agent and shared, as it was committed, and its toggles
  then change both, which carries its copies a second time. Toggles on the
  migration, sent with it, would move it once.
- **The toggles start from what the snapshot read.** `was` is the settings
  as the panel last read them, which a change made elsewhere since, and not
  yet streamed, leaves behind: the lines still name the agents and the mode
  outright, so what a line sets is what the toggles say, but a line may run
  that sets what the disk says already, or one be left out. The form starts
  again from what the snapshot says whenever it reads the settings again.
- **`list --json` reads twice.** Its warnings and exit status are those of
  the listing, read as the text reads it, and its data a read of the
  snapshot's beside it, so a file changed between the two can be a warning
  and a row that reads, or the other way round. Working the warnings out of
  the snapshot's read would make it one read, at the price of an exit
  status the text does not have: the snapshot also says what the listing
  never reads, a template that will not read or a ledger that will not
  open.
- **A copy nobody linked.** `mindweaver relink` refuses a copy it can see: the
  entry still holding the id, or another project the workspace links
  holding it. A copy somewhere on disk nobody linked is beyond what it
  reads, so a project moved and also copied, both holding its id, is
  followed to whichever directory the command names. `mindweaver scan` says the
  two are one id and offers neither as a move, and the TUI relinks only what
  a scan offered. Scanning from the relink would close it, at the price of a
  walk of the disk to change one line of a marker, and of an answer that
  depends on where the walk looked.
- **A moved project linked again by hand.** Linking the directory a project
  moved to, rather than relinking it, leaves two entries for one id: the
  stale one keeps it, for a relink, and the new one is known by none, so
  what is installed there from then on is recorded with no id. `mindweaver
  relink` refuses a directory linked already; unlinking it first hands the
  stale entry's records to it and drops those, which an install made since
  wrote. Merging the two, the newer record of a name winning, would keep
  both, at the price of a relink that sometimes drops an entry rather than
  rewriting one.
- **A project moved before its id was learned.** The ledger learns ids when
  it is written, so a project linked and moved before then is never offered
  as a move; the scan names its entry (`untraced`) for a relink somebody
  types. `mindweaver link` telling the ledger would close it, at the price of
  every link opening the ledger to write, and making it, which only a
  gather, an install or an uninstall does now.
- **A route that holds another project now.** An entry whose directory is
  given to another project once its own has moved opens, so it is not stale:
  the ledger follows the project there (`Ledger::knew`), and the records the
  route had become the newcomer's, which names copies it may not hold. The
  moved project is then found by nobody. Telling the two apart needs more
  than the route and the id: the records' copies being there, or the
  settings' name, which a person can change.
- **Adopting into one project at a time.** `mindweaver adopt` takes one
  project's copy, so a skill several repositories keep by hand, each its own
  copy, is adopted from one of them, and in the others it is still foreign,
  and refused by an install of the adopted one. Adopting from every project
  holding the same bytes at once would bring them all under one record each,
  at the price of a batch that must agree on what the skill is before it
  writes anything, and say why one project differs.
- **A tracked copy in a local project, written by an update.** An adopted
  copy git tracks stays in git, so an update in a local project changes a
  file the team committed, where local mode otherwise edits a tracked file
  only with `edit_tracked`. Adopting by name is taken for consent, and the
  dry run says so; holding such an update back until the project is shared
  or sets `edit_tracked` would make the consent a setting, at the price of
  an adopted copy no update reaches by default.
- **A snapshot is read whole.** Every update reads every project's folders
  and the ledger again, so a workspace of many projects pays for all of them
  each time one changes. Rebuilding only the projects a change's scopes name
  ([live updates](#live-updates)) would cut that, at the price of a
  snapshot put together from parts read at different moments, which could
  disagree about an install that happened between two of them.
- **A change deep inside a skill.** A skill's own folder is watched, its
  `SKILL.md` and what sits beside it, and nothing further down: a script in
  `scripts/` edited by hand changes an installed copy's hash, and so its
  state, without the panel being told until something watched changes, the
  tab is shown again or the page asks by hand. Watching each skill's folders
  all the way down would tell it, at the price of a watch for every folder
  a skill keeps.
- **A folder renamed above everything watched.** `mv ~/Work ~/Projects`
  moves every project under it without a watched folder seeing it: a watch
  is told of its own folder's rename and of what happens directly in it,
  and `~/Work` is neither for any. The watches follow the folders, still
  named by where they were, so the panel learns of it from the next change
  in one of them, the tab shown again or a refresh by hand. Watching every
  folder up to the root would tell it, at the price of waking for every
  file made in each, the home folder's among them.
- **A write the ledger has not checkpointed.** The ledger is watched by its
  file, which a write reaches when its log is checkpointed. A connection
  held open past a write, by a long command or a program reading the
  ledger, delays that, and the change shows when the connection closes. The
  panel's own runs say `command_finished` regardless, and a gather writes
  the shelf's folders too, which are watched; a write elsewhere that nobody
  closes on waits.
- **The known workspaces' own markers.** The default workspace's marker is
  watched, for which workspaces this machine knows, but not each of theirs,
  so one renamed is shown by its old name until something watched changes.
- **One folder by two spellings.** inotify gives one watch to a folder
  however it was named, and notify tells its changes by the last name it
  was watched by, so a folder reached through a link and also by its own
  path is told under one of the two, and a change there may go untold.
  Comparing what the system says by the folder's identity rather than its
  name would close it.
- **Which workspace a project's lines consult is not in the snapshot.** The
  TUI asks it beside the snapshot, from its own lookup, to know when a row's
  lines need `-W .`; the panel's backend needs it no longer, since every
  line it runs names the workspace. Carrying it in the snapshot would make
  what the snapshot says depend on the front end's `-W` and activation,
  where now only the list of known workspaces depends on its home, which is
  why what the panel may switch to is a query of its own beside it.
- **Precedence between projects.** Two projects in one workspace can declare
  the same skill name. Core returns both matches and says nothing about which
  wins, because nothing here yet has to choose. Whatever resolves it (a
  workspace-level override, an explicit order in `weaver.toml`) belongs in core
  when it exists, not in a front end.
- **Forcing from the install screen.** An edited install's box is held on
  the screen, and only `--force` on the command line writes over or takes
  out its copies. A key that marks a row to be forced, confirmed as the
  batch is, would let the screen do what the command line does: the line
  its marks amount to would say `--force`, for the projects forced apart
  from the rest.
- **Gathering still stages beside.** `copy::replace` builds a shelf copy as a
  hidden sibling, `.<name>.mindweaver-new`, under names taken from the skill,
  so a gather stopped halfway leaves its staging on the shelf. Two gathers no
  longer meet there, since each holds the workspace's lock ([Writers at the
  same time](#writers-at-the-same-time)). No agent reads the shelf, which is
  why installs moved first. A `copy::Stage` in a scratch folder of the
  workspace's own would do for gathering what `.mind/tmp` does for installs.
- **Removing is not staged.** `uninstall` deletes each copy where it is, so
  one stopped halfway leaves part of a skill in a folder agents read, or one
  agent's copy gone and the other's still there, with the record naming both.
  Renaming every copy into a stage first, and deleting them there, would make
  a removal as all or nothing as an install, at the price an install already
  pays: a target on another filesystem than the project's `.mind/tmp` could
  no longer be removed, where today it can.
- **Removing everywhere is not one change.** `mindweaver remove --everywhere`
  uninstalls project by project, each taking the lock for itself, and then
  removes the library copy whatever they came to, so a project whose
  uninstall failed, or a writer that came between two of them, leaves an
  orphan behind for `uninstall --orphaned`. Holding the lock across the
  projects and the library would make it one change, at the price of a body
  of each uninstall, rules' render included, that runs under a lock it does
  not take, as an update has.
- **A batch takes turns line by line.** The install screen's lines run one
  after another, and each install in them takes the workspace's lock for
  itself, project by project, as `mindweaver install` given several projects
  does ([one artifact into many
  projects](#one-artifact-into-many-projects)): another writer can come
  between two, and with the workspace busy each waits the whole ten seconds
  before it is refused. Holding the lock across the screen's batch needs
  core to apply it as one, which is where #78 plans it, and would want the
  transcript to say it in a line that runs it again as one.
- **One origin for two folders of one repository.** A source is one URL,
  branch and folder, but its origin (`Offer::origin`) says only the first
  two, so one repository gathered at one branch from two folders that each
  hold a `deploy` offers it twice under one origin: `gather list` prints
  the same line for each, `--from` names neither, and the install screen's
  line for either box is refused as offered twice, where the screen, when it
  installed what it marked itself, installed the one ticked. What it then
  recorded named the source by id, but the origin it recorded named both,
  which is what a record's offer is found by (`install::offered_for`). An
  origin that says a folder other than the kind's own would name each, and
  would change what a record holds, so it needs a ledger step that writes
  the folder into the origins older records keep.
- **Changes to an artifact take no lock.** `add`, `edit`, `rename` and
  `remove` write into a project or the workspace's own folders and then see
  to a record ([A record must not outlive the copy it is
  about](#a-record-must-not-outlive-the-copy-it-is-about)), and neither step
  holds the workspace: an edit is made in an editor, for as long as somebody
  keeps it open, and holding the lock that long would keep every other writer
  waiting. An install that takes the lock after the change finds the copy
  edited, or somebody's, and leaves it. One already past that look when the
  change lands writes over it: an edit is then lost, and the record a
  removal drops or a rename carries is that install's, which leaves its copy
  unrecorded or the renamed one recorded as the install wrote it. Taking the
  lock for the change itself, where there is no editor to wait for, would
  close it for `add`, `rename`, `remove` and `edit --contents`, whose
  contents are written in one step. That would not close a second window,
  `--if-unchanged`'s: the file is hashed and then the temporary renamed
  onto it, and a write by an editor between the two, which takes no lock
  of Mindweaver's, is written over. Nothing short of the editor's
  cooperation closes it; it is as wide as reading a file and renaming
  another onto it.
- **Two first writers making one workspace.** `init` makes a marker with
  `create_new` and then fills it, so a second writer making the same
  workspace at that moment (two first commands in a home with no
  `.mindweaver` yet) finds a marker it cannot read, or fails to create one.
  Writing the marker whole under a name of its own and linking it into place
  would let the second find it whole.
- **Examples outside a clone.** The examples are a folder in the repository,
  so only somebody with a clone has them to load; a binary that arrives some
  other way, as a release would, has none. Embedding them in the binary, or
  copying them into a workspace on `mindweaver init`, would reach everybody, at
  the price of a second way into a workspace beside loading, and of files a
  workspace holds without having asked. #134 left both out, with gathering
  paused; a gathered or published copy of `examples/` would answer it
  without either.
- **Old copies of the ledger.** Each migration leaves a
  `mindweaver.db.v<old>.bak` beside the ledger, and nothing removes one: a
  copy is the way back from a step that went wrong, and when it is no longer
  needed is for whoever owns the workspace to say. A command to list and
  remove them, or a doctor check (#96) that mentions them, would make them
  something seen rather than found by browsing `.mindweaver`.
- **What a copy keeps.** A copy is the ledger as it was, so a URL an older
  Mindweaver stored with its credentials in it stays in the copy once the
  scrub has rewritten the ledger itself without them, and the copy is taken
  as the file is opened, before any scrub. Rewriting the copy too, or taking
  it without them, would close that, at the cost of a way back that is no
  longer the file as it was: two of its sources the rewrite makes one would
  have to be merged in the copy, as the scrub merges them in the ledger.
- **Seeing to a record from elsewhere.** A change sees to the record of the
  workspace its command resolved, wherever that sits, but not of any other
  workspace that links the same project: nothing in the project says which
  workspaces link it, so each of them keeps a stale record of a copy
  removed, or of one renamed under its old name. Every workspace linking a
  project would have to be findable from the project for that to change. The
  TUI no longer runs into it from inside: a project's row whose own workspace
  is not the one on screen runs `mind -W . -C <entry>`, which consults the
  one on screen (see [the TUI looks where it was
  told](#the-tui-looks-where-it-was-told)).
- **A project stored by its absolute path.** The ledger names a project by
  the route from the workspace's root to where its entry leads, whatever the
  command called the project. An entry `link` stored absolute, because no
  route from the root reached it, still gives a route that depends on how the
  root itself was spelled: `/tmp/hub` and `/private/tmp/hub` climb a
  different number of `..`s to it. `-C` could always spell a root two ways;
  `-W` and an activation make it likelier. Keying such a project by its entry
  as stored would settle it, at the price of re-keying the records already
  written under the route.
- **Writes that refuse after making the home workspace.** A command that
  writes makes the default workspace before it finds out whether there is
  anything to do in it. Outside every workspace, `mind install` and `mind
  uninstall` in a project nothing manages, `mindweaver install -p` and `mindweaver
  uninstall -p` (not with `--all-projects`, which resolves as a read), and
  `unlink` at either level leave a `~/.mindweaver` behind and then refuse,
  since a workspace just made manages nothing. `mindweaver edit`,
  `rename` and `remove` make it to change its own artifacts, `remove`'s dry
  run included. Each could resolve with `Intent::Read` first and make it only
  when it is about to write; whether each should is a choice per command, and
  they were left as writes when the reads stopped making it.
- **A workspace marker older than its directories.** A marker written
  before workspaces kept artifacts of their own names no folders, so it now
  reads them from the library. One that had artifacts added by a build in
  between, which put them in `skills` and `rules` at its root, stops listing
  them: they stay where they are, untouched, until `[directories]` names
  them. `mindweaver migrate --library` moves only what a marker names, so it plans
  nothing for such a workspace. It could look for those folders wherever a
  marker names none, leaving them to a project at the root that keeps a kind
  there.
- **An unlinked project at the root.** `own_kinds` counts what a project at
  the workspace's root shares with it as the workspace's own unless the
  workspace links that project; a migration leaves it to the project linked
  or not, so the workspace stops offering it and an install of it keeps a
  record nothing offers. Counting the root as a holder in `own_kinds` too
  would make the two agree, at the price of a home's skills dropping out of
  `mindweaver list` and the install screen until the home is linked; moving them
  would take a project's artifacts, which a migration never does.
- **Links inside a moved skill.** A skill holding a link aimed by a relative
  route out of itself, `scripts` to `../../shared/scripts` say, points
  elsewhere once a migration has moved the skill to another depth. The plan
  refuses an artifact that is itself such a link, but does not look inside
  one. Re-aiming every such link would let a migration through where it now
  refuses, at the price of rewriting links somebody made.
- **Migrating from a screen.** The TUI has no key for `mindweaver migrate
  --library`, nor for moving a project to its settings, which run only from
  the command line and, the second, from a project's page in the panel
  (`Task::Migrate`); its header names the second's command where a project
  needs it. A key producing a migration's two lines, the dry run and then
  `--yes`, the panel's task, would bring each to the TUI the way every other
  change gets there.
- **A project's move stopped by a crash.** A migration that fails puts back
  what it had done, but one whose process is killed part way puts nothing
  back: copies it placed are recorded nowhere, and read as somebody's, and
  copies it moved into its stage go with the next install's sweep while the
  `mind.toml` may still name them. Writing down each step before it is
  taken, for the next run to undo or finish, would close it, at the price
  of a journal in `.mind`.
- **An edited install left in the folder a project authors in.** Moving a
  project to settings leaves a copy edited since it was installed where it
  is, and drops its record, as a change of agents does. In a project marked
  by its settings, that folder is read by no agent unless it is one of
  theirs, so the edit stops reaching any until somebody moves it where one
  reads, or installs the skill again. Offering to move it into the agents'
  folders would keep the edit reaching them, at the price of a copy
  Mindweaver would then write without a record.
- **Gathering a repository that is a workspace.** A team repository made by
  `mindweaver init` and `mindweaver add` keeps its own skills in
  `.mindweaver/library/skills`, where a workspace made before the library kept
  them in `skills`, the folder `mindweaver gather` harvests by default. So a plain
  `mindweaver gather git <url>` of it, which used to work, now says there is no
  `skills` folder, and `--path .mindweaver/library/skills` is needed. Gather
  could read a cloned workspace's `weaver.toml` for where its own are when no
  `--path` is given, as `load` reads a loaded one's, or look in
  `.mindweaver/library/skills` among the layouts #101 harvests; whether to
  trust a marker that came from a source is part of that choice.
- **Directories edited into a marker by hand.** Where a holder's directories
  may be (inside it and, for a workspace, clear of the shelf and the cache) is
  checked when `init` writes the marker, not each time it is read, so a marker
  edited afterwards is read as written. Checking on every open would refuse
  workspaces configured on purpose: one keeping its own skills at its root
  (`skills = "."`), from which discovery reads nothing in `.mindweaver`, holds
  the shelf by the same rule. Whether an open should hold a marker to those
  rules, and what a refusal then says, is still to choose.
- **Two workspaces changing one project's settings.** A change to a
  project's agents or share mode holds the lock of the workspace that links
  it (#37), which keeps out that workspace's installs and changes, but not a
  second workspace linking the same project: two changes at once from two
  workspaces each read the settings, and the one renamed last keeps only its
  own, each carrying its own installs along by its own ledger. A lock in
  `.mind`, taken by every change to the settings beside the workspace's, would
  keep the settings whole, at the price of a file a shared project would have
  to ignore, and still not the installs, which each workspace records apart:
  the wall disowning from elsewhere meets too.
- **A leftover older marker nobody is told about.** A project holding more
  than one marker is read from the newest, and `MindProject::warnings` names
  each one ignored, but no front end prints it yet. This build leaves none:
  `mind init` gives a project an older marker marks no settings, and the
  migration moves the old marker out of the way. One is still left by hand,
  by a copy, and by an older build: one from before #29 asks only whether a
  `mind.toml` is there, and one from before #166 whether a `settings.json`
  or a `mind.toml` is, so its `mind init`, or its TUI's making a repository a
  project, writes its own beside a `settings.toml` and reads the project
  from it from then on, while this build reads the `settings.toml`. With
  `mind init` making a `settings.toml` by default, two builds on one machine
  are enough. A `warning:` line wherever a project is opened, or a doctor
  check (#96), would make it seen.
- **`targets` written by hand into a version 3 file.** `targets` came with
  version 4 of a `settings.json`, which a build from before it refuses, and a
  file that set them through Mindweaver was written at 4 ([a project's own
  targets](#a-projects-own-targets)). One a person gives `targets` while
  leaving it at 3 is read with them by this build and read past by an older
  one, which installs into the standard folders and says nothing, until `mind
  migrate` moves it to a `settings.toml`, which no such build reads. Refusing
  such a file as malformed would close it, at the price of a project no build
  opens over a version number; a warning naming the version it needs is the
  other way. A version 3 file whose `targets` do not read as targets at all
  (not an object, a kind with no folder or something other than a list of
  them) is read past, as every build before them read the key, rather than
  refused: a project those builds opened still opens, and installs where its
  agents read without a word, where a version 4 file holding the same is
  refused as any key the format defines is. The move to a `settings.toml`
  writes what this build read, no `targets`, and the copy it keeps holds the
  key as it was.
- **A key a settings.json holds that this build does not know.** Its move
  to a `settings.toml` writes what this build reads, the keys the format
  defines and an unknown one inside `directories` or `targets`, and leaves a
  key it does not know at the top of the file, a `$schema` say, in the copy
  kept in `.mind/tmp` alone, as #38's move leaves a `mind.toml`'s. Nothing
  wrote one but a person, and none means anything to Mindweaver, so it is
  not said. Carrying each into the TOML is the other way, at the price of a
  JSON `null`, which TOML cannot hold, refusing the move.
- **An agent pointed at a target some other way.** Whether an agent reads a
  project's own target is told by where docs/agents.md says it reads, so one
  pointed there by a link from a folder it reads is seen, and one pointed
  there by a setting of its own, an OpenCode configuration naming another
  skills folder say, is warned of as reading none, with a non-zero `status`
  for good. Reading each agent's configuration where it says where it reads
  (#44, #96 read those files for other reasons) would tell the two apart.
- **Reads from a screen on the unmade default.** The TUI's reads (`v`, `s`,
  and showing an artifact) run from the default workspace's root, as its
  changes do. Before the default is made, with a workspace above home, they
  walk up to that one and read it instead. A change makes the default first;
  a read cannot without making the home workspace to read it. Nor can `-W`
  name it ([D-08](#d-08-an-explicit-or-activated-workspace-wins)): a named
  directory with no marker is refused. A way to say "no walk, only the
  default", or a `-W` that takes the unmade default for a read, would settle
  it.
- **A transcript from a screen opened on a named workspace.** A screen
  opened with `-W` runs every line against that workspace, but its
  transcript does not spell the `-W`, as it does not spell `--home`: the
  lines read as somebody would type them at the root, and replayed in a
  shell without the same `-W` or activation they walk up instead. Writing it
  into every line would make the transcript replayable anywhere, at the
  price of lines nobody would type from inside the workspace.
- **A prompt rebuilt every time.** Starship, powerlevel10k, oh-my-posh and
  the like write the prompt anew before each one, which drops the prefix
  activating puts in front of it: the workspace is still active, and `mindweaver
  which` still says so, but the prompt does not. A
  `$MINDWEAVER_DISABLE_PROMPT`, as a virtualenv has
  `$VIRTUAL_ENV_DISABLE_PROMPT`, and a word on showing `$MINDWEAVER_WORKSPACE`
  from the prompt's own configuration would serve them.
- **A wrapper a virtualenv throws away.** In fish and PowerShell, a
  workspace activated on top of a virtualenv wraps the virtualenv's prompt,
  and the virtualenv's `deactivate` puts back the prompt it saved, without
  the wrapper. The workspace stays active without its prefix, and since the
  prompt is wrapped once per shell, a later activation in that shell shows
  none either. Nothing breaks, and the prefix is missing. Wrapping again
  whenever the wrapper is no longer reachable would mend it, given a way to
  tell that without breaking whatever wrapped it since.
- **Shells elsewhere.** The tests that run a shell run on Linux only. macOS
  has bash 3.2 and zsh, and Windows has PowerShell, and each wants running
  there before activation is said to work on it.
- **Worktrees share one exclude file.** Every linked worktree of a repository
  reads the same `info/exclude`, and an entry is anchored at whichever work
  tree reads it. An install keys a project's group by its id, so a project
  checked out in two worktrees with one settings file, committed in shared
  mode or copied in local mode, has one group for both, and uninstalling in
  one stops excluding the copy the other still has; `mind share shared` in
  one forgets the group for both. A shared project writes no entries, so
  only a copied local `.mind` meets it. A key that names the checkout rather
  than the project would keep them apart.
- **A project that moves.** Exclude entries are anchored at the route the
  project had when they were written. Moved within its repository, a project
  keeps a group of lines pointing where it was, which removing its entries by
  their new route never matches. Rewriting a project's whole group from what
  the ledger says it has installed would heal it.
- **A copy kept out of git through a link.** An exclude entry names a copy
  by the route its record names, so an uninstall takes out the line it
  finds, and git does not follow a link: a copy in `.skills`, recorded as
  `.agents/skills/<name>` because the agents read it through links there, is
  kept out of git by a line that matches nothing, and shows as untracked.
  An install into a linked folder writes such a line too, and so does a
  change that respells a kept copy's record. Writing the entry by the folder
  on disk would keep it out, at the price of a line an uninstall has to work
  out again from the filesystem rather than from the record. Claude's rule
  files meet it too: a link inside the project on the way to one, `.claude`
  linked to `config/claude`, is followed, while the entry names
  `.claude/rules/mindweaver/<route>.md`, so in a local project the file
  shows as untracked. Their entries are spelled from the route, as an
  install's are from the record, and one answer should serve both.
- **Agents changed by hand.** `mind agents` carries what is installed along
  (#37), and `mind targets` (#40), but the settings are TOML anybody can edit,
  and the comments in them say how. An install after such an edit replaces its
  record's targets with the ones the project has now, so a copy in the folder
  of an agent the project stopped naming, or one its `targets` stopped naming,
  is no longer recorded: Foreign, left where it is with its exclude entry.
  `mind agents` naming the agents the file already names, or `mind targets
  set` naming the folders it names, re-applies the installs, and takes such
  copies out, only while the install has not dropped them from its record
  first. An install that carried the installation's other copies along itself,
  as `reapply` does, would close it.
- **An install surveyed before a change.** An install works out its targets
  from the project it is handed, read before it takes the workspace's lock,
  as it does its standing, which it looks at again under the lock. One
  surveyed before `mind agents` or `mind targets` changed the project's
  settings, and applied after, puts a copy where the settings no longer send
  one; recorded, so the next `mind agents` with the same agents takes it
  out. Reading the settings again under the lock would close that.
- **A copy as installed, to make another from.** A change of agents gives a
  new folder a copy of one the record names that still holds what was
  installed, by its hash. An installation whose every copy was edited, or
  was recorded without a hash, has none, so the new agent gets nothing and
  that is said; installing it again is refused over the edited copy, and
  forced it puts every copy back, the edited one written over. Copying from
  the offer instead would hand the agents two revisions of one skill
  whenever the offer has moved on.
- **Two workspaces, one exclude file.** The workspace's lock keeps one
  workspace's writers apart, and an exclude edit is made under it, but
  projects of one repository linked from two workspaces edit `info/exclude`
  under two locks. The rename keeps the file whole, and one of two edits at
  the same moment can still drop the other's line. A lock of the exclude
  file's own, beside it in the common dir, would close that.
- **A team's `.gitignore` outranks the block.** Git ranks every `.gitignore`
  in the tree above `info/exclude`, so a negation there (`!.claude/skills/**`)
  takes back what the block excludes, and a local install shows up in `git
  status` with nothing said: the entry is written all the same, and only
  `Excluded::Outside` warns. Asking gix's `excludes` whether each path is
  ignored once it is written, as the tests do, would let the installing side
  or a doctor check say so.
- **Ctrl-C during `mindweaver gather`.** The watch's flag is there to be set, but
  the CLI installs no signal handler, so Ctrl-C ends the process wherever the
  gather has got to. Ended before it places anything, it costs the clone and
  can leave registered a source nothing came from; ended while it places, it
  can leave the shelf between two revisions, and the ledger behind it. A
  handler that cancels the watch would make the command line stop where the
  TUI's `esc` does.
- **Gathering rules.** Only skills are gatherable. A rule is a loose file at any
  depth, so harvesting one means deciding what its name is relative to: the
  same question `Catalog::take_files` answers inside a project, asked of a
  folder that is not one.
- **Private repositories.** Mindweaver gives `gix` no credentials of its own,
  so a private source gets through only when a git-config helper (or, for an
  ssh URL, ssh itself) has them, and otherwise the gather fails and says how to
  provide one, never at a prompt (see
  [Why `gix`](#why-gix-rather-than-running-git)). A URL is stored, logged and
  shown without credentials, and a gather URL that carries one is refused, so
  a helper is also the only place a token typed into a URL can go (see
  [A URL is kept without its
  credentials](#a-url-is-kept-without-its-credentials)). What else it should
  be given is decided in
  [D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt): a
  provider the front end supplies, through that same callback. #110 carries it
  out, and #109 checks the helpers and ssh-agent against a real private
  repository. ssh's own questions are still open: a passphrase for a key the
  agent does not hold, a password when the server accepts password
  authentication, or whether to trust a host seen for the first time, is asked
  by ssh on the terminal, out of the callback's reach. `BatchMode=yes` in
  the ssh command would make those failures too. One connection never sees
  the callback either: when the remote has a branch named `HEAD`, which
  clashes with the `HEAD` a clone fetches to know what to check out, `gix`
  0.87 retries on a new connection without calling `configure_connection`, and
  that one runs the helpers and then prompts, as `gix` does by default. While
  the helpers are the only source, it asks them what the first connections
  already got answered; once #110 supplies a token through the callback, that
  connection will not have it. An in-memory `gitoxide.credentials.terminalPrompt=false`
  on the clone would keep that prompt off the terminal, though not an askpass
  named by `GIT_ASKPASS` or `SSH_ASKPASS`; a `gix` that configures the retry
  too would close it. In the TUI either prompt is worse than at the command
  line: a gather runs apart while the loop goes on reading the terminal and
  drops every key but `esc` and Ctrl-C, so the loop races the prompt for its
  answer; and neither key stops a gather waiting on one, since `gix` takes a
  cancel only once it is receiving. `BatchMode=yes` would close the TUI's ssh
  case too.
- **When the scrub can stop.** `gather::scrub` runs whenever a command
  opens the ledger to write (`WeaverWorkspace::ledger_to_write`), because an
  older build could write a token into the same file. Schema 3 (#30) changes
  that, since a build older than it refuses the file: the scrub could run
  once, as part of that migration, and stop. It has not moved there, because
  a read migrates too ([A read leaves the ledger as it
  finds it](#a-read-leaves-the-ledger-as-it-finds-it)), so a scrub folded into
  that step would run on a read as well, rewriting clones as well as the file,
  and a read would then have to say what it took out, as only a command that
  writes does now.
- **A user name on its own in an https URL.** #25 allows a bare user name,
  and its examples are ssh's (`git@host:...`, `ssh://git@host/...`). Only ssh
  keeps one here: over http(s) a user name is refused at gather and taken out
  by the scrub, because GitHub takes a token in that place and nothing in the
  text tells a token from a user name. That refuses the https URLs Bitbucket
  Cloud and Azure DevOps hand out (`https://<user>@bitbucket.org/...`), which
  then have to be gathered without the user name, with a credential helper
  naming it. The other reading would let them through and store and show
  them without the user name, which also lets a token typed there through,
  for that one fetch, from the command line. The error and the scrub's note
  say a user name was refused or taken out rather than calling it a password.
  Which reading holds is the owner's to confirm.
- **OpenCode's folder and `$XDG_CONFIG_HOME`.** Without
  `$OPENCODE_CONFIG_DIR`, OpenCode's folder is taken to be
  `~/.config/opencode`. Whether OpenCode moves it with `$XDG_CONFIG_HOME`, as
  programs that follow the XDG base directories do, is unverified: #17 ran
  only Claude Code, and [docs/agents.md](agents.md) marks it docs-only, the
  docs being silent. If a check on the binary finds that it does, that
  variable becomes the step before the default for `--opencode-config-dir`,
  and tests that spawn a binary clear it too.
- **`$OPENCODE_CONFIG_DIR` beside OpenCode's folder, not instead of it.**
  OpenCode's documentation loads the folder the variable names after the
  global one and the project's `.opencode`, so it can override their
  settings, and the global folder is still read. `AgentHomes` takes both
  while the variable names a folder (`opencode_dir_in_home`), and doctor
  reads both, as it reads `~/.claude/skills` for OpenCode whatever
  `$CLAUDE_CONFIG_DIR` says (`claude_dir_in_home`). #17 did not run
  OpenCode, so both stay docs-only. If a check on the real binary finds
  that the variable replaces the global folder after all, or that OpenCode
  follows `$CLAUDE_CONFIG_DIR`, doctor reads a folder OpenCode does not and
  says too much, never too little, and the folder in the home goes.
- **An empty agent variable.** An empty `$CLAUDE_CONFIG_DIR`, `$CODEX_HOME` or
  `$OPENCODE_CONFIG_DIR` means no folder at all here, following
  `$MINDWEAVER_HOME`. An agent that reads an empty one as unset uses its
  default instead, and a check would then leave out a folder that agent does
  read. #17 did not settle it: it ran only Claude Code and did not try an
  empty `$CLAUDE_CONFIG_DIR`, so how each agent reads one is for an addition
  to `scripts/agent-probe` to record.
- **A `CLAUDE.md` at `/`.** #44's walk stops below `/`, as Claude's own check
  most likely does ([docs/agents.md](agents.md)): #17 inferred it from the
  debug log's directory counts and could not put a file at `/` to try it.
  Were Claude to count `/` after all, a `CLAUDE.md` there with none nearer
  would have the predicate say Claude reads the block while it reads none,
  and the rules about every file would miss Claude. A probe run allowed to
  write at `/`, in a container say, would settle it.
- **An import from `.claude/CLAUDE.md`.** The import goes there, as
  `@../AGENTS.md`, only when the project's `CLAUDE.md` is not there or may
  not be edited. That Claude resolves an import from the file holding it,
  and reads both `CLAUDE.md` and `.claude/CLAUDE.md` in one folder, is
  docs-only ([docs/agents.md](agents.md)); were either wrong, the block
  would miss Claude in that one case. A `claude-import` run with the line in
  `.claude/CLAUDE.md` would settle both.
- **Claude started below a project's root.** The predicate looks from the
  project's root up, for Claude started there. A `CLAUDE.md` in a folder
  between the root and a deeper start keeps Claude from reading `AGENTS.md`
  natively there, while the reach says it does, and no import is added.
  Looking below the root too would tell, at the price of walking the whole
  project, and of an import for a start nobody may use.
- **A `CLAUDE.md` hard linked to `AGENTS.md`.** One file under two names is
  told by resolving both paths, which sees a symbolic link and not a hard
  one, and the block's render replaces `AGENTS.md` through a rename, which
  splits a hard link. What follows depends on when the link was made. One
  made before any block was written is split by the first render: the two
  are then files of their own, the import goes into `CLAUDE.md` as into any
  other, and Claude reads the team's text twice, once in each, though each
  rule once. One made once `AGENTS.md` holds the block, the likelier order,
  since the block is there whenever a team links the two, gives a
  `CLAUDE.md` holding the block's rule markers, which `ClaudeMd::parse`
  refuses (`Malformed::Ruled`), as it refuses any copy of `AGENTS.md`: every
  reach and every render of the import is refused, `render(false)` too,
  until somebody edits that file or takes the link away, and a block render
  that changes the rules meanwhile splits the link and leaves `CLAUDE.md`
  the copy with the rules as they were. Telling a hard link by the file's
  device and inode alone would not end it, since such a render splits the
  link before the import is decided. Writing `AGENTS.md` in place rather
  than by a rename, where it has more than one link, would keep them one
  file for that to tell as `Reach::Same`, at the price of a write that is
  no longer all or nothing.
- **An import left in a file git tracks.** An import no longer wanted in a
  file that may no longer be edited, a `CLAUDE.md` committed with it in a
  project turned local, stays and is said at every render
  (`claude_md::Warning::Tracked`), until `edit_tracked` or shared mode lets
  it be taken out. Once the last rule is gone it imports an `AGENTS.md`
  holding no block, which gives Claude nothing more. Beside a `CLAUDE.md`
  that is the `AGENTS.md` through a link, Claude reads the block through
  both, and whether it loads the one file twice is not recorded.
- **Codex and OpenCode on their binaries.** #17 ran only Claude Code, so
  every Codex and OpenCode fact about skills and rules in
  [docs/agents.md](agents.md) is docs-only, each with a fallback. Its checks
  2 (OpenCode given the same skill in both folders) and 5 (Codex and project
  trust), and the Codex and OpenCode halves of 1 and 4, are what `codex.sh`
  and `opencode.sh` beside the probe would run beside the MCP checks they
  hold now (#18). #34 relies on them already, writing `.agents/skills` for both
  and trusting OpenCode with identical copies in two folders, and #42 will.
  The [target table](#agents-and-where-each-kind-reaches-them)'s
  `.agents/skills` for Codex and OpenCode rests on them too; were one to read
  elsewhere, its arm in `standard` is the one line to change.
- **Codex's MCP facts are docs-only.** #18's checks ran on 2026-11-05 for
  Claude Code 2.1.318 and OpenCode 2.2.5, 2.1.3 and 1.19.4, and not for
  Codex, since the owner has no Codex subscription
  ([docs/agents.md](agents.md#mcp-servers)). What #87 and #91 build on for
  Codex stands as its documentation and binary say, and D-10 and D-11 say
  what each result would change; `codex.sh` holds the runs for when there
  is a login to make them with.
- **Whose environment fills OpenCode's references.** V2's background
  server fills `{env:}` from its own environment, not from the client's
  that asks (D-11, the run of 2026-11-05), so a secret given to a V2
  server Mindweaver did not start reaches it only through the shell that
  started it or `opencode service set env`. Whether #92 warns of it, and
  doctor checks the server's variables, is the owner's to say. #96 was
  built without it: doctor checks the variables the front end says are set
  where it runs, and says nothing of a V2 server's own.
- **Folders an agent reads and is sent nothing in.** A project marked by its
  settings is read in its agents' targets and in its authored folder, not in
  every folder they read ([read where its agents
  read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)):
  `.opencode/skills`, and `.claude/skills` in a project that names OpenCode
  and not Claude, are read only when the project authors there, though
  OpenCode loads what is in them. The targets are where an install lands and
  can meet somebody's work, so those are what a listing has to know; reading
  every folder `Agent::reads` names would list the rest as well, from folders
  no install of Mindweaver's reaches. Doctor (#96) reads every one of them,
  for copies of one name that differ there and for a skill at user scope
  that shadows one, but lists nothing of what is only there. An install refuses on the targets alone too,
  so in a project that names OpenCode and not Claude it writes
  `.agents/skills/<name>` beside somebody's `.claude/skills/<name>` or
  `.opencode/skills/<name>`, and OpenCode loads two versions of one skill.
  Refusing on every folder an agent reads would close that, at the price of
  refusing over a copy no listing shows; reading those folders for both is
  the other way, and whichever is chosen, a listing and an install should
  agree on it.
- **Listing reads every copy it compares.** Copies of one name in a v3
  project's agents' folders are compared on every listing by `hash::tree`,
  which reads each of their files whole: with a copy installed in each
  target, that is every installed skill, on every `list`, `show`,
  `validate`, `status` and refresh of the TUI. A skill is a few small files,
  so it is accepted for now. The cheaper ways each miss something:
  `copy::same` stops at the first difference but follows links, which a copy
  here may hold to anything; comparing sizes first saves nothing when the
  copies are alike, which after an install they are; and the hash an
  installation records is the ledger's, which discovery does not open, and
  a foreign copy has none. If a skill's assets make a listing slow, that
  recorded hash, compared where `Catalog::with_records` already reads the
  ledger, is where to start. `status`, and so the TUI's rows, reads more:
  each installation's standing (`install::standings`) hashes every copy its
  record names again, and compares one of them with the offer it came from,
  reading both, so that an outdated install can be told from a current one,
  as `status` and an update both say. Telling an edit alone would need no
  offer but for a
  record with no hash; the price was taken so that a report and a refusal
  are told by one function.
- **A screen that lists what it cannot change.** The TUI's project rows
  come from the same discovery, so a v3 project's rows list the skills in its
  agents' folders as well. A row is one name, so a change aimed at it reaches
  the project's own copy when it authors one, and is refused (`NotAuthored`)
  when the name sits only in its agents' folders. An own copy that does not
  read takes the row its name has rather than a second one, whose keys would
  reach it all the same. Each row says its state, so one that is installed
  or foreign reads as such before a key is pressed, but the keys that change
  an artifact are still offered on it and refused only once pressed; leaving
  them out of the footer on such a row would say so sooner.
- **An install gone under a name somebody keeps.** When every copy of an
  install has gone and the project holds something else by that name, status
  says the name as what is there, and nothing of the record, which lingers:
  the next install of it is refused as foreign when what is there sits where
  it would write, and `mindweaver uninstall` forgets the record without touching
  what is there. While a copy of the install is left, the entry holds it and
  says missing, even over the project's own copy. A second entry of the same
  name would say it, at the price of the one name a row the screen and every
  listing keep to.
- **Errors outside the envelope.** A command that fails says why as text on
  stderr, `--json` or not, and prints no envelope, so a script tells failure
  only by the exit status. Every error has had a code since #61 ([a code for
  what went wrong](#a-code-for-what-went-wrong)), and a serialized error is
  its code and its message, but none of them is printed: whether a failing
  command's `--json` prints an envelope holding its error is left to #63,
  which carries codes on the CLI's warnings and failures.
- **An agent's CLI writing the same config file.** The workspace's lock keeps
  Mindweaver's writers apart, not the agents': `claude mcp add` rewriting
  `.mcp.json` between `ConfigFile::edit`'s read and its rename is undone by the
  rename. Reading the file again just before the rename would narrow that to
  the rename itself; nothing closes it while the agents take no lock of ours.
- **A folder made a link while a config file is written.** `ConfigFile::edit`
  looks for links on the route again just before it writes, and the rename is
  onto the path as spelled, so a link that the file itself has become is
  replaced, not written through. A folder on the route that becomes a link in
  the moment between that look and the rename is still followed by the
  operating system. Only opening each folder without following links and
  writing relative to it (`openat` with `O_NOFOLLOW`, then `renameat`) closes
  that, which is a platform layer of its own for a window that small.
- **What Codex reads from below a project's root.** The override and the
  budget are worked out for Codex started at the project's root, where its
  `AGENTS.md` is. Started in a folder below, Codex reads that folder's
  `AGENTS.md` files too, after the project's, and its config can rename the
  files it looks for (`project_doc_fallback_filenames`), say what marks the
  root (`project_root_markers`) or change the budget, none of which
  Mindweaver reads yet. Doctor (#96) reads Codex's config for its trust and
  its servers and none of those keys; reading them too would make each
  warning exact for one more case.
- **Claude Code's servers at user scope.** Doctor finds a server of an
  installed one's name in Codex's and OpenCode's own config, and not in
  Claude's: Claude keeps a user's servers, and those of each project it
  was told of at local scope, in a `.claude.json` beside its folder, or in
  it while `$CLAUDE_CONFIG_DIR` names one (its docs), and `AgentHomes`
  says which folder, not which way it was found. Which of a project's and a
  local one Claude starts is docs-only too. A home of its own for that file,
  decided where the agent homes are, would let doctor read it.
- **A stage in use, or left.** Doctor takes no lock, so a stage in
  `.mind/tmp` an install is building as it reads is found as one an
  interrupted install left, and said "unless one is running now". Taking
  the stage lock shared, as an install does, would tell them apart, at the
  price of a read that makes the lock file.
- **The variables of the moment.** The names doctor is told are set are
  read once per command line, so the TUI and the panel are told what they
  were opened in, and a variable set in a shell since is found unset until
  they are opened again, as the agents the TUI was opened beside would not
  see it either.
- **What doctor finds, every time a snapshot is read.** The snapshot reads
  every folder of skills at user scope and hashes each skill a project
  shares a name with there, on every live update. A skill is a few small
  files, as for [listing every copy it
  compares](#open-questions), so it is accepted for now.
- **`.claude/rules` above where Claude starts.** Claude reads every
  `AGENTS.md` and `CLAUDE.md` from where it starts up the tree, but whether
  it reads a `.claude/rules` in a folder above, as it reads `.claude/rules`
  at any depth below, is in neither [docs/agents.md](agents.md) nor the
  probe. Claude's rule files are refused above a project the workspace links
  as the block is, which at worst keeps rules from a project above others
  that Claude would not have read in them anyway. A probe check with a rule
  one folder above the start directory would settle it, and let that one
  refusal go if Claude reads no such folder.
- **A project that authors rules in `.claude/rules`.** A project may keep
  its own rules where Claude reads them, and its listing then reads
  `.claude/rules/mindweaver` as its own rules, named `mindweaver/<route>`,
  which a lifecycle change can rename or remove. Nothing is lost, since a
  render writes a file it recorded again where it finds none, and never
  writes over one it did not record; but the listing shows Mindweaver's
  files as the project's. The ledger records them now ([installing
  rules](#installing-rules)), so a listing could tell them apart as it
  tells installed copies from authored ones; leaving out the `mindweaver`
  folder where a project authors in `.claude/rules` would keep the two
  apart, and the status entry of each rule its one.
- **A file replaced by a render stopped part way.** A render records each
  file it may make before it makes it ([installing
  rules](#installing-rules)), so one killed part way leaves no made file
  unrecorded. A file it replaced, a rule changed since the last render, is
  recorded by its new hash only once the render is done: killed between,
  the file reads as edited, and is left, until a render finds it holding
  its rule as written again. Recording both hashes for the moment between
  would close that, at the price of a target that holds two.
- **A rule whose offer has gone.** Every render an install runs gives each
  rule from the offer it was installed from, so one no longer offered there
  (deleted from the library, a load gone) refuses every install of another
  rule into the project until it is put back or uninstalled ([installing
  rules](#installing-rules)). An update's render, and an uninstall's, keep
  it instead, as the project holds it, which reads as orphaned
  (`Standing::Orphaned`) and is said ([bringing what was installed up to
  date](#bringing-what-was-installed-up-to-date)); an install's render
  doing the same would let the rest change there too, at the price of an
  install that quietly carries a rule no offer stands behind.
- **Every render brings every rule up to date.** A render gives each rule
  as its offer holds it now, so installing one rule updates the sections of
  the others too, where installing a skill touches no other skill. The
  block is one file written whole, and rendering only one rule would mean
  reading the others back from it, as a render now reads back a section
  somebody edited, and keeps it. #48 chose to keep that for an update: one
  rule named brings the project's other outdated or missing rules along,
  and its plan names each, so what is written is what is said. An install
  of a rule still brings them along unsaid, and keeps only the copy of a
  rule somebody edited, writing its other copy from the offer, where an
  update keeps every copy of it. Holding an install's render to the same
  would spare its agents two versions of one rule too.
- **A copy recorded where nothing reads it.** Status calls an installation
  missing when discovery does not find one of its copies, and an update
  writes it when its standing says a copy is gone. A copy that is still
  there holding what was written, but where discovery no longer reads it (a
  folder dropped from the project's settings by hand, which `mind agents`
  would have carried along), reads as missing in status and as current to
  an update, which writes nothing. Telling such a copy as missing in the
  standing too would make the update rewrite every target, as an install
  does, and record the copies afresh.
- **A `git clean -dX` takes a local project's settings.** A local project
  keeps its whole `.mind` out of git (`MIND_IGNORE`), so the `-dX` that
  takes its copies, each in a folder `git clean -X` does not go into, takes
  its settings with them, and the project is no project any more: an
  update finds nothing to put back, and `mindweaver update --all` says the
  workspace's entry would not open. `mind init` again, with the agents,
  share mode and folders it had, marks it once more, and the next update
  puts every copy back from the ledger, which knows the project by its
  route; the new settings come with a new id, so the copies are kept out
  of git under a new group, and the old id's group is left in the exclude
  file naming the same places, which nothing takes out. Keeping the
  settings out of the ignored folder would show them in `git status`,
  which is what a local project is for avoiding; keeping a copy of them in
  the workspace, beside its entry, would let a command mark the project
  again as it was, its id included.
- **MCP servers after a change of agents or share mode.** `reapply`
  carries a project's skills along, and not its MCP servers' entries: a
  project that names another agent is given no entry for it until the
  server is installed again, one that stops naming an agent keeps that
  agent's entry and its record, which an uninstall takes out, and one
  turning local leaves a config file an install made while it was shared
  in git's view, an install again writing no line for a file that is
  there. Carrying the entries as it carries copies, under the lock it
  holds, would close it: an entry written where an agent is named now,
  one taken out while it holds what was written where one is no longer,
  and a line for each file the ledger says Mindweaver made.
- **Rules after a change of agents or share mode.** `reapply` carries a
  project's skills along when its agents, targets or share mode change, but
  not its rules: those are rendered again by the next install or uninstall
  of one, or by `install::rules::render`, which nothing calls on a change
  yet. Until then a project that stops naming Claude keeps Claude's files,
  and one turning local leaves an `AGENTS.md` Mindweaver made in git's
  view. Rendering the rules again at the end of `reapply::apply`, under the
  lock it holds, would close it, with what the render says in what the
  change says.
- **Claude's reach in a report.** Status says Claude reads a rule through
  the block when the render gave it no file of its own, as the ledger
  records it, rather than asking `claude_md` again, which needs the agents'
  own folders a report is not handed. A file for Claude that was somebody's
  when the render ran is the one case that reads wrong: no file is
  recorded, and Claude reads neither. Handing a report the agents' folders
  would let it ask.
- **A link made while `AGENTS.md` is written.** A link in place of the file
  is looked at before the read, and one leading out of the project is
  refused; a link made in the moment between that look and the rename is
  followed by `replace_bytes`, which resolves the path first. Writing to the
  path as spelled, as `ConfigFile` does, would close it, at the price of
  replacing the link to a file inside the project that some teams keep
  between `AGENTS.md` and `CLAUDE.md`.
- **A config file left empty by its last entry.** `claude_mcp::remove`,
  `codex_mcp::remove` and `opencode_mcp::remove` take their file away with
  its last server when their caller says Mindweaver made it and nothing
  else is left, through `ConfigFile::take`; without that word `.mcp.json`
  is left holding `"mcpServers": {}`, `.codex/config.toml` nothing and
  `opencode.json` `"mcp": {"servers": {}}`. `.codex` stays
  either way, made by Mindweaver or not: it is Codex's own folder, and
  which folder Mindweaver made is no record anybody keeps, so a project
  given a server and then none keeps an empty `.codex`, which git does not
  see. Which files Mindweaver made is `created_files`' to record, as it
  records an `AGENTS.md`, which an install of a server notes its config
  file in when it makes one (#94), the ledger's entries (#93) recording
  each entry, not the file it is in.
- **An entry left in an OpenCode file that no longer comes first.** A
  recorded entry is looked for in the file its record names
  ([how an entry stands](#how-an-entry-stands)), so one written into
  `.opencode/opencode.json` reads as current after somebody adds an
  `opencode.json`, which comes first among the four and is the one a new
  entry goes into. Whether OpenCode reads all four or the first is
  docs-only; if the first, the entry reaches OpenCode no more, and an
  update would have to say so, or move it. An uninstall takes it out of
  the file its record names (`opencode_mcp::remove_from`), but an install
  writes the server into the first, and the record follows it there,
  leaving the entry in `.opencode/opencode.json` recorded by nothing, so
  somebody's from then on; taking it out as the new one is written, while
  it holds what was written, would close it.
- **An input given a value rather than a variable.** An input that is no
  secret may name no variable, its value being somebody's to give, and
  Claude's entry then has nothing to refer to: `claude_mcp` refuses it
  (`claude_mcp_input_unread`), and `codex_mcp` and `opencode_mcp` too
  (`codex_mcp_input_unread`, `opencode_mcp_input_unread`), which `show`'s
  report of each agent does not say, since it is no agent's limit. So does `codex_mcp` one that
  names a variable and is written where Codex reads none, an argument or a
  part of the address `add --arg-env` and `--url-var` write
  (`codex_mcp_input_in_text`): every server `add` writes with either
  reaches Codex only once a value is handed. #95 gave the commands what
  installing a server will run and which variables to set, and no way to
  hand a value: one given when the server is installed would be the
  commands' to ask for, an option of `install` or a question the screen
  asks, and the install's to hand the renderer, which would then write it
  as it is, held to what a value of the transport is held to; the ledger's
  record of an entry (#93) keeps its fingerprint, not its values. Until
  then an install of such a server reaches none of those agents, and says
  why of each as a warning with that refusal's code, its preview as much
  ([installing an MCP server](#installing-an-mcp-server)).
- **Variables Claude empties that the table does not follow.** What Claude
  keeps from a remote server was read from 2.1.318's binary
  (`mcp/blanked.rs`), and only what it keeps in every environment: a
  credential-shaped name under a packaging tool's prefix
  (`ORG_GRADLE_PROJECT_`, `POETRY_`, `CONAN_`, `BUNDLE_`, `INPUT_`), which
  it judges by a pattern of words, and what it empties only in some
  environments (under GitHub Actions or `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB`,
  every credential-shaped name but GitHub's own tokens) are written, and
  reach the server empty there. So is the shorter list it empties in a
  program's `args` and `env`, its own session's, which no server's input is
  read from. A run with one of them in `claude-mcp`'s header, or a newer
  Claude, changes the table.
- **The palette in two places.** `src/theme.ts` names the palette for Nuxt
  UI, and `main.css` spells it out as the variables Nuxt UI's colours plugin
  would have written at run time. The build test holds the two to each
  other. A Vite plugin writing those variables into the stylesheet at build
  time, from `src/theme.ts`, would leave the palette in one place.
- **A light panel starts dark.** `index.html` carries `dark`, so the default
  never flashes light, but a browser that chose light sees one dark frame
  before the script applies the choice. A script in the page that read it
  first is what the CSP refuses. The choice is in a cookie the server
  receives with the page's request, but acting on it would need the server
  to rewrite the page as it sends it.
- **A style nonce for the panel.** The policy allows no inline style, so a
  `<style>` element that a library writes into the page at run time is
  blocked, as theming and colour-mode helpers often do; style the panel's
  scripts set through the CSSOM, as Vue's bindings do, is not governed by
  it. The panel needs no nonce so far: Nuxt UI's palette is declared in the
  stylesheet and the colour mode switches by class ([Nothing the CSP
  refuses](#nothing-the-csp-refuses)). The one library left that writes a
  `<style>` is vaul-vue, under Nuxt UI's `UDrawer`, when it loads, with the
  nonce of a `meta[property=csp-nonce]` if the page has one; nothing loads
  it yet, since the sidebar opens over a narrow screen as a slideover. A view
  that wants a drawer needs vaul's styles in the stylesheet, or a nonce: the
  server would issue one with each launch, add it to `style-src`, and write
  it into `index.html` where Vite's `html.cspNonce` leaves its placeholder.
  `script-src` never gets `'unsafe-inline'` either way.
- **A hostile server on loopback.** The cookie reaches every port of
  127.0.0.1, so a server there that the browser visits while the panel runs
  receives the token and can use it as Bearer
  ([What keeps other pages out](#what-keeps-other-pages-out)). On a machine
  one person uses, that server is theirs, and a hostile program of theirs
  could read the browser's profile as easily; on a shared machine it can be
  another user's, and nothing here keeps it out. Two changes would. The panel
  could hold the token in memory and send it in a header of its own, which
  goes nowhere the panel does not send it, at the price of the `HttpOnly`
  that keeps the token from the panel's scripts, and of a reload that loses
  it. Or each launch could bind an address of its own in 127.0.0.0/8, so the
  cookie's host is the panel's alone, where the system allows it: Linux and
  Windows answer the whole block, macOS only 127.0.0.1 until an alias is
  added. Either changes the launch #15 validated (127.0.0.1, and a cookie
  named per port), so it is the owner's to choose, or to accept as it is.
- **A run that panics.** A backend that panics while running a task has no
  outcome to record, so the task is not in the transcript, though it may have
  changed something before it stopped. The CLI's commands collect their
  failures rather than panic, so this is a bug's path, but the transcript then
  cannot say what the bug left behind.
- **A link made between a check and what follows it.** A file the panel
  reads is read through the path its links resolved to when they were
  judged, so a link put in its place afterwards is not followed; a folder on
  that path made a link in the moment between is. A path the policy judged
  outside every `.mindweaver` can likewise be made a link into one before
  the command runs. The scratch folder a save puts its text in is looked at
  once it is there, and refused as a link, so one put in its place between
  that look and the file's making is followed the same way. Opening each
  folder on the way without following links would close the first and the
  third, as `ConfigFile` narrows its own window, at the price of code per
  platform; the second is a race with somebody who can already write where
  the command acts.
- **A listing reads an artifact through its links.** The snapshot lists a
  rule with its summary, its first line when it has no front matter, read
  through any link as the catalog follows one, so a rule linked out of its
  project hands the panel one line of what it leads to, as `mind list`
  prints it. Its whole file the panel never gets (`show_unheld`,
  `contents_outside`). Holding the catalog to each holder would close it, at
  the price of a rule shared by a link no longer being listed at all.
- **A task with no fields takes any field.** serde passes over the fields of
  an object read as a variant of an internally tagged enum that has none,
  since there is nothing to hold it to, so `{"type": "shelf", "home":
  "/etc"}` reads as `shelf`, where #63 promises a field a task does not have
  is refused. Nothing reads such a field, so no line changes. Making those
  variants empty struct variants, as `Query::Snapshot {}` is, would refuse
  it, at the price of spelling every one with braces wherever it is built or
  matched.
- **Names outside ASCII.** The specification allows "unicode lowercase
  alphanumeric characters" in a name, while listing `a-z` and `0-9`, and its
  reference validator accepts more than ASCII. Mindweaver holds names to ASCII,
  the narrower reading and the one every agent accepts, so a gathered `café` is
  warned about and not installed. If the agents settle on the wider one, the
  name rule is one function (`is_kebab_case`).
- **Folders a workspace scans.** `mindweaver scan DIR` and `mindweaver minds DIR`
  look where they are told for one command, or one session of the screen;
  nothing keeps the folders, so the TUI's `m` and link form opened another
  time look under the workspace root again, and a default workspace in home
  offers the whole home, three levels down. Keeping them in `weaver.toml`
  would be a format change, which ranked suggestions (#58) did without: they
  rank what a scan finds wherever it looks, so the screen puts what is worth
  linking first even under a whole home, and keeping the folders is still
  open. Nor does the screen take a depth: `mindweaver minds` looks
  `DEFAULT_DEPTH` levels down, and a project further down is linked by its
  path.
- **What a suggestion counts.** An agent's files count by name at a
  candidate's root: a `.claude` holding only somebody's local settings ranks
  a repository as one holding skills does, and an `AGENTS.md` in a
  subfolder counts for nothing. Reading what each holds, or looking deeper,
  would rank better at the price of opening files in every repository under
  a home. Nor do agent files make a folder a candidate: one that is neither
  a project nor a repository, notes kept beside a `CLAUDE.md`, is walked
  through and never offered, however much an agent reads there, since
  offering it would offer a home directory for its `.claude` too. And the
  ranking stops at the reason: within one, candidates go by route, not by
  how many agent files each holds or how recently it changed.
- **The shelf in other reports.** `mindweaver validate` reads the shelf; `mindweaver
  list` and `show` do not, because `gather list` is the shelf's listing. Seeing
  a gathered skill's contents before installing it means reading its folder.
- **An em dash in a commit message.** `make lint/em-dash` reads the tree, so a
  commit message is checked by nobody but its author ([No em dash in what the
  repository says](#no-em-dash-in-what-the-repository-says)). A step in CI
  over a pull request's own commits would catch one before the merge without
  tripping on the history from before the rule.
- **Remembering on activate.** #56 asked `mindweaver activate` to remember the
  workspace it activates as well as `mindweaver init`; it remembers none, since
  activating writes nothing ([the workspaces this machine
  knows](#the-workspaces-this-machine-knows)), and `mindweaver init` run again on
  a workspace remembers it. Remembering there would want a write of its own
  beside the code it prints and never on stdout, taking the default
  workspace's lock without waiting, so a busy one is passed over rather than
  waited for at every new shell, and saying a failure only as a hint, so a
  shell's startup file is neither slowed nor made noisy; and the rule that
  activating writes nothing would have to say why it no longer holds.
- **A remembered workspace that moves.** The list names a workspace by where
  it was, so one moved with `mv` is listed as one that cannot be opened until
  it is forgotten and `mindweaver init` run where it went. Nothing in a workspace
  names it for good, as a project's id names a project (#57 relinks those), so
  there is nothing to match a moved one by.
- **Lines core hands over as text.** A warning carries the code of the value
  it renders, and a few are built from text core hands over without one: why
  an exclude entry could not be dropped (`still_excluded`, an install's
  `Rendered::still_excluded`), why a copy was recorded without a hash, what a
  copy left out and why, and the notes an `Excluded` gives. Each says a
  condition the panel may want to act on, and carries no code until core
  hands it over as a `code::Said`, as a render's warnings now come. Spelling
  core's code by hand at the line was the other way, rejected for a code no
  test could hold to the value it names. Nor do the CLI's own warnings,
  worded from what it found (a gathered name that is not one folder name, an
  update that cannot tell where a copy came from), carry a code of the CLI's:
  each would join `mindweaver_cli::CODES` once the panel acts on it.
- **A run left behind by Ctrl-C.** The first Ctrl-C to `mindweaver ui` waits
  for every request under way, but a run whose client stopped waiting is no
  request any more: it goes on on the server's writer thread, which nothing
  waits for, and the process can end under it, as it does under any run at
  a second Ctrl-C. The server could hand the front end that thread to join
  once it stops, at the price of a Ctrl-C that waits for a run nobody
  watches.
- **Ctrl-C off Unix.** `ui_test` stops the panel with SIGINT, and with
  SIGTERM, sent by `kill`, so it runs on Unix only. On Windows `ui` listens for the console's
  Ctrl-C (`tokio::signal::windows::ctrl_c`), which no test sends yet (#154).
- **A release compiled from nothing on every run.** `build (core)`, `build
  (cli)` and `build (server)` each build the release profile from scratch
  every time: every job shares one cargo cache key, saved by the first job
  to finish, which builds no release. A key of their own would keep the
  release's dependencies, at the price of a second target folder in a cache
  GitHub caps per repository.
- **The release check on one platform.** `make cli/release/check` runs on
  Linux only: its script needs bash 4.4 and SIGINT, and the macOS and
  Windows jobs build no release. The compatibility pass (#154) could take it
  there, with the console's Ctrl-C on Windows.
- **Which panel `build (cli)` embeds rests on the runner.** `make
  cli/release` embeds the downloaded panel because no pnpm is on the
  runner's PATH. A self-hosted runner whose image has one, which nothing
  here rules out, would have it build the panel again, from the same
  lockfile, on whatever Node the image has, rather than embed the one
  `test (web)` tested; a variable telling `make web/build/if-node` to take
  `dist/` as it is would settle it, at the price of a knob only CI turns.
- **A writer waiting while a workspace is moved to the new names.** The
  move holds the workspace's lock, which is on a file in the folder it
  renames, and follows it there; a writer that opened the lock file before
  the move and waited takes the lock once the move lets go, and then works
  from the paths it read before, which are gone: a ledger it opens there
  is made again, under the old name, in a folder made again. A writer
  reading the workspace again once it holds the lock, as a migration does,
  would close it, at the price of a second read in every writer.
- **The ledger opened between the move's look and its rename.** The move
  refuses a ledger whose log is beside it once its own connection has
  closed, and looks again before renaming it; a process that opens it in
  between, a panel's read, keeps a log under the old name that the rename
  leaves behind. Reads take no lock, by design; renaming the log with the
  file would move it under a connection SQLite has open.
- **On Windows, a folder holding an open file is not renamed.** The move
  renames `.mindflayer` while it holds the lock file in it open, which
  Windows refuses, so there the move fails at its last step and puts back
  every step before it. Letting go of the lock for the last rename would
  leave a moment with no lock held; the compatibility pass (#154) is where
  to weigh it.
- **A project's rules in two folders until it is moved.** A rule file a
  Mindflayer build recorded stays in `.claude/rules/mindflayer`, and one
  given a file since goes to `.claude/rules/mindweaver`, both read by
  Claude, once each; `migrate --names` brings them together. Filing a new
  one in the old folder while the project has one there would keep them
  together, at the price of writing a new file under an old name.
- **The hint of an old workspace, on every line.** Every line typed on a
  workspace still under Mindflayer's names ends with how to move it, a read
  included, until it is moved, and the panel shows nothing of it. A word in
  the snapshot saying whose names a workspace is under would let the panel
  say it beside its other warnings, a field the snapshot adds without a
  new version.
- **A Mindflayer build still on the PATH.** One run after the move finds no
  workspace where it looks, and a write of its makes a `~/.mindflayer`
  beside the moved `~/.mindweaver`, which this build reads past and names
  as left over. Removing the old binary, which the reference says how to
  do, is what keeps it from happening; nothing in a workspace can stop a
  build that does not know its names.
