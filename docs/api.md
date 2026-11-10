# The HTTP API

`mindweaver ui` serves the control panel, and under `/api/v1` an HTTP API for
scripts and tools on this machine. The panel is its first client, and asks
the server nothing a script cannot: what a project holds, what an install
would do, and every change Mindweaver makes, previewed and then run as the
command line it is.

This guide walks through it with `curl` on a small workspace. Every answer
it shows is what the server answers: a test
(`apps/cli/tests/api_guide_test.rs`) builds the same workspace with the
commands shown, starts `mindweaver ui` as shown, sends every request below and
fails when an answer here is not the one the server gives. [The
reference](reference.md#control-panel) has the API beside the panel, and
[its description](#its-description) every route, parameter, field and code.

## Contents

- [A workspace to try it on](#a-workspace-to-try-it-on)
- [Starting the server for a script](#starting-the-server-for-a-script)
- [The token](#the-token)
- [Versions](#versions)
- [Reading](#reading)
- [Changing things](#changing-things)
- [When something goes wrong](#when-something-goes-wrong)
- [Following what happens](#following-what-happens)
- [Its description](#its-description)
- [Stopping it](#stopping-it)

## A workspace to try it on

The examples are of a workspace in `/home/you/Projects` linking one
project, `api`, with one skill of its own, `review`, and beside them a
repository, `web`, made a project and not linked yet:

```console
$ cd /home/you/Projects
$ mindweaver init
initialized weaver workspace `Projects` in /home/you/Projects/.mindweaver
$ git init --quiet api && mind -C api init
initialized mind project `api` in /home/you/Projects/api/.mind
$ mindweaver link api
linked api as api
$ mindweaver add skill review 'Read a change before it lands'
created skill review at /home/you/Projects/.mindweaver/library/skills/review/SKILL.md
$ git init --quiet web && mind -C web init
initialized mind project `web` in /home/you/Projects/web/.mind
```

Your own answers differ from these where they name something of your
machine's: the server's port, here `40185`, is the one the system picked,
the token is made new at every launch, the paths are your own, and each
project's id is the one `mind init` gave it at random. They are of a home
with no default workspace yet: where yours has one (`~/.mindweaver`),
`mindweaver init` also says the default remembered the new workspace, and
`/workspaces` and the snapshot's `known` list the default beside it, the
new one with `"remembered": true`. The rest is the same for the same
workspace.

The examples pipe what curl prints through [jq](https://jqlang.org), which
lays the JSON out as it is shown here, or picks a part of it; without jq,
curl prints each answer on one line.

## Starting the server for a script

`--no-open` serves the API without opening a browser, and `--token-file`
writes where it is served and its token to a file:

```console
$ mindweaver ui --no-open --token-file /home/you/.local/state/mindweaver.json
http://127.0.0.1:40185/?token=bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513
note: serving the workspace at /home/you/Projects on 127.0.0.1:40185 until Ctrl-C
```

It serves the workspace a command run in that folder would use, until it
is stopped. The link is what a browser opens the panel with, and the first
line on stdout; where it serves goes to stderr. The file is new, only your
account can read it, and it is written whole before the link is printed.
`--token-file` writes over nothing and makes no folder: a file already at
the path, or a folder that is not there, stops the launch with nothing
served ([the reference](reference.md#control-panel) says the rest). From
another terminal, or a script that did not start the server, read where
it is, and the token, from the file:

```console
$ cat /home/you/.local/state/mindweaver.json
{
  "version": 1,
  "api": "http://127.0.0.1:40185/api/v1",
  "token": "bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513"
}
$ api=$(jq -r .api /home/you/.local/state/mindweaver.json)
$ token=$(jq -r .token /home/you/.local/state/mindweaver.json)
```

`version` is how the file is laid out, `api` where every route is served,
and `token` what to send with each request. Every example below uses `$api`
and `$token`.

A script that starts the server itself, in the background, waits for the
link before it reads the file, since the file is whole by then, and gives
up when the server stops without one, as it does when the file is refused:

```sh
log=$(mktemp)
mindweaver ui --no-open --token-file /home/you/.local/state/mindweaver.json > "$log" &
pid=$!
while [ ! -s "$log" ] && kill -0 "$pid" 2>/dev/null; do sleep 0.1; done
[ -s "$log" ] || exit 1
```

## The token

Every route asks for the launch token, which a script sends as
`Authorization: Bearer <token>`:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/snapshot" | jq .workspace
{
  "made": true,
  "name": "Projects",
  "path": "/home/you/Projects",
  "reason": "above"
}
```

Without it, or with another launch's, the answer is a 401 (`-w` has curl
print the status after the answer):

```console
$ curl -s -w '\n%{http_code}\n' "$api/snapshot"
{"code":"token_required","error":"this needs the launch token, as the cookie its link sets or as a Bearer token"}
401
```

The panel's page sends the token as the cookie its link sets instead, and
a change sent with that cookie is held to checks a script never meets
([what keeps other pages
out](architecture.md#what-keeps-other-pages-out)). Each launch has a token
of its own, so a token left from one the server stopped opens nothing. The
server listens on 127.0.0.1 alone, and nothing from another machine
reaches it.

## Versions

Everything is under `/api/v1`. A route, a parameter or a field once served
stays as it is there: dropping or renaming one, or changing what it holds,
moves the API to `/api/v2`, and adding one does not, so read an answer by
the names you need and let the rest be. A version the server does not
serve, or a path with none, is a 404, so a script written for another
version is refused rather than misread:

```console
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" http://127.0.0.1:40185/api/v2/snapshot
{"code":"unknown_route","error":"no such endpoint: GET /api/v2/snapshot"}
404
```

What the snapshot holds is versioned apart, by its `format`, and the token
file by its `version`, each moved by the same rule.

## Reading

A read is a GET naming what it reads in its parameters, never in its path,
since a registry entry or a reference can hold a `/`. A read that takes a
structure, the boxes of a plan or an artifact to judge, is a POST that
changes nothing. A parameter the route does not take, one given twice that
takes one value, and one it needs left out are each refused, so a
misspelled name is said rather than read as nothing.

### The snapshot

`GET /snapshot` is everything the workspace holds, as the panel draws it:
`workspace` (above), `projects`, `library`, `loaded`, the workspaces it
loads, and `known`, those this machine knows, `shelf` and `sources`,
`offers`, everything there is to install and where it comes from,
`boxes`, how each project holds each of those, `doctor`, what keeps each
project's agents from what it gives them, as `mindweaver doctor` says it,
`history`, what the ledger logged lately, `summary`, what it counts, `warnings`, what could not be
read, and `format`, the version of what it holds; `kinds`, `states`,
`box_states`, `healths` and `agents` are the order each is listed in. The
routes below cut parts of it, or read what it does not hold.

### Projects

`GET /projects` is the snapshot's projects, and `GET /project?entry=<entry>`
one of them, by its entry as the registry spells it:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/projects" | jq '.[].entry'
"api"
$ curl -s -H "Authorization: Bearer $token" "$api/project?entry=api" | jq .settings
{
  "agents": [
    "claude",
    "codex",
    "opencode"
  ],
  "edit_tracked": false,
  "share": "local",
  "targets": []
}
```

Each holds its agents and settings, what it holds of each kind with how
each stands (`current`, `outdated`, `edited`, `missing`, `orphaned`,
`foreign` or `authored`) and where its copies are, the counts of those,
and its `health`, one word for the whole project: `ok`, `attention`,
`broken` or `stale`.

### The library

`GET /library` is the workspace's own skills, rules and MCP servers, and
which projects hold a copy of each; under `servers`, each MCP server as
each agent would be given it, what it would run or reach and what it asks
before it starts one, and `given`, how it is started as `add mcp`'s
options say it:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/library" | jq .artifacts
[
  {
    "agents": [],
    "issues": [],
    "kind": "skill",
    "name": "review",
    "origin": null,
    "state": null,
    "summary": "Read a change before it lands",
    "targets": [
      ".mindweaver/library/skills/review"
    ],
    "unread": null
  }
]
```

### An artifact's files

`GET /files?reference=<kind:name>` lists every file an artifact holds, and
`GET /contents?reference=<kind:name>&file=<route>` reads one of them as
text, of the workspace's own, or of a project's with `&project=<entry>`:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/files?reference=skill:review" | jq .
{
  "entries": [
    {
      "folder": false,
      "path": "SKILL.md",
      "refused": null,
      "size": 74
    }
  ],
  "kind": "skill",
  "manifest": "SKILL.md",
  "name": "review",
  "truncated": false
}
$ curl -s -H "Authorization: Bearer $token" "$api/contents?reference=skill:review&file=SKILL.md" | jq .
{
  "file": "SKILL.md",
  "hash": "785660eaf65b8fce834ca050b26ed265b4d787871877ff87f05b52212d32dbca",
  "kind": "skill",
  "manifest": {
    "body": "\n# review\n",
    "front_matter": [
      {
        "key": "name",
        "value": "review"
      },
      {
        "key": "description",
        "value": "Read a change before it lands"
      }
    ]
  },
  "name": "review",
  "size": 74,
  "text": "---\nname: review\ndescription: Read a change before it lands\n---\n\n# review\n",
  "withheld": null
}
```

A file is only ever text inside JSON, never served as itself: one too
large, or not text, says why in `withheld` and holds no `text`, and a link
leading out of the artifact's folder is listed with why it is `refused`
and never followed. `hash` is the SHA-256 of what the file held when it
was read, which a save names so it never writes over a change made since.

### Finding projects

`GET /scan?root=<path>&depth=<n>` is what `mindweaver scan --suggest` finds
under each `root`, an absolute path, given once for each folder to look
under, or the workspace's folder without one; what is not linked comes
first, the most worth linking first:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/scan?root=/home/you/Projects&depth=1" | jq .
{
  "depth": 1,
  "found": [
    {
      "found": {
        "agent_files": [],
        "id": "0123456789abcdef0123456789abcdef",
        "linked": false,
        "mind": true,
        "moved": null,
        "path": "/home/you/Projects/web",
        "reason": "id",
        "route": "web"
      },
      "moved_from": null,
      "offers": "link",
      "why": "mind project with an id"
    },
    {
      "found": {
        "agent_files": [],
        "id": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5",
        "linked": true,
        "mind": true,
        "moved": null,
        "path": "/home/you/Projects/api",
        "reason": "id",
        "route": "api"
      },
      "moved_from": null,
      "offers": null,
      "why": null
    }
  ],
  "roots": [
    "/home/you/Projects"
  ],
  "truncated": false,
  "untraced": [],
  "warnings": []
}
```

`GET /folders?path=<path>` lists the folders directly inside one, each said
to be a project, a workspace, a git repository or a plain folder, and
whether the workspace links it; hidden ones only with `&hidden=true`:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/folders?path=/home/you/Projects" | jq .folders
[
  {
    "is": "project",
    "linked": true,
    "name": "api",
    "path": "/home/you/Projects/api",
    "symlink": false
  },
  {
    "is": "project",
    "linked": false,
    "name": "web",
    "path": "/home/you/Projects/web",
    "symlink": false
  }
]
```

A path the API takes is absolute and normalized, and never inside a
`.mindweaver`, nor the `.mindflayer` of a workspace a Mindflayer build
made: any other is refused, there or not, with the code of why.

### Updates and workspaces

`GET /updates` is what `mindweaver update --all` would do, with nothing run,
and `GET /workspaces` every workspace the server may switch to:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/updates" | jq .
{
  "projects": [],
  "states": [
    {
      "count": 0,
      "state": "outdated"
    },
    {
      "count": 0,
      "state": "edited"
    },
    {
      "count": 0,
      "state": "missing"
    },
    {
      "count": 0,
      "state": "orphaned"
    }
  ],
  "unread": [],
  "writes": false
}
$ curl -s -H "Authorization: Bearer $token" "$api/workspaces" | jq .
[
  {
    "default": false,
    "in_use": true,
    "launched": true,
    "name": "Projects",
    "path": "/home/you/Projects",
    "projects": 1,
    "reason": "above",
    "remembered": false,
    "stale": null,
    "stale_projects": 0,
    "switchable": false
  }
]
```

### Plans and drafts

`POST /plan` takes boxes of a project and an offer, each ticked or not, as
Assign's matrix has them, and answers what they amount to: the tasks that
carry them out, which `/tasks` runs as they are, and what clashes or is
locked, with nothing run. A POST says its body is JSON in `Content-Type`,
which curl's `-d` alone does not:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"selection": [{"project": "api", "reference": "skill:review", "origin": "workspace", "wanted": true}]}' \
    "$api/plan" | jq .
{
  "asks": [],
  "conflicts": [],
  "locked": [],
  "tasks": [
    {
      "from": "workspace",
      "kind": "skill",
      "name": "review",
      "projects": [
        "api"
      ],
      "type": "install",
      "yes": false
    }
  ]
}
```

An MCP server from a workspace this one loads, or from the shelf, is
somebody else's command for every agent to run, so its install waits for a
yes: its task carries `"yes": false`, and its line says what each agent
would run and installs nothing. `asks` holds each such task by its place
in `tasks`, what it would give each agent of each project, `lines` as
`mindweaver install` says them, and `confirmed`, the same task with `"yes":
true`, to send in its place once those lines have been read. A yes sent
with any other install is refused (`install_yes_unasked`).

`POST /validate` judges an artifact a command is about to write, as that
command would refuse it and as `validate` would say of it once written,
with nothing written:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"holder": {"type": "workspace"}, "draft": {"type": "add", "kind": "skill", "name": "Review", "description": "Read a change before it lands", "template": null}}' \
    "$api/validate" | jq .
{
  "issues": [],
  "refused": {
    "code": "add_name_unfit",
    "message": "`Review` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens"
  }
}
```

## Changing things

Every change is a task: the command lines it is, run through the same
parser as the ones you type, so the API changes nothing the command line
cannot. `POST /tasks/preview` answers the lines a task is, with nothing
run, and `POST /tasks` runs them. Each line names the workspace with `-W`,
as you could type it in the workspace's folder.

An MCP server is added by an `add` task of the kind `mcp` whose `server`
says how an agent starts it or reaches it, `{"type": "program", "command":
..., "args": [...], "env": [[key, value]], "secret_env": [...], "arg_env":
[...]}` or `{"type": "address", "url": ..., "events": false, "headers":
[...], "secret_headers": [[header, variable]], "bearer": ..., "url_vars":
[[name, variable]]}`, each secret the name of the variable it is read
from; an `edit_server` task, `{"type": "edit_server", "holder": ...,
"name": ..., "server": ...}`, writes how one is started again. Each is the
line `add mcp` or `edit` takes with those options, and a `validate` draft of
either is judged as that line would judge it. The first word of a pair, a
setting's variable, a header's name or a part of the address's, holds no
`=`, where the line's option ends it: a task naming one is refused before a
line is built, as its draft is (`add_mcp_key_unsayable`).

### Linking a project

`web` is not linked yet. A `link` task names its folder by its absolute
path:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "link", "path": "/home/you/Projects/web"}' \
    "$api/tasks/preview" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects link /home/you/Projects/web"
  ],
  "confirm": [],
  "cancellable": false
}
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "link", "path": "/home/you/Projects/web"}' \
    "$api/tasks" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects link /home/you/Projects/web"
  ],
  "outcome": {
    "ok": true,
    "runs": [
      {
        "command": "mindweaver -W /home/you/Projects link /home/you/Projects/web",
        "error": null,
        "ok": true,
        "stderr": [],
        "stdout": "linked web as web\n"
      }
    ]
  }
}
```

`confirm` is what of the lines takes something away, a `remove --yes`, an
`uninstall`, an `unlink` or a `--force` among them, each with what it
takes in the command's own words: the panel asks before it runs those, and
the server runs a task whatever a preview said, as a shell runs a line
typed into it. `cancellable` says whether a run of it can be asked to stop
while it runs, which only a gather and a backup can ([stopping a
run](#stopping-a-run)). A run answers what each line said, its stdout,
each line of its stderr with its code, and whether it went well: a run is
a 200 when it ran, however the command went, and `outcome.ok` says that.

### Installing a skill

An `install` task names the projects, the kind and the name, and `from`,
the place it is offered from as `--from` takes it, or `null` for wherever
the name is offered, and `yes` only for an MCP server's confirmation, as
`/plan` hands it:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "install", "projects": ["web"], "kind": "skill", "name": "review", "from": null}' \
    "$api/tasks/preview" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review"
  ],
  "confirm": [],
  "cancellable": false
}
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "install", "projects": ["web"], "kind": "skill", "name": "review", "from": null}' \
    "$api/tasks" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review"
  ],
  "outcome": {
    "ok": true,
    "runs": [
      {
        "command": "mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review",
        "error": null,
        "ok": true,
        "stderr": [],
        "stdout": "installed review into web, from workspace\n"
      }
    ]
  }
}
```

A run answers what the command said, not what it changed: read again for
that. `web` holds `review` now, a copy where each of its agents reads:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/project?entry=web" | jq .artifacts
[
  {
    "agents": [
      "claude",
      "codex",
      "opencode"
    ],
    "issues": [],
    "kind": "skill",
    "name": "review",
    "origin": "workspace",
    "state": "current",
    "summary": "Read a change before it lands",
    "targets": [
      ".agents/skills/review",
      ".claude/skills/review"
    ],
    "unread": null
  }
]
```

Every task the panel sends is one of these, `Task.json` in [the
schemas](api/schemas) says each; a task that needs a terminal, one that
opens an editor or a screen, is refused with a 422 and its code.

### What ran

`GET /transcript` is every task run since the server started, from a
script or from the panel, oldest first, each as its run answered:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/transcript" | jq '.[].lines'
[
  "mindweaver -W /home/you/Projects link /home/you/Projects/web"
]
[
  "mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review"
]
```

Tasks run one at a time, in the order they arrive, whoever sends them, and
a run is in the transcript before its answer leaves. A read, a plan and a
preview are no runs, and are not there.

### Stopping a run

A gather is the network, and takes as long as that does. A `gather` task,
`{"type": "gather", "url": ..., "folder": ".", "reference": null}`,
is previewed as `cancellable`, and while it runs `/events` tells each step
it takes ([following what happens](#following-what-happens)). It gathers
skills; with `"kind": "rule"` it gathers the rules in the folder it names,
as `mindweaver gather git --kind rules --path <folder>` does, a folder of
`"."` then being the whole repository, said as `--path .`. Its source
is an address, which the gather refuses when it carries a credential, or a
folder on this machine named by its absolute path, or a `file://` URL of
one, as a folder to link is. It carries no token either, nor the name of a
variable holding one: a private repository logs in as git config's
credential helpers and ssh do on the server's machine, and a token for a
gather is handed over from a shell alone (`--token-env`, `--gh`).

A search of the skill directories is the network too. A `search_skills`
task, `{"type": "search_skills", "query": ...}`, is `mindweaver gather search
--json`, which asks the finders the workspace's `weaver.toml` enables
([the reference](reference.md#searching-the-skill-directories)) with no
token, and so never GitHub's code search, which needs one; its run's output
is what the command prints, the results in the envelope's `data`. It is
previewed as `cancellable`, and asks nothing until it runs:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "search_skills", "query": "pdf"}' \
    "$api/tasks/preview" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects gather search --json pdf"
  ],
  "confirm": [],
  "cancellable": true
}
```

So is the MCP Registry, the official one, since a task names no other. A
`search_servers` task, `{"type": "search_servers", "query": ...}`, is
`mindweaver gather mcp search --json`, the servers it lists in the
envelope's `data`; an `import_server` task, `{"type": "import_server",
"server": ..., "version": null, "yes": false}`, is `mindweaver gather mcp
add`, which says what the server would be written as and writes nothing
until it is sent again with `"yes": true` ([the
reference](reference.md#from-the-mcp-registry)). Neither starts anything,
nor takes a cancel:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "import_server", "server": "ai.clawfetch/mcp", "version": "0.4.0", "yes": true}' \
    "$api/tasks/preview" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects gather mcp add --version 0.4.0 --yes ai.clawfetch/mcp"
  ],
  "confirm": [],
  "cancellable": false
}
```

A backup is the network too. A `backup` task, `{"type": "backup", "url":
..., "branch": null, "kinds": [], "message": null, "token_env": null}`, is
`mindweaver backup`: its repository is named as a gather's source is, and
a token only by the variable holding it, in the environment the server was
started with, never the token itself. The server reads such a variable
only when it is named for a token, ending in `TOKEN`, since what it holds
is sent to the repository named; any other is refused
(`token_variable_refused`). An `export` task, `{"type": "export",
"folder": ..., "kinds": [], "yes": false}`, writes into a folder named by
its absolute path, and with `"yes": true` is previewed with what it takes
away to confirm:

```console
$ curl -s -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "backup", "url": "https://github.com/you/library-backup.git", "branch": null, "kinds": ["skill"], "message": null, "token_env": "GITHUB_TOKEN"}' \
    "$api/tasks/preview" | jq .
{
  "lines": [
    "mindweaver -W /home/you/Projects backup --kind skills --token-env GITHUB_TOKEN https://github.com/you/library-backup.git"
  ],
  "confirm": [],
  "cancellable": true
}
```

`POST /tasks/cancel` asks the run under way to stop, by the number
`command_started` gave it. A gather stops before it puts anything on the
shelf, and its outcome says it was cancelled (`gather_cancelled`); one
already placing what it read finishes, as its outcome says too. A backup
stops before it pushes, the `git` it was running stopped and the
repository as it was (`backup_cancelled`); one already pushing finishes. A
search stops before it asks the next finder (`search_cancelled`). A run that
is not the one under way, over already or not started yet, is refused with
a 409 and nothing is asked of any run, so a cancel that crosses the end of
its run never stops the one behind it; so is the run under way when its
preview did not say `cancellable` (`uncancellable`):

```console
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"run": 2}' "$api/tasks/cancel"
{"code":"not_running","error":"run 2 is not under way, and nothing is: a run is stopped only while it runs"}
409
```

A cancel taken is answered with the run and its lines, `{"run": 3,
"lines": [...]}`, and told to every stream as `cancelling`. A task or a
switch sent while a run is under way waits its turn behind it, and every
stream is told `busy`, with the run it waits behind.

### Switching workspace

`POST /switch` has the server act on another workspace from then on, one
`/workspaces` lists, named by its path as listed there; any other is
refused, and the server stays where it was:

```console
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"path": "/home/you/Notes"}' "$api/switch"
{"code":"switch_unlisted","error":"`/home/you/Notes` is none of the workspaces this machine knows, nor the one the panel was launched on: only one of those is switched to"}
422
```

## When something goes wrong

Every answer under `/api` that is no success is an object of two fields:
`error`, what happened in words, naming the route, the parameter or the
header, and `code`, one word for it that never changes when the words do,
which is what a script acts on. Its status says which kind of thing went
wrong:

```console
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" "$api/project"
{"code":"missing_parameter","error":"/api/v1/project needs `entry`, and was sent none"}
400
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" "$api/project?entry=blog"
{"code":"not_registered","error":"blog: not registered in the workspace at /home/you/Projects"}
404
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"type": "link", "path": "web"}' "$api/tasks/preview"
{"code":"path_relative","error":"`web` is not an absolute path: name a folder by its absolute path, which reads the same from anywhere"}
422
$ curl -s -w '\n%{http_code}\n' -H "Authorization: Bearer $token" \
    -d '{"type": "link", "path": "/home/you/Projects/web"}' "$api/tasks/preview"
{"code":"json_required","error":"Expected request with `Content-Type: application/json`"}
415
```

What the server finds itself has a code of its own:

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
| 401 | `token_required` | no launch token, or another server's |
| 403 | `unknown_host` | the server named otherwise than as `127.0.0.1`, `localhost` or `[::1]` on its port |
| 403 | `foreign_origin` | sent from a page of another origin |
| 403 | `same_origin_required` | a change sent with the panel's cookie from anywhere but its page |
| 403 | `x_mindflayer_required` | a change sent with the panel's cookie without `X-Mindweaver: 1` |
| 404 | `unknown_route` | a path no route names, of another version or of none |
| 405 | `method_not_allowed` | a method the route does not take |
| 409 | `not_running` | a cancel of a run that is not the one under way |
| 409 | `uncancellable` | a cancel of the run under way, which takes none |
| 413 | `body_too_large` | a body larger than the server reads |
| 415 | `json_required` | a body not sent as `application/json` |
| 422 | `invalid_body` | JSON that is not what the route reads |
| 500 | `backend_failed` | Mindweaver failed while it answered |
| 500 | `writer_stopped` | the thread that runs tasks has stopped |

The header's code still says `mindflayer`: the header was `X-Mindflayer`
when the code was given, and a code once given is never renamed.

Everything else is Mindweaver's own refusal, by the code of the rule it
breaks, from the list a command's `--json` takes its codes from: a 404 for
a read naming what is not there
(`not_registered` for an entry, `not_found` for an artifact,
`contents_missing` for a file, `browse_missing` for a folder), and a 422
for what it will not do (`path_relative`, `opens_editor`,
`switch_unlisted`...). A task, a switch, a plan or a draft naming what is
not there is a 422 too: the route it was sent to is there, and the code
says the rest. A code is never renamed, nor given to something else; the
API's description lists every one, `CoreCode` and `CliCode` among its
schemas.

## Following what happens

`GET /events` tells what happens as it happens, as server-sent events, one
`data:` line of JSON each, until the client lets go; `-N` has curl print
each as it comes:

```console
$ curl -s -N -H "Authorization: Bearer $token" "$api/events"
data: {"type":"resync","switches":0}

data: {"type":"watching"}

```

Each is tagged by its `type`:

- `resync`, first on every stream and again for one that fell behind: read
  everything again rather than trust what was read before. `switches` is
  how many switches the server has taken, so a reader that missed one
  learns of it.
- `watching`, or `unwatched` with its `reason`, right after it and
  whenever that changes: whether every file the snapshot reads is watched,
  which it may not be past the system's limit on watches.
- `changed`, with the `scopes` a change on disk reached, each the
  workspace (`{"type": "workspace"}`) or a project by its entry, made from
  anywhere: the API, the panel, a terminal or an editor. Nothing is said of
  what changed: read it again.
- `command_started` and `command_finished`, with a run's `lines`, around
  each run, whoever sent it, and its `run`, its number, counted from 1 in
  the order runs start, which a cancel names it by; `command_started` says
  too whether it is `cancellable`.
- `progress`, with a run's `run` and a step it took, `progress`: its
  `stage` as one word, `connecting`, `receiving`, `resolving`,
  `checking_out`, `harvesting` or `placing` for a gather, and
  `checking_out`, `exporting`, `committing` or `pushing` for a backup,
  which counts nothing, how far it got,
  `done` of `total`, either `null` where it counts nothing or does not
  know yet, the same in the words a terminal draws it in, `said`, and
  whether a cancel still stops it, `cancellable`. Each stage's first step
  and last are told, and no more than ten a second in between.
- `cancelling`, with `run`, once a run is asked to stop, from anywhere:
  how it finishes says whether it did.
- `busy`, with the `run` under way, its `lines`, whether it is
  `cancellable` and whether it is `cancelling`, when a task or a switch is
  queued behind it, and right after `watching` on a stream opened while
  one runs, followed by the latest step it took.
- `switched`, with `switches`, once the server acts on another workspace:
  everything is to be read again.

A comment line keeps the connection open while nothing happens. The
stream is a GET like any read, with the token as any request has it.

## Its description

`GET /openapi.json` is the API described as an OpenAPI 3.1 document: every
route with its parameters, its body, its answer and each status it may be
refused with, the codes of each listed. The repository holds it as
[`docs/api/openapi.json`](api/openapi.json), and each type a route takes
or gives as JSON Schema in [`docs/api/schemas`](api/schemas), both written
from the Rust that serves them and checked in CI, so they are what the
server answers:

```console
$ curl -s -H "Authorization: Bearer $token" "$api/openapi.json" | jq '.paths["/project"].get.parameters'
[
  {
    "description": "The entry, as the registry spells it and the snapshot lists it",
    "in": "query",
    "name": "entry",
    "required": true,
    "schema": {
      "type": "string"
    }
  }
]
```

An answer's schema leaves room for a field it does not name, since one
added keeps the version: a script that checks what it is answered with
does not refuse an answer for one. What each event holds is under
`x-itemSchema` on `/events`, as OpenAPI has no place for what each item of
a stream holds.

## Stopping it

Ctrl-C stops the server, and so does `kill`, whose `SIGTERM` it takes as a
Ctrl-C: it asks a run under way that takes a cancel to stop, as a cancel
does, lets anything else running finish, ends every event stream, takes
the token file away, and prints after its link every line it ran that
changed something, with what each said, so the session can be run again
from a shell:

```
$ mindweaver -W /home/you/Projects link /home/you/Projects/web
linked web as web
$ mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review
installed review into web, from workspace
```

A script that started it in the background stops it with `kill "$pid"`.
One killed with `SIGKILL`, or hung up, leaves its token file behind,
holding a token that opens nothing any more, and the path is refused until
it is taken away.

There is no client library: any HTTP client that sends a header will do.
There is no MCP front end.
