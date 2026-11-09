# AGENTS.md

Guidance for coding agents working in this repository.

Mindweaver manages the skills and rules coding agents read, the way git manages
code. A **mind project** (`.mind/settings.toml`) holds artifacts beside its
code; a **weaver workspace** (`.mindweaver/weaver.toml`, or the
`.mindflayer/flayer.toml` a Mindflayer build made) links mind projects,
holds artifacts of its own, gathers skills from git onto a shelf and installs
them into projects. `README.md` is the first page a newcomer reads: short,
friendly and centred on the control panel (`mindweaver ui`), with screenshots
`make docs/screenshots` takes and no command's output but one, what an MCP
server's install prints (#95). `docs/reference.md`
is the user-facing reference: every command with its literal output, the
formats, `--json`, the TUI, the panel in full and upgrading.
`docs/api.md` is the guide to the HTTP API `mindweaver ui` serves, every
example a `curl` line and its literal answer, which a test replays.
`docs/development.md` is how to build, test and work on the panel.
`docs/architecture.md` records why each mechanism is the way it is: read the
section for whatever you are about to change before changing it, and keep it
true afterwards. Its "Decisions" section holds what the owner already chose for
the roadmap, and the one choice still pending: read it before changing a
format, installing, MCP, credentials or the server, and carry a decision out
rather than reopen it. `docs/agents.md` records where Claude Code, Codex and
OpenCode read skills and rules, and what they make of MCP servers, each fact
verified on the binary, docs-only with a fallback, or pending a probe run: an
agent path in code cites it, and a change that disagrees with it starts
there.

## Commands

The root `Makefile` is what CI runs, so prefer it to raw cargo. `make help`
lists everything.

```bash
make test                 # every suite, the panel's first: needs Node 24 and pnpm
make fmt/check            # CI runs this first
make lint                 # no em dash in a tracked file, then clippy --workspace --all-targets, with the ts feature too
make run ARGS="weaver list"
make core/test            # one app: make <app>/<target>
```

One suite, one module or one test:

```bash
cargo test -p mindweaver-cli --test tui_test                 # apps/cli/tests/tui_test.rs
cargo test -p mindweaver-cli --test e2e_test gathering::     # the CLI end to end, one area of use cases
cargo test -p mindweaver-cli --test api_e2e_test changing:: # the server's API end to end, one area of use cases
cargo test -p mindweaver-core --test e2e_test installing::  # core end to end, one area of use cases
cargo test -p mindweaver-cli --test tui_e2e_test changing:: # the TUI end to end, in a terminal of its own
cargo test -p mindweaver-core --test install_test <substring>
cargo test -p mindweaver-core --lib workspace                # src/workspace_test.rs
cargo test -p mindweaver-server --test commands_test         # apps/server/tests/commands_test.rs
```

The panel (`apps/web`) needs Node 24 and pnpm at the version `package.json`
pins in `packageManager`. The targets that run a tool install from the
lockfile first, and `PNPM` names pnpm where corepack has not put it on the
PATH:

```bash
make web/test             # vitest: apps/web/tests/<area>.test.ts
make web/lint             # ESLint, its style included; make web/fmt fixes that
make web/build            # vue-tsc, then apps/web/dist, which the server embeds
make web/types            # write the types from Rust into src/api/generated, then vue-tsc
make web/types/check      # cargo and git only (CI): fail when they are not the ones committed
make web/fixtures         # write the snapshots and answers the views' tests render, from Rust (cargo only)
make web/fixtures/check   # cargo and git only (CI): fail when they are not the ones committed
make api/schemas          # write the API's JSON Schemas from Rust into docs/api/schemas (cargo only)
make api/schemas/check    # cargo and git only (CI): fail when they are not the ones committed
make api/openapi          # write the API's OpenAPI document from Rust into docs/api/openapi.json (cargo only)
make api/openapi/check    # cargo and git only (CI): fail when it is not the one committed
make web/typecheck        # vue-tsc alone
make web/test PNPM='npx --yes pnpm@12.15.0'
pnpm --dir apps/web exec vitest run tests/shell.test.ts      # one suite
make web/e2e/browsers     # Chromium and WebKit for the browser suite, into apps/web/node_modules
make web/e2e              # build the panel and the debug binaries, then drive them in both browsers
make web/e2e BROWSERS=chromium ARGS='-g overview'            # where Playwright's WebKit cannot run, as on Arch
make web/e2e/run E2E_BIN=<dir>                               # the suite alone, on the mindweaver and mind in <dir>
make docs/screenshots     # build the panel and the binaries, then take the README's screenshots (Chromium)
```

`make test` and `make build` run the panel's targets first (`web` is in the
root Makefile's `APPS`), so they need Node 24 and pnpm; an app's own Rust
targets (`make cli/test`, `make cli/build`) do not. `make fmt` and `make
lint` are Rust only, so a change to the panel runs `make web/lint` too; only
`make lint`'s em dash check (`make lint/em-dash`) reads every tracked file,
the panel's included. A change to a type core or the CLI hands
the panel runs `make web/types` and commits what it writes: CI's `types
drift (web)` runs `make web/types/check`, which needs no Node. A change to
what the snapshot holds runs `make web/fixtures` too, and commits the
snapshots it writes into `apps/web/tests/fixtures`, which the same job
checks with `make web/fixtures/check`. A change to a type the API takes or
gives, core's, the CLI's or the server's, runs `make api/schemas` and
commits the schemas it writes into `docs/api/schemas`, which the same job
checks with `make api/schemas/check`, once the fixtures are written again,
since it holds each to its schema. Such a change, or one to a route of
the CLI's table or the server's own, runs `make api/openapi` too, which
writes the API's OpenAPI document, `docs/api/openapi.json`, from the routes
and those schemas, and which the same job checks with `make
api/openapi/check`. The other way
round, `make install` and `make cli/release` build the panel before the
binaries that embed it where Node and pnpm run, and warn otherwise (`make
web/build/if-node`), and `make web/build` touches
`apps/server/src/panel.rs` so the server compiles again and takes the panel
in. `make cli/release/check TARGET=<triple>` fails unless the release for
that triple serves the panel from the binary once `apps/web/dist` is gone;
CI runs it.

The browser suite (`make web/e2e`) is in neither aggregate: it needs
Playwright's browsers, which `make web/e2e/browsers` downloads into
`apps/web/node_modules` (`PLAYWRIGHT_BROWSERS_PATH`, never `~/.cache`), and
binaries built with the panel in them, which `make web/e2e` builds, the
panel first. Its driver, `apps/web/e2e/run.ts`, makes the fixture workspace
and an empty home in the system's temporary folder, and refuses one with a
`.mind`, a `.mindweaver` or a `.mindflayer` above it, so never point
`TMPDIR` under your home for it. It starts Playwright without
`MINDWEAVER_WORKSPACE`, `MINDWEAVER_HOME`, the `MINDFLAYER_*` names they
are still read by, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and
`OPENCODE_CONFIG_DIR`, so neither the server nor any command a spec runs
sees them; each of those gets the temporary home as `HOME`, `-C` and
`--workspace` naming the fixture and every agent-home option empty; and the
run fails unless that home is as it was made once the server has stopped.
`pnpm exec playwright test` alone is refused, since only the driver builds
the fixture. Playwright's WebKit needs libraries it installs only on Debian
and Ubuntu (`WITH_DEPS=1`, with sudo): anywhere else run it with
`BROWSERS=chromium`, and leave WebKit to CI's `e2e (web)`. A local `make
web/e2e` writes `apps/web/dist`, as `make web/build` does.

`make docs/screenshots` takes the README's images into `docs/images` with the
same harness: `apps/web/e2e/screenshots.ts` builds the demo workspace
(`e2e/demo.ts`) in `mindweaver-demo` under the system's temporary folder, a
fixed place so the paths the images show stay put, serves it with
`mindweaver ui`, and takes each page of the tour in Playwright's Chromium (`make
web/e2e/browsers BROWSERS=chromium`). It fails on a page showing a path
under a home other than the reference's `/home/you`, the account's name as a
folder or the launch token, on an image over 300 KB, and unless its home is
still empty; it removes the folder after. It writes `apps/web/dist` too.
Neither CI nor an aggregate runs it: whoever changes what the panel shows
runs it, looks at every image it wrote, and commits them.

`make dev/link`, `make dev/unlink` and `make install` write into `~/.cargo/bin`
(or `BINDIR`), outside the repository. Don't run them unless asked.

`scripts/agent-probe/probe.sh` runs the real coding agents against their APIs
with your own login, in a temp home under `target/`, to check where they read
skills and rules and what they make of a project's MCP servers. It is never
run by CI or `make test`; don't run it unless asked.

## Layout

Three crates in one Cargo workspace, and the panel they serve, each an app
under `apps/` with its own `Makefile`:

- `apps/core` (`mindweaver-core`): the engine. Projects, a project's settings
  (`settings`, the marker from v3 on), workspaces, which workspace a command
  uses (`resolve`), the shell code that activates one (`activate`), kinds
  (`kind`: skills, rules and MCP servers, and the `Layout` each is kept in,
  a folder, a file at any depth or a file directly in its folder), what an
  MCP server's definition declares, one TOML file a server in a format of
  its own (`mcp`), which names a server can be given (`mcp::name`), what in
  one looks like a token written as it is (`mcp/token.rs`), what each
  agent cannot be given of one, as a renderer asks and `show` says
  (`mcp::Inexpressible`, `mcp::Expressibility`), among it the variables
  Claude reads as empty toward a remote server (`mcp/blanked.rs`), one
  rendered into Claude's `.mcp.json` as its one entry and taken out again
  (`claude_mcp`), into Codex's `.codex/config.toml` the same way
  (`codex_mcp`), and into OpenCode's `opencode.json`, in either of its
  shapes (`opencode_mcp`), each with what it would write planned and
  nothing written (`codex_mcp::planned`, `opencode_mcp::planned`), what an
  agent runs or reaches of one, as its entry spells it (`mcp::Runs`,
  `mcp/runs.rs`), and what `add` is told of
  how one is started or reached, a program or an address, and the
  definition it writes from it (`mcp::Given`), read back from one as the
  options would say it (`Given::of`) and written again into one, every
  other key kept (`mcp::retransported`, `lifecycle::retransport`, what
  `edit` told the options does), the
  agents and where each kind goes for them (`agent`), the catalog, gathering
  (through `port::Remote`), the ledger (through `port::LedgerStore`), installing, keeping installs out of git
  (`exclude`), a project's rules rendered into the block of its `AGENTS.md`
  (`agents_md`) and into Claude's rules folder (`claude_rules`), whether
  Claude reads that block and the `@AGENTS.md` import in a project's
  `CLAUDE.md` that makes it (`claude_md`), installing a rule through those
  three, or as a copy into a project a `mind.toml` marks (`install::rules`),
  installing an MCP server as an entry in the config file of each agent a
  project names, through those renderers, and taking it out, and what an
  install would give each agent, nothing written (`install::mcp`,
  `install::mcp::preview`), changing a project's agents or share mode and carrying its installs along
  (`reapply`), what each project holds, from where and for which agents,
  each MCP server's entry in its agents' config files among it, as it stands
  (`status`, `status::EntryStatus`, told in `install::entries`), finding a project that moved again by its id and giving a
  copy made with its `.mind` one of its own (`relink`), the workspaces this
  machine knows, a list the default
  workspace keeps, and those a front end on one may switch to
  (`known`, `known::switchable`), creating and changing artifacts, whose
  names a file on the disk was written under, Mindweaver's or a Mindflayer
  build's (`names`), and moving a workspace's own into its library, a
  workspace a Mindflayer build made to Mindweaver's names with what it
  wrote in its projects (`migrate::names`), and a project from its
  `mind.toml` or `settings.json` to a `settings.toml` (`migrate`), bringing what was
  installed up to date with where it came from, and what doing so in every
  project would do and why it would leave what it leaves, as a read
  (`update`, `update::Updates`), installing one
  artifact into several projects at once, or taking it, or whatever nothing
  offers any more, out of them (`assign`), planning what a selection of
  boxes ticked and unticked amounts to, which offer of a name each project
  holds, and what clashes or is locked, as a read (`install::plan`,
  `install::holding`), removing one of the
  workspace's own from every project it links before the library
  (`everywhere`), and bringing a skill somebody keeps in a project's agents'
  folders into the library, its copies recorded as installed from there
  (`adopt`), what keeps a project's agents from what it gives them, which
  no agent says, found from file reads alone and told the agent homes and
  the names of the variables set (`doctor`, `doctor::Machine`), the stable
  code each of its errors gives (`code`), the
  snapshot every front end reads a workspace through, with what it counts,
  each project's health, every offer with each project's box of it,
  what doctor finds of each and each of the library's MCP servers as each
  agent would be given it (`view::LibraryServer`), and the part of it
  `list --json` prints (`view`), every path a snapshot reads that a front end watches for a
  change, each with the holder it is of (`watch`), one file of an artifact
  read as text for
  the panel, cut at its front matter when it is the one the artifact is
  read from, and every file it holds listed (`contents`), which paths and
  entries a request from outside
  may name, a workspace to switch to among them (`policy`), the folders
  directly inside one, each said to be a mind project, a workspace, a git
  repository or a plain folder, for a front end with no terminal to pick
  one from (`browse`), what `show`
  and `validate` say of each
  artifact (`reading`), and what core says of one a command is about to
  write, by the command's own checks and in `validate`'s words, for a
  front end judging what is typed (`draft`). Every record, report and view
  it builds is an aggregate of the value objects in `value` (a copy's
  `ContentHash`, a `Redacted` URL or message, an `Origin`, an
  `ArtifactName`, a `Revision`, a `ProjectId`), each of which, and each
  record, a test builds through its object mother in `mother`, compiled only
  for tests (`cfg(test)`, or the `mother` feature a dev-dependency turns
  on). Its only I/O beyond files
  goes through a port (`port`: a `Remote` a gather fetches through, a
  `LedgerStore` the ledger keeps its records in, a `Clock` it stamps them
  by), each implemented by an adapter in `adapter` (`adapter::gix::Gix`,
  its credentials callback in `adapter/gix/credentials.rs`;
  `adapter::sqlite::Sqlite`, every query and schema step, its tests in
  `adapter/sqlite_test.rs`; and `SystemClock`). `ledger::Ledger` is the
  facade every caller holds, reaching its store's methods through `Deref`.
  It never reads the environment. Under its `ts` feature, `typescript` gives what it hands a
  front end TypeScript (the roots its export writes), under its `schema`
  feature `schema` gives it JSON Schema (the roots its export writes, each
  given or taken, `Schemas`, `Together`), and under either `described` says
  what a type serde writes by hand says of itself (`words!`, which writes
  both, and the fields written through a function of their own).
- `apps/cli` (`mindweaver-cli`): the binaries `mind` (`src/main.rs`) and
  `mindweaver` (`src/bin/mindweaver.rs`), and the TUI. `src/lib.rs` only re-exports
  the public API. The parsers are in `src/parse.rs`. `src/commands.rs`
  dispatches them and asks core what a command acts on, and each family of
  commands has a file under `src/commands/`: `project` (init, link, unlink,
  relink, scan), `workspace` (load, unload), `activate` (activate, which), `known`
  (workspaces, workspaces forget), `artifact` (add, edit, rename, remove,
  templates), `read` (list, show, validate), `status`, `doctor`, `settings`
  (agents, share, targets), `gather`, `install`, `update` and `migrate`
  (each at both levels), `adopt` (mindweaver adopt) and `ui` (the panel, at
  both levels).
  `add mcp`'s options for how a server is started or reached are one
  struct both binaries' `add` and `edit` flatten (`ServerOptions`), which
  hands core a `mcp::Given`, and which a template, an editor and contents
  each conflict with by the group clap gives it. `src/render.rs` holds `Outcome`,
  `Failure` and `Warning` (a stderr line, with its code beside it), the
  table helpers and the envelope every `--json` prints (`json`, and
  `as_json`, which lays a text outcome out in it),
  `src/error.rs` holds `CliError` and `CODES`, every code the CLI gives of
  its own, and `src/homes.rs` decides the default home and the agent
  homes, and gathers them with `-W` and `$MINDWEAVER_WORKSPACE` into a
  `Lookup`. `src/task.rs` holds `Task`, work a front end asks for as the
  command lines it is, every front end's rather than the TUI's alone,
  `Refusal`, what a front end with no terminal does not run, and
  `Caution`, what a line takes away, which such a front end asks about
  first.
  `commands::run_line` runs one line a front end built, for the TUI and the
  panel alike. `src/admit.rs` holds a task sent from outside to the path
  policy (`Admission`), and `src/backend.rs` is the panel's backend
  (`PanelBackend`, the server's `Backend` with the CLI's `Query`, `Task`
  and `Outcome`), which `ui` serves (`src/commands/ui.rs`), its reads
  served under `/api/v1` from the table in `src/routes.rs`
  (`routes::table`), each route the query it becomes. The backend,
  `ui` and the server they need are behind the crate's `ui` feature, on by
  default: `--no-default-features` builds the rest without axum
  (`tests/crate_test.rs`), and `make cli/build` builds it that way first.
  Under its `ts` feature, `src/typescript.rs` lists the roots of the
  panel's types the CLI gives (a query, a task, an outcome, the envelope)
  and `Code`, and under its `schema` feature `src/schema.rs` the roots of
  the API's schemas it gives, those and the bodies of the reads that take
  one, the token file and the server's own answers, and `src/openapi.rs`
  the API's OpenAPI document, written from both tables and those schemas,
  each type a route names found among the roots (`openapi::document`,
  `Described`, the server's `schema::Describe`). `src/tui/` and
  `src/install/` are the two screens.
- `apps/server` (`mindweaver-server`): the local server behind the web
  panel, a library `mindweaver ui` runs. `src/backend.rs` is the `Backend`
  trait and the `Refusal` it answers with (with its `Grounds`),
  `src/code.rs` the conditions the server turns a request away for itself,
  each with its code and its status (`Condition`), `src/route.rs` the
  table of reads a backend hands over (`Route`, `Parameter`, `Shape`,
  `API`), a query string read by it, and the table of the server's own
  routes it serves beside them (`Own`, `Sent`, `Answer`), which a
  description of the API walks with the backend's, each route with what a
  request to it may be refused for (`conditions`),
  `src/server.rs` binds and routes, `src/auth.rs` holds the token's
  cookie and Bearer checks, `src/guard.rs` the Host, Origin and Fetch
  Metadata checks and the CSP, `src/api.rs` the JSON endpoints,
  `src/writer.rs` the thread every run and every switch goes through,
  `src/transcript.rs` what
  the session ran, `src/events.rs` the bus `/api/v1/events` streams,
  `src/watcher.rs` what watches the paths the backend names and tells the
  bus what changed, `src/log.rs` the redacting log, a line for each
  request and for each job the writer takes, with how long it took,
  `src/panel.rs` the embedded panel (`apps/web/dist`), and under its
  `schema` feature `src/schema.rs` the roots of the API's schemas it gives
  of its own (`Preview`, `Ran`, `Event`, `Refused`) and its part of the
  API's OpenAPI document, what is HTTP of each route of both tables
  (`paths`), each type a route names asked of the front end (`Describe`,
  `Carried`, `Way`).
- `apps/web` (`mindweaver-web`): the web panel, in Vue and Nuxt UI on Vite,
  with pnpm. `src/main.ts` mounts what `src/panel.ts` installs (router,
  Pinia, Vue Query, Nuxt UI, the transport), which the shell test installs
  too. `src/api/transport.ts` is the `Transport` interface and `useTransport`,
  and `src/api/http.ts` its HTTP implementation, `src/api/snapshot.ts` the
  one query of the snapshot every view reads, `src/api/artifact.ts` the
  queries of an artifact's files and of one file, `src/api/live.ts` the
  tab's one subscription to the server's events, which it turns into what
  Vue Query asks for again, how the server watches, and how a run and a
  switch wait for their views (`settle`, `settleSwitch`),
  `src/api/commands.ts` the runner every change goes
  through (`useRunner`: a task previewed, shown, confirmed when it takes
  something away, and run), `src/api/transcript.ts` the query of the
  server's transcript, `src/api/scan.ts` the query of a scan, asked for
  by hand, `src/api/plan.ts` the queries of a plan of the boxes marked
  on the matrix and of the lines it runs (`usePlan`, `usePreview`), of one
  box once, for the command palette (`usePlanOnce`), and the batch a plan
  applies (`batchOf`, each install said yes to as the task its
  confirmation carries), and
  `src/api/updates.ts` the query of what an update of every project would
  do (`useUpdates`) and the task of updating them all (`UPDATE_ALL`),
  `src/api/folders.ts` the query of the folders inside one, read again
  each time the explorer opens (`useFolders`),
  `src/api/draft.ts` the query of what core says of
  a draft as it is typed (`useJudged`), and `src/api/workspaces.ts` the
  query of what the header may switch to (`useWorkspaces`).
  `src/pages/` holds one
  page per route (file routes): the overview (`index`), the projects
  (`projects`), a project's page (`project`, which `src/routes.ts` names by
  its entry in the query), what could be linked (`discover`, the folders it
  scans in the query), the matrix of offers against projects (`assign`,
  its filters and which way round it is in the query), what an update
  would do (`updates`), the library
  (`library`), one of its own
  (`artifact`, named by its kind and name in the query), its editor
  (`edit`, named the same way), the page adding one (`new`, its kind in
  the query), the library's MCP servers, each as each agent would be
  given it, and the form adding one or changing how the one the query
  names is started (`mcp`), the workspaces it loads and those this machine knows
  (`loads`), the shelf (`shelf`) and the sources (`sources`). `src/address.ts` reads and changes
  the query a page keeps its state in, `src/library.ts` finds the projects
  holding one of the library's own, `src/boxes.ts` a box of the matrix in
  the snapshot, for the matrix and the command palette alike,
  `src/markdown.ts` draws a skill's
  markdown from its tokens, and `src/time.ts` says the ledger's times.
  `src/components/` holds the frame
  (`AppSidebar`, which lists the views for the palette too, `AppHeader`,
  `AppLogo`), what opens over every page on its keys, the command palette
  (`CommandPalette`, Ctrl+K) and the sheet of keyboard shortcuts
  (`ShortcutSheet`, `?`), and the views' pieces: the badges
  of a state, a health and the agents, a count, the warnings panel, the
  commands an empty state names, what is wrong with an artifact
  (`IssueList`), a rendered body (`MarkdownBody`), an artifact's files
  (`FileTree`), the ledger's latest actions (`ActivityFeed`),
  `SnapshotGate`, which draws a page once the snapshot is answered, the
  header's workspace in use and its menu of what it may switch to
  (`WorkspaceSwitcher`), the
  folder to link (`LinkForm`), the explorer it and Discover choose a
  folder from, a folder at a time as core lists it (`FolderPicker`), the
  path of a workspace to load
  (`LoadForm`), a project's agents and share toggles
  (`SettingsForm`) and its migration, dry run first (`MigrateOffer`), the
  removal of one of the library's own everywhere, dry run first
  (`RemoveOffer`), its rename (`RenameForm`), the editor, CodeMirror in a
  shadow root of its own (`MarkdownEditor`), what core says of a draft
  (`DraftVerdict`), the form of an MCP server's transport, each of its
  fields an option's value and a secret a variable's name
  (`ServerForm`), the
  matrix's grid, drawn a screen at a time (`AssignGrid`), and
  what the runner draws beside the pages:
  `CommandRunner`, which makes it and says how each task ended in a toast,
  the task shown before it runs (`CommandDialog`), what the last run said
  (`WarningsDrawer`) and the transcript (`TranscriptDrawer`).
  `src/stores/` holds Pinia's UI state, the boxes marked on the matrix
  among it (`marks.ts`),
  `src/query.ts` the Vue Query client, `src/theme.ts` the palette, `src/color-mode.ts` the
  colour mode, and `src/overrides/` what replaces two pieces of the
  libraries for the CSP. `src/*.d.ts` are written by the Vite plugins and
  committed, and so is `src/api/generated/`, written from Rust by `make
  web/types`. `e2e/` is the browser suite: `run.ts` its driver,
  `environment.ts` what every process it starts runs in, `fixture.ts` the
  workspaces it builds with the commands, `panel.ts` what the specs share,
  and the specs; `playwright.config.ts`, beside `vite.config.ts`, starts
  `mindweaver ui` over the shared workspace. Beside the suite, through the same
  `environment.ts`, `demo.ts` builds the README's demo workspace and
  `screenshots.ts` takes the images of it (`make docs/screenshots`).

Beside them, `examples/` is content and not an app: a weaver workspace whose
own two skills and two rules anyone can `mindweaver load` to try Mindweaver
without gathering. `apps/cli/tests/examples_test.rs` holds every example to
`validate` with no warning and finds the "Try it with the examples" output
of the reference, `docs/reference.md`, in what the commands print, so
changing an example's summary line means changing the reference too.

Outside `apps/`, `scripts/agent-probe/` holds the agent probe: `probe.sh`
isolates a run and checks the real home is left alone, `<agent>.sh` holds
one agent's checks (`claude.sh`, and `codex.sh` and `opencode.sh`, which
check MCP servers only so far), and `mcp.sh` and `mcp_server.py` are what the
MCP checks share, the second a stand-in MCP server that writes down what an
agent started it with and sent it. `scripts/serves-panel.sh` is
what `make cli/release/check` runs: it starts a release's `mindweaver ui` with a
home of its own and every agent home empty, and asks it for the panel.
`docs/api/schemas` holds the API's JSON Schemas, a file a root, which the
crates' `schema` tests write (`make api/schemas`) and nothing edits by
hand, and `docs/api/openapi.json` its OpenAPI document, which the CLI's
`openapi` test writes (`make api/openapi`) and the binaries embed and
serve.

## Invariants that hold the design together

These are easy to break from a single file without noticing.

- **Rules live in core.** The CLI parses, asks core and renders. Name limits,
  front matter, where a kind lives, where it goes for each agent, where each
  agent reads it and sort order are all core's, so a future front end cannot
  disagree with this one. No front end spells an agent's folder or calls
  `TargetTable::standard`: which table applies is the project's, in core, the
  standard one overridden for each kind its settings name `targets` for, with
  every folder an install may use (`misplaced_target`), so code that needs
  one asks the project (`MindProject::targets`), as discovery and an install
  (`install::targets`) do. Which agents read none of a project's targets is
  core's too (`ProjectStatus::unreached`), and so is which read a rule it
  received nowhere (`ProjectStatus::missed`). Which kind a name typed bare
  is, to install or take out, is core's (`install::offered_typed`,
  `uninstall_typed`), and so is what a render of rules warns of
  (`Rendered::warnings_of`). Every rule of a batch of boxes is core's
  plan's (`install::plan`, `Plan::of`): which offer of a name a project
  holds (`install::holding`), that an offer ticked in place of another
  replaces it, one line an offer and one a name, kind by kind, and that two
  offers of one name clash and a change to somebody's or an edited copy is
  locked, by the code the command refuses it with (`Standing::refused`).
  Where an agent reads
  (`Agent::reads`) is kept apart from where it is sent a copy: OpenCode reads
  `.claude/skills` and is sent nothing there. Which config file an agent is
  given an MCP server's entry in is core's too (`Agent::mcp_file`), asked of
  the renderer that writes it, OpenCode's the first of its four there is.
  Which folders a project is read
  from is core's too (`Catalog::folders`), and so are the names at a
  project's root that show an agent works there (`Agent::reads_at_root`) and
  the order `mindweaver scan --suggest` ranks what it finds in (`scan::Order`),
  in which the minds screen, the link form and the panel's page of what
  could be linked show what is not linked, without sorting it.
- **Values are value objects, and the world is reached through ports.**
  A field that holds a hash, a URL or words quoting one, an origin, an
  artifact's name or a revision holds the value in `value`, never a
  `String`: one held to a rule (`ContentHash`, `ProjectId`, `Redacted`) is
  made only the way its rule allows, so no `From<String>`, and one named
  apart (`ArtifactName`, `Origin`, `Revision`, and an MCP server's
  `InputName` and `VariableName`) holds any text, since core reads
  leniently, the last two judged by a rule they say of themselves and are
  not made by (`is_writable`, `is_portable`). A new value joins `value` through its `text_value!`
  macro, so it reads as its text and goes out as one, said in TypeScript
  and JSON Schema as a `String` is, and no contract moves
  (`make web/types/check`, `make api/schemas/check`); its store's adapter
  says how it is kept (the ledger's `ToSql` and `FromSql`), never `value`.
  Whatever core needs of the world beyond its files is a trait in `port`,
  in core's own words, its library's errors and types turned into those at
  the adapter: a gather fetches only through `Request::remote` (`Gix`
  unless `through` hands another), the ledger keeps its records only in its
  `LedgerStore` (SQLite unless `Ledger::new` hands another) and reads the
  time only through its `Clock` (`with_clock`), and no module but
  `adapter` names `gix` for a fetch or `rusqlite` at all: a store's error
  reaches `LedgerError` boxed, so the port names no store's type. A new library outside core is an adapter of a port, injected
  with the default built in, as D-14 has it for HTTP; a new front end is
  one more driver of the command lines (`commands::run_line`), never a
  path of its own into core (docs/architecture.md, "Values, ports and
  adapters", D-25).
- **Two binaries, one parser.** `mindweaver <cmd>` is `mind weaver <cmd>`, and
  both reach the same function. `mind add` and `mindweaver add` (with or without
  `-p`) are one implementation given a different holder
  (`WeaverWorkspace::own` makes the workspace look like a `MindProject`).
- **The TUI and the panel only run commands.** A key produces a `Task`
  (`src/task.rs`, shared by every front end that runs commands),
  and `Task::commands` turns it into argument lines that a `Runner` hands to
  `execute`, which runs each through `commands::run_line`: it parses them
  with `Cli`/`WeaverCli` and runs them through `run_with`/`run_weaver_with`
  with the screen's `Lookup` and a `gather::Watch`, what the binaries'
  `run_watched`/`run_weaver_cli_watched` call once they have read the
  environment. A test's stand-in `Runner` is handed the same words. The
  panel is the second front end built this way: the browser sends a task as
  JSON, the backend holds it to the path policy (`admit::Admission`), shows
  its lines before they run (`Backend::preview`) and runs them through
  `run_line` on the server's one writer thread, with the `Lookup` `ui` was
  launched with, the workspace pinned and named with `-W` in each line's
  own words (`task::naming`), so a line previewed, run and kept in the
  transcript says which workspace it reached. A switch from the panel's
  header re-pins it (`Backend::switch`), on the same thread, only to a
  workspace core lists (`known::switchable`, held by `policy::switched`),
  and is no line: nothing of it is in the transcript, which stays true
  since every line names its workspace. A new feature is written as a command
  first; the TUI and the panel gain it by producing that command's words.
  Never add a TUI-only or panel-only code path, nor one where the backend
  does the work itself rather than run a line. The panel's command palette
  is another way to a page's change, never a change of its own: it sends
  the task the page offering it sends (`UPDATE_ALL`, the link form's, one
  box's plan applied with `batchOf`), through the same runner. A gather's lines run on a
  thread of their own (`Task::runs_apart`), still through the same parsers:
  the thread changes where they run, not what they are. The install screen applies the same way, in the TUI or
  opened by `mindweaver install` (`install::apply`): its marks are the
  `mindweaver install` and `mindweaver uninstall` lines they amount to
  (`Screen::task`), as core plans them (`install::Plan::of`), each change
  the task that runs it (`Task::from`), each run whatever came of the others
  (`Task::chained`), so its transcript runs again. An MCP server marked
  that comes from elsewhere than the library is previewed when the screen
  is built (`install::mcp::preview`), each agent's line shown in the
  confirmation before the screen applies (`Screen::previews`), and only
  its line carries `--yes` (`Task::confirmed`). The panel's matrix confirms
  one the same way: a plan hands, beside its tasks, each install that
  waits for a yes with the same lines (`Planned::asks`, laid out by
  `install::foreseen` under `install::heading`, which the screen uses too)
  and the task carrying the yes (`Task::confirmed`), which the page sends
  in the place of the plan's own only once those lines are ticked as read
  (`batchOf`); `Task::Install`'s `yes` is JSON for that alone, the panel
  setting it on no task of its own, and the path policy refuses one sent
  with any other install (`policy::confirmed`, `install_yes_unasked`). The
  MCP page's form sends `Task::Add` with a `server` and `Task::EditServer`,
  `add mcp`'s and `edit`'s options as an `mcp::Given`, for the task module
  to spell. A batch a screen builds
  from its marks or its toggles is built in the task module, from what was
  marked (`Task::minds`, from each folder's `MindsChange`) or what the
  settings and the toggles say (`Task::settings`, from two `Toggles`), and
  the panel sends its marks and its toggles (`Task::Marks`,
  `Task::Toggled`) for the same function to build, never the batch: one
  rule for which lines they are, which `backend_test` holds the minds
  screen and the panel to, the same folders for the same marks, though the
  panel names a folder inside the workspace by its absolute path where the
  screen names it by its route. What a front end with
  no terminal does not run is one list, `task::Refusal`, read off the
  parsed command by a match with no wildcard (`Refusal::of_mind`,
  `of_weaver`), so a new command says whether it is refused before it
  compiles; a task is refused for its lines (`Task::refusal`). `edit` is
  refused in an editor and with `--contents -`, standard input, and run
  with `--contents FILE` (`Refusal::of_edit`); the one task that builds
  that line, the panel's save (`Task::Save`, its text and the hash it was
  read with), names the workspace's scratch file for its text
  (`task::scratch_route`, by its hash, so the line previewed is the line
  run), which the backend writes before the line runs and takes away
  after, whatever came of it (`backend::Scratch`), so a browser names no
  file to read. The TUI refuses only its screens,
  `ui`, which would hold it until a Ctrl-C the screen keeps for itself, and
  standard input, which is the screen's own keys. What a line takes away, which a front end with
  no terminal asks somebody to confirm before it runs it, is one list too,
  `task::Caution`, read the same way (`Caution::of_mind`, `of_weaver`,
  `task::cautions`), and the panel's backend says it of the lines it
  plans with each preview (`backend::Confirm`), never a list in
  TypeScript. Each front end hands back, when it closes, every
  line that changed something (`Task::writes`), laid out by one function
  (`tui::state::transcript`): the TUI when it quits, `ui` when Ctrl-C stops
  it.
- **A gather is watched, and stops only before it places.** Its `Request`
  carries a `gather::Watch`, core's progress callback and cancel flag; no gix
  type reaches a front end. A cancel is honoured until the shelf starts to
  change, and leaves the shelf and the ledger as they were, so a write to
  either before that last check is undone on a cancel, as registering the
  source is.
- **Output is built, then printed.** Commands return an `Outcome` (stdout
  text, stderr lines, ok), so tests assert on the exact text a user sees.
  The two that run until they are stopped write as they run, the TUI its
  screen and `ui` its link, a note, the token file it is asked for and,
  with `--log`, a line on stderr for each request answered and each job
  its writer takes, as it happens (`ui::on_stderr`), and hand back their
  transcript.
  Each stderr line, and each of a `Failure`'s warnings, is a
  `render::Warning`: the text a terminal shows, word for word, and beside
  it the code of what it says (`code()` of the core error, refusal or
  failure it renders, or `CliError::code`), `None` for a hint or a note. A
  line built from a value with a code takes that code; never a code spelled
  by hand for a line built from text.
- **Reading commands print JSON in one envelope, and its names are a
  contract.** A reading command's `--json` (`list`, `show`, `validate`,
  `templates`, `status` and `doctor` at both levels, `workspaces`, `scan`
  and `gather list`) prints `render::json`'s `{"version": 2, "data": ..., "warnings":
  [...], "coded_warnings": [...]}`, never a shape of its own, with what its
  text says on stderr as `warning:` lines in `warnings`, the same lines
  with their codes in `coded_warnings`, and the exit status its text has;
  those lines are still said on stderr too, a `hint:` there alone. One that
  reads as the text does lays out the text's own outcome
  (`render::as_json`), so that every warning it says, one added on the way
  out included, reaches the envelope, and its exit status is the text's by
  construction. `data` is a type core builds and sorts,
  deriving `Serialize` with its fields named in snake_case; a kind, an agent
  or a state goes out as the word a command line takes for it
  (`Kind::slug`, `Agent::name`, `State::slug`, `Standing::slug`), serialized
  by hand, never derived from the variant. The read types #61 named
  serialize so (`docs/architecture.md`, "The read types serialize", lists
  them and the older ones that do not yet), and every new read type
  derives `Serialize` from the start, written by hand only where a derive
  would print the wrong thing (a word like those, a type holding more than
  a reader is given, an error, which goes out as its `code` and message
  through `code::Said`), with a test that pins its JSON name by name. A
  path goes out as text whatever bytes it holds (`paths::lossy`), and a
  URL only through `url::redacted`. A name once printed is a
  contract: dropping or renaming one, or changing what it holds, moves
  `ENVELOPE_VERSION`, and adding one does not. A test reads it back with
  `common::enveloped`, into structs of its own that refuse a field they do
  not name.
- **Front ends read a workspace through the snapshot.** What a screen shows
  of a workspace is core's `view::Snapshot` (`Snapshot::read`, or
  `Snapshot::of` for a front end that resolved the workspace itself), and of
  a project no workspace links `view::Project::read`: built and sorted in
  core from the read types, never put together from the reports in a front
  end. It is a read: the ledger only through `ledger_for_reading`, the
  default workspace never made, nothing scrubbed, and no scan, which is a
  query of its own (a test counts scans with `scan::scans`, a test build's).
  What doctor finds of each project is in it (`doctor`), and of a project
  no workspace links beside its read (`view::Project::read_examined`),
  told the agent homes and the names of the variables set the front end
  decided (`doctor::Machine`, handed to `Snapshot::of` and
  `Snapshot::read`), and what it reads is watched as the rest is
  (`watch::watched`, given the same agent homes).
  Its names are a contract under `view::FORMAT`, as a report's are under
  `ENVELOPE_VERSION`, and every URL in it, an error's message included,
  goes out through `url::redacted`. What a front end reads beside it is
  only what it cannot hold: a scan (the panel's `Query::Scan`, asked for
  by hand and answered with the backend's `Scanned`), the folders
  directly inside one, for the panel's explorer to choose from
  (`Query::Folders`, answered with core's `browse::Folders`, which no
  command line has, since a shell has `ls`), a plan of boxes
  (`Query::Plan`, answered with the backend's `Planned`, core's plan with
  each change the task that runs it, which the panel sends back as one
  `apply_install`, never a task of its own), what an update of every
  project would do (`Query::Updates`, answered with core's
  `update::Updates`), what core says of a draft about to be written
  (`Query::Validate`, answered with core's `draft::Judged`), what the
  panel may switch to, which depends on where it was launched
  (`Query::Workspaces`, answered with core's `known::switchable`), and
  which workspace
  a project's own lines consult (`docs/architecture.md`, "The read
  model").
- **Every error has a stable code.** Each public error type in core,
  `ValidationIssue`, and `install::Refused`, why an install or an uninstall
  wrote nothing (`Installed::refused`, `Removed::refused`, and for a copy
  an update or a change of agents leaves, `update::Step::refused` and
  `reapply::Why::refused`), is given `code()` with `code::codes!`, which
  writes it as a match with no wildcard, so a new variant does not compile
  without one, and lists the codes it gives in `CODES` from the same arms.
  Each arm names one variant, and the macro refuses a wildcard or an
  or-pattern, which would give
  several one code and list it once. A code is unique across core, not
  only its type, never renamed nor moved to another condition, and named
  for the condition in snake_case. A variant
  that only carries another error of core's, adding where or what came of
  it, takes that error's code (`carried`), and one that says another's
  condition in words of its own echoes that one's code (`echoed`), as
  `update::Left` says an edit left alone by `install::Refused`'s
  `edited`. A new error type joins
  `code::ALL`; `code_test.rs` holds no two codes alike and finds a type
  that derives `Error` and never joined. The server, which can use no
  mindweaver crate, gives its own conditions codes the same way with a
  `codes!` of its own (`apps/server/src/code.rs`, `Condition`, its status
  beside each code), and the CLI's `error_test.rs` holds them unique
  beside core's and the CLI's, which only the CLI can see together; every
  answer under `/api` that is no success is `{"error", "code"}`, a
  backend's refusal with its front end's code and anything else with the
  server's, and the TypeScript `Code` holds all three lists.
- **Failures are collected, not raised.** One unreadable artifact or stale
  registry entry is a `warning:` on stderr (in the envelope too, under
  `--json`) and a non-zero exit, never a reason to hide the rest. An absence (no
  `skills/` folder yet) is not a failure.
- **Never overwrite what somebody wrote.**
  - New markers are written from string templates so their comments survive:
    `weaver.toml`, and a project's `.mind/settings.toml` (`Settings::text`),
    made with `create_new`. Edits go through `toml_edit` and one `rewrite`,
    which reads the file again, hands its text to the edit and writes back
    only what changed through a temp file of its own (`replace_file`, named
    for the run) plus a rename: `edit_array`, or `edit_marker_held` for a
    writer that holds the lock already, and `MindProject::change_settings`,
    which edits each key whose value changed where it stands
    (`settings::edited`), `directories` and `targets` an entry at a time,
    keeping the comment on a changed value's line: comments, a key this
    build does not know, the key order and a hand-made layout survive, and
    nothing is written when nothing changed. Only a `settings.toml` is
    changed (`MindProject::changeable`): a `settings.json`, read as it was
    written, is refused with a pointer to `mind migrate`, and `config_entry`
    is never used to edit a project's settings.
    Its agents, targets and share mode change through `reapply::apply`
    (`mind agents`, `mind targets`, `mind share`), which carries what is
    installed along with them. Its id is refused there, naming the project
    for good, and changes only through `MindProject::change_id`, which
    `relink::new_id` calls for a copy made with its `.mind` (`mind init
    --new-id`).
  - Creation uses `create_new` or `create_dir`.
  - `rename` rewrites only the `name:` line (`frontmatter::with_key`).
  - `remove` (`mindweaver remove --everywhere` too), `migrate` at either level,
    `agents`, `share`, `targets`, `update` and `mindweaver adopt` are dry runs
    without `--yes`.
    A migration of the library moves nothing a project holds, refuses as a
    whole, and keeps the marker it rewrites as `weaver.toml.v<version>`
    (`create_new`). A project's (`migrate::project`) writes its
    `settings.toml` with `create_new`, keeps the `mind.toml` or
    `settings.json` it replaces as `.mind/tmp/<name>.v<version>`, never runs
    git (it says `git rm -r --cached .mind` instead), and puts back every
    step it took when a later one fails. A workspace's move to Mindweaver's
    names (`migrate::names`) refuses as a whole over a name taken where
    anything would go, a block it cannot read and a ledger another process
    holds open, moves of a project only its blocks' markers, its exclude
    lines and the rule files it recorded unchanged, renames the ledger and
    never migrates it, never runs git, and puts back every step it took
    when a later one fails.
  - A file somebody else owns is edited only inside Mindweaver's marked
    blocks or, in an agent's config file, which can carry no marks, one entry
    at a time (next bullet). Every byte outside them is kept, and a malformed
    block is refused, not repaired. The blocks:
    `# mindweaver:begin`/`# mindweaver:end` in
    `$GIT_COMMON_DIR/info/exclude` (`exclude`), where an edit rewrites only its
    own project's `# project <key>` group: an install writes one entry per
    copy for a local project, keyed by its id, before the copies land, an
    install of an MCP server one per config file it makes, before the file
    lands, and a render of Claude's rule files one per file, and none opens
    the file for a shared project or one a `mind.toml` marks. A
    project turning shared has its whole group forgotten (`exclude::forget`),
    and one turning local an entry written for each copy in place. No entry
    is written for a copy git tracks (`install::untracked`), by an install,
    an adoption or a project turning local: it would keep none of the copy
    out, and hide what is added to it later. A
    relinked project's lines, and a copy's given a new id, are carried to its
    new place or key (`exclude::carry`), never left as a stale group. A team's
    `.gitignore` and `core.excludesFile` are never edited; the `.gitignore`s
    Mindweaver writes are `.mind/tmp`'s and a local project's `.mind`'s own
    (`MIND_IGNORE`), each inside a folder it owns, made with `create_new`,
    and the second is removed, when the project turns shared, only while it
    still holds `MIND_IGNORE`. And `<!-- mindweaver:begin -->` and
    `<!-- mindweaver:end -->` in a project's root `AGENTS.md` (`agents_md`),
    a section a rule under `<!-- mindweaver:rule <route> -->`: a render
    replaces the block whole, adds it after what the file holds, an empty
    line apart, or drops it, that line with it, and gives the file back as
    it was, in the file's own line endings. A
    marker written any other way, a rule's marker outside the block and one
    route twice are refused like a broken block. A missing `AGENTS.md` is
    made with `create_new`, and only then said to be created; one that is
    there goes through `replace_bytes`, written through a link only to a
    file inside the project; and the file is taken away only when
    Mindweaver made it, as the ledger notes (`created_files`), and nothing
    else is left in it. No block
    is left in a workspace's root, unless a project it links is there, nor
    in a folder above a project it links, whose agents would read it too,
    nor inside an agent's own folder (`AgentsMdError::UserScope`); a render
    that takes one out goes ahead there.
  - Claude's rule files, `.claude/rules/mindweaver/<route>.md`
    (`claude_rules`), hold a rule as written, front matter included. A
    render is handed what the ledger records of them
    (`claude_rules::Recorded`: each file's route and its `hash::bytes`),
    each file it may make recorded before it is made (`install::rules`), and
    a file is Mindweaver's only while it holds exactly that: only such a
    file is replaced or taken away. A file nothing
    recorded, even one holding the rule, or one changed since, is left
    exactly as it is and said, and so is a link at a rule's place. A new
    file is made with `create_new`; one replaced goes through
    `replace_bytes_unresolved`, whose temporary ends in `.tmp`, which
    Claude never loads; emptied folders go up to and including `mindweaver`
    and no further. A render asked for a rule is refused wherever the block
    is (`agents_md::misplaced`), and both renderers ask
    `RuleManifest::is_scoped` which rules are about some files only.
  - The import of `AGENTS.md` in a project's own `CLAUDE.md` or
    `.claude/CLAUDE.md` (`claude_md`) is one line between the block's
    markers, read by the same search (`agents_md::locate`), at the top of
    the file, an empty line below it that goes with it; a rule's marker
    there is refused like a broken block. Whether Claude reads the block is
    one predicate (`claude_md::reads_agents_md`): the three names in the
    project's root and every folder above it but `/`, the user memory file
    left out. `claude_md::reach` decides Native, the import, or Claude's
    rules folder (`Reach::mode`), never more than one, so Claude reads each
    rule once; an `@AGENTS.md` line somebody wrote in one of Claude's files
    counts as the import, and nothing is written. `claude_md::render` never
    makes a `CLAUDE.md` or a `CLAUDE.local.md`, never edits a file above
    the project (it is the fallback's reason instead), a link, the user
    memory file or the `AGENTS.md` itself, edits a file git tracks
    (`exclude::tracks_file`) in a local project only with `edit_tracked`,
    and takes the import out with the last rule, giving the file back as it
    was; one it may no longer take out is left and said.
  - An entry in a config file somebody else owns (`.mcp.json`,
    `opencode.json`, `.codex/config.toml`) is changed one at a time through
    `config_entry`: JSON and JSONC entries are edited through jsonc-parser's
    CST (an insert spliced into the text where it says) and TOML through
    `toml_edit`, never parsed and serialized again, so every byte outside the
    entry is kept. What an edit wrote is read back and refused unless only
    the entry changed, and a file that does not parse is refused, never
    rewritten. A changed file goes through `replace_bytes_unresolved`,
    keeping its permissions; a new one is made with `create_new`; and one is
    taken away only through `ConfigFile::take`, when its caller says what it
    holds is nothing. The module knows no agent: the file, the key path,
    the entry, how much of a file is read (`ConfigFile::capped`) and which
    documents are empty are the caller's. Claude's `.mcp.json`
    (`claude_mcp`) is read as strict JSON up to 2 MiB, one holding a comment
    or a trailing comma refused (`mcp_json_not_strict`) rather than given an
    entry Claude would never load; its entry always says its `type`, writes
    each input as `${VAR}`, asks `McpDefinition::inexpressible` what Claude
    cannot be given and writes no approval; and the file goes with its last
    server only when the caller says Mindweaver made it and nothing else is
    in it (`config_entry::holds_only`). Codex's `.codex/config.toml`
    (`codex_mcp`) is TOML up to 2 MiB, its table written with each key only
    when it holds something and each input only where a key names its
    variable (`env_vars`, `env_http_headers`, `bearer_token_env_var`, the
    shapes `passed_on` and `bearer` say and the report asks of a secret),
    never `experimental_environment` nor `environment_id`; it reads, never
    writes, the user's `config.toml` in the Codex home `AgentHomes` gives,
    refusing a name it holds (`codex_mcp_collision`) and warning of a
    project it does not mark trusted (`codex_mcp_untrusted`), refuses a
    project whose file is inside an agent's own folder, takes its file
    away only when taking the entry out of the text leaves it blank, since
    a comment is somebody's and the document holds none, and leaves
    `.codex` when its file goes. OpenCode's (`opencode_mcp`) is the first
    of `opencode.json`, `opencode.jsonc`, `.opencode/opencode.json` and
    `.opencode/opencode.jsonc` there is, a link counting so that it is
    refused, and `opencode.json` made when none is, each read as JSONC up
    to 2 MiB, as OpenCode reads all four; its entry is V2's minimal
    `mcp.servers.<name>`, and V1's `mcp.<name>` with `enabled` only in a
    file that keeps a server so (a server replaced in the shape it stands
    in, one in both refused, `opencode_mcp_twice`, and V1's written
    without `cwd`, `opencode_mcp_v1_cwd`), each input as `{env:VAR}` and
    what OpenCode cannot be given the report's (`text_substituted` among
    it); it refuses a project whose file is inside an agent's own folder
    (`opencode_mcp_user_scope`), as Codex's does; and only the
    `opencode.json` Mindweaver made goes with its last server, when what
    taking the server out leaves is the objects on the way, empty, and no
    comment (`config_entry::jsonc_commented`).
  - `ui --token-file` (`TokenFile`, in `src/commands/ui.rs`) makes its
    file with `create_new`, mode 0600 on Unix, so whatever is at the path,
    a link included, is refused and never written over or through, and
    takes it away when `ui` stops only while it holds, byte for byte, what
    was written: anything else there is somebody's, left as it is and said.
    It holds the token, which goes nowhere else new: not to stderr, not to
    a `Debug`, and the link and the transcript stay as they were.
  - `mindweaver init` refuses a directory for the workspace's own that overlaps
    the shelf or the cache, which are reserved for gathering
    (`gathered_overlap`). `mindweaver init` and the workspace's own artifacts
    never put an `AGENTS.md`, a `CLAUDE.md` or a kind's folder in the
    workspace's directory; they live in `.mindweaver`. An install into a
    project at the workspace's root is that project's (D-03).
- **Mindweaver only manages what it installed.** The ledger's record of an
  install (one installation, a target for each copy it wrote, its origin and
  the `hash::tree` of what it wrote, that hash's one definition, or for an
  MCP server an entry target for each entry it wrote into an agent's config
  file, keyed by the agent, with the `hash::fingerprint` of what it wrote,
  schema 6's `installation_entries`) is what
  allows overwriting and deleting. An install writes a copy in every target
  `install::targets` gives (each folder the project's agents read, or the one
  a `mind.toml` names), built from one tree and put in place together or not
  at all (`copy::Stage::place`). Standing is by the path each record names
  (`Ledger::copies`), never by name: anything unrecorded in any target is
  `Standing::Foreign`, refuses the whole install with nothing written in any
  target, and is never touched. A recorded copy is told by the hash of what
  was written there and by what its origin offers now (`install::Standing`,
  `install::standings`, D-04): `Current`, `Outdated`, `Edited`, `Missing` or
  `Orphaned`, the first true of edited, orphaned, missing and outdated said
  over the rest. An MCP server's entry stands in the same words, told by its
  record, the fingerprint its agent's file holds under its name and what is
  offered now as its renderer renders it for that agent
  (`install::entries`, `install::entry_standings`): foreign by its name in
  the file rather than by a path, missing when the file cannot be read too,
  which is said with the renderer's code (`install::ConfigUnread`), and
  each file read as its renderer reads it to write. An `Edited` copy refuses the whole install or uninstall as a
  foreign one does (`Installed::Edited`, `Removed::Edited`) unless it is
  forced (`install_with`, `uninstall_with` and `uninstall_typed` given
  `Edits::Forced`, the CLI's `--force`), and forcing never reaches a copy
  nothing records; a render keeps an edited section of the block as it holds
  it. A record with no hash counts as current only while its copy matches its
  offer byte for byte, and as edited otherwise; the first install over a match
  stores its hash. An update (`update::apply`) writes an `Outdated` or
  `Missing` installation again from the place its record says it came from,
  through the install's own body (`install::install_held`), and a project's
  rules in one render that keeps an orphaned rule as it is
  (`install::rules::update_held`); it leaves an `Edited` one, every copy of
  it, unless forced, and says an `Orphaned` one without failing. An
  uninstall removes the copies its record names and nothing else. An MCP
  server is installed as entries (`install::mcp`): one installation, an
  entry in the config file of each agent the project names, written through
  the agent's renderer and recorded by its fingerprint, all of them or none,
  a file that fails putting back each written before it and recording
  nothing. An agent its renderer refuses the definition (one it cannot
  express, an input read from no variable, a name the user's Codex config
  holds), one whose file git tracks in a local project without
  `edit_tracked`, and Codex in a project it does not trust, which is written
  all the same, are said as not reached by their code
  (`install::mcp::Rendered`), the others given it; one reaching none is
  refused (`mcp_unreached`). A server of the name nothing records in a file
  it writes is `Standing::Foreign` and refuses the whole install
  (`Installed::Foreign`), and an edited entry refuses an install or an
  uninstall unless forced, forcing never reaching an entry nothing records.
  An agent its renderer now refuses loses the entry an earlier install
  recorded for it, taken out as an uninstall takes it, and one whose file
  is withheld keeps it, recorded. An
  uninstall takes out only the entries its record names, from the file
  each names (`opencode_mcp::remove_from`), every other server kept, and a
  file Mindweaver made with its last server; one in a file git tracks in a
  local project without `edit_tracked` refuses it (`mcp_entry_unedited`).
  An update writes one through `install_held`, as a skill. A local project
  keeps out of git only the config files an install makes, a line each
  written before the file lands, noted in `created_files`. A server is
  stepped through as every kind is (`install::installs`, which `survey`,
  `receives`, a name typed bare and the plan follow), and never asked for
  a folder (`install::targets` refuses it, `not_copied`). What an install
  gives each agent is decided before anything is written (`decide`), and
  a preview asks the same of each renderer's plan, writing nothing
  (`install::mcp::preview`, `codex_mcp::planned`, `opencode_mcp::planned`);
  what each agent runs or reaches is read off its renderer's own entry
  (`mcp::Runs`, `Entry::runs`), never spelled again in a front end. One
  from elsewhere than the library, a loaded workspace's or the shelf's,
  is only previewed until somebody says yes (`install::asks_first`,
  `install::Confirmed`, the CLI's `--yes`), the library's own installed
  as a skill is. No adoption or gather writes one; `add`
  writes a definition where its holder keeps servers, as it writes a
  skill, and records nothing, writing nothing into an agent's config. No copy
  holds a skill's top-level `.claude-plugin`
  (`install::LeftOut`), and none lands inside an agent's own folder
  (`InstallError::UserScope`). A v3 project is listed from its agents' folders
  too, where an unrecorded copy outside the folder it authors in is
  `Provenance::Foreign` (by the path a record names, `Catalog::with_records`),
  and a lifecycle change looks only where the project authors
  (`Catalog::discover_authored`), so it never reaches one. Core's lifecycle
  changes see to the record of the copy the change reached, and only that
  copy's (D-04): `finish_edit` and `replace_contents` keep it, so the copy
  reads as edited; `rename`
  carries it to the new name and place (`Ledger::renamed_copy`, dropping it
  only when another installation holds the new name); `remove` uninstalls the
  copy, dropping its record (`Ledger::uninstalled_copy`); and `add` drops a
  stale record under the name it writes. A v3 project whose authored folder is
  also a target keeps the record of its other copies, and a local one stops
  excluding a removed copy's place, or moves the entry with a renamed one.
  Each returns what it did (`lifecycle::Record`, and the `scrub` that opening
  the ledger to write ran; an edit only reads it), which the CLI only renders.
  A change of a project's agents or targets (`reapply::apply`) moves only what
  the ledger records: a new folder gets a copy of one whose `hash::tree` is
  the one recorded, never one over somebody's, and a dropped folder loses a
  recorded copy only while its hash is the one recorded; one changed since is
  left there, and its record and exclude entry dropped, since no install
  writes in that folder again. A project's move from a `mind.toml`
  (`migrate::project`) moves its installs the same way, through
  `reapply::retarget`, and holds a record from before hashes to its offer
  (D-04). Being made once, it is refused when the workspace `mind migrate`
  consults cannot be read, never made as if that one recorded nothing.
  Removing one of the workspace's own everywhere (`everywhere::apply`) and
  `uninstall --orphaned` (`assign::uninstall_orphaned`) take copies out only
  through `install::uninstall_with`, the first never forced, so an edited
  copy and somebody's are left as an uninstall leaves them.
  `install::disown` and `install::carry` are private to core, so no front end
  drops or moves a record on its own. The one way a copy nobody installed
  becomes recorded is `mindweaver adopt` (`adopt::apply`), asked for by name: it
  copies the skill into the library and records the project's copies, every
  target an install uses, under the `hash::tree` they hold, so they read as
  current (a target with no copy as missing, for an update to fill), never
  writes in the project, and refuses whatever an install could not have
  written there (a link, a top-level `.claude-plugin`, copies that differ,
  a skill `artifact::strict_issues` refuses), the project's own, and a name
  the library holds. A front end adds through
  `lifecycle::add`, never `create` or `create_from`, which leave a stale
  record in place. A rule is one installation too: in a project marked by its
  settings, its targets are its section of the block, recorded as `AGENTS.md`,
  and its file for Claude, each with a hash of its own
  (`Target::content_hash`, since the two hold different bytes); in one a
  `mind.toml` marks, its copy in the rules folder, filed by its route.
  Installing or uninstalling one renders every rule the project has
  (`install::rules`), each from the offer it was installed from: a section in
  the block, or a file at a rule's place for Claude, that the ledger does not
  record is `Standing::Foreign`, and a block holding such a section refuses
  the render (`NotRecorded`), as a rule no longer offered does
  (`NoLongerOffered`). What a project received as rules is listed from the
  ledger and the block (`status`, D-23), never by discovery, which reads no
  block, and a front end finds what to change through
  `lifecycle::find_to_change`, which refuses such a rule as received rather
  than as nothing, and an MCP server the ledger records as one the project
  received too (`LifecycleError::ReceivedServer`), with a hint to uninstall
  it or change the library's definition and update.
- **Strict when writing, lenient when reading.** A skill's front matter is
  the Agent Skills format. `mind add`, `rename`, `install`, `mindweaver adopt`
  and `edit --contents` (`lifecycle::replace_contents`, which also refuses
  a name other than the skill's folder, pointing at `rename`) refuse a name
  or description that breaks its rules, all through
  `artifact::strict_issues`
  (what `ValidationIssue::is_strict` picks out), so change the rules there and
  nowhere else; gathering
  takes such a skill with a `warning:` per problem, and `validate` reports
  everything (`mindweaver validate` without `-p` reads the shelf too) but calls
  an artifact invalid only for what is not `ValidationIssue::is_warning`. Keys the format does not define pass through
  untouched. A rule's front matter is optional (`rule::RuleManifest`): read
  only when its fences hold a YAML mapping, and whatever it leaves unread is a
  warning, never a reason to hide the rule. `mind add --template` refuses that
  same list (`artifact::rule_issues`) for the rule it is about to write, and
  `edit --contents` refuses it, and a rule with nothing to say, for the one
  it is given (`artifact::rule_text_issues`), before a byte is written. An
  MCP server's definition (`mcp::McpDefinition`) is read from its `format`
  first, one from a newer format refused as that (`mcp_format_too_new`), and
  a file that does not read is a failure beside the rest; one that reads is
  listed whatever is wrong with it, and `validate` says what (a transport
  naming nothing to start or reach, an input no value uses, a name that is
  not its file's, one no agent keys a server by, `^[A-Za-z0-9_-]+$`, one an
  agent keeps for itself, in any case, and two beside each other that
  differ only by case, an input no placeholder can be written of or read
  from a variable no agent can refer to, a secret read from no variable,
  and a token written as it is in a transport, an input's name or its
  `env_var` or the extras, said by where it is and what it starts with and
  never whole, an input named so said of nothing else),
  which `add`, `edit --contents` and `rename` refuse to leave behind
  (`artifact::mcp_strict_issues`, and
  `mcp_name_strict_issues` for a name alone, both reading what sits beside
  it off the folder written in, `lifecycle::twins_beside`). `anthropic-skills`
  is only warned of. What an agent cannot be given of one is never a reason
  to call it invalid: core says it (`McpDefinition::inexpressible`), `show`
  says it, and a renderer asks it before it writes. `add` writes one from
  a command line's options (`ServerOptions`, a `mcp::Given`) or from a
  template, held to the same strict function, every secret it declares
  only the name of the variable it is read from; what no definition can
  hold (both transports, an option of the other's, a transport or any
  option of one with `--template`) is the grammar's to refuse, and what no
  definition's rule can judge, a key given twice and an address typed with
  a credential, core refuses with codes of its own (`add_mcp_key_repeated`,
  `add_mcp_url_userinfo`).
- **Loaded and gathered content is untrusted.**
  - So is a task's every value, which a browser will fill in: it reaches
    its command as that value, never as an option or a command. The task
    module spells each line's own words, and puts a value after the option
    that takes it (`=` when it starts with a dash), behind `./` for a `-C`
    or `-p`, or after a `--` among positionals; a test feeds every variant
    values that start with a dash. A save's text reaches no line at all:
    the line names the workspace's scratch file for it, in `.mindweaver`
    (`WeaverWorkspace::scratch_dir`, and `scratch_route` for the line, both
    core's), which the backend writes with
    `create_new`, in a folder made with `create_dir` and refused when it is
    a link, taking a file there under the name only when it holds the text
    exactly.
  - A task or a query the panel sends names only what core's path policy
    allows (`policy`), which `admit::Admission` asks of every value that
    names something on the disk, in a match over every variant with no
    wildcard: a project or a loaded workspace by its entry as the marker
    spells it, which the backend turns into the absolute folder it resolves
    to for the line's one `-C` or `-p` (and each project a plan's boxes
    name, held to it by `Admission::selection` before core plans), and a
    folder not linked yet, or one
    a scan is to look under or the explorer is to list, by an absolute,
    normalized path outside every `.mindweaver`, verbatim, each folder a
    listing holds held to the policy again and left out when refused
    (`browse`). The
    browser supplies no `-C`, `--home`, `-W` or agent home: the backend runs
    every line with the launch's, against the workspace it acts on, the one
    it was launched on or one switched to since, named with `-W` in the
    line. A switch names a workspace only by its path as core's list of
    what may be switched to spells it (`known::switchable`), matched
    character for character against that list read again as it arrives
    (`policy::switched`): another spelling, a path never listed or one
    starting with a dash is refused (`switch_unlisted`), a listed one that
    does not open is refused with why, never made, and a refusal switches
    nothing. A yes an install carries is one to an MCP server that asks
    first (`policy::confirmed`), held by `Admission::task` as every value
    is, and refused otherwise. A refused task runs nothing, and every
    refusal has a code.
  - A file reaches the browser only as text inside JSON, no larger than
    `contents::CAP`, binary or larger reported by its size alone, and never
    served as itself (`contents::Contents`); its links are judged by
    `copy::followed`, the check `copy::tree` makes, so one leading out of its
    holder is refused: a project's root, or the folder the workspace keeps
    its own of that kind in, whose root may be a home. It is the only way: `show`, which prints an artifact
    whole through any link, is on the `Refusal` list (`show_unheld`). What
    an artifact holds is listed in the same module, to the same bound
    (`contents::Files`): a link further down that leads out, back into a
    folder it is in, or to nothing is listed with why and never followed,
    and a listing stops at `contents::FILES_CAP`.
  - An artifact's file given its contents whole (`edit --contents`,
    `lifecycle::replace_contents`) is followed through its links once and
    written at the path it resolves to, only when that lies inside the
    folder its holder keeps the kind in, a skill's own folder and a
    project's root (`lifecycle::written_at`, `edit_leads_outside`
    otherwise), so never anywhere a read of it for the panel would not
    reach. The library is held to its kind's folder alone, its root being
    a home perhaps, told by that folder rather than by the holder, a
    `MindProject` either way. It goes through a temporary renamed onto it,
    and with `--if-unchanged` only over what the hash given was taken of
    (`edit_stale`), a value that is no SHA-256 refused as one
    (`hash::is_digest`, `edit_hash_malformed`) rather than called stale. The front end reads what it
    was given, a file or standard input, and hands core the bytes, which
    core alone judges; core never reads standard input.
  - An installed name must be one folder name for a skill
    (`install::is_folder_name`), and for a rule a route held to the name
    rules, which never climbs out (`install::is_installable_name`).
  - Symlinks are followed only inside the source (`copy::tree`'s `within`),
    and a rule offered for install that leads out of the workspace offering
    it is refused (`InstallError::Outside`).
  - A project's artifact directories must stay inside the project, and so
    must the folders its `targets` name, outside `.mind` and any
    `.mindweaver` too (`misplaced_target`): refused when a change names one,
    and passed over and said (`MindProject::misplaced_targets`) when the
    file was written by hand.
  - A config file is named by a route of folder names from the project, and
    a symlink anywhere on it, the file included, is refused, never followed
    (`config_entry::ConfigFile`): the route is looked at when the file is
    read and again just before it is written, and the write is never
    resolved. A folder made a link between that last look and the rename is
    the one window left (architecture.md, open questions).
  - An `AGENTS.md` that is a link leading out of its project is refused
    (`agents_md`), since a repository can link it to the user's own
    `~/.codex/AGENTS.md`; one to a file inside the project is written
    through. So is a rule file whose folders lead out of the project
    through a link (`claude_rules`), written or taken away, since `.claude`
    can be a link to the user's own `~/.claude`; the file itself is never
    followed. A `CLAUDE.md` reached through any link is never given the
    import (`claude_md`), whose path would then mean wherever the link
    leads, unless it is the project's `AGENTS.md` itself, which Claude then
    reads the block as (`Reach::Same`).
  - URLs are stored, logged and shown without credentials. `url::redacted`
    is the one function that takes them out: the ledger applies it to every
    URL it writes and reads, `GitError` to what it says, the gix adapter's
    scrub (`Remote::scrub`) to a clone's config and reflogs. A gather refuses a URL that carries one, and one typed as its branch or
    folder, which the ledger would keep word for word. A
    command that writes to the ledger opens it with
    `WeaverWorkspace::ledger_to_write`, which scrubs what an older build
    stored first and hands back what it took out to be said; one that only
    reads opens it with `ledger_for_reading`, which scrubs nothing. The
    install screen is a read until it applies, and the lines it then runs
    scrub as any `install` does. Anything new that keeps or prints a URL
    holds it as a value made without its credentials: a `value::Redacted`
    (made only through `Redacted::new`, which redacts) for a URL or words
    that may quote one, a `value::Origin` for where an install came from,
    which writes itself out redacted whatever built it. Never a `String`
    redacted where it is written.
- **Writers take turns; readers never wait.** Core's writers hold the
  workspace's `.mindweaver/lock` (`WeaverWorkspace::lock`) while they change
  it: `edit_array`, `install::install` and `uninstall`, an MCP server's
  among them (`install::mcp`), `install::rules::render`, `gather::gather`,
  `gather::scrub`, which `WeaverWorkspace::ledger_to_write` runs, when it has
  clones to rewrite, `migrate::library`, from its plan to its rewrite of
  the marker, `migrate::names`, from its plan to the rename of the folder
  the lock is in, whose new place it then follows (`WorkspaceLock::moved`),
  `migrate::project`, for a move from a `mind.toml`, from before
  it reads the project again until its ledger is written (a move from a
  `settings.json` moves no copy, and takes none), `reapply::apply`,
  from before it reads a
  project's settings until their change and every copy it carries along are
  written, `relink::relink`, from its ledger checks until the entry and the
  records it moves are written, `relink::new_id`, `adopt::apply`, from the
  plan it makes again until its record is written, and `update::apply`, from the plan it makes again until its last
  installation is written, through `install::install_held` (the body
  `install_with` hands to once it holds the lock) and
  `install::rules::update_held`, which take none of their own. A new
  writer takes it the same way, inside core; `config_entry::ConfigFile`,
  `claude_mcp::render` and `remove`, `codex_mcp::render` and `remove`,
  `opencode_mcp::render`, `remove` and `remove_from`, `agents_md::render`, `claude_rules::render`, `claude_md::render` and
  `MindProject::change_settings` take none, since
  the writers that call them hold it, and `assign::install` and
  `uninstall` none across their batch, leaving it to each project's
  install, in the install screen's lines too. A second workspace linking
  the same project is kept out by neither lock, an open question in
  `docs/architecture.md`.
  Opening the ledger takes no lock: a schema migration keeps itself apart
  in its own transaction. The lock is not re-entrant: a writer never calls
  another while it holds it, and a front end never takes it. Reads
  (`WeaverWorkspace::open`, `ledger_for_reading`) never take it. A wait is
  bounded (`lock::WAIT`), ending in a `LockError::Busy` that names the
  holder's pid, except a record drop's or carry's (`install::disown`,
  `install::carry`), which follows a change already made.
- **A gather never prompts for a credential itself.** Every connection `gix`
  lets a clone configure gets the callback in `adapter/gix/credentials.rs`, which
  runs the git-config credential helpers with `gix`'s prompt and askpass
  turned off. A credential none of them has is `GitError::Credentials`, never
  a question on a terminal the TUI owns or a server lacks. Anything new that
  connects to a remote takes the same callback. Two gaps are open questions in
  the architecture doc: what the system ssh asks on the terminal, and `gix`'s
  retry after a remote branch named `HEAD`, a connection the callback cannot
  reach.
- **Formats are versioned.** Each marker carries its own `version`
  (`MIND_FORMAT_VERSION` for a project's, `WEAVER_FORMAT_VERSION` for
  `weaver.toml`, so bumping one never makes older builds refuse the other)
  and the ledger uses SQLite's `user_version`. A project's marker is
  `mind.toml` up to `MIND_TOML_VERSION`, which only `MindProject::init` still
  writes, for tests of such a project; `settings.json` at `SETTINGS_VERSION`,
  or `TARGETS_VERSION` when it names `targets`, which an older build would
  read past, read as written and changed by nothing but `mind migrate`; and
  `settings.toml` at `SETTINGS_TOML_VERSION` whatever it sets, which `mind
  init` writes (`MindProject::init_asked`, which fills in each folder it was
  not told about that has a default). A kind added since markers said where
  each kind lives has no default (`Directories::default_for`, `None` for MCP
  servers): no `mind.toml` reads a folder for it, `mind init` and `mind
  migrate` name none, and a workspace keeps it in its library unless its
  marker names a folder for it, which none `mindweaver init` writes does, so
  no marker's version moved. An
  MCP server's definition carries a format of its own (`MCP_FORMAT`), refused
  from a newer one as a marker is. Each version is written in one file
  (`MarkerFile::of_version`), and one found in another is refused. Any of the
  three makes a directory a project (`MindProject::is_marked`), and the
  newest wins when more than one is there. See `DIRECTORIES_VERSION` for how
  a v1 `mind.toml` is still read, and `SETTINGS_VERSION` for why a kind a
  settings file does not name has no folder (`directory_for` is `None`).
  Anything from a newer version is refused. A format change must keep
  reading old files the way they were written, and move one forward only
  when asked, never on a read (`mindweaver migrate --library`, `mind migrate`).
  A key an older build reads past without misreading anything moves no
  version, as the default workspace's `known` did not.
  - A ledger schema step is one more block in the SQLite store's migration
    (`adapter::sqlite::Sqlite::migrate`), which takes
    the write lock, copies the file to `mindweaver.db.v<old>.bak`
    (`Sqlite::back_up`) and runs every step in one transaction. A step fills
    in old rows in its own SQL, not through today's code, and adds a fixture
    of what the version before it wrote
    (`apps/core/tests/fixtures/ledger-v<N>.sql`) for a test to migrate.
    What no SQL can know is handed to the step rather than read by it:
    schema 5 is told each linked project's id (`Ledger::open_knowing`, from
    `WeaverWorkspace::known`).
- **What is written outside the code is Mindweaver's, and a Mindflayer
  build's is read beside it, in one constant each.** Mindweaver was
  Mindflayer (D-26, in `docs/architecture.md`): what a build writes on the
  disk or sends on the wire is spelled the new way, each name in the
  constant of the module that writes it, its old spelling beside it, and
  the pair chosen by one word, `names::Names`: `WEAVER_DIR` and
  `FLAYER_DIR` (`.mindweaver`, `.mindflayer`), `WEAVER_CONFIG` and
  `FLAYER_CONFIG`, `LEDGER_FILE` and `FLAYER_LEDGER_FILE` (each copy a
  schema step keeps named after it), the blocks' markers (`agents_md::BEGIN`,
  `END`, `RULE` and their `FLAYER_` twins, `exclude::BEGIN`, `END` and
  theirs), Claude's rules folder (`claude_rules::folder_under`), the
  staging suffix (`copy`'s `BESIDE`, `FLAYER_BESIDE`), `MIND_IGNORE` and
  `FLAYER_MIND_IGNORE`, `activate::WORKSPACE_VARIABLE`, `DEACTIVATE` and
  their `FLAYER_` twins, the CLI's `homes::HOME_VARIABLE` and
  `FLAYER_HOME_VARIABLE`; the server's `guard::PANEL_HEADER` and
  `auth::COOKIE_PREFIX`, and the panel's preference cookies
  (`src/preferences.ts`'s `PREFIX`, `FLAYER_PREFIX`), its header
  (`src/api/http.ts`'s `HEADERS`) and the folder its load form names
  (`LoadForm.vue`'s `WEAVER_DIR`), which changed outright. What a build
  makes is spelled the new way; what it finds keeps the names it was found
  under: a workspace a Mindflayer build made is read and written in its
  `.mindflayer` (`WeaverWorkspace::names`), never given a `.mindweaver`
  beside it, a block is rendered again under the markers it was found
  with, and a rule file for Claude stays where its record says. Nothing
  moves one but `migrate --names` (`migrate::names`). A guard that looks
  for a workspace above a folder, or reserves its folder, knows both
  names. Code that uses one, a message included, names it through its
  constant, never by its spelling; a doc comment, a doc and a test may
  spell it, and what a Mindflayer build wrote is built in a test by core's
  `MindflayerMother`, never by hand. The codes `path_in_mindflayer` and
  `x_mindflayer_required` stay as they are, as every code does.
- **A project is found again by its id, never by a guess.** The ledger
  keeps the id it last saw at each route (`projects`), learned from the
  projects themselves every time it is opened to write (`Ledger::knew`),
  and a stale entry keeps its id until `relink::relink` moves it. A relink
  hands records only to a directory holding that id, from an entry nothing
  marks any more (`MindProject::is_vacant`: one whose marker would not open
  still holds its project), and two directories holding one id are a copy
  (`relink::Duplicate`), a collected failure and never a move.
- **Nothing is staged where agents look.** An install builds its copies in a
  `copy::Stage` under the project's `.mind/tmp` and renames them into place.
  A temporary beside an installed copy is a skill an agent may load, so never
  write one there. The next install sweeps what an interrupted one left in
  `.mind/tmp`, only when the stage lock says no other run is going, clears the
  `.<name>.mindflayer-new` and `-old` older versions left beside the copy
  (and any under the new suffix, `.mindweaver-`), and
  reports both, even when its own copy then fails.
- **Paths are lexical and portable.** Route arithmetic lives in
  `apps/core/src/paths.rs`. Registry entries are stored relative to the
  workspace with forward slashes, and `link` verifies a route before storing
  it. Directory walking is where Windows differs, which is why CI has a
  macOS/Windows matrix.
- **One function decides the workspace.** Core's `resolve` takes the
  workspace `-W` names, then the one `$MINDWEAVER_WORKSPACE` activates, then
  the first one up the tree, then the default in home, and says which rule
  chose it (`Reason`). A named or activated path that is not
  a workspace is an error, never a fallback, and is never made. A project's
  workspace (`resolve::managing`, `resolve::consulted`) and which project
  `-p` names (`WeaverWorkspace::pick`, `holder`) are core's too: no front end
  walks up, or picks a holder, itself.
- **A change is handed its workspace.** The lifecycle changes
  (`lifecycle::add`, `finish_edit`, `rename`, `remove`) and template lookup
  (`template::templates`, `find`) take it as a parameter and never resolve
  one. The command passes what
  `resolve::consulted` resolves from the project, which walks up only when
  nothing is named or activated; a walk misses a workspace that links the
  project from beside it, and `-W` is how that one is reached.
- **Home and the chosen workspace come from the front end.** `Lookup::given`
  takes `--home`, then `$MINDWEAVER_HOME`, then the user's home, and `-W`,
  then `$MINDWEAVER_WORKSPACE`, which is not read once `-W` is given; empty
  means none, and an empty `-W` keeps the activated one out too. Each
  variable not set at all is read under its Mindflayer name in its place,
  `$MINDFLAYER_HOME` or `$MINDFLAYER_WORKSPACE` (`homes::Older`), which the
  line typed says in a note, never a warning. Core gets
  them as `Inputs` and never reads the environment. Only commands that write
  may create `~/.mindweaver` (`Intent::Write`), and none where a
  `~/.mindflayer` is, which is the default until it is moved. Reads resolve with
  `Intent::Read`, which falls back to the workspace `init` would make without
  making it; a read opens `mindweaver.db` only through `ledger_for_reading`,
  which never creates it either. The TUI hands its own `Lookup` to every line
  it runs (`tui::execute`), so none of them reads the environment again.
- **So do the agent homes.** `homes::agent_homes` takes each from its option
  (`--claude-config-dir`, `--codex-home`, `--opencode-config-dir`,
  `--agents-dir`), then `$CLAUDE_CONFIG_DIR`, `$CODEX_HOME` or
  `$OPENCODE_CONFIG_DIR`, then `~/.claude`, `~/.codex`, `~/.config/opencode`
  or `~/.agents`; empty means none. While `$CLAUDE_CONFIG_DIR` or
  `$OPENCODE_CONFIG_DIR` names a folder of its own, the one in the user's
  home is kept beside it (`claude_dir_in_home`, `opencode_dir_in_home`),
  which OpenCode reads all the same (docs/agents.md); an option names the
  one folder outright, so a pinned test reaches neither. Those folder
  names are core's (`AgentHomes::defaults_under`), and only the user's
  home is the CLI's to find. Core takes the answer as `AgentHomes` and never resolves one.
  `Lookup::given` decides them once per command line, into the `Lookup`
  (`Lookup::agents`), and install is the first to take them, to refuse a
  copy inside one, as `agents_md::render` and `claude_rules::render` take
  them to refuse rules there, `claude_md` to leave the user memory file
  out of what keeps Claude from reading `AGENTS.md`,
  `codex_mcp::render` to refuse a `.codex/config.toml` inside one and to
  read, never write, the Codex home's `config.toml`,
  `opencode_mcp::render` to refuse OpenCode's config inside one, and
  doctor to read, never write, what each holds that a project's agents
  read too (`AgentHomes::skills_read_by`), told beside them the names of
  the environment variables set, names alone (`Lookup::variables`,
  `Lookup::machine`), decided once per command line the same way.
  `tui::execute` hands each line the TUI's own
  with the rest of its `Lookup`, and so does its install screen: a command
  the TUI runs carries none of the four options, so it would decide them
  again from the environment.
- **The server holds no rules.** `apps/server` carries JSON between the
  browser and a `Backend`, whose `Query`, `Task`, `Outcome` and `Switch`
  are associated types, and depends on no mindweaver crate (`tests/crate_test.rs` checks it
  with `cargo metadata`); a front end depends on it, as the CLI does
  (`backend.rs`), never the other way. A check, path rule or naming rule
  written there is one the panel could disagree with the CLI about: it
  belongs in core, reached through the backend, which says it as a
  `Refusal`, a code, a message and its grounds, sent as a 404 when it is
  `Grounds::Absent` (in the CLI's backend, a read whose parameters name
  what is not there, decided where the refusal is built, `Absence`) and a
  422 when it is `Grounds::Unfit` (everything else, whatever a body names
  among it): the server never reads a code to pick a status. A task
  refused when its lines are asked for is never run, nor kept in the
  transcript. Everything
  it serves is under `/api/v1` (D-24), and a route, a parameter or a field
  once served is a contract, as the envelope's names are: dropping or
  renaming one, or changing what it holds, moves the API to `/api/v2`,
  adding one does not, and no unversioned route answers. Its reads are the
  backend's, handed over as a table (`Backend::routes`, `route::Route`):
  each route's method, path, parameters by name and shape, body and answer
  are data, which the server matches, reads a query string by and refuses
  by (a 400 for a parameter no route takes, one given twice, one it cannot
  read or one it needs, as `deny_unknown_fields` refuses a field), and the
  route makes the backend's `Query` of what was sent; the server never
  writes a handler of a read of its own, and an identifier goes in a
  parameter, never the path, since an entry or a reference can hold a `/`.
  A read with a structure is a POST that changes nothing; a write is a
  task, previewed and run at `/api/v1/tasks/preview` and `/api/v1/tasks`.
  A preview
  is the server's `Preview`, the lines and what the backend says to
  confirm, carried unread as a watched path's scope is; the server
  enforces none of it, so a run sent without the question is run, as a
  shell runs what is typed into it. Queries and
  previews run on tokio's blocking pool, and every run on the one writer
  thread, in the order it arrived, and so does a switch of what the backend
  acts on (`POST /api/v1/switch`, `Backend::switch`), which only the backend
  can say it may make: once it is taken, the watcher asks for the paths
  again and drops those no longer named (`Watcher::again`) before every
  subscriber is told `switched`, and nothing of it is kept in the
  transcript. Which paths it watches is core's
  (`watch::watched`), handed over by the backend (`watched_paths`), each
  with a scope it carries to the panel unread: the watcher knows files and
  folders, each watched on its own and never a tree, and nothing of which
  ones a project has. The bus and the watcher stay free of axum, so a
  desktop shell reuses them. It binds
  127.0.0.1 only, answers only a Host naming it on its port as `127.0.0.1`,
  `localhost` or `[::1]`, refuses an Origin other than its own, answers under
  `/api` only a request that carries the launch token (in the cookie
  `mw_<port>`, or as Bearer), and takes a change by cookie only with
  `Sec-Fetch-Site: same-origin`, `X-Mindweaver: 1` and `application/json`.
  The one widening is the panel's dev server's origin, exactly, when a front
  end names it (`Server::dev_server`, `mindweaver ui --dev`). The API's
  description is the front end's, handed over as text
  (`Backend::document`) and served at `GET /api/v1/openapi.json` as it is,
  behind the token like every route.
  Every answer carries the CSP and `nosniff`, and every one under `/api`
  `Cache-Control: no-store`, since a read is a GET a browser would keep;
  there is no CORS layer, and the token is never logged.
- **The panel holds no rules either, and talks only through its
  transport.** It asks and draws: nothing a query answers is worked out in
  TypeScript. Every request goes through the `Transport` from
  `useTransport()`, and only `src/api/http.ts` may use `fetch`,
  `EventSource`, `WebSocket` or `XMLHttpRequest` (an ESLint rule, tested in
  `tests/lint.test.ts`). It asks each query at the route the backend's
  table serves it at under `/api/v1`, one case for every query
  (`route`), and spells no route the table does not hold. Authentication
  is the transport's: relative URLs,
  the `HttpOnly` cookie, the `X-Mindweaver: 1` header. What the server
  answers belongs in Vue Query; Pinia holds UI state only. The server's
  events reach the views only as Vue Query marking what they reached stale
  (`src/api/live.ts`), or, on `switched`, resetting every answer, never as
  a read of their own and never a scan, over
  one subscription a tab, let go while it is hidden: a scan's query is
  keyed outside what they mark (`scan`), never stale on its own, and asked
  for again only by its page, as after a run of its own. Every change goes
  through the runner (`useRunner().ask`): the lines it shows are the ones
  the server previewed, never one the panel builds; a preview's `confirm`,
  the CLI's, makes it a confirmation, never a list of the panel's; and a run
  is over once the stream told it finished and the views read again, or
  they read again after `PATIENCE` without word (`settle`), never an
  answer written into a view. A view lays
  out, groups and filters what the snapshot lists: every count it shows is
  one the snapshot carries (`summary`, a project's `tally`), every verdict
  one core gave (a project's `health`, whether it `migrates`), and every
  word a scan's answer gives (what each folder `offers`, the entry a moved
  one left), every batch of boxes a plan core made (`Query::Plan`: its
  tasks, what clashes and what is locked), every box of the matrix as the
  snapshot says it stands (`offers`, and each project's `boxes`: which
  offer of a name it holds, by `install::holding`, how the name stands,
  and each kind it can take none of), never told from a copy's origin,
  every installation an update says something of as core surveyed it
  (`Query::Updates`: its state, what an update does with it, the copies
  that names, why it is left, by a code, and how many stand in each
  state), never told from the snapshot's states, every judgement of a
  draft as core gave it (`Query::Validate`: why the command would write
  nothing, by its code and in its words, and what `validate` would say of
  it, word for word), never a rule of a name or of front matter spelled in
  TypeScript, and the hash a save is held to as core gave it with what it
  read (`contents::Contents::hash`), never one hashed in the browser,
  and every order the snapshot's
  (`kinds`, `states`, `healths`, the registry's). A count or a verdict a
  view needs is added to `view::Snapshot` in core, never worked out from
  the lists, and a kind core adds reaches every view with no change
  (`tests/kinds.test.ts`). So is a word it compares or shows: an origin as
  `--from` takes it (each shelf row's, source's and load's `origin`, and
  `library.origin`), never put together from an address and a branch, and
  an artifact's front matter, which core cuts from the file
  (`contents::Manifest`). So is which workspace this machine knows the one
  on screen could load (`known`'s `loadable`, `view::KnownHere`), never
  told from `in_use`, `stale` and the loads' paths, and which one the
  header may switch to and by which rule it is then chosen
  (`Query::Workspaces`: `switchable` and `reason`, `known::switchable`),
  the path a switch names being the one that list spells. So is which projects
  hold a copy of one of the library's own (`library.holders`, by `install::from_the_library`, the
  rule `mindweaver remove --everywhere` follows), never told by comparing a
  copy's origin, which an install from before origins were kept lacks. Every
  page passes axe (`tests/a11y.test.ts`), works from the keyboard, and never
  says a state by colour alone.
- **The panel's types are written from Rust, and are its contract.** Every
  type core and the CLI hand the panel, or take from it, derives
  `ts_rs::TS` beside serde's derive under the `ts` feature
  (`#[cfg_attr(feature = "ts", derive(ts_rs::TS))]`), so a plain build
  compiles none of it and a plain `cargo test` writes nothing. A type serde
  writes by hand says its TypeScript from what its serializer writes,
  beside it, never from the variants: a word through `described::words!`,
  from the function the serializer calls, anything else by hand or with
  `ts(as)`, each held to serde by a test in `typescript_test.rs`. What is
  exported is each crate's `typescript::roots`, into
  `apps/web/src/api/generated` (`TS_RS_EXPORT_DIR`, `.cargo/config.toml`),
  committed and never edited by hand: `make web/types` writes it again,
  and `make web/types/check`, CI's `types drift (web)`, fails when it is
  not what Rust writes.
  A type a view starts to read joins `roots`; the panel spells no shape of
  core's or the CLI's in TypeScript of its own.
- **The API's schemas are written from Rust too, and are its contract.**
  Every type the TypeScript describes, and whatever else the API takes or
  gives whole (the bodies of the reads that take one, the server's own
  answers, the token file), derives `schemars::JsonSchema` beside serde's
  derive under a `schema` feature of core's, the CLI's and the server's
  (`#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]`), so a
  plain build compiles none of it and a plain `cargo test` writes nothing.
  A type serde writes by hand says its schema from what its serializer
  writes, beside it, as its TypeScript: a word through the same
  `described::words!`, anything else by hand or with `schemars(with)`, each
  held to serde by a test in `schema_test.rs`. Each crate's
  `schema::roots` names every root and whether it is given, described as
  serde writes it, or taken, as serde reads it; the CLI's lists the
  server's beside its own. A root is written as `docs/api/schemas/<its
  type's name>.json`, JSON Schema 2020-12 with every type it names in
  `$defs`, committed and never edited by hand: `make api/schemas` writes
  them again, and `make api/schemas/check`, run in CI's `types drift
  (web)`, fails when they are not what Rust writes. A type a route names
  is a root, every root of the panel's types has a schema of its name, no
  two types of any crate go by one name (`Together`), and every fixture
  the panel's views render holds to the schema of what it answers, closed,
  so a field serde writes that a schema does not name fails. An answer's
  schema stays open to a field it does not name, since adding one moves no
  version, a type sent closed only where it is sent
  (`schema::given_settings`); a diff of `docs/api/schemas` that drops or renames a property,
  or changes what one holds, moves the API to `/api/v2`.
- **The API's description is written from both tables, and every answer
  holds to it.** `docs/api/openapi.json`, an OpenAPI 3.1 document, is
  written by the CLI under `schema` (`openapi::document`), never by hand:
  the server writes what is HTTP of each route of `Backend::routes` and
  `Own::ALL` as it serves them (`schema::paths`), each status a route may
  be refused with from what its table says (`Route::conditions`,
  `Own::conditions`), and asks the CLI for every type a route names
  (`schema::Describe`), by the name its table gives it among the roots, by
  `Sent`, or by a word for what the server carries of the backend's
  (`Carried`), so the server still names no type of the front end's. A
  type sent and answered that is described otherwise each way is two
  definitions, the one sent named with `Sent` after it (`TaskSent`). The
  CLI's backend embeds the file as it is committed (`include_str!`), so a
  plain build serves it without schemars; `make api/openapi` writes it
  again and `make api/openapi/check`, run in `types drift (web)`, fails
  when it is not what Rust writes. A route, or a type it names, changed is
  written again into it.
- **The panel needs nothing the CSP refuses.** No URL naming another host,
  no inline `<script>` or `<style>`, no `data:` URL: fonts are Fontsource
  files, icons are bundled Lucide (`i-lucide-<name>` in a `.vue` file) drawn
  by Iconify's offline build (`src/overrides/iconify.ts`), the palette is
  declared in `main.css` while Nuxt UI's colours plugin is a stub
  (`src/overrides/colors.ts`), and the colour mode switches by class
  (`src/color-mode.ts`). The editor is CodeMirror in a shadow root of its
  own (`MarkdownEditor.vue`), where style-mod adopts a constructed sheet
  (`adoptedStyleSheets`) rather than writing the `<style>` element it
  writes into a document, and every style CodeMirror sets goes through the
  CSSOM, never a `style` attribute; its test holds the sheet to being
  adopted and to no `<style>` in the shadow root, which a document's
  observer does not see. What somebody else wrote, a skill's markdown, is
  drawn element by element from marked's tokens (`src/markdown.ts`), never
  put on the page as HTML: raw HTML shows as text, no element gets a style
  or an event, no image loads, and a link keeps its address only to another
  site or a mail address, never to an artifact's file
  (`tests/markdown.test.ts`, `tests/artifact.test.ts`).
  `tests/dist.test.ts` builds the panel and checks it; `tests/shell.test.ts`
  and `tests/views.test.ts` fail on any `<style>` element added at run
  time, and on an icon the bundle lacks; and `e2e/csp.spec.ts` loads every
  page in Chromium and WebKit under the server's real header, where any
  violation fails the test it happens in.
- **Activating prints, and writes nothing.** `mindweaver activate` prints core's
  code (`activate::script`), and nothing on stdout when it fails, so `eval`
  runs nothing. Nor does it remember the workspace in the default one's
  `known`, though #56 asked it to: `mindweaver init` does (`known::remember`),
  only once the default is made, and never makes it (architecture.md, "The
  workspaces this machine knows", and its open questions). Its shell comes
  from `--shell`, then `$SHELL`, read by the front end. The code undoes only
  what it did, through `mindweaver-deactivate` and never `deactivate`,
  running a Mindflayer build's `flayer-deactivate` first where a shell has
  it, and setting nothing of the old activation's; a workspace's name
  reaches a prompt only through `activate::prompt`.
- **`Kind` and `Agent` are closed enums.** `Kind` is `Skill`, `Rule` and
  `Mcp`, each kept in a `Layout` (`Directory`, `Files`, or `Flat` for a
  file directly in its folder, whose name becomes a config key and so is
  never a route). To add a kind, follow "Adding a
  third kind" in the architecture doc, and to add an agent, "Adding an agent",
  which starts in `docs/agents.md`. Either way the compiler's exhaustive
  matches are the checklist: the target table's `standard`, and `read` behind
  `Agent::reads`, match every kind and agent with no wildcard, so neither
  compiles until it says where it goes and where it reads; `at_root`, behind
  `Agent::reads_at_root`, matches every agent the same way. So do the
  matches that say what a kind is not yet given: a default folder beside
  the code (`Directories::default_for`), an install (`install::installs`,
  behind the refusal in `install::targets`, and `install::asks_first`,
  which says whether one waits for a yes), a scaffold
  (`create::scaffolds`), and the words a sentence names it by
  (`Kind::noun`, `Kind::plural`), which every message and count asks rather
  than spelling `{kind}s`. And the matches on the agent that say which MCP
  servers' names it keeps for itself (`mcp::name`'s `reserved` and
  `claimed`), what it cannot be given of a definition
  (`McpDefinition::inexpressible`) and what it asks before it starts one
  (`Agent::starts_a_server`), each from docs/agents.md's "MCP servers".
  "Adding an agent" also lists what no match covers: `Agent::ALL`, and an
  agent home's field, option and place in `common::PINNED` and `CLEARED`.

## Tests

- Integration suites are `apps/<app>/tests/<area>_test.rs`, with shared
  helpers in `tests/common/mod.rs` (`use common::*;`). Unit tests sit in
  `src/<module>_test.rs`, wired in with
  `#[cfg(test)] #[path = "<module>_test.rs"] mod tests;`.
- What a test needs is built by an object mother (`mindweaver_core::mother`,
  `apps/core/src/mother/`), never by hand where a mother makes it: one per
  value object (`ContentHashMother`, `RedactedMother`, `OriginMother`,
  `ArtifactNameMother`, `RevisionMother`, `ProjectIdMother`), per record the
  ledger keeps (`InstallRecordMother`, `TargetMother`, `EntryTargetMother`,
  `SourceMother`, `GatheredMother`, `MovedMother`, `EntryMother`), for a
  skill's and a
  rule's text (`SkillMother`, `RuleMother`), an MCP server's definition,
  one for each transport and one for each shape a rule or an agent is
  about (`McpMother`: `misnamed`, `secret_without_variable`, a token of each
  shape drawn from parts at run time, `tokens` and `with_literal`, never a
  token in a file, the names a server may and may not be given, `name`,
  `unwritable`, `reserved`, `claimed`, `twin`, and each shape an agent
  cannot be given, `secret_argument`, `secret_in_url`,
  `composite_secret_header`, `composite_secret_env` (and `stdio` itself,
  its secret under a name of its own), beside `whole_secret_env`, which
  Codex is given, and the variables Claude empties toward a
  remote server and passes on, `blanked_variables` and `passed_variables`,
  read where `header_read_from`, `url_read_from` and `env_read_from` say,
  and the text Claude reads a reference in and the text it does not,
  `references` and `not_references`, and the text OpenCode fills in and
  the text it does not, `substitutions` and `not_substitutions`, written
  where `with_text` says;
  what Claude's entry is refused for or given beside it, `unread_input` and
  `with_claude_extras`; what Codex is given whole, `program_for_codex`,
  `address_for_codex`, `whole_header_for_codex`, `plain_program` and
  `plain_address`, and given beside it, `with_codex_extras`; what OpenCode
  is given beside its entry, `with_opencode_extras`; what `add` is
  told of one, `given_program`,
  `given_address`, `given`, and the shapes it refuses, `given_unfit`,
  `given_with_literal`, `given_repeating` and `given_refused_unread`; and
  `InputNameMother` and `VariableNameMother` for its values, with what each
  rule refuses), a project's settings
  (`SettingsMother`), the URLs a credential can be typed into (`UrlMother`,
  each form beside what redacting leaves of it) and what lives on the disk
  (`WorkspaceMother`, `ProjectMother`, among it a project a version 1 or 2
  build made, `version_1` and `older`, one a `settings.json` marks as a
  version 3 build wrote it, `json`, and one that names a folder for MCP
  servers, `with_mcp`, `GitRepositoryMother`, `ShelfMother`),
  and what a Mindflayer build left under its names (`MindflayerMother`:
  a workspace in a `.mindflayer`, and a project's blocks, exclude lines and
  rule files for Claude as that build wrote them, records included).
  Each is a unit struct with `random()`, `create(..)` and a named
  constructor for each shape a test asks for by name (`not_hashes`,
  `unwritable`, `every_credentialed`), made only the way its value's rule
  allows; a mother of a record is made of the mothers of its values, so a
  rule changes in one place. `tests/common`'s builders delegate to them. What
  is random is drawn afresh each run (`mother::random`, from `std`'s hasher
  keys, no dependency), so a rule is tried on cases no test spells: a test
  drawing from a mother names the value in what it asserts, and loops over
  many draws where a rule holds of every value. `src/mother_test.rs` holds
  every mother to the rule it claims, through core's own judgement. Core's
  unit tests reach them as `crate::mother` (`cfg(test)`); its integration
  suites and the CLI's through a dev-dependency on core with its `mother`
  feature, which no build of a binary turns on. The CLI's suites add mothers
  of what only a command takes (`tests/common/mother.rs`: `ValueMother`,
  `OlderBuildMother`, `MindflayerShellMother`, the activation a Mindflayer
  build printed, and `ServerOptionsMother`, the options a command line
  gives what `McpMother` makes by and those the grammar refuses together);
  the server depends on no mindweaver crate, so its
  suites keep all of theirs there (hosts, origins, tokens and headers). A new value object gets a mother and a section of
  `tests/value_test.rs`, which holds every value to one set of laws
  (`reads_as_its_text`) and to its rule both ways, and a new port a
  contract suite every adapter of it runs (`tests/ledger_store_test.rs`,
  `tests/remote_test.rs`).
- No test reads the real home, the real agent homes or a workspace the
  developer's shell activated, which `HOME`, `MINDWEAVER_HOME`,
  `MINDWEAVER_WORKSPACE`, `MINDFLAYER_HOME`, `MINDFLAYER_WORKSPACE`,
  `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `OPENCODE_CONFIG_DIR` lead to. In-process tests pin options: CLI tests call
  `common::mind` / `common::weaver`, which parse through the real parsers and
  append `-C <dir>`, `--home=`, `--workspace=` and the four agent-home
  options empty (`common::PINNED`), so no command they run can reach or
  create the real home workspace, use an activated one or read a real agent
  folder. A command a TUI test runs gets only the TUI's own `Lookup`, the one
  the test chose, agent homes included (none, from `Lookup::default`), and
  none of the agent-home options (see "So do the agent homes"). A test that
  spawns a binary (`common::spawn`) clears those variables
  (`common::CLEARED`), sets only the ones it is about, and sets
  `HOME` (`USERPROFILE` on Windows) to a temp dir. Not to empty: the standard
  library then falls back to the account's real home. Keep it that way. A
  core test about what an agent's own folder holds, the user's Codex
  `config.toml` a server's name collides with or that marks a project
  trusted, makes that folder in a temp dir and hands it over in
  `AgentHomes` (`codex_mcp_test.rs`'s `codex_home`).
- Git sources are built in-process with `gix` (`common::repository`): no
  network and no `git` binary. A CLI fixture's project is a repository given
  `mind init` (`common::new_project`), so a local install keeps its copies out
  of git rather than saying on stderr that nothing does, and where an install
  puts its copies is asked of core (`common::copies`). A test about what a
  project an older Mindweaver made does, a copy installed in the folder it
  authors in, makes one with `common::older_project`, and one about a
  project a `settings.json` marks with `common::json_project`, which has
  `ProjectMother::json` write the file as a version 3 `mind init` did. What
  git would list is
  asked of gix too (`common::untracked`), and a core fixture has git track
  its files by writing the index with gix (`common::indexed`). A core test
  about which `CLAUDE.md` Claude finds first holds its temporary folder to
  have none above it (`claude_md_test.rs`'s `clean`).
- Tests that run a shell (`apps/cli/tests/shells_test.rs`, Linux only) start
  it with no startup file and an environment holding only a `PATH`, a
  temporary `HOME` and what the test sets, so no `MINDWEAVER_*` or
  `MINDFLAYER_*` variable or real shell configuration gets in. A shell, or a python3 to make a
  virtualenv with, that is not installed is skipped and said on stderr,
  except under `$CI`, where it fails the test: the `test (cli)` job
  installs zsh so that both shells run there.
- The CLI end to end is `apps/cli/tests/e2e_test.rs`, one target whose
  modules under `tests/e2e/` group the use cases by what a person is
  doing (projects, authoring, installing, gathering, updating, reading,
  workspaces, settings, adopting, removing, loading, upgrading, json,
  mistakes). Each test is one use case: it builds its own
  scenario (`e2e/scenario.rs`, `Scenario`), runs one command line as a
  person types it, and holds its exit status, stdout and stderr word for
  word, and what it left on the disk. A scenario is a temporary folder read
  as `/home`, with `you` as the home, so every path a command prints reads
  as the reference's (`/home/you/Projects/collapse`) and a test spells its
  folders and its lines from `~` or `/home`. Each command is the built
  binary as a process, with an environment holding only `PATH` and that
  home, and no option added to the line: a variable a use case is about
  is given to that command alone (`run_with`). What a person or a git
  server makes is made directly (`repository`, `file`, `source`, `change`,
  with gix, so no `git` binary), and Mindweaver's own state only through
  the commands (`given`), each held to exit 0 with nothing on stderr, but
  for what an older build made, which no command of this build writes
  (`older_project`, a `mind.toml` written by core's `MindProject::init`). A
  source is gathered by its path from the workspace (`../src/acme-skills`):
  a table pads every row to its longest, which a temporary folder's path
  would make depend on the system. No test shares a scenario, so any one
  runs alone. An expected output is the reference's where it shows one;
  a use case added there is a test added here. Unix alone, since a
  Windows path never reads as the reference's.
- The server end to end is `apps/cli/tests/api_e2e_test.rs`, in the CLI's
  crate since `mindweaver ui` is the server's only binary and the server may
  depend on no mindweaver crate. Its modules under `tests/api_e2e/` group
  the use cases by what a script or the panel's page does (access,
  reading, changing, authoring, managing, switching, following, launching,
  mistakes), and it builds the CLI suite's `Scenario`
  (`#[path = "e2e/scenario.rs"]`). Each test serves the scenario with
  `mindweaver ui --no-open` typed in one of its folders (`serve`, or
  `serve_line` for a launch's options), in the same environment as every
  command there, reads the link it prints for the address and the token,
  and sends one request over loopback as curl sends it
  (`api_e2e/served.rs`, `Served`, `Ask`): its status and its JSON,
  whole or the part a script reads (`answers_at`, a JSON pointer, as jq
  picks one), and what it left, on the disk or in what a command reads
  next. Paths in a request are spelled under `/home` and read back so in
  an answer. What only the API leaves, a transcript, is built through it
  (`Served::given`, a task held to run clean), and every answer to a
  route the API describes, and every event, is held to its OpenAPI
  document (`common/contract.rs`). An event a change on disk sets off is
  waited for (`Stream::until`), never counted. An expected answer is the
  guide's (`docs/api.md`) where it shows one. Unix alone, as the server
  is stopped by a SIGINT `kill` sends.
- Core end to end is `apps/core/tests/e2e_test.rs`, its modules under
  `tests/e2e/` grouped by what a front end does (workspaces, projects,
  authoring, installing, gathering, updating, reading, settings, upgrading,
  adopting). Each test builds
  its own world (`e2e/world.rs`, `World`), a temporary folder read as
  `/home` with `you` as the home, as the CLI suite's scenario is: what a
  person or a git server makes is made directly, and Mindweaver's state
  only through core's public calls, each the one a command makes for that
  step (`init_project` is `MindProject::init_asked`, `link` is
  `WeaverWorkspace::link`, `install` is `assign::install`), with the home
  and the agents' own folders under it a front end hands them, and held to
  go cleanly. The act is one public call, and what it answered is held
  as a front end reads it: a read type as its JSON (`World::json`, paths
  read as `/home`), a part of the snapshot every front end reads
  (`World::snapshot`, `at`), a refusal by its `code()` and its words
  (`World::said`), and the disk. A gather is by a source's absolute path,
  since the tests share the process's folder, so the shelf folder it is
  named into is held by its shape alone. Unix alone.
- The TUI end to end is `apps/cli/tests/tui_e2e_test.rs`, its modules under
  `tests/tui_e2e/` grouped by what a person does on the screen (browsing,
  changing, screens, sources, starting), on the CLI suite's `Scenario`. Each test
  starts `mindweaver` or `mind` in a pseudo-terminal of its own
  (`tui_e2e/terminal.rs`, `Terminal`, 120 by 24), its process the leader of
  a session with that terminal as its controlling one (rustix's `pty`,
  `setsid` and `TIOCSCTTY`), since crossterm reads the window's size from
  `/dev/tty`, which would otherwise be the developer's terminal or none
  under CI. The keys are typed as a terminal sends them (`types`,
  `presses`, an escape given a moment alone, or it reads as Alt with the
  next key), what the screen draws is fed to `vt100`, and a test waits,
  bounded, for what it expects (`shows`, `no_longer_shows`, `draws` for
  rows word for word), then quits (`exits`) and holds the exit status, the
  transcript the terminal shows once the window is given back, and the
  disk. A key typed before the screen is drawn is the shell's, echoed where
  the transcript lands, so a test waits for the first draw before typing.
  The terminal answers where its cursor is (`ESC [ 6 n`) as a person's
  does, from the thread that reads it, since ratatui asks when it clears
  the screen, as after an editor; the editor `e` hands the terminal to is
  the use case's (`open_with`, `EDITOR`), and the test waits for the
  screen to give the window up and take it back (`steps_aside`,
  `comes_back`). A gather that waits as long as a test needs is one from a
  server on loopback that holds each request until the test lets it
  answer (`tui_e2e/sources.rs`, `Holding`: `asked`, then `answers`), never
  the network. A row with a path inside a box is held by what it says,
  not word for word: read as `/home`, the path is shorter than the one the
  box was drawn around. Unix alone.
- The server's suites run against a fake backend with types of its own
  (`common::Fake`, its reads `common::fake_routes`), never the CLI's.
  `routes_test.rs` holds the server to the table: a path and its
  parameters reach the backend as the route makes them, and what no route
  takes is refused with its status; `guard_test.rs` holds every route
  under `/api/v1` to every guard. They bind 127.0.0.1 on port 0 and send
  requests through the router in process (`common::send`), or over loopback,
  and serve a fixture panel (`tests/panel`), so they need no Node. Every
  request names the server in Host, and `Auth::Cookie` sends what the panel's
  page would; a test of a guard changes one header of that
  (`common::request`, then `common::deliver`). A test of the event stream
  reads it a frame at a time (`common::subscribe`, `common::opened`), and
  one that watches files watches temporary folders only, through a fake
  that names them (`common::launch_watching`), and waits for what it
  expects at most `common::PATIENCE`. The CLI's backend has a suite
  of its own (`apps/cli/tests/backend_test.rs`), which drives `PanelBackend`
  through the `Backend` trait with the JSON a browser sends, binding no
  port, `apps/cli/tests/routes_test.rs` pins every route of its table by
  name and holds each to the query the panel sends for the same read, and
  `apps/cli/tests/live_test.rs` starts the server's watcher over it on a
  temporary workspace; `apps/cli/tests/ui_test.rs` starts the binaries'
  `ui` on port 0 (`common::start`) and sends the panel's requests over
  loopback (`common::panel`, the client of plain `TcpStream`s every suite
  that calls a served panel uses), every read route among them, and a
  script's with the token read from the file `--token-file` writes, in a
  temporary folder. No test opens a browser: every `ui` one starts passes
  `--no-open`, and the opener is a stand-in in `ui`'s unit tests. A suite
  or test about the panel is `#[cfg(feature = "ui")]`, so a build without
  the feature leaves it out; `make cli/test` runs the unit tests in that
  build too.
- Every answer the CLI's HTTP suites get is held to the API's OpenAPI
  document as it is committed (`common::contract`, with boon): `ui_test`'s
  `Panel::send` holds what each route answered, with its status, to the
  schema the document gives that route and status, and the events the
  stream tells to theirs, and `backend_test` drives the backend through a
  stand-in (`Checked`) that holds each answer as the server would send it,
  a refusal under the status its grounds say. A route answering outside
  its document, or with a status it does not list, fails the test that met
  it. `apps/cli/tests/openapi_test.rs` reads the document with oas3, a
  reader of OpenAPI 3.1 that reaches no network, holds every schema in it
  to JSON Schema 2020-12's meta-schema, which boon carries, and walks both
  tables against its routes; the server's `api_test` holds every refusal
  it brings about to a condition its route lists.
- The panel's suites are `apps/web/tests/<area>.test.ts`, in Vitest. The
  components run in happy-dom, mounted through `createPanel` with a fake
  `Transport` and a memory history, so they never reach a server. The
  views' suites render the snapshots in `tests/fixtures`, never one drawn
  by hand: `apps/cli/tests/fixtures_test.rs`, under the `ts` feature,
  builds the workspaces by running the commands and writes what the
  panel's backend answers (`make web/fixtures`), with the temporary folder
  as `/home`, fixed ids and pinned times, and beside them what opening each
  of the library's own is answered with (`library.json`, every files and
  contents query an artifact's page asks), what scanning the folder of the
  workspace `candidates.json` is a snapshot of is answered with
  (`scan.json`), what the explorer is answered with as it walks that
  workspace's folders, beside a relative path refused with its code
  (`folders.json`), what previewing and running
  a few tasks in turn is (`commands.json`), and, of a workspace built for
  the matrix (`assigning.json`), what planning a few selections of its
  boxes is answered with (`plans.json`) and what applying the first is
  (`applied.json`), and, of a workspace built to update (`updating.json`),
  what asking what an update would do is answered with (`updates.json`)
  and each change its page offers, previewed and run each on the
  workspace as built (`updated.json`), and, of the first workspace, what
  judging drafts of the library's own is answered with (`drafts.json`),
  what `mindweaver validate` says once one is written, which the editor's
  verdict is held to word for word (`validated.json`), and each change
  authoring sends, previewed and run each on the workspace as built
  (`authored.json`), and, of a workspace built to keep MCP servers
  (`serving.json`), what its MCP page and its matrix ask of it, drafts of
  a server judged and plans of one that asks first and one that does not
  (`servers.json`), and each change they send, previewed and run each on
  the workspace as built (`served.json`), none of them holding a token.
  `tests/panel.ts` reads them and
  mounts the panel at an address, answering its queries with them, and a
  task with its preview and its run, told finished on the stream before
  it is answered, as the server tells it (`commanding`); a test about
  something no fixture holds changes its own copy. A test that needs a
  page of its own adds a route to `render`.
  `dist.test.ts` builds the panel in a process of its own, into a temporary
  folder under `node_modules`, and leaves `dist/` alone. `matrix.test.ts`
  holds the matrix of 50 projects by 200 offers to 100 rows in the
  document and times its first render, which it holds to 300 ms: vitest
  runs it alone once every other suite is done (`test.projects` in
  `vite.config.ts`), since a render timed beside them times the runner's
  load too. `licences.test.ts`
  reads every installed package's licence: a new one outside its list of
  GPL-3.0-compatible licences fails it.
- The browser suite is `apps/web/e2e/<area>.spec.ts`, in Playwright, on
  the real `mindweaver ui`: the debug build, or the binaries `E2E_BIN` names,
  as CI names the release. It serves a workspace `e2e/fixture.ts` builds
  with the real commands, each made to exit 0 with no warning, and
  nothing written by hand but the folders `mind init` runs in and, beside
  the workspace, the drafts an `--editor` copies in (`fixture.draft`), as
  an editor saves one, for `mindweaver edit` and `mind edit` to change an
  artifact with, or `--contents` names; a state only a hand edit, a removed folder or a git
  source makes is left to the views' suites and their fixtures. A test
  that changes something builds a workspace of its own (`own`), served
  with the run's home, or for the loads (`loadable`) with one of its own
  beside it, whose default workspace knows what it loads, held to stay as
  it was made as the driver holds the run's. Every process it starts goes through
  `e2e/environment.ts` (`run`, `pinned`, `environment`), and the config
  starts the server the same way, from the fixture's folder. A spec reads
  the server from `served`, what the CLI says from `listing()`, `mindweaver
  list --json` in its envelope, and opens a page with `panel.open`, through
  the launch link, as a person does. After a click that opens another page,
  it acts through what only that page draws (the add page's form, by its
  name, and `exact` labels inside it), never a label or a name the page it
  leaves could match too: the click returns while that page is still
  drawn, and Playwright matches either in any case and anywhere in it.
  Every test fails on a CSP violation or
  an uncaught error on its page (`watch`), and `csp.spec.ts` holds the watch
  to seeing one. No spec changes the shared workspace: a test that changes
  something takes a workspace and a `mindweaver ui` of its own (`own`, built by
  `changeable`, or what the spec's `building` names, `assignable` for the
  matrix's, `updatable` for updating and removing, `authorable` for
  editing, adding and renaming in the library or `serveable` for adding
  an MCP server and assigning it, beside a copy of it
  and served by `serve`, through
  `environment.ts` as every process is, with what the spec's `variables`
  set in its environment alone, an MCP server's secret drawn as the run
  starts among them), and holds what the panel left to
  what the same command leaves in the copy. That `mindweaver ui` runs with
  `--log`, and a test that fails keeps what it said beside its trace
  (`close`, in `panel.ts`), which `log.spec.ts` holds to saying a run and
  never the token; every test that fails keeps how long the machine
  stalled while it ran, from Linux's `/proc/pressure` (`stalls`). Whether
  a test failed is decided in one place (`keep`), and `log.spec.ts` hands
  both teardowns a stand-in for a test that failed, since no test that
  passes is one, and finds each file kept and attached, and nothing kept
  for one that passed.
- The tests of the panel's types are core's and the CLI's
  `src/typescript_test.rs`, compiled only with the `ts` feature, and named
  so `--lib typescript` picks them out: each crate's export
  (`export_bindings`), which writes into the panel, and every type serde
  writes by hand held to what its TypeScript says. `make web/types` and
  `make web/types/check` run them; `make test` does not.
- The tests of the API's schemas are core's, the CLI's and the server's
  `src/schema_test.rs`, compiled only with the `schema` feature, and named
  so `--lib schema` picks them out: each export (`export_schemas`, core's
  and the CLI's, which writes into `docs/api/schemas`), every schema
  written by hand held to what serde writes and to refusing what it never
  writes, and the CLI's holding every fixture in `apps/web/tests/fixtures`
  to its schema, and, with `ts` too, every root of the panel's types to
  one. They validate with boon, a dev-dependency that reaches no network,
  with its loader taken away. `make api/schemas` and `make
  api/schemas/check` run them, with `ts` beside `schema`; `make test` does
  not.
- Screens are tested without a terminal. Drive `state` with key events, or
  call `tui::step`, and render `ui` into `ratatui::backend::TestBackend`
  (`common::tui_drawn`, or `tui_drawn_at` for a size of its own). What
  every form and page shares, the line being typed (`Input`, its cursor
  counted in characters) and the page being read (`Reader`), is held key
  by key in `src/tui/state_test.rs`. Only
  the event loops in `tui.rs` and `install.rs` go untested there, and only
  the TUI's end-to-end suite reaches them, so keep logic out of them. A task
  that runs apart is started by `tui::step` and taken in by
  `tui::settle` or `tui::wait`; to make one slow, stand a `Runner` in for the
  real command lines (`App::runner`).
- Fixtures use `tempfile::TempDir`. Test names are sentences
  (`init_weaver_is_the_same_as_init`).
- `apps/cli/tests/docs_test.rs` holds `README.md` and every page under
  `docs/` to their relative links, in Rust so it runs without Node or the
  network: each file a link names is there, each anchor is a heading's as
  GitHub spells it, and each image in `docs/images` is shown by some page.
  A link to another site is not followed. `examples_test.rs` finds the
  reference's "Try it with the examples" walk-through in what the commands
  print. `api_guide_test.rs` replays the API's guide, `docs/api.md`: it
  builds the guide's workspace with the commands its first block shows,
  starts `ui` with `--token-file` as the guide does, reads every `curl`
  line of a `console` block as curl would and sends it over loopback with
  `common::panel`, holding each answer to the OpenAPI document and then to
  the guide's, with the home read as `/home/you`, the port, the token and
  each project's id pinned. It reads the lines rather than run them, so it
  needs neither curl nor jq, and fails on an option or a filter it does
  not read. Every route of both tables needs an example there, and the
  guide's table of the server's codes is held to `Condition::ALL`.

## Workflow

- Gitflow is enforced by required checks. PRs into `dev` come from
  `feature/*`, `hotfix/*` or `release/*` (or `main`, for a back-merge). PRs
  into `main` come only from `dev`. PRs are merged with merge commits.
- CI runs `fmt and clippy`, then `test (core)`, then `test (cli)`, `test
  (server)`, `test (web)` and `types drift (web)`, one workflow
  (`test-and-build`) whose tests are split by app alone, never by layer:
  an app's unit tests, value objects and integration suites run in its
  `test (<app>)`. It builds each app after its tests pass, in debug and then,
  once that passes, its release, the optimized build (`make core/release`,
  `make server/release`, `make cli/release`), in the same `build (<app>)`
  job. `test (web)` sets up Node, as only `e2e (web)` does
  besides on Linux: it lints, type-checks, tests and builds the panel, and
  uploads the build.
  `build (cli)` waits for it, embeds that build in a release, then removes
  `apps/web/dist`, runs `make cli/release/check` and uploads the release's
  binaries, which `e2e (web)` downloads and drives in Chromium and WebKit
  (`make web/e2e/run`), uploading the traces of what fails. `types drift
  (web)` runs `make web/types/check`, `make web/fixtures/check`, `make
  api/schemas/check` and `make api/openapi/check`. Development targets Linux only for now: macOS
  and Windows (`test (macos)`, `test (windows)`, which set up Node too) run
  only by hand (`workflow_dispatch`) or on PRs labelled `full-matrix`, until
  the compatibility pass (#154) puts them back on pushes to `dev`/`main` and
  PRs from `dev`, `main` or `release/*`. Every Linux job, both workflows',
  runs on the repository's self-hosted runner, asked for by its labels
  (`runs-on: [self-hosted, Linux, X64]`), and only macOS and Windows on
  GitHub's: a step that needs what GitHub's image held installs it where it
  is missing, since a self-hosted runner keeps what a job installed. Its
  image holds `make` and a C compiler (`build-essential`), and it runs as a
  user of its own with sudo, never root, which writes where a test takes the
  permission away: `fmt and clippy` fails first thing on a runner that is
  root.
- The rulesets for `dev` and `main` require every check that runs on every
  pull request: `fmt and clippy`, `test (core)`, `test (cli)`, `test
  (server)`, `test (web)`, `types drift (web)`, `build (core)`, `build
  (cli)`, `e2e (web)` and `build (server)`, and `gitflow branch name` for `dev` or
  `release source branch` for `main`. `test (macos)` and `test (windows)`
  are not required, since they run only when asked for. A check is required
  by its job's display name, so renaming a job means changing both
  rulesets.
- Commit messages: `<area>: <imperative summary>`, where area is `core`,
  `cli`, `server`, `web`, `docs`, `build`, `ci` or `chore` (`core, cli:` when
  a change spans two apps).
- When behaviour changes, update `docs/reference.md` (its examples are
  literal output, so re-run them) and `docs/architecture.md`, including its
  "Open questions". A change to what the API answers, a route added among
  them, updates `docs/api.md` too, whose answers `api_guide_test` holds to
  the server's. Literal output belongs in the reference, never in the
  README, which says what Mindweaver is and tours the panel in a few
  screens: its one example is what an MCP server's install prints, re-run
  with the reference's when that changes. A change to what the panel
  shows, a page, a view or its look, runs `make docs/screenshots`, and its
  PR carries the images it wrote; a change to what the README says of the
  panel goes with it.

## Style

Comments and doc comments explain *why* a choice was made, often naming the
alternative that was rejected. They rarely narrate what the code does. Match
that density and voice in code and docs alike.

No em dash (U+2014), in docs, comments, messages or commit messages: the owner
finds it reads as machine-written. Write a comma, a colon, parentheses or a
new sentence instead, never a hyphen or a spaced en dash in its place.
`make lint` fails on one in any tracked file; a commit message is yours to
check.
