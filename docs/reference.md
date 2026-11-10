# Reference

Mindweaver manages what your coding agents read, the way git manages code: a
project carries a `.mind` directory beside its **skills** and **rules**, and
a workspace above it carries a `.mindweaver` that orchestrates several such
projects at once.

One shared Rust engine (`mindweaver-core`) behind every front end: the CLI
(`mind` for a project, `mindweaver` for a workspace), the TUI it opens, and a
[control panel](#control-panel) in your browser, served by the same binary.
The layout leaves room for a desktop app later without moving the engine.

This page is all of it, in detail: every command with what it prints, the
formats Mindweaver reads and writes, the TUI, the panel and upgrading. The
[README](../README.md) is the short tour; [development.md](development.md)
is for working on Mindweaver itself, and [architecture.md](architecture.md)
says why each part is the way it is.

## Contents

- [The two levels](#the-two-levels)
- [The kinds](#the-kinds)
- [Getting started](#getting-started)
- [Try it with the examples](#try-it-with-the-examples)
- [Commands](#commands)
- [Gathering skills from elsewhere](#gathering-skills-from-elsewhere)
- [Installing into a project](#installing-into-a-project)
- [Loading another workspace](#loading-another-workspace)
- [What each project has](#what-each-project-has)
- [Machine-readable output](#machine-readable-output)
- [Following a project that moved](#following-a-project-that-moved)
- [Keeping projects up to date](#keeping-projects-up-to-date)
- [A repository that already has .claude/skills](#a-repository-that-already-has-claudeskills)
- [The workspace's own skills and rules](#the-workspaces-own-skills-and-rules)
- [The TUI](#the-tui)
- [Control panel](#control-panel)
- [Where a project keeps its artifacts](#where-a-project-keeps-its-artifacts)
- [What a skill looks like](#what-a-skill-looks-like)
- [What an MCP server looks like](#what-an-mcp-server-looks-like)
- [Upgrading](#upgrading)

## The two levels

| | Marker | Holds | Think of it as |
|---|---|---|---|
| **Mind project** | `.mind/settings.toml` | the agents that work in it, and its artifacts, in the directories the marker names | a repository's `.git` |
| **Weaver workspace** | `.mindweaver/weaver.toml` | references to mind projects, and artifacts of its own it shares with them | the directory your repos sit in |

A mind project's `.mind` belongs to the checkout it is in: a short settings
file that keeps itself out of git, the way a Python virtualenv does. What the
project writes itself, its skills and rules, sits beside its code and is
committed with it, so whoever clones the repository gets them. A weaver
workspace is the level above, where several projects are managed together,
and where the skills and rules that belong to all of them live, once, to be
installed into whichever needs them.

Every command that reads or changes artifacts works at both levels: `mind
<cmd>` on the project you are in, `mindweaver <cmd>` on the workspace: its own
artifacts, or one of its projects with `-p`. What only a workspace has
(`scan`, `gather`, `minds`) is `mindweaver`'s, and `mind weaver <cmd>` reaches it
from inside a project.

### The workspace in your home

A workspace is found the way `.git` is, by walking up from where you are. When
nothing up there has a `.mindweaver`, `mindweaver` uses **the one in your home**,
`~/.mindweaver`, so projects scattered anywhere can still be managed together
without choosing a directory first. One a Mindflayer build made, a
`.mindflayer`, is found the same way until it is moved
([from Mindflayer](#from-mindflayer)). `mindweaver init` somewhere else makes a
workspace there, and from inside it that one wins. The TUI says when it is
showing the default one, and `mindweaver which` says which workspace a command
would use, and why.

The first command that writes to it makes it: `mindweaver link`, `add`, `gather
git`, `load <path>`, `install -p PROJECT <SKILL>`. One that only reads (`load`
and `install` on their own among them) leaves your home alone, answers as an
empty workspace would, and says where the workspace will be:

```
$ mindweaver list
you manages no projects yet
  link one with `mindweaver link <path>`
hint: no workspace here or above, and the default one is not made yet; the first command that writes to it, such as `mindweaver link <path>`, makes it in /home/you/.mindweaver
```

Made, it puts nothing in your home but `.mindweaver`. Its own skills and rules
go in its library inside it, where no agent looks, rather than in a `~/skills`
and `~/rules` above every project you have:

```
$ mindweaver add skill commit-style "How every repo here writes commits"
created skill commit-style at /home/you/.mindweaver/library/skills/commit-style/SKILL.md
```

One an older Mindweaver made keeps them in `~/skills` and `~/rules` until you
[move them into its library](#upgrading).

`--home DIR`, or `$MINDWEAVER_HOME`, puts the default somewhere other than your
home; set to empty (`--home ""`) there is no default, and a command outside
every workspace says so instead.

### Choosing a workspace

Walking up is what happens when nothing says otherwise. `-W DIR`
(`--workspace`) names the workspace a command uses, by its directory or its
`.mindweaver`, from wherever you are; `$MINDWEAVER_WORKSPACE`, which `mindweaver
activate` sets ([several weavers](#several-weavers)), does the same for every
command run in a shell, the way an activated Python virtualenv does.
Either one wins over a workspace above you, so you can keep several weavers
for different purposes and pick one:

```
$ cd ~/Projects/collapse                   # under the ~/Projects workspace
$ mindweaver -W ~/weavers/writing list     # a workspace beside it instead
prose-style  How every essay here is written

$ export MINDWEAVER_WORKSPACE=~/weavers/writing
$ mindweaver list                          # the same, for every command
prose-style  How every essay here is written
$ mind link                                # this project, into that one
linked collapse as ../../Projects/collapse
```

`mind` commands use it too: `mind install` installs from it, and a change to
an installed skill drops its record there, which is how a project managed by a
workspace somewhere else is worked on from inside it. What `-W` or the
variable names has to be a workspace already. Anything else is refused,
never passed over for the one above and never made into one:

```
$ mindweaver -W ~/notes list
error: -W names a workspace that cannot be used: /home/you/notes: not a weaver workspace (no .mindweaver/weaver.toml or .mindflayer/flayer.toml)
```

`-W ""` names none and leaves `$MINDWEAVER_WORKSPACE` out too, and an empty
variable counts as unset. `--home` still says only where the default lives.

### Several weavers

A weaver can be made anywhere and switched to for a whole shell, the way a
Python virtualenv is. `mindweaver init DIR` makes one in `DIR`, which the
workspace in your home remembers ([the weavers this machine
knows](#the-weavers-this-machine-knows)), and `mindweaver activate DIR` prints the
shell code that activates it, for `eval` to run:

```
$ mindweaver init ~/weavers/writing
initialized weaver workspace `writing` in /home/you/weavers/writing/.mindweaver
  remembered as weavers/writing by the default workspace
$ cd ~/Projects/collapse
$ mindweaver which
Projects at /home/you/Projects, found walking up from /home/you/Projects/collapse
$ eval "$(mindweaver activate ~/weavers/writing)"
(weaver:writing) $ mindweaver which
writing at /home/you/weavers/writing, activated by $MINDWEAVER_WORKSPACE
(weaver:writing) $ source .venv/bin/activate
(.venv) (weaver:writing) $ deactivate      # the virtualenv's; the weaver stays
(weaver:writing) $ mindweaver-deactivate
$ mindweaver which
Projects at /home/you/Projects, found walking up from /home/you/Projects/collapse
```

Activating sets `$MINDWEAVER_WORKSPACE` and puts `(weaver:<name>)` in front
of your prompt; `mindweaver-deactivate` puts both back as they were, a variable
you had set yourself included. Activating another weaver swaps one for the
other, and `mindweaver activate` on its own activates the one a command run there
would use. `mindweaver which` says which workspace that is and why: named by
`-W`, activated, found walking up, or the default one.

It is `mindweaver-deactivate` and never `deactivate`, so it gets along with a
virtualenv activated before it or after it: in bash and zsh, each one's
deactivate takes out only what its own activate put in, whichever runs first.
In the prompt, the name shows each of `` \ $ ` % ! `` as `_`, since a prompt
would act on them.

The code is written for the shell `$SHELL` names, or for the one `--shell`
does (`bash`, `zsh`, `fish` or `pwsh`). fish and PowerShell run it their own
way:

```
mindweaver activate --shell fish ~/weavers/writing | source
mindweaver activate --shell pwsh ~/weavers/writing | Out-String | Invoke-Expression
```

In those two, a virtualenv activated before the weaver and deactivated before
it takes the weaver's prefix with it: the weaver stays active, as `mindweaver
which` says, but the prompt no longer shows it for the rest of that shell.

Activating writes nothing anywhere, and what cannot be activated prints no
code, so `eval` runs nothing and the shell is left as it was:

```
$ eval "$(mindweaver activate ~/notes)"
error: /home/you/notes: not a weaver workspace (no .mindweaver/weaver.toml or .mindflayer/flayer.toml)
```

### The weavers this machine knows

The workspace in your home keeps a list of the other weavers on this machine,
so they can all be seen in one place and one picked to activate. `mindweaver init`
adds each one it makes, once the workspace in your home is made, and `mindweaver
workspaces` lists them, with a `*` before the one a command run where you are
would use:

```
$ cd ~/Projects/collapse
$ mindweaver workspaces
  you       /home/you                  no projects; the default one
  writing   /home/you/weavers/writing  no projects
* Projects  /home/you/Projects         3 projects, 1 stale; not remembered

  `mindweaver init <path>` remembers the one in use, and changes nothing else in it
$ mindweaver init ~/Projects
weaver workspace `Projects` already initialized in /home/you/Projects/.mindweaver
  remembered as Projects by the default workspace
```

Each one says how many projects it links, and how many of those are stale. A
weaver made before the workspace in your home was, or by an older Mindweaver,
is not on the list until `mindweaver init` runs on it again, which changes nothing
else in it; the one in use is listed all the same, marked as not remembered.
Activating a weaver does not remember it, since activating writes nothing. One
that has gone is listed as `cannot be opened`, with a warning, and `mindweaver
workspaces forget` takes it off the list, under every spelling it was
remembered by, leaving the weaver itself, if it is still there, as it is:

```
$ mindweaver workspaces forget ~/weavers/writing
forgot weavers/writing
```

The list is a `known` array in `~/.mindweaver/weaver.toml`, which you can read
and edit by hand, and which an older Mindweaver reads past and keeps. Listing
never makes the workspace in your home; until something does, it knows no
weaver, and with `--home ""` there is none to know them. `--json` prints the
list in the envelope every reading command's `--json` shares, and `w` shows it
in [the TUI](#the-tui). The [control panel](#control-panel)'s **Loads** page
lists them too, and loads any of them into the workspace it shows.

## The kinds

| | Lives in | Shape | Declares |
|---|---|---|---|
| **Skill** | `skills/<name>/SKILL.md` | a directory each, so it can carry scripts and references beside its instructions | front matter in the [Agent Skills](https://agentskills.io/specification) format: `name`, `description`, and optionally `license`, `compatibility`, `metadata` and `allowed-tools` |
| **Rule** | `rules/<name>.md` | one markdown file each | usually nothing (it is context, and its name is its filename), though it may open with front matter: `paths` and `description`, both optional |
| **MCP server** | `mcp/<name>.toml` | one TOML file each, directly in `mcp/` | how an agent starts the server or reaches it, and what somebody supplies for it: [what an MCP server looks like](#what-an-mcp-server-looks-like) |

MCP servers are kept in the workspace's library, `.mindweaver/library/mcp`,
added with `add mcp` from how an agent starts or reaches one, and listed,
shown, validated, renamed and removed as skills and rules are. Installing
one into a project, as an entry in each agent's own config file, is not
done yet. A project keeps none of its own unless its settings name a
folder for them, so an `mcp/` that holds a server's code is never read as
definitions.

Folders under `rules/` group and mean nothing else, so
`rules/git/no-force-push.md` is the rule `git/no-force-push`. Skills are flat,
because a skill's directory already belongs to it.

Which agents a rule reaches depends on its `paths`. A rule that states none is
about every file, and reaches Claude Code, Codex and OpenCode alike, through a
marked block in the project's `AGENTS.md`, the one file all three read. Claude
reads `AGENTS.md` only where no `CLAUDE.md` or `CLAUDE.local.md` is in the
project or above it, so where the project's own `CLAUDE.md` is in the way,
and does not import `AGENTS.md` already, Mindweaver adds an `@AGENTS.md` line
to it, between markers, and takes it out with the last rule; where it may not
edit one (a `CLAUDE.md` above the project, a `CLAUDE.local.md`, or one git
tracks in a local project), Claude gets the rule as a rule file of its own
instead, so it reads each rule once. Mindweaver never creates a `CLAUDE.md`.
A rule that states `paths` is about those files only, and only Claude Code
honours that: it goes to Claude alone, as
`.claude/rules/mindweaver/<name>.md`, which Claude loads once it reads a file
the patterns match, and never into `AGENTS.md`, where every agent would read
it for every file. Codex and OpenCode read no `.claude/rules`, so a
path-scoped rule does not reach them, and Mindweaver says so for each one it
writes. A rule goes there when it is [installed](#installing-into-a-project),
as a skill does.

Those are the defaults, and a project can say otherwise: see [where a project
keeps its artifacts](#where-a-project-keeps-its-artifacts).

## Getting started

From a fresh clone, one command builds both binaries and links them:

```bash
make dev/link                  # `mind`, `mindweaver` and `mw` in ~/.cargo/bin, pointing at target/debug
```

`mw` is short for `mindweaver`, the command that manages minds: `mw init` makes a
workspace, `mw link` registers projects with it, and `mw` on its own opens
[the TUI](#the-tui). It is only a link that `dev/link` makes, never installed by
`make install`, because a name of two letters is easily another tool's.

They are **symlinks**, not copies, so every later build updates the binaries
you are running without reinstalling anything: `make cli/build`, which needs
only Rust, or `make build`, which builds the panel first and so needs Node 24
and pnpm as well. `make dev/unlink` takes them back off. If you only want to
use the tools rather than work on them, `make install` puts real copies in the
same place. It builds [the control panel](#control-panel) into them first
where Node 24 and pnpm run ([developing the panel](development.md#developing-the-panel) has
which pnpm, and `PNPM` for one corepack has not put on your PATH). Without
them it warns and makes the copies anyway, with the panel `apps/web/dist`
holds from an earlier build, or none, and then their `mindweaver ui` serves only a
page saying the panel was not built.

That place is `~/.cargo/bin`, which is on your PATH if `rustup` put it there
and is **not** if your Rust came from a system package. `make dev/link` says so
when it is missing; `BINDIR` is how you point somewhere else, and
[Development](development.md#development) has the rest.

A build from before the project was called Mindweaver installed `flayer`
where `mindweaver` now goes, and `make dev/link` linked `mf` beside it.
Neither target takes them out, since what is in your bin directory is
yours: `cargo uninstall mindflayer-cli` removes the copies an older `make
install` put there, its `mind` with them, and has to come first, since
cargo will not write a `mind` over one another package installed; `rm
~/.cargo/bin/flayer ~/.cargo/bin/mf` removes the links an older `make
dev/link` made. What such a build wrote in your workspaces and projects
keeps working, and `mindweaver migrate --names` moves it to the new names
when you ask ([from Mindflayer](#from-mindflayer)).

Then make a repository a mind project, and give the folder it sits in a
workspace to manage it, and others like it, from:

```
$ cd ~/Projects/collapse
$ mind init
initialized mind project `collapse` in /home/you/Projects/collapse/.mind
$ mind add skill deploy "Ship the service to staging"
created skill deploy at /home/you/Projects/collapse/skills/deploy/SKILL.md
$ git status --short
?? skills/

$ cd ~/Projects
$ mindweaver init
initialized weaver workspace `Projects` in /home/you/Projects/.mindweaver
$ mindweaver link collapse
linked collapse as collapse
$ mindweaver list
deploy  Ship the service to staging
```

`mind init` makes a `.mind` holding `settings.toml`, which says which agents
work in the project and where it keeps its own skills and rules, and a
`.gitignore` holding `*`, which is why `git status` shows none of it. The
`skills/` and `rules/` it makes beside the code are the project's, to commit
with it. Every agent is named unless `--agents claude,codex` names fewer, and
`--shared` leaves the `.gitignore` out, for a team that commits its `.mind`:
[where a project keeps its artifacts](#where-a-project-keeps-its-artifacts)
has the rest.

Or run `mindweaver` on its own and do all of that from [the TUI](#the-tui): it
opens the workspace at or above where you are (or, with none, the one in your
home, and says so) and lists the repositories under it so linking is a pick
rather than a path to remember. To give `~/Projects` a workspace of its own,
run `mindweaver init` there first.

`init` never overwrites an existing marker. Run it twice and it says so and
changes nothing, so it is safe in a script.

## Try it with the examples

`examples/` in this repository is a workspace of its own, holding two skills
and two rules written to be used as they are, so there is something to
install before you have gathered anything. With the repository cloned in
`~/src/mindweaver`, [name it](#choosing-a-workspace) to see what it holds:

```
$ mindweaver -W ~/src/mindweaver/examples list
skill  code-review     Reviews a change for bugs, safety, tests and scope, most severe first.…
skill  commit-style    Writes commit messages as "area: imperative summary". Use when writing,…
rule   git/commits     Commit one coherent step at a time
rule   style/comments  Comments say why, not what
```

[Load it](#loading-another-workspace) into the workspace that manages your
projects, and install from it:

```
$ cd ~/Projects
$ mindweaver load ~/src/mindweaver/examples
loaded examples as ../src/mindweaver/examples: 2 skills and 2 rules to install
  into the workspace at /home/you/Projects

$ mindweaver install -p collapse commit-style
installed commit-style into collapse, from load:../src/mindweaver/examples
$ mindweaver install -p collapse git/commits
installed rule git/commits into collapse, from load:../src/mindweaver/examples
```

`collapse/.claude/skills/commit-style` now holds the skill and the
`references/examples.md` it carries, for Claude Code, and
`collapse/.agents/skills/commit-style` the same for Codex and OpenCode.
`collapse/AGENTS.md` holds the rule, in a block of Mindweaver's that all
three read, and all of it is kept out of git. `style/comments` installs the
same way, but it states which files it is about, so it goes to Claude Code
alone, and the install says so. `mindweaver -W ~/src/mindweaver/examples
validate` says all four are ok.

## Commands

The surface is split the way the model is. `mind` acts on the project you are
standing in; `mindweaver` acts on the workspace above it.

```bash
mind                       # the TUI, on this project
mind init [--agents AGENTS] [--shared] [--skills DIR] [--rules DIR]
                           # create a .mind here: for which agents, kept out of
                           # git unless shared, and where its artifacts go
mind init --new-id         # give the project here an id of its own: for a
                           # copy made with its .mind
mind add <KIND> <NAME> <DESCRIPTION> [--template NAME]
                           # create a skill or a rule that validates as written
mind add mcp <NAME> <DESCRIPTION> --command CMD [--arg A]... [--env K=V]...
         [--secret-env VAR]... [--arg-env VAR]...
mind add mcp <NAME> <DESCRIPTION> --url URL [--sse] [--header K=V]...
         [--secret-header H=VAR]... [--bearer-env VAR] [--url-var NAME=VAR]...
                           # an MCP server, from how an agent starts it or
                           # reaches it, each secret a variable's name
mind edit <NAME> [--editor CMD]
                           # open it in $VISUAL or $EDITOR, then check it
mind edit <NAME> --contents FILE|- [--if-unchanged SHA256]
                           # replace what it holds with a file, or standard
                           # input, checked before anything is written
mind edit mcp:<NAME> --command CMD ... | --url URL ...
                           # write how an MCP server is started or reached
                           # again, from the options `add mcp` takes
mind rename <NAME> <NEW>   # folder and declared name together; `mv` for short
mind remove <NAME> --yes   # delete it; without --yes, say what would go; `rm`
mind templates [--json]    # what `add --template` can start from here
mind list [KIND] [--json]  # this project's artifacts; `mind list rules` filters
mind show <NAME> [--json]  # one artifact; `rule:deploy` when a name is ambiguous
mind validate [KIND|NAME] [--json]
                           # check a kind, one artifact, or everything
mind status [--json]       # what it holds, from where, for which agents
mind doctor [--json]       # what keeps its agents from what it gives them,
                           # which no agent says; it fixes nothing
mind link / mind unlink    # this project, into or out of the workspace above
mind load / mind unload <path>...
                           # another workspace's skills and rules, into the
                           # one above
mind install <NAME> [--from ORIGIN] [--force] [--yes]
                           # put a skill, a rule or an MCP server in: the
                           # workspace's own, a loaded workspace's, or the
                           # shelf's; --force writes over a copy changed since
                           # it was installed; a server from elsewhere than
                           # the library needs --yes, and without it says
                           # what each agent would run
mind uninstall <NAME>|--orphaned [--force]
                           # take one Mindweaver installed back out, or every
                           # one nothing offers any more
mind update [NAME...] [--force] [--yes]
                           # bring what was installed up to date with where
                           # it came from, and put back copies gone; without
                           # --yes, say what would change
mind agents [add|remove] <AGENT>... [--yes]
                           # which agents work here, carrying what is
                           # installed along; without --yes, say what would
mind share local|shared [--edit-tracked] [--yes]
                           # keep installs to this checkout, or leave them
                           # for the team to commit; without --yes, the same
mind targets set <KIND> <DIR>... [--yes]
mind targets reset <KIND> [--yes]
                           # install a kind in folders of the project's own,
                           # or back where its agents read; the same again
mind migrate [--agents AGENTS] [--local|--shared] [--yes]
                           # move a project an older Mindweaver made to a
                           # settings.toml, and its installs where its
                           # agents read
mind migrate --names [--yes]
                           # mindweaver migrate --names, on the workspace
                           # this project's commands use

mindweaver                 # the TUI: everything below, a key each
mindweaver ui [--port PORT] [--no-open] [--dev] [--token-file PATH] [--log]
                           # the control panel, in a browser, until Ctrl-C;
                           # mind ui is the same; with --no-open and
                           # --token-file, its API for scripts alone; with
                           # --log, what it answered and how long each took
mindweaver init [PATH] [--skills DIR] [--rules DIR]
                           # create a .mindweaver here, or in PATH
mindweaver activate [DIR] [--shell SHELL]
                           # the code that activates a workspace: eval "$(...)"
mindweaver which           # the workspace a command here would use, and why
mindweaver workspaces [--json]
                           # the weavers this machine knows, the one in use
                           # marked
mindweaver workspaces forget <path>
                           # take one off that list
mindweaver add|edit|rename|remove|templates [-p PROJECT] ...
                           # the same as mind's: the workspace's own by
                           # default, one of its projects with -p
mindweaver remove <NAME> --everywhere [--yes]
                           # one of its own, out of every project it was
                           # installed into first; without --yes, the same
mindweaver list|show|validate [-p PROJECT] [--json] ...
                           # the workspace's own and every project's, or one;
                           # validate checks the shelf too
mindweaver status [-p PROJECT] [--json]
                           # what every project holds, or one: current,
                           # outdated, edited, missing, orphaned, foreign or
                           # authored, for which agents, and from where
mindweaver doctor [-p PROJECT] [--json]
                           # mind doctor, for every project or one
mindweaver scan [DIR...] [--depth N] [--suggest] [--json]
                           # the repositories under it, or under each DIR,
                           # which are linked, and which moved; --suggest
                           # lists what is most worth linking first
mindweaver link <path>...  # register mind projects with it
mindweaver unlink <path>...
                           # drop them
mindweaver relink <old> <new>
                           # follow a project that moved, keeping what was
                           # installed into it
mindweaver minds [DIR...]  # a screen for linking and unlinking many at once
mindweaver load <path>...  # offer another workspace's own skills and rules,
                           # read live
mindweaver unload <path>...  # stop offering them

mindweaver gather git <URL> [--name NAME]
                           # collect skills from a repository onto the shelf,
                           # under a name --from takes
mindweaver gather list [--json]
                           # what is on the shelf or loaded, and where from
mindweaver install         # a screen for putting skills, rules and MCP
                           # servers into its projects
mindweaver install (-p PROJECT)...|--all-projects <NAME> [--from ORIGIN] [--force] [--yes]
                           # one, into the projects named or every one,
                           # without the screen
mindweaver uninstall (-p PROJECT)...|--all-projects <NAME>|--orphaned [--force]
mindweaver update (-p PROJECT)...|--all [NAME...] [--force] [--yes]
                           # mind update, for the projects named or every one
mindweaver adopt -p PROJECT <NAME> [--yes]
                           # a skill a project keeps by hand in an agent's
                           # folder, into the library and managed from there;
                           # without --yes, say what would happen
mindweaver agents|share|targets -p PROJECT ...
                           # mind's agents, share and targets, for one of its
                           # projects
mindweaver migrate --library [--yes]
                           # move its own skills and rules into its library;
                           # without --yes, say what would move
mindweaver migrate --names [--yes]
                           # move a workspace a Mindflayer build made, and
                           # what it wrote in its projects, to Mindweaver's
                           # names; without --yes, say what would move
mindweaver migrate -p PROJECT|--all-projects [--agents AGENTS] [--local] [--yes]
                           # mind migrate, for one of its projects or every one

mind weaver <cmd>          # the long way round; `mindweaver <cmd>` is the shortcut
mind ls / mindweaver ls    # alias for list; `mindweaver gather ls` for gather list
mind -C <dir> ...          # work in <dir> instead of the current directory
mind -W <dir> ...          # the workspace to use, from anywhere; "" for none
                           # named; $MINDWEAVER_WORKSPACE otherwise
mind --home <dir> ...      # where the default workspace lives; "" for none
mind --claude-config-dir|--codex-home|--opencode-config-dir|--agents-dir <dir> ...
                           # where each agent's own folder is; "" for none
mind tui / mindweaver tui  # the TUIs, spelled out
```

Both find what they act on the way git does, by walking up from wherever you
are, unless [a workspace is named](#choosing-a-workspace). Both levels answer
a different question, and that is the point:

```
$ cd ~/Projects/collapse && mind list      # just this project
skill  commit-style       How this repo writes commit messages
rule   git/no-force-push  Never force-push a shared branch

$ mind list rules                          # narrowed to one kind
git/no-force-push  Never force-push a shared branch

$ mindweaver list                          # every project it manages
collapse  skill  commit-style       How this repo writes commit messages
tanukeys  skill  ddd-reviewer       Review a change against the DDD rules
collapse  rule   git/no-force-push  Never force-push a shared branch
```

A column appears only when it tells you something. The kind column is absent
when only one kind is in play, the project column when only one project is:
the same rule in both cases.

A name is bare until it needs qualifying. When one name belongs to two kinds,
`mind show deploy` shows both and `mind show rule:deploy` picks one; `mind
install deploy` and `mind uninstall deploy` refuse it, saying which to type.

The qualifier is a **colon**, not a slash, because a rule's name is already a
route: `rules/skills/naming.md` is the rule `skills/naming`, and a slash
qualifier would have made that mean "the skill `naming`". With a colon, a name
is always a name.

A workspace lists the projects it was **told** to manage, not whatever happens
to sit inside it, so the answer does not change with the directory you ran it
from. `mindweaver link` is how you tell it:

```
$ mindweaver link ../collapse
linked collapse as ../collapse
```

The entry is stored relative to the workspace, so the two can be moved
together, unless that route would not actually resolve, in which case the
absolute path is stored instead. Linking the same project twice changes
nothing and says so, naming the spelling already in the file. `unlink` removes
every entry pointing at the project, and still works on one whose directory has
moved away, which is exactly the entry worth removing.

`mindweaver scan` finds what there is to tell it about: every mind project and git
repository under the workspace, three levels down, and whether each is linked
yet. It links nothing.

```
$ cd ~/Projects && mindweaver scan
collapse  linked
notes     not linked  git repository, not a mind project yet
tanukeys  not linked  mind project

  link one with `mindweaver link <path>`
  a repository needs `mind -C <path> init` before it can be linked
```

`--suggest` asks which to link first. It lists what is not linked before
what is, ranked by what each holds, and says why on each row: a mind project
whose settings give it an id, which the workspace can follow when it moves;
then one without (marked by an older `mind.toml`, or by a marker that would
not open); then a git repository holding something an agent reads at its
root, named on the row; then any other repository. What counts as an agent's
is what Claude Code, Codex and OpenCode read there: `AGENTS.md`,
`AGENTS.override.md`, `CLAUDE.md`, `CLAUDE.local.md`, `.mcp.json`,
`opencode.json`, `opencode.jsonc`, `.claude`, `.agents`, `.codex` and
`.opencode`
([where each agent reads](agents.md#what-says-an-agent-works-in-a-project)).
It still links nothing and makes nothing a project: a suggestion is a line
to type, or a row to tick on [the minds screen](#the-tui), which lists them
in this order.

```
$ mindweaver scan --suggest
tanukeys  not linked  mind project with an id
notes     not linked  git repository with CLAUDE.md
collapse  linked

  link one with `mindweaver link <path>`
  a repository needs `mind -C <path> init` before it can be linked
```

Name a folder, or several, and it looks under those instead; `--depth` says
how many levels below each. Every path is spelled the way `mindweaver link` takes
it from where you ran the scan, so a line of the report is a line to type: a
route from here, or the whole path of one outside.

```
$ mindweaver scan ~/Work
/home/you/Work/acme/api  not linked  git repository, not a mind project yet
/home/you/Work/acme/web  not linked  mind project

  link one with `mindweaver link <path>`
  a repository needs `mind -C <path> init` before it can be linked
```

A scan is bounded so that a folder at the top of your home does not turn
into a crawl of the disk. It goes three levels below each folder unless
`--depth` says otherwise. Hidden folders, symlinks, `node_modules` and
`target` are never entered, nor is a repository it finds, whose folders are
its own. Agent files are looked for only at the top of each one it finds,
by name: never opened, and never followed when they are links. However
deep it goes, it stops after listing 4096 folders and says so.
`mindweaver minds ~/Work` opens [the minds screen](#the-tui) on what is there,
and `--json` prints the report in the envelope
[`status --json`](#what-each-project-has) uses, with the id each project's
settings name it by, the reason each is worth linking (`id`, `mind`,
`agents` or `git`) and the agent files it holds. With `--suggest`, `found`
is in the suggested order:

```
$ mindweaver scan ~/Work --depth 2 --json
{
  "version": 2,
  "data": {
    "roots": [
      "/home/you/Work"
    ],
    "depth": 2,
    "found": [
      {
        "path": "/home/you/Work/acme/api",
        "route": "/home/you/Work/acme/api",
        "id": null,
        "mind": false,
        "linked": false,
        "moved": null,
        "reason": "git",
        "agent_files": []
      },
      {
        "path": "/home/you/Work/acme/web",
        "route": "/home/you/Work/acme/web",
        "id": "f4a29f5daaf5bc789c4b5d6b13174f48",
        "mind": true,
        "linked": false,
        "moved": null,
        "reason": "id",
        "agent_files": []
      }
    ],
    "untraced": [],
    "truncated": false
  },
  "warnings": [],
  "coded_warnings": []
}
```

```
$ mind validate
skill:commit-style: ok
rule:git/no-force-push: ok

1 skill and 1 rule checked, 0 invalid
```

Each kind is checked against what it actually declares. A skill is held to the
[Agent Skills rules](#what-a-skill-looks-like): its name has to match its
directory, its description has to fit, and so on; a rule's front matter is
optional, so the only things left to check are that its name is usable and that
the file says something beyond its front matter. `mindweaver validate` checks what
is on the [shelf](#gathering-skills-from-elsewhere) too.

`validate` exits non-zero when anything is wrong, so it works in CI. What an
agent reads past (a `metadata` entry that is not a string, a `compatibility`
over 500 characters, a rule's front matter that does not parse) is listed as a
warning instead, under an artifact that is still `ok`. An artifact that cannot
be read at all is a warning on stderr and does not hide the ones next to it.

### The agents' own folders

Each agent also reads a folder that belongs to no project: Claude Code its
configuration folder, Codex its home, OpenCode its configuration folder, and
more than one of them the skills in `~/.agents`. An install never puts a copy
inside one of them, since a skill there reaches every project, and
[`doctor`](#what-no-agent-says) reads what each holds that a project's agents
read too. Each is taken from its option, then from the variable the agent
itself reads, then from its folder in your home:

| Option | Then | Then |
|---|---|---|
| `--claude-config-dir` | `$CLAUDE_CONFIG_DIR` | `~/.claude` |
| `--codex-home` | `$CODEX_HOME` | `~/.codex` |
| `--opencode-config-dir` | `$OPENCODE_CONFIG_DIR` | `~/.config/opencode` |
| `--agents-dir` | | `~/.agents` |

As with `--home`, set to empty (`--codex-home ""`) there is none, and that
agent's folder is left alone.

Where each agent reads skills and rules, at project and user scope, and which
of those facts were checked on the real binaries, is recorded in
[agents.md](agents.md).

OpenCode itself reads `$OPENCODE_CONFIG_DIR` on top of `~/.config/opencode`
rather than in its place, and Claude's skills in `~/.claude/skills` whatever
`$CLAUDE_CONFIG_DIR` says, by its own documentation. So while either variable
names a folder, the one in your home is still OpenCode's to Mindweaver too:
`doctor` reads it, and no install puts a copy in it. An option names the one
folder outright.

## Gathering skills from elsewhere

A workspace has a **shelf**: skills collected from somewhere else, held by the
workspace and belonging to no project yet.

```
$ mindweaver gather git https://github.com/acme/skills
acme-skills: https://github.com/acme/skills at a1b2c3d
  added  commit-style  How this repo writes commit messages
  added  ddd-reviewer  Review a change against the DDD layering rules

2 skills: 2 added, 0 updated, 0 unchanged
```

The repository's `skills` folder is what gets harvested; `--path agents` takes
another one, and `--ref v2` takes a branch or a tag instead of the default.
The source is called `acme-skills`, after its owner and its repository, which
is what `gather list` shows and `--from` takes; `--name team-skills` calls it
something else, and given again with another name renames it. A name is
letters, digits, `.`, `_` and `-`, and one source's only: one another source
has is refused before anything is fetched. A
repository that is a weaver workspace made by `mindweaver init` keeps its own in
[its library](#the-workspaces-own-skills-and-rules), so it is gathered with
`--path .mindweaver/library/skills`, or [loaded](#loading-another-workspace)
from a clone instead.

With stderr on a terminal it says what it is doing while it does it
(`connecting`, `receiving objects 120/340`, `reading folders 3/40`) on one line
there, which it wipes before the report, so the report reads as above. With
stderr piped or redirected, it says nothing until the report.

Gathering fills the shelf and stops there. **Nothing is written into a mind
project**: which of these a project should carry is a separate decision, and a
`git clone` that quietly edited your repositories would be the wrong kind of
convenient. [`mindweaver install`](#installing-into-a-project) is where that
decision is made.

Run it again and it says what moved, which is the only thing worth reading the
second time:

```
$ mindweaver gather git https://github.com/acme/skills
acme-skills: https://github.com/acme/skills at 9f8e7d6
  updated    commit-style  How this repo writes commit messages
  unchanged  ddd-reviewer  Review a change against the DDD layering rules

2 skills: 0 added, 1 updated, 1 unchanged
```

Two repositories may both offer `commit-style`, and both are kept: each source
gets its own folder under `.mindweaver/skills/`, and the workspace's database
records which came from where.

```
$ mindweaver gather git https://github.com/other/rules
other-rules: https://github.com/other/rules at 4d5e6f7
  added  commit-style  Conventional commits, strictly
  added  pdf--forms    Fill in PDF forms

2 skills: 2 added, 0 updated, 0 unchanged
warning: /home/you/Projects/.mindweaver/skills/github.com-other-rules/pdf--forms/SKILL.md: `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens

$ mindweaver gather list
commit-style  acme-skills  How this repo writes commit messages
commit-style  other-rules  Conventional commits, strictly
ddd-reviewer  acme-skills  Review a change against the DDD layering rules
pdf--forms    other-rules  Fill in PDF forms
```

Each is listed by the name of its source, which is what `--from` takes when
two sources offer one name:

```
$ mindweaver install -p collapse commit-style --from other-rules
installed commit-style into collapse, from https://github.com/other/rules
```

The install records where it came from by the source's URL, as `status` and
`gather list --json` show it (its `origin`), and `--from` takes that too.

One repository is one source however its URL is spelled. On GitHub and
GitLab, `https://github.com/acme/skills`, `https://github.com/Acme/Skills.git`,
`git@github.com:acme/skills.git`, `ssh://git@github.com/acme/skills` and
`acme/skills` are the same repository, so gathering it by another of them
brings the source it already is up to date, fetched by the spelling you gave
this time. `acme/skills` alone is GitHub's: a folder on this machine is a
path, `./acme/skills` when it is relative. Elsewhere only a credential, the
case of the scheme and the host, a trailing `/` and `file://` before a path
make no other source, since another host may serve a different repository at
`host:path` than at `ssh://host/path`, or at `x.git` than at `x`, and at
`alice@host:skills` than at `bob@host:skills`, so the user ssh logs in as is
kept there; a relative path is one source of its own, whatever folder it
names. Another spelling of
a source's URL still works as `--from`, with a warning that it is not the
name, and the install goes ahead:

```
$ mindweaver install -p tanukeys commit-style --from https://github.com/Acme/Skills.git
installed commit-style into tanukeys, from https://github.com/acme/skills
warning: --from https://github.com/Acme/Skills.git names the source `acme-skills` by another spelling of its URL, which --from takes for now: name it `acme-skills`
```

A skill that cannot be read is a warning on stderr and does not stop the ones
beside it from being gathered. One that reads but breaks a rule of the [skill
format](#what-a-skill-looks-like) (a doubled hyphen in its name, like
`pdf--forms` above, or metadata that is not all strings) is gathered anyway,
with a warning saying what is wrong: leaving it behind would hide the problem
along with the skill. `mindweaver validate` says so again whenever you ask, naming
the source folder each skill on the shelf sits in:

```
$ mindweaver validate skills
commit-style (shelf/github.com-acme-skills): ok
commit-style (shelf/github.com-other-rules): ok
commit-style (collapse): ok
ddd-reviewer (shelf/github.com-acme-skills): ok
ddd-reviewer (tanukeys): ok
pdf--forms (shelf/github.com-other-rules): 1 problem
  - `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens

6 skills checked, 1 invalid
```

Installing is where it has to be right, because a project's copy is what its
agents load:

```
$ mindweaver install -p collapse pdf--forms
error: `pdf--forms` cannot be installed: `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens
```

A private repository is fetched with whatever your git credential helpers
already know, or, over ssh, with the keys ssh already uses (your ssh-agent's,
or one in `~/.ssh`). Mindweaver itself never stops to ask for a username or a
password, so a source that needs one nobody has fails, and says what to set
up:

```
$ mindweaver gather git https://github.com/acme/private-skills
error: https://github.com/acme/private-skills: needs credentials, and gathering never prompts for them: configure a git credential helper, or use ssh-agent for an ssh URL
```

ssh is a program of its own and can still ask on the terminal: for the
passphrase of a key your ssh-agent does not hold, for a password when the
server accepts one, or whether to trust a host it has not seen before.

A URL with a password or a token in it is refused before anything is fetched.
What is gathered is stored, listed and reported by its URL, and a token typed
into a command line is already where `ps` and your shell history show it. Give
the token to a git credential helper, and gather the URL without it:

```
$ mindweaver gather git https://me:ghp_t0ken@github.com/acme/private-skills
error: https://github.com/acme/private-skills: a password, a token or, over http(s), a user name in the URL is refused, because gathering stores and shows the URL: gather it without one, and configure a git credential helper that has it
```

So is such a URL typed where the branch (`--ref`), the folder (`--path`) or
the name (`--name`) goes, which the workspace would otherwise keep as that
branch or folder, word for word, and refused with the same words, the URL said
without what it carried.

The user ssh logs in as is not a credential, and is kept
(`git@github.com:acme/skills`, `ssh://git@github.com/acme/skills`). Over https
a user name on its own is refused too, because a token can stand in for one:
GitHub takes `https://<token>@github.com/...`, and nothing in the URL tells the
two apart. That includes the https URLs Bitbucket and Azure DevOps hand out
(`https://<user>@bitbucket.org/...`); gather those without the user name, and
let the credential helper supply it.

A workspace an older Mindweaver gathered into with a token in a URL is cleaned
up by the next command that writes to it: a gather, an install or an
uninstall, or an `add`, `edit`, `rename` or `remove` in a project it manages.
The token is taken out of `mindweaver.db` and out of the clone in
`cache/`, and the command says so once, on stderr, without failing because of
it:

```
$ mindweaver install -p collapse deploy
installed deploy into collapse, from https://github.com/acme/private-skills
note: https://github.com/acme/private-skills: the user name, password or token an older Mindweaver stored in this URL is taken out; gathering it again needs a git credential helper that has it
note: 2 action log entries: the user names, passwords or tokens an older Mindweaver stored in URLs in them are taken out
```

The install screen does it when you apply what you marked, not when it opens.
A command that only reads, such as `mindweaver gather list`, shows the URL without
the token and leaves the file as it is.

A workspace gathered into by two spellings of one repository's URL before
names came in (#98) held two sources of it. The first command to open it with
this version makes them one, keeping the one gathered most recently and what
it gathered, and moves every installation from the other to it, a command
that only reads too. The first command that writes removes the clone and the
shelf folder of the one that went, and says so, once (again by each command
that writes after it while a folder will not go):

```
$ mindweaver install -p collapse ddd-reviewer
installed ddd-reviewer into collapse, from git@github.com:acme/skills.git
note: https://github.com/acme/skills: the same repository as the source `acme-skills` (git@github.com:acme/skills.git), so the two are one source now, and the shelf keeps what `acme-skills` gathered
```

`--from https://github.com/acme/skills` still names it, with the warning
above.

### What a workspace keeps

```
.mindweaver/
  weaver.toml     the projects it manages, the workspaces it loads, and where
                  its own skills and rules are
  weaver.toml.v2  weaver.toml as it was before `mindweaver migrate --library`
                  rewrote it, if it did
  library/skills/<name>/SKILL.md
  library/rules/<name>.md
                  its own, unless weaver.toml says otherwise
  mindweaver.db   what was gathered and from where, what was installed where,
                  and every action taken; a URL goes in without credentials,
                  and one an older Mindweaver stored is taken out by the next
                  command that writes
  mindweaver.db.v2.bak
                  the database as it was before this version of Mindweaver
                  updated it, if it did
  lock            held by a command while it changes the workspace
  cache/<source>/ the clone, kept so a gather can be looked at afterwards
  skills/<source>/<name>/SKILL.md
  templates/      templates for every project it manages, if you add any
```

`mindweaver.db` is SQLite, and it is the one thing Mindweaver writes that is
not meant to be read by hand: it answers questions a file cannot, like which
of two identically named skills came from which repository, at which revision.
The first gather, install or uninstall makes it; a command that only reads
never does.
When a new version of Mindweaver changes how the database is laid out, the
first command to open it updates it, after copying it aside as
`mindweaver.db.v<old version>.bak`. The copy is never removed for you, and it
is the way back: an older Mindweaver refuses the updated database, and reads
the copy put back in its place. It is the database as it was, so a token an
older Mindweaver stored in a URL stays in the copy after the next command that
writes takes it out of `mindweaver.db`.
Timestamps are Unix seconds, so `datetime(at, 'unixepoch')` renders them:

```sql
SELECT datetime(at, 'unixepoch'), action, outcome, detail FROM actions;
```

The TUI and any number of terminals can be open on one workspace, and the
commands that change it take turns. Linking, loading, gathering, installing,
uninstalling and migrating each hold `lock` while they write, with the
process's id in `lock.pid` beside it. One that finds the workspace held waits
up to ten seconds for the other to finish, then gives up and says who it
waited for:

```
$ mindweaver link tanukeys
error: /home/you/Projects/.mindweaver/lock: workspace busy: waited 10s for process 3234405 to finish writing to it; try again once it has
```

Reading never waits for `lock`. Listing, showing and the TUI's screens read
each file whole, as the last write to it left it, though a read made while a
gather or an install is under way can find some of what it changes changed
and the rest not yet. The database keeps a write-ahead log, so a read does not
wait for a write to it either, except on a filesystem that cannot keep one,
where it waits for the write to finish. A workspace on a network share is safe
used from one machine at a time: the lock and the database keep apart programs
on one machine, not programs on two.

## Installing into a project

`mindweaver install` opens a screen. Projects on the left, and on the right what
can be installed into the one under the cursor, one kind at a time, ticked
where that project already holds it:

```
┌ Projects ──────────┐┏ Skills in collapse ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
│> collapse  (1)     │┃  [x] commit-style  (not installed by mindweaver) ┃
│  tanukeys          │┃      How this repo writes commits  [acme/skills] ┃
│                    │┃> [x] deploy  + install                           ┃
│                    │┃      Ship the service to staging  [acme/skills]  ┃
└────────────────────┘┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
↑↓ skill  space mark  tab rules  ←/esc back  v by skill  a apply  q quit
1 to install, 0 to remove
```

Move down the projects and the right column follows. `tab` shows the rules in
it instead, then the MCP servers, and the skills again (`shift+tab` goes the
other way). `→` or
`enter` goes into the column, `space` ticks and unticks, `←` or `esc` comes
back. Nothing touches a file until `a`, which asks first and then does
everything at once, of both kinds: installs and removals together.

What `a` runs is the commands you would have typed: a `mindweaver install` for
each skill, rule or MCP server ticked, with a `-p` for every project ticked to take it
and `--from` for where the one ticked comes from, and a `mindweaver uninstall`
for each one unticked. It prints each with what it said, as the TUI prints a
session when it closes, so a session with the screen in it can be run again
without it:

```
$ mindweaver install -p collapse -p tanukeys --from load:../src/mindweaver/examples skill:commit-style
installed commit-style into collapse, from load:../src/mindweaver/examples
installed commit-style into tanukeys, from load:../src/mindweaver/examples
$ mindweaver uninstall -p collapse rule:git/commits
removed rule git/commits from collapse
```

One that fails says why where it ran, and the rest run all the same. All of
it is on stdout, warnings and errors included, each under the line it is
about.

An MCP server ticked that comes from a loaded workspace or the shelf is
somebody else's command, so the question `a` asks shows, before anything is
written, what each of the project's agents would run of it, or why one would
not be given it, as `install` says it ([what an MCP server looks
like](#what-an-mcp-server-looks-like)):

```
from elsewhere than the library, each agent would run:
mcp linear into collapse, from load:../team:
  claude reaches: https://mcp.linear.app/mcp
  codex reaches: https://mcp.linear.app/mcp
  opencode reaches: https://mcp.linear.app/mcp
  warning: Codex reads `linear` from /home/you/Projects/collapse/.codex/config.toml only in a project it trusts, and /home/you/.codex/config.toml does not mark /home/you/Projects/collapse `trusted`
```

What does not fit the terminal scrolls with `j` and `k`, and `y` waits
until all of it has been shown. Saying yes runs its line with `--yes`,
which is how the transcript prints it:

```
$ mindweaver install -p collapse --from load:../team --yes mcp:linear
```

One of the workspace's own goes in as a skill does, with no question of its
own.

Each project keeps its own ticks, so one pass across the list is one plan for
the whole workspace. The number beside a project is how many of its boxes you
have moved.

`v` turns the screen round, to put one skill or rule into several projects in
a pass: everything of the kind shown on the left, and on the right every
project, ticked where it holds the one under the cursor, which `→` goes into
and `space` ticks as before:

```
┌ Skills ──────────────────────────────────────────┐┏ Projects for deploy ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
│  commit-style                                    │┃> [x] collapse  + install                         ┃
│      How this repo writes commits  [acme/skills] │┃  [ ] tanukeys                                    ┃
│> deploy  (1)                                     │┃                                                  ┃
│      Ship the service to staging  [acme/skills]  │┃                                                  ┃
└──────────────────────────────────────────────────┘┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
↑↓ project  space mark  tab rules  ←/esc back  v by project  a apply  q quit
1 to install, 0 to remove
```

They are the same boxes either way round, so a tick made one way is there the
other, and `a` applies them all: a line for each skill or rule ticked and one
for each unticked, whichever way round the ticks were made. `v` again turns
it back, with the cursor on the same box, and `tab` still shows the other
kind. A project that cannot take the kind shown has no box there, and says
why.

**Unticking removes.** A skill is copied wherever the project's agents read it
([below](#where-a-project-keeps-its-artifacts)), or into the directory a
`mind.toml` names, and unticking it deletes those copies again. A rule is
written into the project's `AGENTS.md`, and into Claude's own rule files
where Claude needs it there ([the kinds](#the-kinds)), and unticking
it takes it back out of both. With one exception, which is the rule the whole
command works by:

> Mindweaver only manages what it installed.

A skill already where an install would put it, that Mindweaver did not put
there, is shown ticked and marked `(not installed by mindweaver)`. It cannot be
unticked and it is never overwritten or deleted: somebody wrote it. The ledger
is what knows the difference. One in any of the folders an install would write
refuses the whole install, and the others are left alone too, so the agents
never read two different skills of one name. So is a rule whose section in
`AGENTS.md` Mindweaver did not write there: another workspace linking the
project did, or a hand that copied a marker.

What Mindweaver installed and somebody changed since is held the same way:
shown ticked and marked `(changed since it was installed)`, and neither
written over nor deleted from the screen, since the change is somebody's
work too. The ledger keeps a hash of what each install wrote, which is how
it tells: a copy edited by hand or with `mind edit`, a file added to it, or a
rule's section of `AGENTS.md` rewritten. Only `--force` on [the command
line](#without-the-screen) writes over it or takes it out.

An install with a copy gone since, deleted by hand or by a `git clean`,
opens unticked and marked `(a copy gone since it was installed)`: ticking it
puts every copy back. One that still has a copy left is taken out on [the
command line](#without-the-screen), with `uninstall`. One whose skill or rule
has changed where it came from since stays ticked, marked `(outdated; mindweaver
update refreshes it)`: [updating](#keeping-projects-up-to-date) brings it up
to date.

Two shelves can offer `commit-style`, but a project has one directory of that
name, so ticking one unticks the other rather than letting both apply and the
second win quietly. The one ticked when the screen opens is the one the
project's copy was installed from, as the workspace recorded it, even when
that copy is behind both places or edited since. A skill and a rule of one
name are two things, and keep a box each.

The screen offers the workspace's own skills and rules too, first and marked
`[workspace]`: that is how something written once for every project reaches
the ones that want it.

### Without the screen

```bash
mind install commit-style                       # into the project you are in
mindweaver install -p collapse commit-style     # into one the workspace manages
mindweaver install --all-projects commit-style  # into every one it links
mind install deploy --from workspace            # when two places offer one name
mind install git/commits                        # a rule, by its route
mind install rule:deploy                        # when a skill and a rule share it
mind uninstall commit-style
```

The same rule holds: a skill or a rule somebody wrote is never overwritten or
removed, and says so. `--from` takes `workspace`, a shelf source's name or
URL, or a loaded workspace's path or name, and is only needed when two places offer the
same name: which of the two lands in the project's one folder of that name is
not something to guess. A word that is both a source's name and a loaded
workspace's name or path names neither, and is refused as offered twice: say
the source's URL or `load:<path>` instead.

`mindweaver install` takes `-p` once for each project, to install into several
at once, or `--all-projects` for every one the workspace links, and `mindweaver
uninstall` takes them the same way. Each project is said as an install into
it alone says it, in the order asked, with what its rules warn of after its
name (`warning: collapse: ...`), and one that refuses is said of it while the
others go ahead:

```
$ mindweaver install commit-style --all-projects
installed commit-style into collapse, from workspace
warning: tanukeys already has a commit-style at skills/commit-style that Mindweaver did not put there, so it was left alone
$ mindweaver uninstall commit-style --all-projects
removed commit-style from collapse
warning: commit-style at skills/commit-style in tanukeys was not installed by Mindweaver, so it was left alone
```

An uninstall from several passes over a project that has nothing of that
name, unless none of them has it, and a refusal every project shares, such as
a name nothing offers, is said once rather than once a project.

A copy changed since it was installed is left alone too, every other copy
with it, until `--force` says to write over it or take it out:

```
$ echo "Keep the subject under 50 characters." >> .agents/skills/commit-style/SKILL.md
$ mind install commit-style
warning: commit-style in collapse was changed since it was installed, at .agents/skills/commit-style, so it was left alone; --force writes over it
$ mind uninstall commit-style
warning: commit-style in collapse was changed since it was installed, at .agents/skills/commit-style, so it was left alone; --force takes it out
$ mind install commit-style --force
updated commit-style in collapse, from workspace
```

Forcing reaches only what Mindweaver installed: somebody's skill that it did
not put there stays out of reach whatever is asked.

A rule's line says it is one, since a route reads like a path. One about some
files only reaches Claude alone, and the install says so, as
[`status`](#what-each-project-has) does whenever asked; a name a skill and a
rule share needs its kind:

```
$ mind install git/commits
installed rule git/commits into collapse, from workspace
$ mind install style/comments
installed rule style/comments into collapse, from workspace
warning: rule `style/comments` states `paths`, so it reaches claude only: codex and opencode read no .claude/rules
$ mind install deploy
error: `deploy` is offered as a skill and as a rule, so say which: `skill:deploy` or `rule:deploy`
$ mind install rule:deploy
installed rule deploy into collapse, from workspace
```

A rule the project received is not one of its own, but rendered from where it
was installed from, so `mind edit`, `rename` and `remove` leave it and say
how to take it out:

```
$ mind remove git/commits --yes
error: `git/commits` is a rule collapse received, rendered from where it was installed from rather than kept in it, so it is not changed from here: uninstall it, or change it where it is offered and install it again
```

### Built out of sight

An install never writes beside the copy it replaces, where an agent reading the
skills folder would find it. It builds the copy in `.mind/tmp`, a folder that
ignores itself, and moves it into place once it is whole (every copy of it,
when the project's agents read two folders), so the skills folder holds the
old copy, the new one or, for the instant between two renames of an update,
neither, and never part of either. Two installs at once each build in a
folder of their own there. One that is stopped halfway leaves its work there
too, and the next install clears it and says so, as it does the hidden
`.<name>.mindflayer-new` an older version left beside a skill:

```
$ mind install commit-style
installed commit-style into alpha, from workspace
  cleared .mind/tmp/1901387-eb475fb4b6b73a14, left by an install that did not finish
```

## Loading another workspace

A weaver workspace can be a repository of skills in its own right: its own
skills and rules, kept for sharing. `mindweaver load <path>` makes one of those a
source for the workspace that manages you, without copying anything:

```
$ cd ~/skills-equipo              # a workspace with skills of its own
$ mw load .
loaded skills-equipo as skills-equipo: 2 skills to install
  into the workspace at /home/you, the default one

$ cd ~/code
$ mw install -p api deploy
installed deploy into api, from load:skills-equipo

$ mw load                         # what is loaded, and what each offers
skills-equipo  skills-equipo  2 skills to install
  in the workspace at /home/you, the default one
```

It goes into the workspace at or above where you run it that is not the one
being loaded, so `mw load .` from inside `skills-equipo` goes into the one
above it, or else [the one in your home](#the-workspace-in-your-home). The last
line always says which.

What it records is where the workspace is, in the `loaded` list of your
`weaver.toml`, and its skills and rules are **read from there every time**:
edit one in `skills-equipo` and the next `mw update -p api`, or `mw install`
of it, [updates the copy](#keeping-projects-up-to-date) in `api`, with
nothing to load again. `mindweaver gather list` and the install
screen show what it offers beside the shelf's, as coming from `load:<path>`,
which is also what `--from` takes, as do the path you gave `load`, from
wherever you type it, and the workspace's name. Only a loaded workspace's own
are offered, never what it loads in turn. `mindweaver unload <path>` forgets it;
what was already installed from it stays where it is. `mind load <path>` does
the same from inside a project, into the workspace above it, and the
[control panel](#control-panel) does both from its **Loads** page.

A loaded workspace is usually somebody else's, so what it offers is read with
suspicion: a skill whose declared name is not one plain folder name (`..`,
`a/b`, an absolute path) is not offered, and neither is a rule filed under a
route no rule may be named; a symlink pointing out of the loaded workspace is
not followed, and nothing it keeps as its own is ever overwritten or removed
from a project that shares its folder. Each of these is a `warning:` line,
like a skill that cannot be read or a loaded workspace that has gone.

## What each project has

`mindweaver status` says what every project the workspace manages holds: whether
Mindweaver put each thing there, which agents read it, and where it came from.
With [the examples loaded](#try-it-with-the-examples), and installed from
there, `commit-style` and `code-review` into `collapse` and `commit-style` into
`tanukeys`, a project an older Mindweaver made:

```
$ cd ~/Projects
$ mindweaver status
collapse  code-review   missing   codex,opencode         load:../src/mindweaver/examples
collapse  commit-style  current   claude,codex,opencode  load:../src/mindweaver/examples
collapse  deploy        authored  -
collapse  notes         foreign   claude,opencode
tanukeys  commit-style  current   -                      load:../src/mindweaver/examples
hint: tanukeys is marked by a mind.toml, which `mind migrate` moves to a settings.toml
```

Each artifact is in one of seven states, the first five for what Mindweaver
installed:

| State | Means |
|---|---|
| `current` | Mindweaver put every copy there, every one is still there as it was put, and where it came from offers the same. |
| `outdated` | the same, but where it came from offers something newer: [`update`](#keeping-projects-up-to-date) brings it up to date. |
| `edited` | a copy it put there holds something else now, edited by hand or with `mind edit`, or a skill renamed with `mind rename`; still Mindweaver's install, but no install or update writes over it and no uninstall takes it out without `--force`. |
| `missing` | a copy it put there is gone, deleted by hand or by a `git clean`, or no longer reads; `update` puts every copy back. Here Claude's copy of `code-review` was deleted. |
| `orphaned` | where it came from no longer offers it, deleted from the library or from a workspace that stopped being loaded; nothing can bring it up to date or put a copy back, so it stays as it is until it is uninstalled, by name or with every other one by `uninstall --orphaned`. |
| `foreign` | somebody else's, in a folder an agent reads; Mindweaver never overwrites or deletes it. Here `notes`, kept in `.claude/skills` by hand. |
| `authored` | the project's own, in a folder it authors in, such as `skills/`. |

Then the agents that read a copy of it, as `--agents` names them: `-` for
none, as for `deploy`, which `collapse` keeps in `skills/`, a folder no agent
reads, and for everything in a project whose `.mind` an older Mindweaver
made, which names no agents. Last, where what was installed came
from, as `--from` takes it: `workspace`, `load:<path>` or a shelf source's
address. A project an older Mindweaver made, marked by a `mind.toml` or a
`settings.json`, gets a hint after the report naming [`mind
migrate`](#a-project-made-before-settings), which moves it to a
`settings.toml`; the exit status stays the report's. `mind status` says the
same of the project you are in, and `mindweaver status -p tanukeys` of one; as in
`list`, the project column is there only when there is more than one project
to tell apart, and the kind column only when there is more than one kind.

A rule the project received is read from what was installed and from its
`AGENTS.md`, and its agents are the ones it reaches: all of the project's
through the block, Claude alone for one about some files, which goes to
Claude's own rule files and nowhere else. One some of the project's agents
read nowhere is said, as for `collapse` given `git/commits` and
`style/comments` [above](#without-the-screen):

```
$ mind status
git/commits     current  claude,codex,opencode  workspace
style/comments  current  claude                 workspace
warning: codex and opencode do not read rule `style/comments` in collapse
```

A linked project whose directory has gone is listed as `stale`, with the
reason on stderr, and the rest of the report still prints; the exit status is
non-zero, as for anything that could not be read. So is a project that
[installs where some of its agents do not read](#installing-somewhere-else):
a warning names the agents that read none of its folders, and `unreached` in
the JSON says the same; and one given a rule some of its agents read nowhere,
or an MCP server some of them are given no entry of, which `missed` in the
JSON says.

`entries` in the JSON lists each MCP server's entry in the config files of
the project's agents (`.mcp.json` for Claude Code, `.codex/config.toml` for
Codex, OpenCode's `opencode.json` or whichever of its config files the
project has), one row an agent: the `agent`, the `file`, the server's
`name`, its `state` and, for one Mindweaver installed, its `origin`. A
server somebody keeps there that Mindweaver did not install is `foreign`,
and one it installed stands as an installed skill does, `edited` once what
the file holds under its name has changed. A config file holding an entry
Mindweaver installed that cannot be read is a warning beside the report,
the entry `missing`; one holding none it installed is passed over, a link
included, and nothing in it is listed. The text lists each entry as a row
of its own, kind `mcp`, with the agent whose file holds it, how it stands
and, for one Mindweaver installed, where it came from, in the place of its
installation's row: Claude's entry can be edited while Codex's is current.
A `foreign` one is listed so too, as a foreign copy is.

`--json` prints the same report in the envelope every reading command's
`--json` shares ([machine-readable output](#machine-readable-output)): the
data, the warnings the text says on stderr, each again with its code, and a
version, which moves if a field ever goes or changes meaning. The names
in it are a contract, spelled as a command line spells them; the version is 2
since `installed` became `current`, `outdated` or `orphaned`, and `partial`
became `missing`:

```
$ mindweaver status -p tanukeys --json
{
  "version": 2,
  "data": {
    "projects": [
      {
        "name": "tanukeys",
        "entry": "tanukeys",
        "path": "/home/you/Projects/tanukeys",
        "marker": "mind.toml",
        "agents": [],
        "stale": null,
        "unreached": [],
        "missed": [],
        "artifacts": [
          {
            "kind": "skill",
            "name": "commit-style",
            "state": "current",
            "agents": [],
            "origin": "load:../src/mindweaver/examples"
          }
        ],
        "entries": []
      }
    ]
  },
  "warnings": [],
  "coded_warnings": []
}
hint: tanukeys is marked by a mind.toml, which `mind migrate` moves to a settings.toml
```

### What no agent says

Some things keep an agent from what a project gives it, and the agent says
nothing: a Codex project nothing trusts reads none of its
`.codex/config.toml`, an `AGENTS.override.md` hides `AGENTS.md` from Codex, a
`CLAUDE.local.md` made after the rules were installed stops Claude reading
`AGENTS.md`, a skill at user scope holds something else under a project
skill's name, and an MCP server's variable is not set. `mindweaver doctor`
finds them in every project the workspace links, `-p` in one, and `mind
doctor` in the project you are in. Where [the status
above](#what-each-project-has) leaves the two, with the rule `git/commits`
installed into `collapse` since, a `CLAUDE.local.md` made there after it,
and a `code-review` of your own in `~/.agents/skills`:

```
$ cd ~/Projects
$ mindweaver doctor
collapse: /home/you/Projects/collapse/CLAUDE.local.md: Claude Code reads it in place of the AGENTS.md of /home/you/Projects/collapse, and nothing imports that file, so these rules in its block reach Claude Code nowhere: git/commits
  fix: install one of them again (`mind install rule:git/commits`), which renders every rule for Claude Code as the project stands, through an import of AGENTS.md or in .claude/rules/mindweaver
collapse: /home/you/Projects/collapse/.agents/skills/code-review: Codex and OpenCode also read a skill `code-review` at user scope, at /home/you/.agents/skills/code-review, which holds something else, and one name at both scopes resolves differently in each agent
  fix: rename the project's or the one at user scope, or make them hold the same

2 projects checked, 2 findings
```

Nothing is said of `tanukeys`, which a `mind.toml` marks and so names no
agent: only what an install left in it and what `status` says are looked at
there.

Each finding names the file it is about and what fixes it. What it checks,
each against [where the agents read](agents.md), for the agents a project
names:

- Codex: a project your Codex config marks `untrusted`, which loses its
  `AGENTS.md` and its `.codex/config.toml`, or marks neither way while it
  has a `.codex/config.toml`; an `AGENTS.override.md` beside `AGENTS.md`;
  no git repository, so Codex reads `AGENTS.md` only when started in that
  folder; and the `AGENTS.md` files from the repository's root down near or
  past the 32 KiB Codex reads.
- Claude Code: the rules of the block in `AGENTS.md` reaching it through
  nothing, since a `CLAUDE.md` or a `CLAUDE.local.md` in or above the
  project keeps it from that file and nothing imports it.
- Skills: copies of one name that differ in folders one agent reads, as
  OpenCode reads `.agents/skills`, `.claude/skills` and `.opencode/skills`;
  and a project skill whose name a skill at user scope holds too, with
  something else in it, in `~/.claude/skills`, `~/.agents/skills` or
  OpenCode's `skills`, where it reads both.
- MCP servers installed into the project: one of the same name in Codex's
  or OpenCode's own config, which each merges with the project's into one
  server, and each variable an input reads that is not set where `doctor`
  runs, a secret said as one. Only the names of the variables set are read,
  never a value.
- What an install that did not finish left, in `.mind/tmp` or beside a
  copy, which the next install clears; and what `status` says the
  project's agents miss.

It reads files and nothing else: no agent is run, nothing is written and
nothing is swept, and the agents' own folders are the ones [their options
say](#the-agents-own-folders). The exit status is non-zero once it finds
anything, as `validate` fails once something does not pass, so it works in
CI, and once something could not be read, which is a `warning:` on stderr as
anywhere else. `--json` prints the report in the envelope: `projects`, each
with its `entry`, `name` and `path`, its `findings`, each with its `code`,
the `agents` it keeps something from, the `path` it is about, its
`message` and its `fix`, and its `failures`, what could not be read, each
with its `code` and `message`.

## Machine-readable output

The commands that report what is there take `--json`, at either level:
`list`, `show`, `validate`, `templates`, `status` and `doctor`, and `mindweaver
workspaces`, `mindweaver scan` and `mindweaver gather list`. Each prints what it
read in one envelope, the same for every command, so a script or an agent
reads them all one way, and reads what a workspace holds, its own
artifacts and each project's, their templates and what it can install from
elsewhere, as every front end reads them. What it loads, the sources its
shelf is filled from and its history have no `--json` yet. Say `collapse`
holds the rule `git/no-force-push`, and a skill folder whose `SKILL.md` has
lost its front matter:

```
$ mind validate --json
{
  "version": 2,
  "data": {
    "artifacts": [
      {
        "kind": "rule",
        "name": "git/no-force-push",
        "path": "/home/you/Projects/collapse/rules/git/no-force-push.md",
        "project": "/home/you/Projects/collapse",
        "issues": [],
        "invalid": false,
        "copies_differ": false
      }
    ],
    "invalid": 0,
    "unreadable": 1,
    "differing": 0
  },
  "warnings": [
    "/home/you/Projects/collapse/skills/broken/SKILL.md: the file does not start with a `---` front matter fence"
  ],
  "coded_warnings": [
    {
      "code": "front_matter_missing",
      "message": "/home/you/Projects/collapse/skills/broken/SKILL.md: the file does not start with a `---` front matter fence"
    }
  ]
}
warning: /home/you/Projects/collapse/skills/broken/SKILL.md: the file does not start with a `---` front matter fence
```

`data` is what the command's text lists:

- `list`: each holder as every front end reads it, `library` for the
  workspace's own (`null` for one project's listing), with the `origin` a
  copy installed from it names and, in `holders`, each project holding a
  copy of one of them, by its entry, and `projects` for every entry the
  workspace links, a stale one saying why in `stale`. A
  project says where it authors each kind, its settings and templates, and
  every artifact it holds: its state, the agents that read it, where it came
  from, where each copy is, what `validate` says of it, and for a file that
  does not read, `unread`, with why, and in `entries` each MCP server's entry
  its agents' config files hold, as `status` lists them. Its `tally` counts
  the artifacts, by kind and by
  state, and its `health` says how it stands as a whole, in the words the
  [control panel](#control-panel) sorts by: `ok`, `attention`, `broken` or
  `stale`. `mind list --json` reads the project against the workspace that
  links it, as `mind status` does, to say how each stands; when that
  workspace cannot be read, the project is read alone and its `failures`
  say why, with no warning the text does not say. `list rules --json` holds
  rules alone, and its `tally` and `health` are of what it holds: a skill
  that does not read leaves a project `ok` there, which `list --json` calls
  `broken`.
- `show`: each artifact the name matches, with its file, what its front
  matter declares and its `body`.
- `validate`: each artifact checked, its `issues` (each with its `code`,
  whether it is only a `warning`, and its `message`) and whether it is
  `invalid`, and the counts the verdict follows.
- `templates`: each template, its `origin` the folder or file it is, `null`
  for one built in.
- `gather list`: what can be installed from elsewhere, each with the `place`
  it comes from (`loaded` or `shelf`) and its `origin`, as `--from` takes it,
  and for one from the shelf its `source`, by its `name` among the rest.
- `status`, `workspaces` and `scan`: the reports shown in [what each project
  has](#what-each-project-has), [the weavers this machine
  knows](#the-weavers-this-machine-knows) and [commands](#commands).
- `doctor`: what it found in each project, as [what no agent
  says](#what-no-agent-says) shows.

`warnings` holds what the text says on stderr as `warning:` lines, without
the prefix, and `coded_warnings` the same lines, in the same order, each
beside its `code`: a word for what went wrong that rewording the message
never changes, `null` for a line none is given to. Each is still said on
stderr too, as above, as the text says it, so piping the JSON into `jq`
leaves them on the terminal; a script reads them in the envelope and need
not read stderr, where a `hint:`, advice for whoever reads the terminal, is
said too and the envelope leaves it out. The exit status is the text's:
non-zero when something could not be read, as above, for `validate`
when something does not pass, and for `doctor` when it finds anything. An error that stops a command is still a line
on stderr, with nothing on stdout.

The names in it are a contract. `version` is 2, and version 2 only grows
additively: a field added leaves it at 2, so a script written for it passes
over a name it does not know, and only a field dropped, renamed or holding
something else would move it.

## Following a project that moved

The workspace knows each project by its route from the workspace, so moving
the repository leaves its entry `stale`, and the record of everything
installed into it naming a place nothing is at. What moves with it is the
`id` in its `.mind/settings.toml`, random, so that only a copy shares it,
which the workspace keeps beside the route it last saw it at. `mindweaver scan`
finds the project by it, and `mindweaver relink` follows it there. Say
`collapse`, with `commit-style` and `git/commits` installed into it from the
examples as [above](#try-it-with-the-examples), is the one project the
workspace links, and moves into `apps/`:

```
$ cd ~/Projects
$ mkdir apps && mv collapse apps/
$ mindweaver status
collapse  stale
warning: /home/you/Projects/collapse: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)
$ mindweaver scan
apps/collapse  not linked  moved: collapse -> apps/collapse

  follow one that moved with `mindweaver relink <old> <new>`
$ mindweaver relink collapse apps/collapse
relinked collapse as apps/collapse, from collapse
  2 installs kept with it
$ mindweaver status
skill  commit-style  current  claude,codex,opencode  load:../src/mindweaver/examples
rule   git/commits   current  claude,codex,opencode  load:../src/mindweaver/examples
```

The entry is rewritten where it stands in `weaver.toml`, comments and all,
and every record moves with it, so what was installed is current again rather
than somebody's. In a local project, what keeps those copies out of git goes
along: lines of `info/exclude` naming the old place are written for the new
one, in the same repository or the one it moved into, and a repository that
moved whole has them right already. `M` in [the TUI](#the-tui), on a stale
project the scan found, does the same.

`mindweaver relink` refuses, with nothing changed: a directory that holds another
id than the one the workspace last saw at the entry, which is some other
project; an entry that still holds a project, even one whose marker would not
open, which has not moved; a project a `mind.toml` marks, which has no id to
be found by ([`mind migrate`](#a-project-made-before-settings) gives it one);
and records from before ids were kept, which nothing says were that
project's. A workspace learns each linked project's id when its ledger is
made or brought up to this version, and again every time it writes, so a
project moved before then is found by nobody. `mindweaver scan` names the entry
it left instead, in a hint to `mindweaver relink <entry> <path>`, for you to say
where it went: the entry keeps its place, where `mindweaver link` would leave it
stale beside a second one.

A repository copied with its `.mind` holds the same id as the one it was
copied from, and nothing can tell which of the two the records were made for.
So the copy is never taken for a move: wherever two projects holding one id
are read together, by `mindweaver scan` or `mindweaver status`, it is a warning
naming both, and a non-zero exit, and `mindweaver relink` refuses to follow it.
`mind init --new-id` in the copy gives it an id of its own and keeps
everything else, its name included:

```
$ cp -r apps/collapse collapse-copy
$ mindweaver scan
apps/collapse  linked
collapse-copy  not linked  mind project

  link one with `mindweaver link <path>`
warning: /home/you/Projects/collapse-copy and /home/you/Projects/apps/collapse are both project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5, copied with its .mind; `mind -C <copy> init --new-id` gives a copy an id of its own
$ mind -C collapse-copy init --new-id
gave collapse a new id, 0123456789abcdef0123456789abcdef (it was 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5)
```

A local copy's lines in `info/exclude` follow its new id, and a workspace
that links it knows it, and what was installed into it, by the new one. A
project still marked by a `settings.json` moves to a `settings.toml` first,
with `mind migrate`.

## Keeping projects up to date

What was installed stays what it was when it went in, so a skill or a rule
changed where it is kept (the workspace's own, a workspace it loads, or the
shelf once a gather brings something newer) leaves every project it went into
behind it, and `status` says `outdated`. `mindweaver update` brings them up to
date from where each was installed from, every copy at once, and puts back
any copy that has gone, kept out of git again in a local project: one deleted
by hand, or an `AGENTS.md` Mindweaver made, which `git clean -X` takes. A
`git clean -dX` takes a local project's `.mind` along with its copies, so run
`mind init` there again first, with the agents and share mode it had, and the
update puts every copy back. Without `--yes` it says what it would do. Say a
pull of `~/src/mindweaver` has changed the examples' `commit-style`, and
somebody has written into `tanukeys`'s copy of it, with the rest as
[above](#what-each-project-has):

```
$ mindweaver status
collapse  code-review   missing   codex,opencode         load:../src/mindweaver/examples
collapse  commit-style  outdated  claude,codex,opencode  load:../src/mindweaver/examples
collapse  deploy        authored  -
collapse  notes         foreign   claude,opencode
tanukeys  commit-style  edited    -                      load:../src/mindweaver/examples
hint: tanukeys is marked by a mind.toml, which `mind migrate` moves to a settings.toml
$ mindweaver update --all
would put back code-review in collapse at .claude/skills/code-review, from load:../src/mindweaver/examples
would update commit-style in collapse, from load:../src/mindweaver/examples
nothing changed: add --yes to go ahead
warning: commit-style in tanukeys was changed since it was installed, at skills/commit-style, so it would be left alone; --force writes over it
$ mindweaver update --all --yes
put back code-review in collapse at .claude/skills/code-review, from load:../src/mindweaver/examples
updated commit-style in collapse, from load:../src/mindweaver/examples
warning: commit-style in tanukeys was changed since it was installed, at skills/commit-style, so it was left alone; --force writes over it
```

A copy somebody changed is never written over unforced, and neither is any
other copy of it, so the agents reading two folders never read two versions
of one skill. `--force` writes over it, saying where the change was:

```
$ mindweaver update -p tanukeys --force --yes
updated commit-style in tanukeys, writing over the change at skills/commit-style, from load:../src/mindweaver/examples
```

`-p` names a project, once for each, and `--all` every one the workspace
links; names after them update only those (`commit-style`, or
`rule:git/commits` when a skill and a rule share one), and one that names
nothing installed is a warning. `mind update` does the same in the project you
are in, and `u` in [the TUI](#the-tui) on a project, or on one artifact of
it. A project's rules are written into its `AGENTS.md` together, so updating
one brings its other outdated rules along, and says so, line by line. What
was installed from somewhere that no longer offers it is `orphaned`: there is
nothing to update it from, so it is said and left as it is, never a failure,
until it is uninstalled:

```
$ mindweaver unload ~/src/mindweaver/examples
unloaded ../src/mindweaver/examples
  from the workspace at /home/you/Projects
$ cd collapse && mind update
nothing to update in collapse
note: code-review in collapse is no longer offered by load:../src/mindweaver/examples, so it would be left as it is; uninstalling it takes it out
note: commit-style in collapse is no longer offered by load:../src/mindweaver/examples, so it would be left as it is; uninstalling it takes it out
```

An update copies what the shelf holds now: what a gathered source offers
moves when it is gathered again.

## A repository that already has .claude/skills

Many repositories keep skills written by hand in `.claude/skills` or
`.agents/skills`, committed with the code. Mindweaver lists each as
`foreign`, somebody else's, and never touches it: no update reaches it, and
an agent that reads the other folder never gets it. `mindweaver adopt` brings
one into the workspace's library, when you ask and never on its own, and
records the project's copies as installed from there, exactly as they are:
not a byte of the project is written. Say `collapse` is a local project that
keeps `notes` in `.claude/skills`, committed:

```
$ cd ~/Projects
$ mindweaver status -p collapse
notes  foreign  claude,opencode
$ mindweaver adopt -p collapse notes
would copy notes from collapse into the library, at /home/you/Projects/.mindweaver/library/skills/notes
would record notes in collapse as installed from workspace, at .claude/skills/notes
would record .agents/skills/notes missing, for `mindweaver update -p collapse` to put there
nothing adopted: add --yes to go ahead
note: git tracks .claude/skills/notes, so it stays in git though collapse is local: what an update writes there shows in `git status`
$ mindweaver adopt -p collapse notes --yes
copied notes from collapse into the library, at /home/you/Projects/.mindweaver/library/skills/notes
recorded notes in collapse as installed from workspace, at .claude/skills/notes
recorded .agents/skills/notes missing: `mindweaver update -p collapse` puts it there
note: git tracks .claude/skills/notes, so it stays in git though collapse is local: what an update writes there shows in `git status`
$ mindweaver update -p collapse --yes
put back notes in collapse at .agents/skills/notes, from workspace
$ mindweaver status -p collapse
notes  current  claude,codex,opencode  workspace
```

From then on `notes` is the workspace's own, managed in `collapse` like
anything installed there: current while nobody changes it, `edited` once
somebody does, and brought up to date by `mindweaver update` when the library
copy changes. The update above gave Codex, which reads only
`.agents/skills`, the copy it had never had; adopting itself writes nothing
into a project, so a folder an install would put a copy in that holds none
is recorded `missing` until then. Every other project can now be given it with
`mindweaver install -p <project> notes`.

A copy git tracks stays in git, even in a local project: a line in
`info/exclude` would keep none of it out, and would hide every file added
to it later. An update writes over it there, for the team to see and
commit. One git does not track is kept out of git in a local project, as
anything installed there is, and a local project no git work tree holds is
told, before and after, that nothing keeps it out.

Without `--yes` it writes nothing. It refuses, with nothing written: a name
the library holds already, saying both paths; copies in `.claude/skills`
and `.agents/skills` that differ, since which is the skill cannot be told; a
skill an install would refuse, its name not matching its folder or its
description empty or too long; one holding a link, or a `.claude-plugin`
folder at its top, which no install copies; one Mindweaver installed
already, or the project's own, in the folder it authors in; and a rule. If
its `SKILL.md` says where `gh skill` got it, under `metadata` keys starting
`github-`, the dry run and the adoption show it in a line under the
first, and nothing more is made of it: no other tool's lockfile is read or
written. `A` in [the TUI](#the-tui), on a `foreign` row, asks and then runs
the same line.

## The workspace's own skills and rules

A workspace keeps skills and rules of its own, for the projects it manages:

```bash
cd ~/Projects
mindweaver add skill commit-style "How every repo here writes commits"
mindweaver add rule git/no-force-push "Never force-push a shared branch"
mindweaver list
```

```
workspace  skill  commit-style       How every repo here writes commits
collapse   skill  deploy             Ship the service to staging
workspace  rule   git/no-force-push  Never force-push a shared branch
```

They live in the workspace's library, `.mindweaver/library/skills` and
`.mindweaver/library/rules`, inside `.mindweaver` where no agent looks: they
reach an agent by being installed into its project, never by sitting in a
directory above it. Its [MCP servers](#what-an-mcp-server-looks-like) are
kept there too, in `.mindweaver/library/mcp`, unless `weaver.toml` names a
folder for them, which no marker written before them does. `mindweaver init`
writes the first two into `weaver.toml`:

```toml
[directories]
skills = ".mindweaver/library/skills"
rules = ".mindweaver/library/rules"
```

`mindweaver init --skills DIR --rules DIR` says otherwise, as `mind init` does for
a project. Any folder inside the workspace will do except the shelf and the
cache, which are reserved for gathering, a folder inside them, or one holding
them, so `--skills .mindweaver/skills` and `--rules .mindweaver` are refused. A
workspace made before the library keeps them in `skills/` and `rules/` beside
its projects, because that is what its `weaver.toml` says, until `mindweaver
migrate --library` [moves them](#upgrading). Either way nothing is made until
the first one is added.

A project holds what is in its own folders, because that is what its agents
read, so the workspace's skills and rules reach a project by being installed
into it (`mind install`, `mindweaver install`, or the install screen) and are
managed there like anything else Mindweaver installed. Every command that
changes an artifact works on the workspace's own the same way it works on a
project's: `mindweaver edit`, `mindweaver rename`, `mindweaver remove`, and `-p <project>`
for one of its projects instead.

### Removing one everywhere

Removing one of the workspace's own takes it out of the library, and only
there: what was installed from it stays in each project, `orphaned`, since
nothing offers it any more. `mindweaver remove` says so, in the dry run and once
it is done:

```
$ mindweaver remove commit-style
warning: commit-style is installed in collapse and tanukeys, which removing it alone would leave orphaned there; --everywhere takes it out of them too
error: nothing removed: this deletes /home/you/Projects/.mindweaver/library/skills/commit-style and the 1 file in it; add --yes to go ahead
```

`--everywhere` takes it out of every project the workspace links first, as
`mindweaver uninstall` would, and then out of the library. Without `--yes` it
says what each project holds of it and how that stands, and writes nothing.
Here a copy in `tanukeys` was changed since it was installed:

```
$ mindweaver remove commit-style --everywhere
would remove commit-style from collapse, where it is current
would leave commit-style in tanukeys, where it is edited, at .agents/skills/commit-style
would remove skill commit-style from /home/you/Projects/.mindweaver/library/skills/commit-style and the 1 file in it
nothing removed: add --yes to go ahead
$ mindweaver remove commit-style --everywhere --yes
removed commit-style from collapse
left commit-style in tanukeys, where it is edited, at .agents/skills/commit-style
removed skill commit-style from /home/you/Projects/.mindweaver/library/skills/commit-style
note: nothing offers commit-style in tanukeys any more; `mindweaver uninstall -p tanukeys commit-style --force` takes it out
```

A copy that is current, outdated or missing one of its copies is
uninstalled. An edited one is left, every copy of it, as an uninstall leaves
it unforced: it is somebody's work, still Mindweaver's install, and offered
by nothing once the library copy is gone. Somebody's own skill of that name,
which Mindweaver never installed, is left too, and said as `foreign`. Only
the projects the workspace links are looked at. A rule goes the same way,
out of every project's `AGENTS.md`, and `d` on the `◆` row of [the
TUI](#the-tui) asks with a second yes, `e`, for everywhere.

What is orphaned already, by a removal without `--everywhere` or a workspace
that stopped being loaded, `uninstall --orphaned` takes out: of the projects
named, of every one with `--all-projects`, or of the one `mind` runs in. An
edited one stays until `--force`:

```
$ mindweaver remove commit-style --yes
removed skill commit-style from /home/you/Projects/.mindweaver/library/skills/commit-style
warning: commit-style is still installed in collapse and tanukeys, orphaned now that nothing offers it; `mindweaver uninstall --orphaned --all-projects` takes out what nothing offers
$ mindweaver uninstall --orphaned --all-projects
removed commit-style from collapse
warning: commit-style in tanukeys was changed since it was installed, at .agents/skills/commit-style, so it was left alone; --force takes it out
```

A copy from a loaded workspace that cannot be opened is not taken for
orphaned, though `status` says it is: what that workspace offers cannot be
told until it is back. Nor is one whose artifact is still there but cannot be
read, its front matter broken: that is a `warning:` to mend, never a reason
to take its copies out.

## The TUI

`mindweaver` on its own opens a screen over the whole workspace: its own artifacts
on the first row, marked `◆`, then every project it manages, and on the right
what the one under the cursor holds.

```
 mindweaver  Projects  · found above · 3 on the shelf  · /home/you/Projects
┏ Projects (3) ━━━━━━━━━━━━━━━━┓┌ collapse ──────────────────────────────────────────────────────────────┐
┃  ◆ Projects  2               ┃│>   skill commit-style       current  CC CX OC  How this repo writes com│
┃> collapse  2                 ┃│    rule  git/no-force-push  authored           Never force-push a share│
┃  tanukeys  1  ✗ 1            ┃│                                                                        │
┃  ! old-api                   ┃│                                                                        │
┃                              ┃│                                                                        │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└────────────────────────────────────────────────────────────────────────┘
q quit  ? keys  → open  a add  u update  m minds  l link  U unlink  v validate  g gather  L load  s shelf
every change is a command, printed when you quit; ? lists them
```

Each of a project's rows says what [`mindweaver status`](#what-each-project-has)
says of it: its state, then a badge for each agent that reads it, in a column
of its own (`CC` for Claude Code, `CX` for Codex, `OC` for OpenCode), blank
where that agent does not. The workspace's own are no project's, and their
row says neither.

`mind` on its own opens the same screen from inside a project: on that project,
if a workspace manages it, or on the project alone if none does, where `l`
links it to the workspace above.

Every key runs a command you could have typed, through the same parser:

| Key | Does | Runs |
|---|---|---|
| `a` | add a skill or a rule | `mind -C <project> add <kind> <name> <description>`; on the `◆` row, `mindweaver add …` |
| `enter` | show an artifact | `mind -C <project> show <kind:name>` |
| `e` | edit it: the TUI steps aside for your editor | `mind -C <project> edit <kind:name>` |
| `r` | rename it | `mind -C <project> rename <kind:name> <new>` |
| `d` | delete it, once you say yes; on the `◆` row, `e` says yes and takes it out of every project too | `mind -C <project> remove --yes <kind:name>`; on the `◆` row, `mindweaver remove --yes <kind:name>`, or with `e`, `mindweaver remove --everywhere --yes <kind:name>` |
| `m` | tick and untick every project under the workspace | `mind -C <path> init`, `mindweaver link <path>...`, `mindweaver unlink <path>...` |
| `u` | bring what is installed in the project up to date, or only the artifact under the cursor; on the `◆` row, in every project | `mind -C <project> update --yes [<kind:name>]`; on the `◆` row, `mindweaver update --all --yes [<kind:name>]` |
| `l` | link a project | `mindweaver link <path>` |
| `U` | unlink the project | `mindweaver unlink <path>` |
| `M` | on a stale project the scan found moved, follow it there | `mindweaver relink <old> <new>` |
| `A` | on a skill somebody keeps in a project's agents' folders, `foreign`, adopt it into the library, once you say yes | `mindweaver adopt -p <project> --yes <kind:name>` |
| `t` | toggle the project's agents and share mode, then apply | `mind -C <project> agents --yes <agent>...`, `mind -C <project> share --yes local\|shared` |
| `T` | type the folders the project installs skills in, or clear them for where its agents read | `mind -C <project> targets set --yes skills <dir>...`, `mind -C <project> targets reset --yes skills` |
| `v` | validate everything | `mindweaver validate` |
| `g` | gather from a git repository, showing how far it has got; `esc` stops it, until it starts placing | `mindweaver gather git <url>` |
| `L` | load another workspace's own skills and rules; enter on a loaded one unloads it | `mindweaver load <path>`, `mindweaver unload <path>` |
| `s` | what is on the shelf | `mindweaver gather list` |
| `w` | the weavers this machine knows, the one in use marked | `mindweaver workspaces` |
| `i` | install skills, rules and MCP servers from the workspace's own, a loaded workspace, or the shelf; `tab` switches between them, and `v` turns the screen round to every project for one of them | `mindweaver install` opens it; `a` runs `mindweaver install -p <project>... --from <origin> <kind:name>`, with `--yes` for an MCP server from elsewhere than the library once `a` has shown what each agent would run, and `mindweaver uninstall -p <project>... <kind:name>` |
| `R` | read the disk again | nothing: it changes nothing |

A project whose own commands would reach another workspace (linked from
outside the workspace's tree, or with a nearer workspace above it) runs `mind
-W . -C <project> …` instead, so the line acts on the workspace on screen
rather than on the one the project would find by itself. `mindweaver -W <dir>` opens the screen on the
workspace it names, and every line it runs uses that one; so does an
activated workspace. The header says which it is showing and why: `named
with -W`, `activated`, `found above`, or `default workspace`. It also counts
the projects an older Mindweaver made, still marked by a `mind.toml` or a
`settings.json`, and names the command that moves them, `mindweaver migrate
--all-projects`, or `mind migrate` for a project on its own
([upgrading](#a-project-made-before-settings)).

`m`, or `mindweaver minds` straight from the shell, opens a list of every
project the workspace links and every repository under it that it does not,
ticked where it does, in the order `mindweaver scan --suggest` gives: what is not
linked first, most worth linking first, each saying why, then what is.
`mindweaver minds ~/Work` looks for those under that folder instead, as
`mindweaver scan ~/Work` does, and says below the list what it could not read.
Tick and untick, then `a` applies all of it in one go, after saying which
repositories `mind init` will write into. `link` and `unlink` are all or
nothing: if one path is refused, none is changed.

That is the rule it is built by: **nothing the TUI does is out of reach of a
command.** A form shows the line it is about to run as you type, and when the
TUI closes it prints every line that changed something, with what each said,
so a session can be read afterwards, and repeated in a script:

```
$ mindweaver link tanukeys
linked tanukeys as tanukeys
$ mind -C tanukeys add skill deploy 'Ship the service to staging'
created skill deploy at /home/you/Projects/tanukeys/skills/deploy/SKILL.md
```

Opened where there is no workspace and no default one either (`--home ""`), it
offers to create one there, then goes straight to linking; with a default, it
opens that one and says so. It stays empty, and marked not made yet, until the
first change you make from it, which runs `mindweaver init` there first. The link
form lists what `mindweaver scan --suggest` finds under the workspace, or under
the folders `mindweaver minds` was given (mind projects, and git repositories that
are not one yet), in its order and with its reasons, and picking a repository
asks before running `mind init` in it, because that writes into it. A project
whose directory has gone is listed with a `!`, where `U` drops it, or, when
the scan found it [moved](#following-a-project-that-moved),
`M` follows it; an artifact `validate` would reject is marked `✗`, with the
reason underneath; and a project [`doctor`](#what-no-agent-says) finds
something in is marked `✚` and how many, its pane saying each with its fix.

## Control panel

`mindweaver ui` serves a control panel for the workspace from the binary itself,
and opens it in your browser. It runs until Ctrl-C:

```
$ mindweaver ui
http://127.0.0.1:40185/?token=bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513
note: serving the workspace at /home/you/Projects on 127.0.0.1:40185 until Ctrl-C
```

The link is the first line on stdout, and the only one while it serves, for
a script to read; what it prints once it stops (below) follows it there.
`--no-open` prints it without opening a browser, and a browser that cannot
be opened, as on a machine with no display, is a warning, not a failure.
`--token-file` writes where the API is and the token to a file of its own
as well, for a script that did not start the panel (below, with the HTTP
API, and in [the API's guide](api.md), which walks through it with curl).
`mind ui` is the same command from inside a project. Either serves the
workspace a command run there would use, decided once as it starts, from
`-C`, `--home`, `-W` and the agent homes, until you switch to another this
machine knows from the header (below): nothing else the browser sends
changes which one.

It listens on 127.0.0.1 only, on a port the system picks, or the one `--port`
names. The link carries a token made for this launch, which the browser
trades for a cookie on its first request, so the token leaves the address
bar. A request without it gets a 401, and a page on another site, or on
another port of your machine, cannot use the cookie to change anything
([what keeps other pages
out](architecture.md#what-keeps-other-pages-out)).

**Everything the panel changes, it changes by running a command**, as the
TUI does. It sends a task; the server turns it into the command lines you
could have typed, shows them before they run, and runs them through the same
parser, one at a time. A command that would open a screen, an editor or a
shell, or read standard input, is refused, and so is a path outside what
the workspace allows, each with a reason and a code. The first Ctrl-C (or,
on Unix, the `SIGTERM` `kill` sends) lets whatever is running finish, a
second stops at once, and on the way out it prints every command that
changed something, with what each said, as the TUI does when it closes:

```
$ mindweaver -W /home/you/Projects install -p /home/you/Projects/collapse skill:commit-style
installed commit-style into collapse, from workspace
```

**`--log`** says on stderr, as it happens, every request the panel
answered and how long the answer took; for each change, how long it waited
its turn behind the ones before it and how long it ran; and for each of its
commands, by its place among them, how long it took and how much of that
it waited for another writer to let go of a workspace, and which process
that was. Each line comes after the time of day it was said at, in UTC, to
read beside a browser's trace or CI's log of the same minutes:

```
04:20:59.460Z POST /api/v1/tasks/preview 200 1ms
04:20:59.757Z run: 0ms waiting its turn, 1ms working out its 1 line
04:20:59.761Z run: line 1 of 1 took 4ms, 0ms of it waiting for a workspace's lock
04:20:59.761Z run: 4ms running its 1 line
04:20:59.762Z POST /api/v1/tasks 200 6ms
```

It is for finding out why something took long. Each request is said by its
path and query, so a read's line names the folders and entries it asked
for, `GET /api/v1/folders?path=%2Fhome%2Fyou%2FProjects&hidden=false 200
1ms` for one, and a log is worth reading over before you pass it on. It
never holds the token, a header, or the body a change sent, and so not
which commands a change ran, which the transcript says.

**Every change is previewed first.** Whatever you ask the panel to change,
it asks the server what that is before anything runs, and shows you the
lines, exactly as the server will run them and as you could type them in
the workspace's folder: nothing runs until you press Run. Each names the
workspace with `-W` and its path, so it says which workspace it acts on,
and acts on that one whatever your shell has activated. A line that takes
something away makes that a question to confirm, saying what each line
takes in the CLI's own words: `remove --yes`, which deletes an artifact,
`uninstall` and `remove --everywhere`, which take installed copies out of
projects, `unlink` and `unload`, which make the workspace forget a project
or a loaded workspace, and `--force`, which writes over or takes out a copy
changed since it was installed. Which lines those are is the CLI's to say,
not the panel's, and Cancel has the focus, so an Enter pressed by habit
runs nothing. Once a run is over and the pages show what it changed, a toast
says how it went; when it failed, or said a warning, the drawer of what it
said opens, every warning, note and error under the line that said it, with
its code. The panel links projects, from the folder you type or browse to
above the projects table or from what **Discover** finds, relinks one
that moved, unlinks one from its page, changes a project's agents and
share mode, migrates a project an older Mindweaver made, installs and
uninstalls skills, rules and MCP servers from **Assign**, adds MCP servers
and changes how one is started from its **MCP servers** page, brings what is installed up to date and
takes out what nothing offers from **Updates**, removes one of the
library's own everywhere, its dry run first, edits, adds and renames the
library's own, adopts a skill somebody keeps in a project's agents'
folders, loads and unloads other workspaces from **Loads**, and switches
to another workspace from the header, and installs, updates all and links
from the command palette (below) too; everything else is still a command
away.

**The transcript**, in every page's header, lists every task the session
ran, from this tab or any other, each line with what it printed, and
**Copy as script** copies those lines, one to a line, exactly as they ran:
pasted into a shell in the workspace's folder, they do it all again. Each
names the workspace it ran against with `-W`, before a switch or after it,
and reads the rest of its words from the folder it is typed in, as any
line does: the entry an `unlink`, `unload` or `relink` names, or the file
a save reads. Across a switch, paste the lines run before it in that
workspace's folder and those after it in the other's. What `mindweaver ui`
prints when it stops is the same, but for the tasks that only read, such as
a `validate`.

**The overview** says which skills and rules your projects have at a
glance. It names the workspace, where it is and how it was chosen, and counts
what it holds: the projects it links and the entries that open none, each
folder once, the artifacts of each kind in its projects and in its library,
the installs that are outdated or edited, and what is on the shelf. A count
worth a look links to the projects it is about. Below, everything that could
not be read is listed under the project or the load it is about. A workspace
with nothing linked, nothing of its own or nothing gathered names the
commands that would change that.

**Projects** lists every entry the workspace links: its name, path, agents
and share mode, how many of each kind it holds, how many of what it was
given are `outdated`, a badge linking to **Updates**, its health, and why an
entry that opens nothing does not. The health is one word for the whole project:
`ok`; `attention` when something asks for a command (an install outdated,
edited, missing or orphaned, something one of its agents never reads, a
folder its targets name that no install uses, copies of one name that
disagree, a warning from `validate`); `broken` when something does not read
or does not pass `validate`; and `stale`. Click a column to sort by it, type
to filter by name or path, or show one health alone. The address keeps all
of it, so a link or a reload opens the table as you left it.

**The explorer** finds a folder for you to link, or for Discover to look
under, so you need not type its path. **Browse**, beside the field above
the projects table, opens it at the folder the field holds, or at the
workspace's own when it holds none. It lists the folders directly inside
one, in the order of their names, each saying what it is (a mind project,
by any of its markers, a weaver workspace, a git repository or a plain
folder), whether the workspace links it, and whether it is a link to a
folder elsewhere, which opens like any other; and says the same of the
folder open. Open a folder by its name, go back up by the trail above
the list, to your home or to the workspace's folder by their buttons, or
type a path and press Enter. Hidden folders are listed only once you
switch them on, which says how many there are; a `.mindweaver`, and a
link into one, are never listed, since nothing in one may be linked or
scanned. A path the panel may not name is refused with its code, as a
typed folder to link is, and the explorer stays where it was. **Choose**
(**Add**, from Discover), on a folder or for the folder open, puts its
path in the field and runs nothing: the link is shown first, as every
change is, and a scan waits for **Scan**. Each time it opens
it reads the folders again; it lists a thousand at most, and says so when
there are more. Only names are looked at: nothing is opened, written or
made, and no command has it, since a shell has `ls` and completion.

**A project's page** groups what it holds by kind. Each artifact says how it
stands, which of the project's agents read it (one that does not is crossed
out), where it came from, where its copies are, and what `validate` says of
it or why its file does not read. What some of its agents never read is a
warning above. A stale entry names `mindweaver relink` and `mindweaver unlink`, and a
kind the project holds none of names `mindweaver install -p` and `mindweaver add -p`.
Every command the panel names says `-W` and the workspace's path, so it acts
on the workspace the panel shows wherever you type it, for a project linked
from outside that workspace too; a workspace not made yet names `mindweaver init`
first, since `-W` never makes the one it names. **Unlink**, at the top,
asks first, and says that the workspace only forgets the project: nothing on
disk is deleted. A row that reads `foreign`, somebody's own in a folder an
agent reads, offers **Adopt**, `mindweaver adopt -p <project> --yes
<kind:name>` ([a repository that already has .claude/skills](#a-repository-that-already-has-claudeskills)), shown before it runs;
what Mindweaver will not adopt, a rule or a skill an install could not have
written, the line refuses, saying why.

Below what it holds, **Agents and sharing** are the project's settings as
toggles: a box for each agent that works in it, `local` or `shared`, and
whether a file git tracks may be edited to reach an agent. **Apply** runs
the lines they amount to, shown first, `mind agents --yes` naming the agents
ticked, those the project names already keeping their order, then `mind
share --yes`, each only when what it sets changes, as the TUI's toggles do;
what is installed is carried to where the agents now read. A project an
older Mindweaver made, marked by a `mind.toml` or a `settings.json`, has no
toggles until it moves to a `settings.toml`, and offers that move instead
([a project made before settings](#a-project-made-before-settings)): **See
what migrating does** runs the dry run, `mindweaver migrate -p <project>`, and
shows what it would write, move and leave, and only then **Migrate** runs it
with `--yes`. One a `mind.toml` marks takes every agent and stays shared, as
it was committed, and one a `settings.json` marks keeps the agents and the
share mode that file names; the toggles change either once it has moved.

**Discover** finds the projects worth linking. Type the folders your
repositories are kept in, one a line, each an absolute path, or add one
with **Browse**, which opens the explorer at the last folder named and
adds the one you choose, once; then how many levels down to look, empty
for three, and press **Scan**: the panel asks for what
`mindweaver scan --suggest` would find there, the workspace's own folder
offered to start from. It scans nothing until you ask, and nothing again by
itself while you look, since a scan reads far more of the disk than the
panel watches; a folder is only read. What is not linked comes first, most
worth linking first, each with why in the command's words: a mind project
with an id, one without, a git repository with the files that show an agent
works in it, any other git repository. Tick the ones you want and press
**Link what is marked**: the panel shows the lines first, a `mind -C <path>
init` for each repository that is not a mind project yet, which it names
before you ask since it writes into the repository, then one `mindweaver link`
of them all, the batch the TUI's minds screen runs for the same marks. The
panel names each folder by its absolute path, where the minds screen names
one inside the workspace by its route from there, so the lines read
differently and act on the same folders.
A project that moved, which the scan tells by its id, the one the workspace
last saw where the entry it left points, offers **Relink** instead, `mindweaver
relink <old> <new>`, which keeps what was installed into it; an entry whose id the workspace never
learned is named with the `mindweaver relink` that would follow it once you know
where it went. Below are the projects it found linked already. The folders
and the depth stay in the address, so a reload scans the same again.

**The library** has a page for each kind, `skill`, `rule` and `mcp`, listing the
workspace's own with what each is for, which projects hold a copy of it and
how each copy stands, and what `validate` says of it; type to filter by name
or summary. A kind it holds none of names `mindweaver add`.
**New skill** and **New rule**, at the top, open a page that adds one: its
name, its description and what it starts from, nothing or one of the
kind's templates ([from a template](#from-a-template)), judged as you type by what
`mindweaver add` would refuse, in its words, and by what `validate` would say
of it once written. **Add** runs `mindweaver add [--template <template>] <kind>
<name> <description>`, shown first, and opens what it added.

**MCP servers**, beside the library in the sidebar, is a page of the
library's MCP servers ([what an MCP server looks
like](#what-an-mcp-server-looks-like)). Each says, for Claude, Codex and
OpenCode, whether the agent is given it, `given whole`, `given in part`
when its entry is written without something, or `refused`, with each
reason by its code and in the words `show` says it in; the command it
would run or the address it would reach, as that agent's own config file
would spell it; and what the agent asks before it starts one: Claude asks
to approve it in a session, Codex starts it only in a project its user's
config marks trusted, and OpenCode asks nothing. Under each, every project
holding it, how its entry stands, and what `mindweaver doctor` finds there,
each by its code with what fixes it: a Codex project nothing trusts, a
variable not set where the agents run.

Below them, a form adds a server: its name and description, then how an
agent gets to it, a program it starts (the command, each argument, each
setting of its environment, each secret passed to it and each argument
read from a variable) or an address it reaches (the URL, server-sent
events or not, each header, each secret header, a bearer token and each
part of the address read from a variable). Every field is a value of an
option of `mindweaver add mcp`, and a secret only ever the name of the
environment variable an agent reads it from, which the form says no value
is stored for: no field in a secret's place takes its value. A setting of
the environment and a header are values written into the library as
typed, as `--env` and `--header` write them, which the form says no secret
goes in. As you type,
core judges it as `mindweaver add mcp` would, and says why it would refuse
it, in its words. **Add the server** runs the line, shown first:

```
$ mindweaver -W /home/you/Projects add --command npx --arg=-y --arg @acme/server --secret-env ACME_TOKEN mcp acme 'Acme tools'
created mcp acme at /home/you/Projects/.mindweaver/library/mcp/acme.toml
```

**Change how it starts**, on a server, opens the same form on how it is
started now, as core reads its definition back, and runs `mindweaver edit
mcp:<name>` with the options, which writes its transport and its inputs
again and keeps everything else its definition holds. A definition the
options cannot say all of (an input described or required, a program's
folder, a secret under a name of its own) is changed in its file instead,
which the server's **Edit its file** opens.

**One of the library's own** opens from its name. It shows what its front
matter declares, as a table, every key in the order written; its body,
rendered as markdown; every file it holds, each of which opens as the text it
is; what `validate` says of it; and the projects holding a copy, with the
state, agents and targets of each, or the `mindweaver install` that would put it
in one. What a skill holds is somebody else's, so the panel shows it and
never runs it: HTML in a `SKILL.md` is shown as the markup it is, an image is
named rather than loaded, and only a link to another site or a mail address
stays a link, opened in a tab of its own. No file is ever served as itself,
and a link in the skill that leads out of the folder the library keeps its
kind in is listed with why and never followed.

**Edit**, at the top, opens the file it is read from, its `SKILL.md` or a
rule's file, in an editor in the page, line numbers and markdown with its
front matter coloured, Ctrl-S to save and Tab to leave it. As you stop
typing, the text is judged as `mindweaver edit --contents` would judge it: why
it would refuse it, in its words (a name other than the skill's folder,
which `rename` changes, front matter that does not read, or a rule the
write is held to), and what `validate` would say of it once written, word
for word as `mindweaver validate` then prints it. **Save** runs, shown first:

```
$ mindweaver -W /home/you/Projects edit --contents .mindweaver/tmp/edit-f67e051f60a9675b.md --if-unchanged 8b7db6a4fb1255506c9f5ec0fa7e343f5a9cb6b047a69f44b7aae72f4317439d skill:review
edited skill review: ok
```

The browser sends the text, never a path: the file the line names is the
text, put in the workspace's `.mindweaver/tmp` for the line to read, named
by the first digits of its SHA-256, and taken away once the line has run,
whatever came of it. `--if-unchanged` is the SHA-256 of what the file held
when the editor read it, so a change made to it since, in an editor or from
a terminal, is never written over: the page says the file changed, and the
save is refused with `edit_stale`, the file left as the change left it,
until **Read it again** starts from what it holds now. A copy installed
from it then reads as outdated, for **Updates** to bring along.

Below its files, **Rename it** runs `mindweaver rename <kind:name> <new name>`,
shown first, and opens it under the name it has then.

At the bottom, **See what removing it everywhere does** runs the dry run of
`mindweaver remove --everywhere <kind:name>`, which deletes nothing, and shows
what it said: each project's copy that would go, with how it stands, a copy
changed since it was installed that would stay, somebody's own of the name,
which is never touched, and the library's copy. Only then does **Remove
everywhere** run the same line with `--yes`, a confirmation since it deletes
the artifact and uninstalls its copies, and go back to the library. The
panel never removes the library's copy alone, which would leave each copy
installed from it orphaned.

**Loads** lists the workspaces this one loads ([loading another
workspace](#loading-another-workspace)), each with its entry, where it is,
what `--from` takes for what it offers (`load:../team`), why one does not
open, and what it offers, each skill and rule by its name and what it is
for, with a link to **Assign** showing those alone. **Unload** runs `mindweaver
unload <entry>`, a confirmation, since the workspace then stops offering
them; what was installed from it stays where it is. Below are the
workspaces this machine knows ([the weavers this machine
knows](#the-weavers-this-machine-knows)), the default first, each saying
whether it is the default, this workspace, loaded already and by which
entry, or one that does not open, with how many projects it links. One the
workspace could load, which is none of this one, one it loads already and
one that does not open, offers **Load**, `mindweaver load <path>`; any other is
loaded by its absolute path, typed below, which the line refuses, saying
why, when no workspace is there or it is this one. Each is shown first, and
is the line the TUI's load form runs for the same workspace.

**The header switches workspace.** The header of every page names the
workspace the panel acts on and how it was chosen: `named with -W`,
`activated`, `found above` or `default workspace`, as the TUI's header says
it. Press it for every workspace you may switch to: those this machine
knows ([the weavers this machine knows](#the-weavers-this-machine-knows)),
the default first, and the one the panel was launched on, each with where
it is, whether it is the default or the one in use, which is checked, how
many projects it links, or why it does not open. Choose one and the panel
acts on it from then on, without a restart: every page shows it, starting
from its overview, and every line previewed names it with `-W`, so its
header then says `named with -W`, but for the one it was launched on,
which keeps how it was chosen then. Nothing is written: a switch is no
command, so the transcript has no line for it, and each line in it still
names the workspace it ran against. Only a workspace on that list can be
switched to, matched by its path as listed: the server refuses any other
path a script sends (`switch_unlisted`), and one that does not open, gone
since the list was read, with why (`not_a_workspace`), never making it. A
task shown in any tab is let go once the panel switched, since its lines
name the workspace before, in a tab hidden then as soon as it is shown
again, and so are the boxes marked on **Assign**; the
switch waits for whatever is running to finish first. Launched with `-W`,
the panel does not read `$MINDWEAVER_WORKSPACE`, as no command does once
`-W` is given, so an activated workspace is listed then only if this
machine knows it.

**The shelf** lists everything gathered, filtered by kind or by name,
summary or origin, each with the commit it was taken at, when, and its
origin as `--from` takes it: the address it came from, with `#` and the
branch when one was asked for. **Sources** lists where the shelf is filled
from, each with that same origin, the branch, the folder harvested, the
commit last gathered and what it supplied, and below them what happened
lately: the latest gathers, installs, uninstalls, relinks and adoptions the
ledger logged, newest first, with every URL in them shown without
credentials. An empty shelf names `mindweaver gather git`.

**Assign** is a matrix of everything there is to install against every
project: a row for each skill, rule and MCP server the library, a loaded workspace or
the shelf offers, under a heading for where it comes from, its origin as
`--from` takes it, and a column for each project, or turned round, **by
project**, with the origins over the columns. A box is ticked where the
project holds that offer, as `mindweaver install` would leave it: of two
places offering one name, the one its install came from, and every one
when somebody else's copy of the name is there. An icon over a box says
how the name stands in the project (`current`, `outdated`, `edited`,
`missing`, `orphaned`, `foreign`), and a box of a kind the project can take
none of, as a project naming no agent, is closed and says why. Type to
filter the offers by name, summary or origin, show one kind or one origin,
or the projects one agent works in; the address keeps all of it, and which
way round it is.

Press a box, or move to it with the arrow keys and press Space, to mark it
the other way round, and again to take the mark away. Whatever is marked
is planned at once, and the plan shows below: the lines that carry it out,
one `mindweaver install -p ... --from <origin> <kind:name>` for each offer
ticked, with every project ticking it, and one `mindweaver uninstall -p ...
<kind:name>` for each name unticked, an offer ticked in place of another of
its name replacing it rather than taking it out first; and what it holds
back, each on its box and listed with its code: a box whose name is
somebody's copy (`foreign`) or one changed since it was installed
(`edited`), which only `--force` on the command line reaches, and one name
asked of a project from two places (`wanted_twice`). **Apply** shows the
lines, a confirmation when one uninstalls, and runs them as one batch, the
install screen's; **Clear the marks** forgets them. An MCP server from a
workspace this one loads, or from the shelf, waits for a yes, as
`mindweaver install` does without `--yes`: the plan shows, under the lines,
what each agent of each project would run or reach, as the install
screen's confirmation does, and a box to say yes once you have read it.
Until it is ticked, the line installs nothing and says what it would;
ticked, the line carries `--yes`, and is shown before it runs like every
other. One of the library's own asks nothing first. A matrix of hundreds
of offers and projects draws only what fits on screen.

**Updates** is what `mindweaver update --all` would do, before it is told
`--yes`: every installation an update says something of, under its state,
`outdated`, `edited`, `missing` and `orphaned`, each counted, with its
project, its origin as `--from` takes it, and what an update does with it:
writes it again from its origin (`refresh`), puts back the copies named
(`put_back`), or leaves it, saying why in core's words and by a code, a copy
changed since it was installed (`edited`), an origin that offers it no more
(`no_longer_offered`), or one installed before origins were kept that two
places offer (`origin_unknown`), which `install --from` settles. An entry
that opens no project is said above. **Update all** runs `mindweaver update
--all --yes`, each project offers its own `mind -C <project> update --yes`
where an update writes something, and each row its change: **Update**, the
same line naming it; **Force update** for an edited copy, the line with
`--force`, a confirmation since it writes over the change; and
**Uninstall** for one nothing offers any more, `mindweaver uninstall -p
<project> <kind:name>`, beside **Uninstall every orphaned one**, `mindweaver
uninstall --all-projects --orphaned`. One nothing offers that was changed
since it was installed, as removing it everywhere leaves one, stands as
`edited`, and neither uninstall takes it out: its row offers **Force
uninstall**, the line with `--force`, a confirmation since the change goes
with it. Each line is shown first, as everywhere in the panel, and the
page is drawn again from what it changed: an edited copy an update left is
still listed, with why.

**The command palette** opens over any page on Ctrl+K, or Cmd+K on a Mac,
and from **Search** at the top of the sidebar. Type to find any page, a
project, one of the library's own, anything on the shelf and the sources it
came from: a word finds what holds it, a letter out of place forgiven, and
each word more narrows what the others found, the group holding the nearest
first. A search lists its twelve nearest, and with nothing typed each group
lists its first twelve, the projects to install into among them: type
enough to narrow it to the rest. Beside each is what it is, a project's
health, a skill's or a rule's kind, and its words are shown as written,
never as markup. Choosing one goes where its link on a page goes. It
changes things too, each as the page offering the same change does:

- **Install** one skill or rule, from wherever it is offered, then one of
  the projects whose box of it on **Assign** is neither ticked nor closed.
  That one box is planned, as ticking it would plan it, and what the plan
  says runs, `mindweaver install -p <project> --from <origin> <kind:name>`,
  replacing another of its name the project holds; what holds it back, a
  copy somebody wrote (`foreign`) say, is said with its code, and nothing
  runs.
- **Update all**, `mindweaver update --all --yes`, the button of **Updates**,
  offered whatever an update would do, which its line then says.
- **Link**, offered last for whatever is typed, `mindweaver link <path>`, the
  line of the form above the projects table, which refuses, saying why,
  anything but the absolute path of a folder.

Each shows its lines before it runs, as every change does. Escape, or Ctrl+K
again, closes the palette, and the focus goes back where it was; Backspace,
with nothing typed, goes back from the projects to install into.

**`?` lists the keyboard shortcuts**, every key the panel answers:

| Keys | Where | What they do |
|---|---|---|
| Ctrl+K, or Cmd+K on a Mac | anywhere | open or close the command palette |
| `?` | anywhere | show or hide the keyboard shortcuts |
| ↑ and ↓, Enter, Backspace, Esc or Ctrl+K | the command palette | move, choose, go back from the projects to install into, close |
| Tab, Enter, Esc | a change asked about | move between Cancel and Run, press the one focused, cancel with nothing run |
| the arrows; Page Up and Page Down | **Assign**'s matrix | move one box; ten rows |
| Home and End; Ctrl+Home and Ctrl+End | **Assign**'s matrix | the first or the last box of the row; of the matrix |
| Space or Enter | **Assign**'s matrix | mark the box the other way round, or take its mark away |
| Ctrl+S, or Cmd+S on a Mac | the editor | save, its line shown first |
| Tab | the editor | leave the editor |

Neither Ctrl+K nor `?` is taken while a field or the editor has the focus,
whose keys they stay (`?` is a character typed there), but for the
palette's own search, which Ctrl+K closes, nor while a change is shown or
runs, whose question is answered first. Where the browser gives Ctrl+K a
use of its own, such as its search bar, the panel takes it while its page
has the focus outside a field, or the palette is open.

**It follows the disk.** A change made from a terminal, the TUI or an
editor shows in the panel within a moment, without a reload: a skill edited,
a project linked, an install. The server watches what the workspace's
snapshot reads, its marker, its ledger, its library and its shelf, and in
each project its marker, the files its agents read and the folders it keeps
skills and rules in, each folder on its own and never a whole repository,
and tells every open tab when something there changed, which then reads the
workspace again. When the system will not let it watch everything, which on
Linux is the limit `fs.inotify.max_user_watches` sets, each page says so
above what it shows, with a button that reads everything again.

Every page works from the keyboard, its keys listed on `?`, and says every
state in words, never by colour alone.

**An HTTP API for scripts.** What the panel asks the server is an API under
`/api/v1`, there for any script on this machine holding the launch token,
the one in the link `mindweaver ui` prints, sent as `Authorization: Bearer
<token>`:

```sh
curl -H "Authorization: Bearer <token>" 'http://127.0.0.1:<port>/api/v1/project?entry=api'
```

[The API's guide](api.md) walks through it with curl on a small
workspace, every answer it shows the one the server gives, as a test holds
it; what follows here is the whole of it, in short.

**With no browser**, `--no-open` and `--token-file` serve the API to
scripts alone: nothing is opened, and where the API is and the token go to
the file named, as well as to the link. A script that did not start the
panel reads them there rather than from what `mindweaver ui` printed:

```
$ mindweaver ui --no-open --token-file /home/you/.local/state/mindweaver.json
http://127.0.0.1:40185/?token=bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513
note: serving the workspace at /home/you/Projects on 127.0.0.1:40185 until Ctrl-C
$ cat /home/you/.local/state/mindweaver.json
{
  "version": 1,
  "api": "http://127.0.0.1:40185/api/v1",
  "token": "bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513"
}
```

```sh
file=/home/you/.local/state/mindweaver.json
curl -H "Authorization: Bearer $(jq -r .token "$file")" "$(jq -r .api "$file")/snapshot"
```

The file is JSON, its names a contract as the API's are:

- `version`: how the file is laid out, `1`. A name dropped or renamed, or
  holding something else, moves it; one added does not.
- `api`: where every route is served, `http://127.0.0.1:<port>/api/v1`,
  the server's own address even with `--dev`.
- `token`: the launch token, to send as `Authorization: Bearer <token>`.

It is a new file, which only your account can read (mode 0600; on Windows,
whoever its folder lets in), written whole before the link is printed, so
a script that started the panel and waits for the link finds it whole.
`--token-file` writes over nothing and makes no folder: anything already at
the path, a file, a folder or a link, is left as it is and the panel is not
served, nor is it when the file's folder does not exist. When the panel
stops the file goes with it, unless it no longer holds what was written
there: it is then somebody's, left as it is, and a note says so. A script
that runs the panel in the background stops it with `kill`, whose
`SIGTERM` is taken as a Ctrl-C, and the file goes. A panel killed rather
than stopped (`kill -KILL`), or hung up (`SIGHUP`, which `nohup` ignores),
leaves it, holding a token that opens nothing any more, and the path is
refused until it is taken away. Each launch has a token of its own, so
name a file for each panel served at once.

A read is a GET naming what it reads in its parameters, and a read that
takes a structure is a POST that changes nothing:

- `GET /api/v1/snapshot`: everything the panel draws; `GET
  /api/v1/projects`, `GET /api/v1/library` and `GET
  /api/v1/project?entry=api`: its projects, its library and one project by
  its entry, each as the snapshot says it.
- `GET /api/v1/files?reference=skill:deploy` and `GET
  /api/v1/contents?reference=skill:deploy&file=SKILL.md`: what one artifact
  holds, and one file of it as text, with `&project=<entry>` for a project's
  rather than the workspace's own.
- `GET /api/v1/scan?root=/home/you/Projects&depth=2`: what Discover lists,
  one `root` for each folder to look under, the workspace's root without
  one.
- `GET /api/v1/folders?path=/home/you/Projects&hidden=true`: what the
  explorer lists of a folder, the workspace's without `path`.
- `GET /api/v1/updates`: what Update all would do, with nothing run.
- `GET /api/v1/workspaces`: what the header may switch to.
- `POST /api/v1/plan` with `{"selection": [{"project": "alpha",
  "reference": "skill:deploy", "origin": "workspace", "wanted": true}]}`:
  what boxes of a project and an offer, ticked or not, amount to, the tasks
  that carry them out beside what clashes and what is locked, with nothing
  run.
- `POST /api/v1/validate` with `{"holder": {"type": "workspace"}, "draft":
  {"type": "add", "kind": "skill", "name": "deploy", "description": "Ship
  a release", "template": null}}`: what Mindweaver says of an artifact
  about to be written, with nothing written.

Every change is a task, the command lines it is, as the panel's are: `POST
/api/v1/tasks/preview` for the lines a task is and what of them the panel
asks you to confirm (`{"lines": [...], "confirm": [...]}`), `POST
/api/v1/tasks` to run them, which asks nobody, answered with what each line
said, `POST /api/v1/switch` with `{"path": "/home/you/Notes"}` to switch to
a workspace `/api/v1/workspaces` lists, answered with its row, `GET
/api/v1/transcript` for everything the session ran, and `GET
/api/v1/events` for what happens as it happens, as server-sent events:
`resync`, to read everything again, first and whenever a reader fell
behind; right after it `watching`, or `unwatched` with a `reason` when the
server cannot watch everything, and either again whenever that changes;
`changed` when something watched changed; `command_started` and
`command_finished` around each run; and `switched` once the panel acts on
another workspace, to read everything again.

A POST sends its body as JSON, and says so in `Content-Type`, which curl's
`-d` alone does not (it says a form):

```sh
curl -H "Authorization: Bearer <token>" -H 'Content-Type: application/json' \
  -d '{"type": "link", "path": "/home/you/Projects/beta"}' \
  'http://127.0.0.1:<port>/api/v1/tasks/preview'
```

What goes wrong is an object of two fields, `error`, what happened in
words, naming the route, the parameter or the header, and `code`, one word
for it that never changes when the words do, which is what a script acts
on:

```sh
$ curl -H "Authorization: Bearer <token>" 'http://127.0.0.1:<port>/api/v1/project?entry=web'
{"code":"not_registered","error":"web: not registered in the workspace at /home/you/Projects"}
```

Its status says which kind of thing went wrong, and the code which one:

| Status | Code | What went wrong |
| --- | --- | --- |
| 400 | `malformed_json` | the body is not JSON |
| 400 | `body_unreadable` | the body could not be read whole |
| 400 | `body_refused` | the body was refused some other way |
| 400 | `unknown_parameter` | a parameter the route does not take |
| 400 | `repeated_parameter` | a parameter that takes one value, given twice |
| 400 | `malformed_parameter` | a value its parameter cannot read: `depth=deep`, `hidden=yes` |
| 400 | `missing_parameter` | a parameter the route needs, left out |
| 400 | `invalid_parameters` | parameters the route cannot read together |
| 401 | `token_required` | no launch token, or another server's (with `WWW-Authenticate: Bearer`) |
| 403 | `unknown_host` | the server named otherwise than as `127.0.0.1`, `localhost` or `[::1]` on its port |
| 403 | `foreign_origin` | sent from a page of another origin |
| 403 | `same_origin_required` | a change sent with the panel's cookie from anywhere but its page |
| 403 | `x_mindflayer_required` | a change sent with the panel's cookie without `X-Mindweaver: 1` |
| 404 | `unknown_route` | a path no route names, `/api` without its version among them |
| 404 | Mindweaver's | a read naming what is not there: `not_registered` for an entry, `not_found` for an artifact, `contents_missing` for a file, `browse_missing` for a folder |
| 405 | `method_not_allowed` | a method the route does not take (with `Allow`) |
| 413 | `body_too_large` | a body larger than the server reads |
| 415 | `json_required` | a body not sent as `application/json` |
| 422 | `invalid_body` | JSON that is not what the route reads |
| 422 | Mindweaver's | what Mindweaver will not do, by the code of its rule: `path_relative` for a path that is not absolute, `opens_editor` for a task that needs a terminal |
| 500 | `backend_failed` | Mindweaver failed while it answered |
| 500 | `writer_stopped` | the thread that runs tasks has stopped |

A task, a switch, a plan or a draft that names what is not there, an entry
no registry holds say, is a 422 rather than a 404: the route it was sent
to is there, and the code says the rest. Outside `/api`, what the server
answers is for a person in a browser and stays text: the panel's pages, and
the launch link's refusal of a token that is not this server's.

A route, a parameter or a field once there stays as it is under
`/api/v1`: one dropped, renamed or holding something else moves the API to
`/api/v2`. A code is never renamed, nor given to something else.

What each body and answer holds is described as JSON Schema (2020-12, the
dialect OpenAPI 3.1 takes), one file for each in the repository's
[`docs/api/schemas`](api/schemas), named as the type it describes:
`Snapshot.json` for what `/api/v1/snapshot` answers, `Task.json` for a
task as it is sent, `Ran.json` for what a run is answered with,
`Event.json` for each event, `Refused.json` for what goes wrong, and
`TokenFile.json` for the token file. An answer's schema leaves room for a
field it does not name, since one added keeps the version, so a script
that checks what it is answered with does not refuse an answer for one.

The API whole, every route with its parameters, its body, its answer and
each status it may be refused with, the codes of each listed, is described
as an OpenAPI 3.1 document, which the panel serves behind the token, as
every route, and the repository holds as
[`docs/api/openapi.json`](api/openapi.json):

```sh
curl -H "Authorization: Bearer <token>" 'http://127.0.0.1:<port>/api/v1/openapi.json'
```

Its definitions are those schemas, by the same names, but for a type the
API is both sent and answered with that is described otherwise each way, a
task above all: as it is sent, it is named with `Sent` after it
(`TaskSent`). What each event of `/api/v1/events` holds is under
`x-itemSchema` on that route, as OpenAPI 3.1 has no place for what each
item of a stream holds.

The panel is built into the binaries by a cargo feature, `ui`, which is on by
default. `cargo build -p mindweaver-cli --no-default-features` leaves it out,
and with it the server and what only the panel needs (axum, the embedded
files, the browser opener); `ui` is then no command.

## Where a project keeps its artifacts

`.mind/settings.toml` is the marker and the configuration; the artifacts
themselves sit beside the code, because these are files the **agents** read
and an agent does not know what a `.mind` is. `mind init` writes it:

```toml
# Mindweaver mind project: what Mindweaver needs to know about the project
# this .mind is in, and nothing else. No agent reads it.
#
# `name` is what the project is shown as, and `agents` the agents that work
# in it (claude, codex, opencode): what is installed into the project is
# copied where each of them reads. `mind agents` changes them.
#
# `share` is "local" to keep .mind, and what is installed, out of git
# for this checkout alone, or "shared" to leave both for the team to
# commit. `edit_tracked` lets a file git tracks, such as AGENTS.md, be
# edited to reach an agent in a local project. `mind share` changes both.
#
# `directories` says where the project keeps the skills and rules it writes
# itself, relative to the directory holding .mind, to commit with its
# code; a kind left out has no folder. A `[targets]` table, when there,
# names the folders a kind is installed in instead of where the agents
# read: `mind targets` writes it.
#
# `id` names the project for good, wherever it moves, and `version` is how
# this file is laid out: change neither.
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

It is TOML, as the workspace's `weaver.toml` is, and its comments say what
each key does. `agents` are the agents that work in the project, and so
where what is installed into it goes (`--agents` names them,
comma-separated); `share` says whether that is kept to this checkout or left
for the team to commit; `edit_tracked` whether a file the team tracks may be
edited to reach an agent; `directories` where the project keeps the skills
and rules it writes itself; and the `id` names the project wherever it
moves. A project that installs somewhere other than where its agents read
says where in `targets` too ([installing somewhere
else](#installing-somewhere-else)). Edit it by hand if you like:
`mind agents`, `mind share` and `mind targets` change only the keys they set,
and keep your comments, your layout and any key Mindweaver does not know.
Point the directories wherever the agents already look, and Mindweaver
follows:

```bash
mind init --skills .claude/skills
mind init --skills docs/skills --rules docs/rules
```

Both are relative to the project, and have to stay inside it: the artifacts
are the project's own, so `--skills /etc/skills` is refused rather than made.
So is a folder inside `.mind`, such as `--skills .mind/skills`: a local
project's `.mind` keeps itself out of git (below), and would take the
project's own skills with it.
`init` never overwrites an existing marker, so a second `mind init --skills
elsewhere` changes nothing and says so: moving a project's artifacts is not
something `init` should do behind your back.

The folders are written out even when they are the defaults, so the answer to
"where do this project's skills go" is in the file rather than in somebody's
memory. A kind `directories` leaves out has no folder at all, rather than the
default.

`.mind` keeps itself out of git, as a Python virtualenv does: `mind init`
writes a `.mind/.gitignore` holding `*`, so `git status` never offers the
settings and your own `.gitignore` needs no line for them. The skills and
rules beside the code are not kept out: they are the project's, to commit
with it. `mind init --shared` writes no `.gitignore` and sets `share` to
`shared`, for a team that commits its `.mind`, and what is installed into the
project with it, so that every clone gets the same.

A project an older Mindweaver made is marked by `.mind/mind.toml` instead,
which names the same folders in a table and was committed with the project,
or by `.mind/settings.json`, which holds these same keys in JSON. Mindweaver
still reads either the way it was written, and `mind init` leaves it as it
is. When a project holds more than one, it reads `settings.toml`, then
`settings.json`, then `mind.toml`. `mind migrate` moves it to a
`settings.toml` when you ask, and until then its settings are not changed
([upgrading](#a-project-made-before-settings)).

A project is listed from the folders its agents read as well (one an older
Mindweaver made, from the one folder its `mind.toml` names):
`.claude/skills` for Claude, `.agents/skills` for Codex and OpenCode. A skill
there is listed once, whoever put it there, and once with the project's own of
that name, which it reads as; when it sits in both folders with different
contents `list` and `validate` warn, since the agents would not all load the
same skill. Mindweaver changes only what sits in the folders the project
authors in, so `mind edit`, `rename` and `remove` reach the project's own copy
and leave a skill in an agent's folder alone.

What is installed into a project goes where its agents read, never into the
folder it authors in (one an older Mindweaver made takes it in that folder):
one copy in `.claude/skills` for Claude, one in
`.agents/skills` that Codex and OpenCode share, the same bytes, put in place
together or not at all, and recorded as one installation. A local project
keeps them out of git with a block of its own in the repository's
`.git/info/exclude`, never in your `.gitignore`, written before the copies
land and taken out after they go; a shared one leaves them for the team to
commit:

```
$ mindweaver install -p collapse commit-style
installed commit-style into collapse, from workspace
$ tail -5 collapse/.git/info/exclude
# mindweaver:begin
# project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5
/.agents/skills/commit-style/
/.claude/skills/commit-style/
# mindweaver:end
```

Outside a git repository a local install says nothing keeps its copies out
of git, as a `note:`. A skill's own `.claude-plugin` folder goes into no copy,
with a warning, since Claude would load the skill as a plugin, hooks and MCP
servers included. And a project at your home is refused an install, since its
`.claude/skills` and `.agents/skills` are the ones every project reads.

A rule goes into the block in the project's `AGENTS.md` instead ([the
kinds](#the-kinds)), rendered there with every other rule the project
has. An `AGENTS.md` Mindweaver makes for it is kept out of git by the same
block in a local project, and goes with the last rule; one that was there is
yours, and keeps everything outside the block. A local project writes into
one git tracks only with `edit_tracked` (`mind share local --edit-tracked`):
without it, Claude gets each rule from a file of its own in
`.claude/rules/mindweaver`, and the install warns that Codex and OpenCode read
none of them. A project an older Mindweaver made is given a copy of the rule
in the folder its `mind.toml` names, filed by its route.

### Changing its agents, or how it shares

When a project starts using Codex, stops using Claude, or moves between local
and shared, what is installed into it follows. `mind agents` changes the
agents it names (`mind agents claude codex` names exactly those, `add` and
`remove` change the ones it names), and says first what that does:

```
$ mind agents add codex
would set agents to claude,codex in collapse, where they were claude
would copy commit-style to .agents/skills/commit-style, for codex, kept out of git
nothing changed: add --yes to go ahead
$ mind agents add codex --yes
set agents to claude,codex in collapse, where they were claude
copied commit-style to .agents/skills/commit-style, for codex, kept out of git
$ mind agents remove claude --yes
set agents to codex in collapse, where they were claude,codex
removed commit-style from .claude/skills/commit-style
left .claude/skills/notes where it is: Mindweaver did not install it
```

Every installation is brought to where the agents the project names now
read: a folder a new agent reads gets a copy of one already there, the same
bytes, and a folder no agent it names reads any more loses Mindweaver's
copy. Only a copy nobody has changed since it was installed is taken out:
one that was edited is left where it is and becomes yours, and somebody
else's skill is never touched, wherever it is. One where a new copy would go
is left alone with a warning, since that agent then reads it rather than
what was installed; one in a folder no agent reads any more is left and
said, as `notes` is above. Running the same command again changes nothing
once it is done. A project has to name at least one agent, so removing the
last is refused, and so is a project an older Mindweaver made, marked by a
`mind.toml` or a `settings.json`, which `mind migrate` moves to a
`settings.toml` first. Its rules follow later, at the next install or
uninstall of one, which renders every rule the project has for the agents
and the share mode it has then.

`mind share` sets the share mode, and `edit_tracked` with it, off unless
`--edit-tracked` is given:

```
$ mind share shared --yes
set share to shared in collapse, where it was local
removed .mind/.gitignore, so .mind can be committed
stopped keeping /.agents/skills/commit-style/ out of git
```

Turning shared removes `.mind/.gitignore`, if it is still the one
Mindweaver wrote, and the project's whole group in `.git/info/exclude`, so
the settings and the copies are the team's to commit; turning local writes
both back, an entry for each copy installed that git does not track: one the
team committed stays in git. `mindweaver agents -p collapse ...` and
`mindweaver share -p collapse ...` do the same from the workspace, and `t` in
[the TUI](#the-tui) toggles them for the project under the cursor. A project
no workspace links still changes its settings, which are its own, and is told
that nothing installed in it was carried along.

### Installing somewhere else

The folders each agent reads are the default, since that is where every
agent finds a skill without being told. A project that keeps one folder of
skills for every agent says so in its settings, and what is installed into it
goes there instead, one copy for all of them:

```
$ mind targets set skills .skills
would set the targets of skills to .skills in collapse, where they were the folders its agents read
would copy commit-style to .skills/commit-style, for claude, codex and opencode, kept out of git
would remove commit-style from .agents/skills/commit-style
would remove commit-style from .claude/skills/commit-style
nothing changed: add --yes to go ahead
note: claude, codex and opencode would read none of the folders collapse would install skills in (.skills), so none of them would see what is installed there until pointed at one
$ mind targets set skills .skills --yes
set the targets of skills to .skills in collapse, where they were the folders its agents read
copied commit-style to .skills/commit-style, for claude, codex and opencode, kept out of git
removed commit-style from .agents/skills/commit-style
removed commit-style from .claude/skills/commit-style
note: claude, codex and opencode read none of the folders collapse installs skills in (.skills), so none of them sees what is installed there until pointed at one
```

The settings then end in a `[targets]` table holding `skills = [".skills"]`,
and name as many folders as you give it, each relative to the project. A
folder that leads out of the project, an absolute one, or one inside `.mind`
or `.git` is refused, and one written there by hand is given no copy and said
by `status`, by an install and in the TUI. Copies move as they do for [a
change of agents](#changing-its-agents-or-how-it-shares): only Mindweaver's
untouched ones, never over somebody's, and a dry run first.

No agent reads `.skills` until it is pointed there, so `status` warns until
it is. A link from a folder the agent reads is one way, and one the report
sees:

```
$ mind status
commit-style  current  -  workspace
warning: claude, codex and opencode read none of the folders collapse installs skills in (.skills), so none of them sees what is installed there until pointed at one
$ rm -r .claude/skills .agents/skills
$ ln -s ../.skills .claude/skills && ln -s ../.skills .agents/skills
$ mind status
commit-style  current  claude,codex,opencode  workspace
```

Every Mindweaver that reads a `settings.toml` reads `targets` too, so
setting them changes no version: a build from before them finds no project
in a `settings.toml` at all, rather than installing into the standard
folders behind your back. `mind targets reset` sends the kind back to where
each agent reads, and takes the table out. With the links taken out again,
and somebody's `notes` kept in `.skills`, it leaves `notes` where it is:

```
$ mind targets reset skills --yes
set the targets of skills to the folders its agents read in collapse, where they were .skills
copied commit-style to .agents/skills/commit-style, for codex and opencode, kept out of git
copied commit-style to .claude/skills/commit-style, for claude, kept out of git
removed commit-style from .skills/commit-style
left .skills/notes where it is: Mindweaver did not install it
```

`mindweaver targets -p collapse ...` does the same from the workspace, and `T` in
[the TUI](#the-tui) types the folders for the project under the cursor.

## What a skill looks like

`skills/commit-style/SKILL.md`:

```markdown
---
name: commit-style
description: How this repo writes commit messages and branch names
license: MIT
compatibility: Needs git 2.30 or later
metadata:
  author: platform-team
  version: "1.2"
allowed-tools: Read Grep Bash(git log *)
---

Use `<area>: <imperative summary>`, imperative and in English.
```

The front matter is the [Agent Skills](https://agentskills.io/specification)
format, which Claude Code, Codex and OpenCode all read:

| Key | | Rule |
|---|---|---|
| `name` | required | lowercase letters and digits joined by single hyphens, at most 64 characters, the same as its directory |
| `description` | required | 1 to 1024 characters |
| `license` | optional | a license's name, or a file the skill bundles |
| `compatibility` | optional | what it needs to run, in 1 to 500 characters |
| `metadata` | optional | strings mapped to strings |
| `allowed-tools` | optional | separated by spaces; commas still work, and a YAML list does too |

Anything else in the front matter (an agent's own keys, like Claude Code's
`paths`) is carried along untouched, and so is a `metadata` entry or a
`compatibility` that is not a string: `validate` says it is ignored, and it
stays in the file.

The name and the description are the rules Mindweaver will not break when it
writes a skill, because agents skip a skill that does: `mind add`, `rename` and
`install` refuse one that breaks either. Everything else is reported by
`validate`, and gathering is lenient about all of it.

```
$ mind show commit-style
commit-style
/home/you/Projects/collapse/skills/commit-style/SKILL.md

How this repo writes commit messages and branch names

allowed-tools: Read, Grep, Bash(git log *)
license: MIT
compatibility: Needs git 2.30 or later
metadata:
  author: platform-team
  version: 1.2

Use `<area>: <imperative summary>`, imperative and in English.
```

A rule is simpler, because it usually declares nothing at all. `rules/git/no-force-push.md`:

```markdown
# Never force-push a shared branch

Use `--force-with-lease`, which refuses when someone else has pushed.
```

Its name is `git/no-force-push`, from where the file sits. Listings show its
opening line, which is why leading with a heading or a one-line summary is
worth doing. Every name, declared or derived, is checked segment by segment:
lowercase letters and digits joined by single hyphens, each segment at most 64
characters.

A rule may open with front matter, as Claude Code's rules do to say which files
they are about. `rules/api/errors.md`:

```markdown
---
description: How errors leave the API layer
paths:
  - "src/api/**/*.{ts,tsx}"
---

# Return errors as values

Never throw across a handler boundary; return a `Result`.
```

| Key | | Rule |
|---|---|---|
| `paths` | optional | glob patterns, as a list or one on its own; quote a pattern that opens with `*`, which YAML reads as an alias |
| `description` | optional | a string |

The fences only open front matter when what they hold is a YAML mapping, so a
rule that opens with a `---` horizontal rule and a plain sentence reads as it
always did. A line shaped like `key: value` is a mapping, though, even when it
is prose such as `Note: ...`, so it is front matter and left out of what an
agent is given. Front matter that does not parse (prose opening with `**`
does not either) is read as content, fences and all, and `validate` warns
about it. It warns too about a `paths` or a `description` it cannot read,
which is ignored and left in the file; any other key passes through untouched.
Listings show the opening line below the front matter, and `show` prints what
it declares above that:

```
$ mind list rules
api/errors         Return errors as values
git/no-force-push  Never force-push a shared branch

$ mind show api/errors
api/errors
/home/you/Projects/collapse/rules/api/errors.md

How errors leave the API layer

paths:
  - src/api/**/*.{ts,tsx}

# Return errors as values

Never throw across a handler boundary; return a `Result`.
```

### Creating one

```bash
mind add skill deploy "Ship the service to staging"
mind add rule git/no-force-push "Never force-push a shared branch"
```

Each writes the least that passes `validate` (a skill's front matter and a
heading, a rule's opening line) in the directory the project keeps that kind
in. The name is checked before anything is written, by the rule `validate`
applies; a skill cannot be filed in a folder; and nothing already there is
written over, not even a folder that is not a skill. The description is one
line, because the rest belongs in the file.

A skill made this way is its author's, not Mindweaver's: `mindweaver install`
treats it like anything else it did not put there, and never overwrites or
removes it.

#### From a template

```bash
mind add skill deploy "Ship the service" --template full
```

`full` ships with Mindweaver: a manifest with sections to fill in, and the
`scripts/` and `references/` folders the skill format sets aside. Your own go in
`.mind/templates/` for one project, or in the workspace's
`.mindweaver/templates/` for every project it manages: a folder holding a
`SKILL.md` for a skill, a markdown file for a rule, and a definition for an
MCP server ([what one looks like](#what-an-mcp-server-looks-like)). `mind
templates` lists what is available and where each comes from; of two with
one name, the closer wins.
`mindweaver add -p` finds the workspace's wherever the project sits; `mind add`
finds those of [the workspace named or
activated](#choosing-a-workspace), or else the one above the project, or else
the one in your home.

A template is copied whole. In a skill's manifest, `name` and `description` are
set by `mind add`, whatever the template had for them, and `{{name}}` and
`{{description}}` in the body are filled in. Other files are copied as they are.
A rule template is filled in throughout, front matter included, and a
`description` its front matter declares is set by `mind add` in the same way.
One without `{{description}}` gets the description as a heading on top of its
body, below any front matter, which stays front matter. Front matter that
`validate` would warn about is refused, and nothing is written.

### Changing one

```bash
mind edit deploy              # $VISUAL, then $EDITOR, then vi
mind edit deploy --contents draft.md
                              # what draft.md holds, checked first
mind rename deploy ship       # the folder and the `name` inside, together
mind remove ship              # says what would be deleted, deletes nothing
mind remove ship --yes        # deletes it
```

All three find an artifact by what it declares, or else by where its name puts
it, so a skill whose front matter does not parse, which no listing shows, can
still be opened and fixed. `edit` checks the file once the editor closes, and
says what it broke. `rename` rewrites the `name` line and nothing else: the
comments, the key order and the description stay as they were. `remove`
without `--yes` is a dry run, because it is the one thing here that cannot be
taken back.

With `--contents`, `edit` opens no editor: what the file holds, or standard
input with `-`, takes the place of the skill's `SKILL.md` or the rule's
file, the rest of a skill's folder left as it is. It is checked first, so
what would break the artifact is never written: a skill's front matter has
to read, with a name and a description `add` would take, and the name has
to be its folder's, since `rename` is what changes the two together; a rule
has to say something, and its front matter, if it has any, to read whole.
`--if-unchanged` takes the SHA-256 of what the file held when the contents
were made from it, its 64 hex digits alone, which `sha256sum` prints
before the file's name (`sha256sum FILE | cut -d' ' -f1` prints them
alone), and refuses to write over a change made since; a value that is no
SHA-256 is refused as one:

```
$ mind edit deploy --contents draft.md
edited skill deploy: ok
$ mind edit deploy --contents renamed.md
error: the contents given for `deploy` call it `ship`: a skill's name is its folder's too, so it is changed with `mind rename`, which moves both, and nothing was written
```

The file is replaced through a temporary renamed onto it, and only where
the project keeps that kind, inside the project: a `SKILL.md` that links
out of its skill's folder or the skills folder is refused rather than
written through, and so is one under a folder linked out of the project,
as `.claude` can be. The workspace's own are held to the folder it keeps
them in, which may be linked elsewhere.

What `mindweaver install` put in an agent's folder is not changed from the
project: `edit`, `rename` and `remove` refuse a skill found only where its
agents read, which `mind uninstall` takes out. Where a copy sits in a folder
the project writes in too (one whose `--skills` is an agent's folder, or a
project an older Mindweaver made, whose one folder takes the copy), the
change reaches it. Edited or renamed, it stays Mindweaver's install, and
[`status`](#what-each-project-has) says it is edited: no install writes over
it and no uninstall takes it out unless forced. A rule renamed in a project
a `mind.toml` marks is moved as it is, so it still holds what was installed,
and `uninstall` takes it out unforced, as `rename` says. Removed, it is uninstalled,
so a skill you write later under the same name does not inherit a record
that was about something else. In `app`, made with `mind init --skills
.claude/skills` and linked:

```
$ mind install commit-style
installed commit-style into app, from workspace
$ mind rename commit-style commits
renamed skill commit-style to commits at /home/you/Projects/app/.claude/skills/commits
  it was installed by `mindweaver install`, and stays so under its new name; renamed, it is left alone by `mindweaver install` and `mindweaver uninstall` unless they are forced
$ mind status
commit-style  current  codex,opencode   workspace
commits       edited   claude,opencode  workspace
$ mind remove commits --yes
removed skill commits from /home/you/Projects/app/.claude/skills/commits
  it had been installed, and is uninstalled with it; `mindweaver install` can put it back
```

The copy in `.agents/skills`, which the rename did not reach, is still the
install as it was. The record changed is the one held by the workspace the
command works in: `mindweaver`'s, wherever the project sits, or for `mind` the
one named or activated, or else the workspace above the project, or else the
one in your home. Another workspace that links the same project keeps its
own (an [open question](architecture.md#open-questions)).

## What an MCP server looks like

An MCP server is one file in the workspace's library,
`.mindweaver/library/mcp/<name>.toml`, saying how an agent starts the server
or reaches it, in a format of its own that each agent's entry will be written
from. `add` writes one from how an agent starts the server, a program and
what it is started with:

```
$ mindweaver add mcp github "GitHub's issues, pull requests and code" \
    --command npx --arg -y --arg @modelcontextprotocol/server-github \
    --secret-env GITHUB_PERSONAL_ACCESS_TOKEN
created mcp github at /home/you/Projects/.mindweaver/library/mcp/github.toml
```

or from where an agent reaches it, an address and what every request to it
carries:

```
$ mindweaver add mcp linear "Linear's issues" --url https://mcp.linear.app/mcp \
    --bearer-env LINEAR_API_KEY
created mcp linear at /home/you/Projects/.mindweaver/library/mcp/linear.toml
```

A secret is given as the name of the environment variable an agent reads
it from when it starts the server (`--secret-env`, `--secret-header`,
`--bearer-env`), never as its value. `--env` and `--header` write what they
are given as it is: a value that looks like a token is refused, but nothing
tells another secret from a setting, so one given there is written into the
library. Every option takes a name or a value written as it is:

| Option | | Writes |
|---|---|---|
| `--command CMD` | a program | `type = "stdio"` and its `command` |
| `--arg A` | a program | one of its `args`, in order; again for each, and one led by a dash is taken as it is |
| `--env K=V` | a program | `K` in its `env`, `V` as written |
| `--secret-env VAR` | a program | `VAR = "{VAR}"` in its `env`, and a secret input read from `VAR`, which the program is given under that name |
| `--arg-env VAR` | a program | `{VAR}` after the `--arg`s, and an input read from `VAR`; no secret |
| `--url URL` | an address | `type = "http"` and its `url` |
| `--sse` | an address | `type = "sse"` instead, server-sent events |
| `--header K=V` | an address | `K` in its `headers`, `V` as written |
| `--secret-header H=VAR` | an address | `H = "{VAR}"` in its `headers`, and a secret input read from `VAR` |
| `--bearer-env VAR` | an address | `Authorization = "Bearer {VAR}"` in its `headers`, and a secret input read from `VAR` |
| `--url-var NAME=VAR` | an address | an input `NAME` read from `VAR`, written `{NAME}` in the URL where it goes; no secret |

What reaches a server through its environment or a header is a secret. What
goes into its arguments or its address is not: a process list or a log
shows it, and Codex reads no secret into either. An option of one transport
given with the other, both transports, or any of these options with
`--template` is refused before anything is read.

`github.toml`, as `add` wrote it:

```toml
# An MCP server, as Mindweaver keeps it: each agent's own entry for it
# is written from this file, and `show` says how each agent is given it.
format = 1
name = "github"
description = "GitHub's issues, pull requests and code"

# How an agent starts it: the program, what it is started with, and what
# its environment is given beside the agent's own.
[transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_PERSONAL_ACCESS_TOKEN = "{GITHUB_PERSONAL_ACCESS_TOKEN}" }

# What is supplied when an agent starts it, each written between braces
# where it goes and read from the environment variable it names: no value
# is written here, and a secret never.
[[inputs]]
name = "GITHUB_PERSONAL_ACCESS_TOKEN"
kind = "env"
secret = true
env_var = "GITHUB_PERSONAL_ACCESS_TOKEN"
```

`edit` writes how one is started or reached again from the same options,
the server's `transport` and `inputs`, and keeps the rest of what its
definition holds as it is: its name, its description, its provenance and
its extras, with their comments. What it writes is held to the rules `add`
holds a definition to, and it writes nothing when nothing changes:

```
$ mindweaver edit mcp:linear --url https://mcp.linear.app/sse --sse --bearer-env LINEAR_API_KEY
edited mcp linear: ok
```

A definition can be written by hand too, or changed with `mindweaver edit`
in an editor or with `--contents`, and holds:

| Key | | Holds |
|---|---|---|
| `format` | required | `1`; a definition written in a newer format is refused, and said to be |
| `name` | required | the name agents will key it by, the same as its file's: letters, digits, hyphens and underscores, none an agent keeps for itself, and no other beside it that differs only by case |
| `description` | required | what it is for; its first line is what a listing shows |
| `transport` | required | `type = "stdio"`, a program, with `command`, and optionally `args`, `env` and `cwd`; or `type = "http"` or `"sse"`, an address, with `url` and optionally `headers` |
| `inputs` | optional | what somebody supplies when the agent starts the server: each a `name` (letters, digits, hyphens and underscores), a `kind` (`env`, `header`, `arg` or `url_var`), and optionally a `description`, `required`, `secret` and the `env_var` it is read from, which a secret has to name |
| `provenance` | optional | `registry`, `upstream_name`, `version` and `fetched_at`, for one imported from a registry |
| `extras` | optional | a table for each agent, `[extras.claude]`, passed on to its entry as it is |

An input's value is never in the file: write the input's name between braces
where it goes, `{token}`, in a value of `env` for an `env` input, one of
`args` for an `arg`, a value of `headers` for a `header`, or the `url` for a
`url_var`, and the agent will read it from the environment variable when it
starts the server. A brace naming no input is left as it is, so an argument
such as `{"debug":true}` means what it says. Any other key is carried along
untouched.

A secret is never written in the file either: an input with `secret = true`
names the `env_var` it is read from, and each agent's entry will refer to
that variable in the agent's own way. A value of `args`, `env`, `headers` or
`url`, or of `extras`, holding what looks like a token (a `Bearer` value, or
one starting `sk-`, `ghp_` or `github_pat_`) outside an input's braces is
refused, and so is one written as an `env_var`, where the variable's name
goes, or as an input's name: each is said by where it is and what it starts
with, never by the token.

`add` holds what it writes to every one of those rules before a byte is
written, and refuses whatever `validate` would call invalid: a name an
agent keeps for itself, one beside another that differs only by case, a
value shaped like a token (`--env API_KEY=sk-…`, `--header
Authorization=Bearer …`), a variable no agent can refer to, a part of the
address the URL does not write. Beside them, it refuses what no file
should hold: a key given twice, and an address typed with a credential,
which it says without it; either, holding what looks like a token, is said
by what that starts with. An option taking `NAME=VALUE` given no `=`
(`--header "Authorization: Bearer …"`, say) is refused without repeating
what it was given. Given neither a program nor an address, it says what it
needs:

```
$ mindweaver add mcp computer-use "Ours" --command npx
error: `computer-use` is a name Claude Code keeps for a server of its own (`computer-use`), so no server of yours can be keyed by it
$ mindweaver add mcp leaky "Leaky" --command npx --env A=1 --secret-env A
error: `A` is given twice for the transport's `env`, which holds each key once
$ mindweaver add mcp leaky "Leaky" --url https://me:hunter2@example.com/mcp
error: `https://example.com/mcp` was typed with something before its host, which a definition never keeps and no agent is given: send a credential in a header read from an environment variable instead (`--secret-header` or `--bearer-env`)
$ mindweaver add mcp jira "Jira's issues"
error: an MCP server is added with how an agent starts it, `--command`, or where it reaches it, `--url`, or from a `--template` that says so; or write its definition as <name>.toml in /home/you/Projects/.mindweaver/library/mcp
```

A definition in a templates folder, `.mindweaver/templates/<template>.toml`
or a project's `.mind/templates/`, is a template for one: `add mcp
--template` takes its transport and its inputs as they are, sets its `name`
and `description`, and keeps every other byte, comments included; what it
writes is held to the same rules. With `remote.toml` there, a copy of
`linear.toml`:

```
$ mindweaver templates
full    skill  built in
remote  mcp    .mindweaver/templates/remote.toml
$ mindweaver add mcp sentry-remote "Sentry, reached" --template remote
created mcp sentry-remote at /home/you/Projects/.mindweaver/library/mcp/sentry-remote.toml
```

They are listed beside the skills and rules, and narrowed with `mcp`:

```
$ mindweaver list
skill  commit-style       How every repo here writes commits
rule   git/no-force-push  Never force-push a shared branch
mcp    github             GitHub's issues, pull requests and code
mcp    linear             Linear's issues

$ mindweaver list mcp
github  GitHub's issues, pull requests and code
linear  Linear's issues
```

`show` says what one is for, how an agent starts or reaches it, what is
supplied for it, each input by the variable it is read from, and how each
agent will be given it; `--json` says the same under `transport`, `inputs`
and `expressibility`, beside the file whole as `body`:

```
$ mindweaver show github
github
/home/you/Projects/.mindweaver/library/mcp/github.toml

GitHub's issues, pull requests and code

transport: stdio
  command: npx
  args:
    - -y
    - @modelcontextprotocol/server-github
  env:
    GITHUB_PERSONAL_ACCESS_TOKEN: {GITHUB_PERSONAL_ACCESS_TOKEN}

inputs:
  GITHUB_PERSONAL_ACCESS_TOKEN: env, secret, read from $GITHUB_PERSONAL_ACCESS_TOKEN

agents:
  claude: given it whole
  codex: given it whole
  opencode: given it whole
```

`validate` holds each to its format: a name that is its file's, a transport
that names what it needs to start or reach the server, and every input used
where its kind says. `sentry.toml`, written by hand, declares an `arg` input
no argument writes:

```
$ mindweaver validate mcp
github: ok
linear: ok
sentry: 1 problem
  - input `token` is used nowhere: an input of kind `arg` is written `{token}` in one of the transport's `args`

3 MCP servers checked, 1 invalid
```

And to the names agents take and the secrets a file holds: a name an agent
keeps for itself, in any case (Claude Code's `workspace`,
`claude-in-chrome`, `computer-use`, and `Claude Preview` or `Claude Browser`
spelled `claude_preview`, `claude-preview` or `claudepreview`; OpenCode's
`servers` and `timeout`), two names beside each other that differ only by
case, which a file system that folds case holds as one file, a secret
naming no `env_var`, and a token written as it is are each a problem;
`anthropic-skills`, which Claude Code gives Anthropic's own skills, is only
a warning. A copy of `github.toml` written as `computer-use.toml`, its
`name` changed with it:

```
$ mindweaver validate mcp:computer-use
computer-use: 1 problem
  - `computer-use` is a name Claude Code keeps for a server of its own (`computer-use`), so no server of yours can be keyed by it

1 MCP server checked, 1 invalid
```

Not every agent can be given every server. Codex has no way to read a
secret argument, a secret inside a URL, a secret header but its whole
value or `Authorization: Bearer` and the token, or a secret in a program's
environment but the whole value of the variable named as the one it is read
from (`GITHUB_PERSONAL_ACCESS_TOKEN = "{token}"` read from `GITHUB_TOKEN`
is one it cannot; `--secret-env` always writes one it can); neither Codex nor
OpenCode takes a server over sse; Claude starts a program in no folder it
is told, so its `cwd` is left out there; and Claude reads a credential of
its own or of a tool on the machine (`ANTHROPIC_API_KEY`, `NPM_TOKEN`,
`HTTPS_PROXY` and the like) as empty in a remote server's address and
headers, so it is given no server whose address or headers are read from
one, nor one that writes `${` as it is where no input stands, which it
would read as a variable. OpenCode, in the same way, fills `{env:NAME}`
and `{file:path}` in over its config, so it is given no server that writes
either as it is where no input stands. `show` says so of each, and
`--json` says it by a code under `expressibility`. The server is no less
valid for it, and each agent that can be given it will be: `notion`,
reached over sse, reaches Claude alone:

```
$ mindweaver add mcp notion "Notion's pages and databases" --url https://mcp.notion.com/sse \
    --sse --bearer-env NOTION_TOKEN
created mcp notion at /home/you/Projects/.mindweaver/library/mcp/notion.toml
$ mindweaver show notion
notion
/home/you/Projects/.mindweaver/library/mcp/notion.toml

Notion's pages and databases

transport: sse
  url: https://mcp.notion.com/sse
  headers:
    Authorization: Bearer {NOTION_TOKEN}

inputs:
  NOTION_TOKEN: header, secret, read from $NOTION_TOKEN

agents:
  claude: given it whole
  codex: given none of it:
    - the sse transport, server-sent events, is not one taken there
  opencode: given none of it:
    - the sse transport, server-sent events, is not one taken there
```

`mindweaver rename mcp:github octocat` moves the file and rewrites its `name`
and nothing else, and `mindweaver remove mcp:github --yes` takes it away;
`mindweaver edit` opens it, and `--contents` refuses a definition that does
not read or names another server.

Installing one into a project writes it into the config file of each agent
the project names, an entry each, every other server and setting there kept,
all of them or none. Its name is enough, as a skill's is, or `mcp:github`
when a skill or a rule shares it. What the install says under its line is
what each agent now runs, or the address it reaches, spelled as that agent
is given it, then the variables to set where the agents run, and what each
agent asks before it starts the server:

```
$ mindweaver install -p collapse github
installed mcp github into collapse, from workspace
  claude runs: npx -y @modelcontextprotocol/server-github
  codex runs: npx -y @modelcontextprotocol/server-github
  opencode runs: npx -y @modelcontextprotocol/server-github
  set GITHUB_PERSONAL_ACCESS_TOKEN (a secret) where the agents run: each reads it there, and no file holds it
  claude asks to approve it before it starts it in a session, though a run with `-p` starts it unasked
  opencode starts it without asking
warning: Codex reads `github` from /home/you/Projects/collapse/.codex/config.toml only in a project it trusts, and /home/you/.codex/config.toml does not mark /home/you/Projects/collapse `trusted`
```

Codex reads a project's servers only once its own config marks the project
trusted, which is yours to do, so its entry is written all the same and the
warning says why Codex will not start it yet. An input reaches Claude as
`${VAR}` and OpenCode as `{env:VAR}`, which is what their lines show. An
agent that cannot be given the server, or whose file git tracks in a local
project without `edit_tracked`, is passed over and said as a warning with
the code of why, and the rest are given it; one that reaches none of them is
refused, saying why of each. A server of the name somebody keeps in one of
those files refuses the install, as a skill somebody keeps does, and so does
an entry changed since it was written, until `--force`. In a local project,
a config file the install makes is kept out of git, and goes with its last
server.

A server from a workspace this one loads, or from the shelf, is a command
somebody else wrote that every agent in the project would run, so without
`--yes` the install says what each agent would run and writes nothing,
`would update` when the project has it already, and that it is already
current, with nothing to say yes to, while each agent holds what it would
be given:

```
$ mindweaver install -p collapse linear
would install mcp linear into collapse, from load:../team
  claude reaches: https://mcp.linear.app/mcp
  codex reaches: https://mcp.linear.app/mcp
  opencode reaches: https://mcp.linear.app/mcp
nothing installed: it comes from elsewhere than this workspace's library, so add --yes to go ahead
warning: Codex reads `linear` from /home/you/Projects/collapse/.codex/config.toml only in a project it trusts, and /home/you/.codex/config.toml does not mark /home/you/Projects/collapse `trusted`
$ mindweaver install -p collapse linear --yes
installed mcp linear into collapse, from load:../team
  claude reaches: https://mcp.linear.app/mcp
  codex reaches: https://mcp.linear.app/mcp
  opencode reaches: https://mcp.linear.app/mcp
  set LINEAR_API_KEY (a secret) where the agents run: each reads it there, and no file holds it
  claude asks to approve it before it starts it in a session, though a run with `-p` starts it unasked
  opencode starts it without asking
warning: Codex reads `linear` from /home/you/Projects/collapse/.codex/config.toml only in a project it trusts, and /home/you/.codex/config.toml does not mark /home/you/Projects/collapse `trusted`
```

An input read from no variable cannot be given to any agent as a reference,
and no install hands one a value yet, so each agent it would reach says so,
by its code. `update` writes a changed definition into every entry as an
install does, saying the same lines: without `--yes`, what each agent would
be given, under each `would update`, and with it, what each now is.
`status` lists each entry an agent each, how it stands and where it came
from, beside a server somebody keeps in one of those files, `foreign`:

```
$ mindweaver status -p collapse
github  current  claude    workspace
linear  current  claude    load:../team
github  current  codex     workspace
linear  current  codex     load:../team
github  current  opencode  workspace
linear  current  opencode  load:../team
```

A server one of the project's agents is given no entry of is a warning
beside it, as a rule some of them read nowhere is.
`mindweaver uninstall -p collapse linear` takes out each entry it wrote, and
nothing else:

```
$ mindweaver uninstall -p collapse linear
removed mcp linear from collapse
```

`rename`, `edit` and `remove` aimed at a server a project received are
refused there: uninstall it, or change its definition in the library and
`update`.

A project keeps MCP servers of its own only where its settings name a folder
for them, under `[directories]` as `mcp = "mcp"`, and `mind add mcp` writes
one there; `mind init` names none, so an `mcp/` holding a server's code is
not read, and adding one there says how:

```
$ cd collapse && mind add mcp github "GitHub" --command npx
error: /home/you/Projects/collapse: keeps no MCP servers of its own, since its settings.toml names no folder for them; its `[directories]` can name one
```

A project an older Mindweaver made keeps none either: one a `mind.toml`
marks whatever folders it has, and one a `settings.json` marks unless its
`directories` name a folder for them, as somebody may have written there by
hand. Adding one to either otherwise points at `mind migrate` first.

## Upgrading

A newer Mindweaver reads what an older one wrote exactly as it was written. It
changes a marker only when you tell it to, and the database only after
copying it aside ([what a workspace keeps](#what-a-workspace-keeps)).

### From Mindflayer

Mindweaver was called Mindflayer, and its second command `flayer`: what
was `flayer <cmd>` is `mindweaver <cmd>`, what was `mind flayer <cmd>` is
`mind weaver <cmd>`, and a flayer workspace is a weaver workspace. A script
that runs `flayer` needs the new name. The binaries an older build
installed are yours to remove: [getting started](#getting-started) says
how.

What a Mindflayer build wrote on the disk keeps working as it was written.
A workspace it made is a `.mindflayer` holding `flayer.toml` and
`mindflayer.db`: it is found walking up and, while no `~/.mindweaver` is
there, as the default in your home, `~/.mindflayer`, and it is written
under those names, so nothing makes a `.mindweaver` beside it. The block it
wrote in a project's `AGENTS.md` and the import in its `CLAUDE.md`
(`<!-- mindflayer:begin -->`), and the block in git's exclude file
(`# mindflayer:begin`), are read and rendered again under their own
markers, and a rule file it gave Claude in `.claude/rules/mindflayer` stays
there; only a block, or a rule file, made from now on is Mindweaver's. A
shell its `activate` set up keeps its workspace: `$MINDFLAYER_WORKSPACE`
and `$MINDFLAYER_HOME` are taken while `$MINDWEAVER_WORKSPACE` and
`$MINDWEAVER_HOME` are not set at all, each said in a note, and
`mindweaver activate` runs the old `flayer-deactivate` before it activates
anything. Every command run on such a workspace ends with a hint:

```
$ mindweaver status
review       current  claude,codex,opencode  workspace
rust-errors  current  claude                 workspace
warning: codex and opencode do not read rule `rust-errors` in app
hint: the workspace at /home/you/Projects was made by a Mindflayer build, and is kept under its names; `mindweaver migrate --names` moves it, and what it wrote in its projects, to Mindweaver's
```

`mindweaver migrate --names` moves it, with what it wrote in the projects
it links. On its own it changes nothing and says what `--yes` would do:

```
$ mindweaver migrate --names
would move /home/you/Projects/.mindflayer to /home/you/Projects/.mindweaver
would rename /home/you/Projects/.mindflayer/flayer.toml to /home/you/Projects/.mindweaver/weaver.toml
would rename /home/you/Projects/.mindflayer/mindflayer.db to /home/you/Projects/.mindweaver/mindweaver.db
would point skills in the marker at .mindweaver/library/skills
would point rules in the marker at .mindweaver/library/rules
would spell the markers of the block in /home/you/Projects/app/AGENTS.md as Mindweaver's
would move /home/you/Projects/app/.claude/rules/mindflayer/rust-errors.md to /home/you/Projects/app/.claude/rules/mindweaver/rust-errors.md
would spell the markers of the block in /home/you/Projects/app/.git/info/exclude as Mindweaver's
nothing moved: add --yes to go ahead

$ mindweaver migrate --names --yes
moved /home/you/Projects/.mindflayer to /home/you/Projects/.mindweaver
renamed /home/you/Projects/.mindflayer/flayer.toml to /home/you/Projects/.mindweaver/weaver.toml
renamed /home/you/Projects/.mindflayer/mindflayer.db to /home/you/Projects/.mindweaver/mindweaver.db
pointed skills in the marker at .mindweaver/library/skills
pointed rules in the marker at .mindweaver/library/rules
spelled the markers of the block in /home/you/Projects/app/AGENTS.md as Mindweaver's
moved /home/you/Projects/app/.claude/rules/mindflayer/rust-errors.md to /home/you/Projects/app/.claude/rules/mindweaver/rust-errors.md
spelled the markers of the block in /home/you/Projects/app/.git/info/exclude as Mindweaver's
```

The folder is renamed `.mindweaver`, and in it `flayer.toml` to
`weaver.toml`, `mindflayer.db` to `mindweaver.db` and each copy kept of
either beside them (a `flayer.toml.v2`, a `mindflayer.db.v4.bak`) to the
new name with the same version; each folder the marker names inside
`.mindflayer` is pointed at the same place in `.mindweaver`, every other
byte of it kept, and what the database records of the shelf moves with it.
The database is renamed, never migrated. In each project, the markers of
its blocks are spelled `mindweaver:`, every other byte kept, and each rule
file the database records in `.claude/rules/mindflayer` moves to
`.claude/rules/mindweaver`, its record and its line in the exclude file
with it; one you changed since is left where it is, its record naming it
there, and the line says so, while one that is not there has its record
and its line pointed at the new folder, so `mindweaver update` puts it
back there. What was installed stays current, and a second run has nothing
to do:

```
$ mindweaver migrate --names --yes
nothing to move: the workspace is under Mindweaver's names already
```

Unless the first left something under the old names: a rule file you
changed, which it says again each time, a block git tracks in a local
project, or a project it could not find then. Run once the file is as it
was written, or the project is back where the workspace links it, it moves
what is left, and nothing else.

`mind migrate --names`, in a project, moves the workspace its commands use
the same way. The default workspace in your home is moved by the same
line, run where it is the one in use.

Nothing moves while anything is in the way, and every reason is said at
once: a `.mindweaver` already there, something where a renamed file or a
carried rule file would go, a block that cannot be read, a project whose
marker cannot, or a database another process has open, a `mindweaver ui`
or a TUI, which would go on writing to the old name: close it, and run the
line again. A move that fails partway puts back every step it took. Git is
never run: where git tracks a rule file the move carries, the line says
the `git rm --cached` and the `git add` that take the move into the index,
naming each such file and never the old folder, which may still hold one
the move left there, and a tracked `AGENTS.md` or `CLAUDE.md` shows the
markers' change as yours to commit. In a local project without
`edit_tracked`, a block git tracks is left under its old markers, which
every reader still takes, as a render leaves it, and the line says so.

The panel keeps the colour mode and the sidebar its browser chose under
Mindflayer: it reads them once, and keeps them under the new name. The
header its page sends and the cookie its token travels in are
`X-Mindweaver` and `mw_<port>` now; each launch sets the cookie anew, so
nothing of the old is read.

### A workspace made before the library

`mindweaver init` used to keep a workspace's own skills and rules in `skills/` and
`rules/` beside its projects: `~/skills` and `~/rules`, for the workspace in
your home. Such a workspace still works, because its `flayer.toml` names those
folders, and keeps them there until you move them into its library. A
Mindflayer build made it, so it is a `.mindflayer` still, and its library is
there until [`--names`](#from-mindflayer) moves the whole. On its own the
command changes nothing and says what `--yes` would do:

```
$ mindweaver migrate --library
would move /home/you/skills/commit-style to /home/you/.mindflayer/library/skills/commit-style
would move /home/you/rules/git/no-force-push.md to /home/you/.mindflayer/library/rules/git/no-force-push.md
would point skills and rules in /home/you/.mindflayer/flayer.toml at the library, keeping it as it is in /home/you/.mindflayer/flayer.toml.v2
nothing moved: add --yes to go ahead

$ mindweaver migrate --library --yes
moved /home/you/skills/commit-style to /home/you/.mindflayer/library/skills/commit-style
moved /home/you/rules/git/no-force-push.md to /home/you/.mindflayer/library/rules/git/no-force-push.md
pointed skills and rules in /home/you/.mindflayer/flayer.toml at the library, kept as it was in /home/you/.mindflayer/flayer.toml.v2
```

Each artifact is moved as it is, a skill whose front matter is broken
included, and a folder it leaves empty goes with it, so your home keeps no
empty `~/skills`. Only the values in `[directories]` change; the comments and
everything else in `flayer.toml` stay as they were. Run again, it has nothing
to do and writes nothing:

```
$ mindweaver migrate --library --yes
nothing to move: the workspace keeps its own in its library already
```

What you installed into a project from what moved stays installed, and
current: an install records that it came from `workspace`, not from which
folder, so `mindweaver install` still finds the project's copy the same as the
skill it came from.

Nothing moves while anything is in the way, and every reason is said at once:
something already where an artifact would go, a `flayer.toml.v2` an earlier
migration kept, a link whose target is relative to where it is, or a folder
that cannot be read:

```
$ mindweaver migrate --library --yes
error: nothing moved: /home/you/.mindflayer/library/skills/commit-style: already there, so /home/you/skills/commit-style cannot be moved onto it
```

Nothing moves either while a folder it would empty is not the workspace's:
one `flayer.toml` names outside the workspace or on its shelf, which `mindweaver
init` would have refused, or one that is a link, or is reached through one, as
`~/skills -> dotfiles/skills` is, which keeps its skills in your dotfiles on
purpose.

A project's artifacts are never moved. When your home is itself a project
keeping its skills in `~/skills` too, those are the project's: they stay where
they are, the command says so, and the workspace's own skills start empty in
its library. If you never linked your home, the workspace counted those skills
as its own until then, and the line says it no longer does: one you installed
from there stays installed and recorded, so `mindweaver uninstall` still takes it
out, but nothing offers it to `mindweaver install` any more. A move that fails puts back what had moved, and leaves
`flayer.toml` as it was. To undo a migration that worked, put `flayer.toml.v2`
in place of `flayer.toml` and move the artifacts back.

### A project made before settings

A project an older Mindweaver made is marked by `.mind/mind.toml`, which
names the folders it authors in and nothing else: no agents, so whatever was
installed into it went into its `skills/` beside its own, and no share mode,
since its `.mind` was committed by design. It keeps working that way until
you move it to a `settings.toml`. On its own the command changes nothing and
says what `--yes` would do:

```
$ mind migrate
would write .mind/settings.toml for collapse: agents claude,codex,opencode, share shared
would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is
would copy commit-style to .agents/skills/commit-style, for codex and opencode
would copy commit-style to .claude/skills/commit-style, for claude
would remove commit-style from skills/commit-style
would leave skills/deploy where it is: the project's own
nothing changed: add --yes to go ahead
$ mind migrate --yes
wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared
moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was
copied commit-style to .agents/skills/commit-style, for codex and opencode
copied commit-style to .claude/skills/commit-style, for claude
removed commit-style from skills/commit-style
left skills/deploy where it is: the project's own
```

The settings keep the name and the folders `mind.toml` gave, name every
agent unless `--agents` says fewer, and leave the project shared, as it was
committed, unless `--local`. `mind.toml` goes to `.mind/tmp/mind.toml.v2`,
which the folder's own `.gitignore` keeps out of git even in a shared
project. What the workspace installed is copied where each agent reads, the
same bytes, and taken out of `skills/`, but only while it still holds what
was installed: a copy you edited stays where it is, becomes yours and is
said, as what you wrote yourself is. A copy already where an agent reads,
in a project whose `mind.toml` kept its skills in `.claude/skills`, stays
where it is, and somebody's skill where a new copy would go is never
written over. Run again, it has nothing to do:

```
$ mind migrate --yes
nothing to migrate: collapse is marked by its settings.toml already
```

What `mind migrate` carries along is what the workspace it finds records,
walking up from the project. In a project linked from a workspace beside
it, which walking up never reaches, it carries nothing and says so in a
`hint:`; `-W` names that workspace, as for any `mind` command.

`mindweaver migrate -p tanukeys` does the same from the workspace, and `mindweaver
migrate --all-projects` does it for every project the workspace links that a
`mind.toml` or a `settings.json` marks, each on its own: without `--yes` it
lists every one and writes nothing, and with it one that cannot be moved is
said and the others go ahead. `mindweaver migrate` alone says which of the three
to ask for, [the TUI](#the-tui)'s header counts the projects still marked
by either, naming the command, and [`status`](#what-each-project-has) gives
each one a hint.

`--local` keeps `.mind` and the copies out of git, as `mind init` does for
a new project. A `.gitignore` keeps out only what git does not track yet,
though, and a `.mind` that was committed stays tracked, so the command says
how to stop that and leaves it to you:

```
$ mindweaver migrate -p tanukeys --local --agents claude --yes
wrote .mind/settings.toml for tanukeys: agents claude, share local
moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was
wrote .mind/.gitignore, keeping .mind out of git
copied commit-style to .claude/skills/commit-style, for claude, kept out of git
removed commit-style from skills/commit-style
hint: git tracks /home/you/Projects/tanukeys/.mind, and a .gitignore keeps out only what git does not track; `git rm -r --cached .mind` in /home/you/Projects/tanukeys stops that, and leaves the files where they are
```

A project is moved whole or not at all: one that something stops part way,
a folder that cannot be written, say, has what was done put back, and is
still marked by its `mind.toml`. Nothing is moved while there is a reason
not to, every reason said at once: a `mind.toml.v2` an earlier migration
kept, or a project from the first Mindweaver that keeps its skills inside
`.mind`, where settings cannot name them. Move them out and name their
folder in `mind.toml`, and such a project moves. Nor while the workspace
that installed into it cannot be read, since what it records is what says
which copies to carry.

A project a `.mind/settings.json` marks has its settings already, in the
JSON Mindweaver wrote them in before TOML. It is read as it was written,
installs go where its agents read, and nothing changes its settings until it
moves: `mind agents`, `mind share` and `mind targets` point at
`mind migrate`, which writes the same settings as a `settings.toml` and
keeps the `settings.json` as it was in `.mind/tmp`:

```
$ mind migrate
would write .mind/settings.toml for collapse: agents claude,codex,opencode, share local
would move .mind/settings.json to .mind/tmp/settings.json.v3, as it is
nothing changed: add --yes to go ahead
$ mind migrate --yes
wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share local
moved .mind/settings.json to .mind/tmp/settings.json.v3, as it was
```

No copy moves, since the agents, the share mode and the folders are the
ones the file named, so the workspace's records are not needed for it.
`--agents`, `--local` and `--shared` are what a `mind.toml` is given, and a
key Mindweaver does not know stays in the copy kept. A `settings.json.v3` an
earlier migration kept refuses the move, as a `mind.toml.v2` does.
