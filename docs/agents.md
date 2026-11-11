# Where the agents read skills, rules and MCP servers

This is the reference for where Claude Code, Codex and OpenCode look for
skills and rules, at project and user scope, and what each does with
duplicates and ignored files, and for what each makes of a project's MCP
servers. Mindweaver's agent adapters build on it: the target table (#28),
installing into several folders (#34), the managed block in `AGENTS.md`
(#42), Claude's rule files (#43), the predicate that decides how Claude
reaches the block (#44), the names a suggestion ranks a repository by (#58),
what an MCP server's definition is held to for each agent (#87), the
entry Claude's `.mcp.json` is given of one (#90), the one Codex's
`.codex/config.toml` is (#91) and the one OpenCode's `opencode.json` is
(#92), the MCP installs they serve (#93, #94), what `mindweaver doctor`
finds keeps an agent from what a project gives it (#96), and how a gather
reads a Codex marketplace and the plugins it lists (#104).
[docs/architecture.md](architecture.md) says what Mindweaver does with these
facts and why; this page says what the agents do.

**Last verified: 2026-10-26, on Claude Code 2.1.309.** Codex and OpenCode were
not run, so every fact about their skills and rules is docs-only, as of the
same date. **The MCP checks were written on 2026-11-04** and Claude Code's
and OpenCode's ran on 2026-11-05, on Claude Code 2.1.318 and OpenCode 2.2.5,
2.1.3 and 1.19.4 ([the MCP runs](#the-mcp-runs)). Codex's were not run: the
owner has no Codex subscription, so its MCP facts stand as its
documentation says, and the work built on them takes them as they stand.

Each fact carries one of three marks:

- **verified**: seen on the real binary by `scripts/agent-probe/probe.sh`, in
  one of the runs recorded under [the Claude Code runs](#the-claude-code-runs)
  or [the MCP runs](#the-mcp-runs).
- **docs-only**: taken from the agent's documentation or, where that is
  silent, from its source code, and not run here. Each one Mindweaver relies
  on names a fallback: what Mindweaver does, or what still holds, if the
  binary turns out to disagree.
- **pending**: a check `probe.sh` holds that has not run yet. Until it does,
  the fact stands as its source says, docs-only, and its expected result is
  what a run is held to.

## What the decisions rest on

- [D-02](architecture.md#d-02-installs-stay-local-unless-a-project-shares-them),
  installs stay local: **holds for Claude Code.** An excluded skill, an
  excluded symlinked skill, an excluded `AGENTS.md`, excluded rules under
  `.claude/rules/mindflayer` and an excluded `CLAUDE.local.md` all loaded.
  Git confirmed every exclusion in `claude-excluded`; `claude-local` excluded
  its `CLAUDE.local.md` with the same kind of block but did not ask git, as
  the script now does there too. For Codex and OpenCode it is docs-only:
  their source does no gitignore filtering that anyone found, and shared mode
  is the fallback.
- [D-03](architecture.md#d-03-rules-reach-every-agent-through-agentsmd),
  rules reach every agent through `AGENTS.md`: **holds for Claude Code.** It
  read `AGENTS.md` when no `CLAUDE.md` or `CLAUDE.local.md` was in its start
  directory or above, and did not when a `CLAUDE.md` sat above the git root or
  a `CLAUDE.local.md` beside it. Rule text inside Mindweaver's marked block
  reached the model, whether `AGENTS.md` was read natively or imported, and
  the markers, HTML comments, were stripped. A `CLAUDE.md` holding
  `@AGENTS.md` between markers loaded it once. The user memory file did not
  count as a `CLAUDE.md`, also with the project under the home, where Claude's
  walk passes it; nor did `.claude/rules` files. A rule with `paths:` loaded
  only once Claude read a matching file.
- [D-07](architecture.md#d-07-one-copy-per-agent-folder), one copy per agent
  folder: **holds.** Claude did not load a skill that was only in
  `.agents/skills`, so the second copy in `.claude/skills` is needed, and it
  loaded a relative symlink to `.agents/skills` once, the condition D-07 set
  for the symlink opt-in. One thing that opt-in has to know: git never follows
  a symlink, so the exclude entry for one must not end in `/` as a directory's
  does, or it ignores nothing.

**One corner differs from D-03 as first written, and #44 follows Claude:**
Claude's `AGENTS.md` check most likely stops below `/`. The debug log counted
four directories searched from a start directory four levels below `/`, and
five from one five levels below, each one short of the directories up to and
including `/`. So a `CLAUDE.md` at `/` itself would not stop Claude reading
`AGENTS.md`, where D-03 counted every directory above. That is read from the
counts, not tried: no run could put a file at `/`. #44's predicate
(`claude_md::reads_agents_md`) stops its walk below `/` too, so the two agree
there; walking up to and including `/` would have chosen Fallback while Claude
went on reading `AGENTS.md` natively, and Claude would have read each rule
about every file twice, once in the block and once in `.claude/rules`. Were
Claude to count `/` after all, a `CLAUDE.md` there with none nearer would have
the predicate say Claude reads the block while it reads none, and those rules
would miss Claude: a file only the machine's administrator can put there,
which a probe run that may write at `/` would settle. Nothing else
contradicts D-02, D-03 or D-07.

## How the checks are run

`scripts/agent-probe/probe.sh` builds throwaway git repositories and asks the
agent about each one. It talks to the agent's API with the operator's own
login, so it is run by hand, never by CI or `make test`.

- **One temp dir.** Everything lives in `target/agent-probe-run`, deleted when
  the script exits, on an error or Ctrl-C too, the login copy included. Only a
  run killed outright (SIGKILL, a crash) cannot clean up: it leaves the temp
  dir, login copy and all, until the next run deletes it or `make clean`
  removes `target/`. The only files taken from the real home are the logins:
  Claude's `.credentials.json`, and for a Codex run `$CODEX_HOME/auth.json`
  (by default `~/.codex/auth.json`); OpenCode's runs call no model and get
  none. Settings, skills, rules, `CLAUDE.md`, plugins and Codex's
  `config.toml` stay out, because they are exactly what must not take part.
  A login close to expiry is refused, since a refresh inside the run could
  rotate the refresh token the real session holds. The OpenCode 1.19.4 that
  the MCP checks run is fetched from the npm registry into the same temp
  dir, never from the real home.
- **A mount namespace.** The agent starts under bubblewrap, with the real
  home an empty tmpfs, the temp dir mounted at `/mnt/agent-probe`, `/tmp` a
  private tmpfs, the environment cleared and a PID namespace of its own, so
  nothing the agent started outlives the script. `HOME`, the `XDG_*_HOME`
  variables, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `OPENCODE_CONFIG_DIR` all
  point into the temp dir. Pointing the variables away is not enough on its
  own: Codex and OpenCode read `~/.agents/skills` whatever their own variables
  say. Mounting the temp dir somewhere new also makes the parent-folder checks
  possible: Claude reads instruction files from every folder above where it
  starts, and above `target/` sit this checkout's `AGENTS.md` and, in a linked
  worktree, the main checkout's `CLAUDE.md`.
- **Guards.** The part that runs agents refuses unless `HOME` is the temp
  home and the real one is out of sight. Before every check it asserts that
  the temp home holds no user-scope skills, rules, `CLAUDE.md` or `AGENTS.md`
  (one check puts a user memory file there on purpose, and removes it), and
  that the only instruction files above the scenario are the ones it wrote.
- **The real home.** Before and after the run it fingerprints `~/.agents`,
  `~/.codex`, `~/.claude/skills`, `~/.claude/rules`, `~/.claude/CLAUDE.md`,
  `~/.claude.json`, `~/.config/opencode`, and OpenCode's two logins,
  `~/.local/share/opencode/auth.json` and `mcp-auth.json`, and fails if any
  changed. Not the whole `~/.claude`: a Claude Code session running the
  script writes there itself, as an OpenCode session does in the rest of
  `~/.local/share/opencode`.
- **Tokens.** Every file under test carries a token of its own
  (`PROBE-AGENTS-MD-1`, ...), and each run asks the model once which tokens it
  holds and which skills it can invoke. A token in the answer means the file
  reached the model. Skills are counted in the session's init message, not in
  the answer. Claude's `InstructionsLoaded` hook, set in the temp home's user
  settings, records each instruction file that loads and why, which does not
  depend on the model at all. A natively read `AGENTS.md` fires no hook, so
  for it the script counts the debug log's `AGENTS.md loaded: <path>` lines
  instead, and fails a run whose count is not the expected one.

To re-run it: Linux with `bwrap`, and `jq` and `git` in `/usr/bin` or `/bin`,
the only `PATH` inside the namespace; and a logged-in Claude Code, its native
binary on `PATH` or named by `PROBE_CLAUDE_BIN`. A launcher script, as an npm
install gives, is refused, and so is a version manager's shim, since the
binary must call itself Claude Code: `mise which claude` names the one behind
mise's.

```bash
scripts/agent-probe/probe.sh            # same as: probe.sh claude
PROBE_ONLY='claude-import claude-local' scripts/agent-probe/probe.sh
```

It prints every command, answer and verdict, keeps them in
`target/agent-probe-results/<time>/`, and exits non-zero when a result
differs from what this page says. Each run of Claude's or Codex's is a paid
call, so `PROBE_ONLY` names the runs to do when only some changed; a name
that is no run is refused. Each agent's checks are in a file beside the
script, `claude.sh`, `codex.sh` and `opencode.sh`, defining
`<agent>_prepare` (copy the login in, name the binaries), `<agent>_checks`
and `<agent>_runs`. `codex.sh` and `opencode.sh` hold only the MCP checks so
far, so `probe.sh codex` and `probe.sh opencode` run those alone, and what
Codex and OpenCode do with skills and rules stays docs-only. [MCP
servers](#mcp-servers) says what those checks need besides, and the command
that runs them.

## Claude Code

### Skills

| Fact | Mark | Evidence, or source and fallback |
|---|---|---|
| Project skills are `.claude/skills/<name>/SKILL.md` in the start directory. | verified | `claude-excluded`; the debug log lists `project=[<repo>/.claude/skills]`. |
| `.agents/skills` is not read. | verified | `claude-excluded`: `probe-agents-skill` is in neither the init message nor the answer. |
| A skill excluded through `info/exclude` loads. | verified | `claude-excluded`. |
| A relative symlink `.claude/skills/<n>` -> `../../.agents/skills/<n>`, excluded, loads once. | verified | `claude-excluded`: listed once in the init message; the debug log says `Loaded 2 unique skills (... project: 2 ...)`. |
| A skill's description reaches the model at start; its body does not until it is invoked. | verified | The `-BODY` tokens never appeared. |
| User skills are `<config dir>/skills`. | verified, in part | The debug log lists `user=/mnt/agent-probe/home/.claude/skills`. The run set `CLAUDE_CONFIG_DIR` to `$HOME/.claude`, so it cannot tell which of the two moves the folder; the [docs](https://code.claude.com/docs/en/env-vars) say `CLAUDE_CONFIG_DIR`. |
| Managed skills are read from `/etc/claude-code/.claude/skills` on Linux. | verified | The debug log lists `managed=/etc/claude-code/.claude/skills`. Mindweaver never writes there. |
| Project skills are also read from every folder above the start directory up to the repository root, and from a nested `.claude/skills` once Claude touches a file below it. | docs-only | [skills](https://code.claude.com/docs/en/skills). Not relied on: Mindweaver installs at the project root. |
| One name at several scopes resolves managed, then personal, then project. | docs-only | [skills](https://code.claude.com/docs/en/skills). Fallback: Mindweaver installs at project scope only ([D-22](architecture.md#d-22-user-scope-installs-come-later)), and `mindweaver doctor` (#96) reports a project skill a user-scope one shadows. |
| A skill folder holding `.claude-plugin/plugin.json` loads as a plugin, with its hooks and MCP servers. | docs-only | [skills](https://code.claude.com/docs/en/skills). Fallback: #34 drops that folder whatever Claude would do with it. |
| A linked worktree with no `.claude/skills` of its own loads the main checkout's. | docs-only | [worktrees](https://code.claude.com/docs/en/worktrees). A caveat of D-02; nothing relies on it. |
| Before 2.1.280, a `manifest.json` in `~/.claude/skills` moved the skills it listed to `.trash`. | docs-only | [skills](https://code.claude.com/docs/en/skills), [changelog](https://code.claude.com/docs/en/changelog). Fallback: Mindweaver never writes in `~/.claude/skills` (D-22). |

### Rules and instruction files

| Fact | Mark | Evidence, or source and fallback |
|---|---|---|
| `CLAUDE.md` is read from the start directory and every folder above it, past the git root. | verified | `claude-parent`. |
| `AGENTS.md` is read natively only when no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` exists in the start directory or above. | verified, in part | `claude-excluded` and `claude-user-memory` (read), `claude-parent` and `claude-local` (not read). No run placed a `.claude/CLAUDE.md`, but the debug log shows the check looking for all three names. Fallback for that one: #44 counts it. Were Claude not to, an `@../AGENTS.md` import there, Mindweaver's or somebody's, would be redundant rather than a second read, since an import never loads the file twice ([memory](https://code.claude.com/docs/en/memory)). But where `.claude/CLAUDE.md` is the only file that stops Claude and it gives the fallback, tracked by git in a local project, reached through a link, or one of which git cannot say whether it tracks it (`Fallback::Tracked`, `Linked`, `Untold`), Claude would read `AGENTS.md` natively as well as every rule in its rules folder, so each rule about every file twice. A `claude-local` run with a `.claude/CLAUDE.md` in place of the `CLAUDE.local.md` would settle it. |
| That check is made by a built-in plugin, `cc-plugin-agents-md@builtin`. | verified | Listed in the init message's plugins; the debug log's lines come from it. The [memory](https://code.claude.com/docs/en/memory) docs' "Project instructions" setting lives under that plugin's ID. |
| That check most likely stops below `/`: a `CLAUDE.md` at `/` itself does not count. | verified, in part | Debug: `found 0 of 4 directories` for the `CLAUDE.md` names from a start directory four levels below `/` (`claude-excluded`, and `claude-user-memory` in run 1), `found 0 of 5 directories` from one five levels below (`claude-user-memory`, run 2). Each count is one short of the directories up to and including `/`; no run could put a file at `/` to show it outright. #44's walk stops below `/` to match, which a unit test holds to these counts. Fallback: were Claude to count `/`, a `CLAUDE.md` there with none nearer would keep the rules about every file from Claude, since #44 would take the block for read; only an administrator can put one there. |
| An `AGENTS.md` excluded through `info/exclude` is read. | verified | `claude-excluded`. |
| Native reading looks for `AGENTS.md` and `.claude/AGENTS.md` in every folder from the start directory up, and reads each one it finds. | verified, in part | The debug log shows the search: `AGENTS.md, .claude/AGENTS.md found 1 of 4 directories`. No run had more than one, so reading each is docs-only ([memory](https://code.claude.com/docs/en/memory)). Fallback: #42 leaves no block of rules in an `AGENTS.md` above a linked project, or in a workspace's root where no linked project is, which Claude would otherwise read in every project below. |
| `AGENTS.local.md`, `AGENTS.override.md` and anything under `.agents/` are never read as instructions. | docs-only | [memory](https://code.claude.com/docs/en/memory). Fallback: Mindweaver writes none of them; the `AGENTS.override.md` warning (#42) is about Codex. |
| A `CLAUDE.md` holding `@AGENTS.md` loads `AGENTS.md` once, also when that line sits between Mindweaver's markers. | verified | `claude-import`, both runs; in run 2 the line sat between markers, as #44 writes it. Each time one `include` record in the hook, no native read in the debug log, and the token counted once. |
| An `@` import's relative path is taken from the file that holds it, not from the start directory, so `.claude/CLAUDE.md` brings in the project's `AGENTS.md` as `@../AGENTS.md`. | docs-only | [memory](https://code.claude.com/docs/en/memory). Fallback: #44 imports into `.claude/CLAUDE.md` only when the project's own `CLAUDE.md` is not there or may not be edited. Were the path taken from the start directory, that import would name the `AGENTS.md` one folder above the project, and the block would miss Claude; a `claude-import` run with the line in `.claude/CLAUDE.md` would settle it. |
| `CLAUDE.md` and `.claude/CLAUDE.md` in one folder are both read. | docs-only | [memory](https://code.claude.com/docs/en/memory) names both places for a project's instructions. Fallback: as for the row above, #44 imports into `.claude/CLAUDE.md` only when `CLAUDE.md` cannot take the import, and an import already in either file stays where it is. |
| Import parsing skips Markdown code spans and fenced code blocks, and a path ends at the first space, so `@AGENTS.md` alone on a line is an import anywhere but in a fence. A `CLAUDE.local.md` is treated as a `CLAUDE.md` is. | docs-only | [memory](https://code.claude.com/docs/en/memory). Fallback: #44 takes such a line somebody wrote in `CLAUDE.md`, `.claude/CLAUDE.md` (as `@../AGENTS.md`) or `CLAUDE.local.md` for how Claude reaches the block, and writes no import. Were Claude not to follow one it counts, the rules about every file would miss Claude, so it counts only a line of its own, outside a fence and outside an HTML comment over several lines, whose imports no run tried. A `claude-import` run with the line in `CLAUDE.local.md`, and one with it in a fence, would settle it. |
| A `CLAUDE.local.md` alone, excluded, is read and stops `AGENTS.md` being read. | verified | `claude-local`. |
| The user memory file (`$CLAUDE_CONFIG_DIR/CLAUDE.md`) is read, once, and does not count for the `AGENTS.md` check, also when the walk up from the project passes the home. | verified | `claude-user-memory`, run 2, with the repository under the temp home: the hook records the file once, as `User`; the debug log found none of the `CLAUDE.md` names in five directories and loaded `AGENTS.md` natively. Run 1 had the repository outside the home, where the walk never met the file. |
| `.claude/rules` files do not count as a `CLAUDE.md` for that check. | verified | `claude-excluded`: the rules and `AGENTS.md` both loaded. |
| `AGENTS.md` read natively fires no `InstructionsLoaded` hook. | verified | `claude-excluded`: `AGENTS.md` reached the model and the hook recorded only the rule. The debug log's `AGENTS.md loaded: <path>` is the record instead. |
| After Claude reads a file in a subfolder, the `AGENTS.md` check also looks in that subfolder. | verified | `claude-touch`, debug: `found 0 of 1 directories` right after reading `src/lib.rs`. |
| `.claude/rules/**/*.md` is searched recursively. | verified | The rules sat two folders down, in `.claude/rules/mindflayer/probe/`. |
| User rules are `~/.claude/rules/**/*.md`, loaded before project rules. | docs-only | [memory](https://code.claude.com/docs/en/memory). Whether `CLAUDE_CONFIG_DIR` moves the folder, as it moves user skills, is not documented. Fallback: Mindweaver writes rules at project scope only ([D-22](architecture.md#d-22-user-scope-installs-come-later)), so a user rule is read beside them, and the run's temp home had none. |
| A rule without `paths:`, excluded, loads at session start. | verified | `claude-excluded`: hook `session_start`. |
| A rule with `paths:`, excluded, does not load at start, and loads when Claude reads a matching file. | verified | `claude-excluded` (absent), then `claude-touch`: hook `path_glob_match`, triggered by `src/lib.rs`, globs `["src/**/*.rs"]`. |
| Front matter is stripped before a rule is injected, and `paths` is the only key read. | docs-only | [memory](https://code.claude.com/docs/en/memory). Fallback: harmless either way; #43 keeps the front matter in the file. |
| Block-level HTML comments are stripped before injection, and the text between two comment lines is kept. So #42's block reaches Claude without its markers. | verified | Run 2, `claude-excluded` (native), `claude-import` (imported) and `claude-user-memory` (native): the token inside the rule's marker never reached the model, and the rule text below it did. The [memory](https://code.claude.com/docs/en/memory) docs say the same. |
| `AGENTS.md` is read natively since 2.1.277 (2.1.281 on Bedrock, Vertex, Foundry, gateways and with telemetry off). | docs-only | [changelog](https://code.claude.com/docs/en/changelog). The run had non-essential traffic off and read it on 2.1.309. Fallback: the import (#44) covers older versions when a `CLAUDE.md` exists; an older Claude in a project without one is documented, not handled. |
| The "Project instructions" setting can switch native `AGENTS.md` reading off or on beside `CLAUDE.md`. | docs-only | [memory](https://code.claude.com/docs/en/memory). Out of #44's scope, which assumes the default. |
| How an empty `CLAUDE_CONFIG_DIR` is read. | docs-only | The [docs](https://code.claude.com/docs/en/env-vars) are silent, and no run tried one. Fallback: Mindweaver reads an empty variable as no folder, as it reads an empty `$MINDWEAVER_HOME`, so if the agent takes it for unset and reads its default folder, a check leaves that folder out; `--claude-config-dir` names it outright. An [open question](architecture.md#open-questions). |

### The Claude Code runs

Two runs, the same day, with the same binary on the same host. Run 1 did
every scenario. Run 2 came after review and did again only the three whose
files changed, with `PROBE_ONLY='claude-excluded claude-import
claude-user-memory'`: `claude-excluded` and `claude-import` with Mindweaver's
marked block in `AGENTS.md`, and `claude-user-memory` with its repository
moved under the temp home. `claude-touch`, `claude-parent` and `claude-local`
stand from run 1; their kept files meet every verdict added since (the tools
called, the turns taken and the debug log's count of native `AGENTS.md`
reads).

The runs spelled the markers and Claude's rules folder as Mindflayer wrote
them, `mindflayer:` and `.claude/rules/mindflayer` (D-26), and are recorded
as they ran: what they verified is of a comment and a folder under
`.claude/rules`, whatever word either holds, so it holds of `mindweaver:`
and `.claude/rules/mindweaver` alike. The probe's scripts spell them as
Mindweaver writes them since #18, so a run made again puts the new names
where these records show the old.

- **When:** 2026-10-26; run 1 from 18:43 to 18:44 UTC, run 2 from 19:18:24
  to 19:18:40 UTC.
- **Binary:** Claude Code 2.1.309, the native Linux x86-64 build
  (`~/.local/share/mise/installs/claude/2.1.309/claude`).
- **Model:** `--model haiku`, which resolved to `claude-haiku-5-5`.
- **Host:** Linux 7.2.8, bubblewrap 0.13.1, git 2.56.0.
- **Cost:** run 1 six `claude -p` runs, 0.0101 USD; run 2 three, 0.0054 USD.
  Nine in all, against the owner's cap of about ten.
- **Login:** `~/.claude/.credentials.json` copied in, valid for another 255
  minutes in run 1 and 220 in run 2, and refreshed in neither.
- **User settings in the temp home:** only the `InstructionsLoaded` hook,
  appending each record to `results/<run>.loaded.jsonl`.

The environment, cleared first and then set to:

```
HOME=/mnt/agent-probe/home
XDG_CONFIG_HOME=/mnt/agent-probe/home/.config
XDG_DATA_HOME=/mnt/agent-probe/home/.local/share
XDG_STATE_HOME=/mnt/agent-probe/home/.local/state
XDG_CACHE_HOME=/mnt/agent-probe/home/.cache
CLAUDE_CONFIG_DIR=/mnt/agent-probe/home/.claude
CODEX_HOME=/mnt/agent-probe/home/.codex
OPENCODE_CONFIG_DIR=/mnt/agent-probe/home/.config/opencode
PATH=/usr/bin:/bin
LANG=C.UTF-8
PROBE_REAL_HOME=<the operator's home, hidden by the namespace>
PROBE_ONLY=claude-excluded claude-import claude-user-memory     (run 2 only)
PROBE_CLAUDE_BIN=<the binary above>
```

and, for each `claude` only, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`
(telemetry, error reports and the updater off: the run needs the API and
nothing else) and `PROBE_SCENARIO=<run>`, which names the hook's log.

Every run was, from inside the scenario's directory:

```
claude -p <prompt> --model haiku --output-format json --verbose \
  --permission-mode dontAsk --no-session-persistence --max-budget-usd 0.10 \
  --debug-file /mnt/agent-probe/results/<run>.debug <extra>
```

with, as the extra:

- `--tools Read --allowedTools Read --max-turns 3` for `claude-touch`, the
  one run that has to read a file, which had no other tool;
- `--tools Skill --max-turns 2` for the runs that list in run 2, whose init
  messages name `Skill` as the only tool;
- `--max-turns 2` alone for the runs that list in run 1, which therefore kept
  Claude's default tools, all 22 of them in the init message. They were asked
  to call none, and the kept JSON shows each answered in one turn with no
  tool call, but nothing held them to it then.

The script now fails any run that calls a tool it was not meant to, or takes
more turns than that needs: one for a run that lists, two for `claude-touch`.
Every call in both runs met that.

The prompt for the runs that list:

```
This is an automated configuration probe. Do not call any tool. Reply with exactly two lines and nothing else.
Line 1: "TOKENS:" followed by every distinct string that starts with "PROBE-" and appears anywhere in your system prompt, memory files, rules or skill descriptions, each followed by how many times it appears there, in parentheses, as in TOKEN-A(1) TOKEN-B(2); or "none".
Line 2: "SKILLS:" followed by the name of every skill you can invoke, separated by spaces; or "none".
```

The prompt for `claude-touch`:

```
This is an automated configuration probe. Call the Read tool exactly once, on src/lib.rs, and call no other tool. Then reply with exactly one line and nothing else: "TOKENS:" followed by every distinct string that starts with "PROBE-" and appears anywhere in your system prompt, memory files, rules, skill descriptions or tool results, separated by spaces; or "none".
```

Every skill file's description reads `Probe skill. If asked for probe tokens,
include <token>.`, its body the same with `-BODY` appended to the token. Every
other file under test holds the one line `If asked for probe tokens, include
<token>.`, except where a run below says otherwise. The 17 skills every init
message listed besides the probe ones are Claude's own (`deep-research`,
`dataviz`, `update-config`, ...).

In run 2, an `AGENTS.md` that should reach the model holds such a line and
then Mindweaver's block with one rule, as #42 will render it, with a token of
its own inside the rule's marker (`<n>` is the scenario's number):

```
If asked for probe tokens, include PROBE-AGENTS-MD-<n>.

<!-- mindflayer:begin -->
<!-- mindflayer:rule PROBE-AGENTS-MARKER-<n> -->
If asked for probe tokens, include PROBE-AGENTS-BLOCK-<n>.
<!-- mindflayer:end -->
```

A run where `AGENTS.md` should not reach the model keeps the plain line, so a
stripped block cannot make its verdict pass for the wrong reason.

#### `claude-excluded`: checks 1, 3, 4 and 6

A fresh repository at `/mnt/agent-probe/stage/claude-excluded`:

```
AGENTS.md                                    PROBE-AGENTS-MD-1, and in run 2 the block
.claude/skills/probe-claude-skill/SKILL.md   PROBE-CLAUDE-SKILL-1
.claude/skills/probe-linked-skill            -> ../../.agents/skills/probe-linked-skill
.agents/skills/probe-linked-skill/SKILL.md   PROBE-LINKED-SKILL-1
.agents/skills/probe-agents-skill/SKILL.md   PROBE-AGENTS-SKILL-1
.claude/rules/mindflayer/probe/unscoped.md   PROBE-RULE-UNSCOPED-1
.claude/rules/mindflayer/probe/scoped.md     PROBE-RULE-SCOPED-1, under paths: ["src/**/*.rs"]
src/lib.rs                                   PROBE-FILE-1, in a comment
```

with this block appended to `.git/info/exclude`, as Mindweaver's local mode
writes one:

```
# mindflayer:begin
# project .
/AGENTS.md
/.agents/skills/probe-agents-skill/
/.agents/skills/probe-linked-skill/
/.claude/skills/probe-claude-skill/
/.claude/skills/probe-linked-skill
/.claude/rules/mindflayer/probe/unscoped.md
/.claude/rules/mindflayer/probe/scoped.md
# mindflayer:end
```

Git agreed every one was ignored, in both runs:

```
$ git status --porcelain --ignored --untracked-files=all
?? src/lib.rs
!! .agents/skills/probe-agents-skill/SKILL.md
!! .agents/skills/probe-linked-skill/SKILL.md
!! .claude/rules/mindflayer/probe/scoped.md
!! .claude/rules/mindflayer/probe/unscoped.md
!! .claude/skills/probe-claude-skill/SKILL.md
!! .claude/skills/probe-linked-skill
!! AGENTS.md
```

The answers, run 1 and then run 2:

```
TOKENS: PROBE-RULE-UNSCOPED-1(1) PROBE-AGENTS-MD-1(1) PROBE-CLAUDE-SKILL-1(1) PROBE-LINKED-SKILL-1(1)
SKILLS: probe-claude-skill probe-linked-skill dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

```
TOKENS: PROBE-CLAUDE-SKILL-1(1) PROBE-LINKED-SKILL-1(1) PROBE-RULE-UNSCOPED-1(1) PROBE-AGENTS-MD-1(1) PROBE-AGENTS-BLOCK-1(1)
SKILLS: probe-claude-skill probe-linked-skill dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

`PROBE-AGENTS-BLOCK-1`, the rule text inside the block, reached the model;
`PROBE-AGENTS-MARKER-1`, inside the rule's marker, did not. The rest was the
same in both runs:

- Init message: `probe-claude-skill` and `probe-linked-skill` once each, no
  `probe-agents-skill`, and Claude's own 17.
- `InstructionsLoaded`: `session_start Project .../.claude/rules/mindflayer/probe/unscoped.md`,
  and nothing else.
- Debug log: `$.fs.ancestors (cc-plugin-agents-md): CLAUDE.md, .claude/CLAUDE.md, CLAUDE.local.md found 0 of 4 directories`,
  then `AGENTS.md, .claude/AGENTS.md found 1 of 4 directories`, then
  `no CLAUDE.md found; AGENTS.md loaded: /mnt/agent-probe/stage/claude-excluded/AGENTS.md`.

#### `claude-touch`: check 6, on touching a matching file

Run 1 only. The same repository, then with a plain `AGENTS.md`, asked to read
`src/lib.rs` first; it called Read once and answered in its second turn:

```
TOKENS: PROBE-FILE-1 PROBE-RULE-UNSCOPED-1 PROBE-AGENTS-MD-1 PROBE-RULE-SCOPED-1
```

- `InstructionsLoaded`: `session_start` for `unscoped.md`, then
  `path_glob_match` for `scoped.md`, with `trigger_file_path`
  `.../src/lib.rs` and `globs` `["src/**/*.rs"]`.
- Debug log, after the read: `AGENTS.md, .claude/AGENTS.md found 0 of 1 directories`
  and the same for the `CLAUDE.md` names, for `src/`.

#### `claude-import`: check 4, `CLAUDE.md` importing `AGENTS.md`

A fresh repository, nothing excluded. In run 1, `CLAUDE.md` held
`@AGENTS.md` and then `PROBE-CLAUDE-MD-2`, and `AGENTS.md` held
`PROBE-AGENTS-MD-2`. In run 2, `AGENTS.md` had the block, and `CLAUDE.md`
held the import between markers, as #44 writes it:

```
<!-- mindflayer:begin -->
@AGENTS.md
<!-- mindflayer:end -->

If asked for probe tokens, include PROBE-CLAUDE-MD-2.
```

The answers, run 1 and then run 2:

```
TOKENS: PROBE-CLAUDE-MD-2(1) PROBE-AGENTS-MD-2(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

```
TOKENS: PROBE-CLAUDE-MD-2(1) PROBE-AGENTS-MD-2(1) PROBE-AGENTS-BLOCK-2(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`, in both: `session_start Project .../claude-import/CLAUDE.md`,
  then `include Project .../claude-import/AGENTS.md`, once.
- Debug log, in both: no `AGENTS.md loaded` line, so no native read beside
  the import.

#### `claude-parent`: check 4, a `CLAUDE.md` above the git root

Run 1 only. `/mnt/agent-probe/stage/claude-parent/CLAUDE.md` holding
`PROBE-PARENT-CLAUDE-MD-3`, the stand-in for `~/Work`, and a fresh repository
below it at `claude-parent/repo` with `AGENTS.md` holding
`PROBE-AGENTS-MD-3`. Claude started in the repository.

```
TOKENS: PROBE-PARENT-CLAUDE-MD-3(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start Project .../claude-parent/CLAUDE.md`
  only.
- Debug log: no `AGENTS.md loaded` line.

#### `claude-local`: `CLAUDE.local.md` alone

Run 1 only. A fresh repository with `CLAUDE.local.md` holding
`PROBE-CLAUDE-LOCAL-MD-4` and `AGENTS.md` holding `PROBE-AGENTS-MD-4`, both
excluded in a block as above. The run did not ask git about them; the script
now does.

```
TOKENS: PROBE-CLAUDE-LOCAL-MD-4(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start Local .../claude-local/CLAUDE.local.md`
  only.
- Debug log: no `AGENTS.md loaded` line.

#### `claude-user-memory`: the user memory file does not count

A fresh repository with an `AGENTS.md`, and `$CLAUDE_CONFIG_DIR/CLAUDE.md`,
that is `/mnt/agent-probe/home/.claude/CLAUDE.md`, holding
`PROBE-USER-CLAUDE-MD-5` for this run only.

In run 1 the repository was `/mnt/agent-probe/stage/claude-user-memory` and
`AGENTS.md` held `PROBE-AGENTS-MD-5`. The temp home is not above that
folder, so Claude's walk never met the memory file: the run showed that the
file loads and that `AGENTS.md` was still read, not that the check passes
over the file.

```
TOKENS: PROBE-USER-CLAUDE-MD-5(1) PROBE-AGENTS-MD-5(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start User /mnt/agent-probe/home/.claude/CLAUDE.md`.
- Debug log: `found 0 of 4 directories` for the `CLAUDE.md` names, then
  `no CLAUDE.md found; AGENTS.md loaded: .../claude-user-memory/AGENTS.md`.

In run 2 the repository was `/mnt/agent-probe/home/work/claude-user-memory`,
under the temp home as real projects sit under `~`, so a walk that reaches
the home meets the memory file there under the name `.claude/CLAUDE.md`, one
of those the check looks for. `AGENTS.md` had the block.

```
TOKENS: PROBE-USER-CLAUDE-MD-5(1) PROBE-AGENTS-MD-5(1) PROBE-AGENTS-BLOCK-5(1)
SKILLS: dataviz update-config keybindings-help code-review simplify fewer-permission-prompts loop schedule claude-api workflow-authoring run plugin-authoring init security-review
```

- `InstructionsLoaded`: `session_start User /mnt/agent-probe/home/.claude/CLAUDE.md`,
  once, and not again as a project file.
- Debug log: `CLAUDE.md, .claude/CLAUDE.md, CLAUDE.local.md found 0 of 5 directories`,
  then `AGENTS.md, .claude/AGENTS.md found 1 of 5 directories`, then
  `no CLAUDE.md found; AGENTS.md loaded: /mnt/agent-probe/home/work/claude-user-memory/AGENTS.md`.
  Six directories lead from the repository up to `/`, and five were
  searched. With run 1's four of five, the likeliest reading is that the walk
  stops below `/` and so did search the home, where it passed over the memory
  file; either way, that file did not stop the read, which is what #44 needs.

#### `git-symlink`: excluding a symlinked skill folder

Git only, no agent. A repository with `.claude/skills/linked` a symlink to
`../../.agents/skills/linked`: with `/.claude/skills/linked/` as the only line
of `info/exclude`, `git check-ignore` did not report it ignored; with
`/.claude/skills/linked` it did. Git never follows a symlink, so to it the
link is a file, and a pattern ending in `/` matches only directories.

#### The real home, before and after

| Path | Before and after, in each run |
|---|---|
| `~/.agents` | a folder, unchanged |
| `~/.codex` | a folder, unchanged |
| `~/.claude/skills` | a folder, unchanged |
| `~/.claude/rules` | absent both times |
| `~/.claude/CLAUDE.md` | absent both times |
| `~/.claude.json` | a file, unchanged |

Unchanged means the same names, link targets and file contents, by SHA-256.

## Codex

Nothing here was run. The issue targets Codex 0.16x; these facts are from the
documentation and source as of 2026-10-26. Codex's documentation moved from
developers.openai.com/codex to learn.chatgpt.com.

### Skills

| Fact | Mark | Source, and the fallback |
|---|---|---|
| Project skills are `.agents/skills/<name>/SKILL.md` in every folder from the start directory up to the repository root. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Fallback: the target table (#28, `TargetTable::standard` in `apps/core/src/agent.rs`) is one table in core, so moving Codex's target is a one-line change; a `codex.sh` check before #34 ships settles it. |
| User skills are `$HOME/.agents/skills`, taken from the OS home, not from `CODEX_HOME`. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills); source, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Fallback: Mindweaver does not install at user scope (D-22); `--agents-dir` points checks at it. |
| `/etc/codex/skills` (admin) and bundled system skills are read too. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Not used. |
| `<project>/.codex/skills` (trusted projects only) and `$CODEX_HOME/skills` (deprecated) are still read. | docs-only | Source, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Never targeted ([D-07](architecture.md#d-07-one-copy-per-agent-folder)). |
| `.agents/skills` is probed with no trust check. | docs-only | Source, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Fallback: were it gated, skills would reach Codex only in trusted projects, and the agents left unreached are reported (D-02). |
| Two skills with one name are both shown, not merged. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Fallback: D-07's copies are identical; `mindweaver doctor` (#96) reports shadowing. |
| Symlinked skill folders are followed. | docs-only | [build-skills](https://learn.chatgpt.com/docs/build-skills). Not relied on: installs are copies. |
| Ignored files are read: discovery checks the disk and does no gitignore filtering. | docs-only | Source search only, [host_roots.rs](https://github.com/openai/codex/blob/main/codex-rs/ext/skills/src/host_roots.rs). Fallback: shared mode (D-02), where installed copies are plain files in the tree. |
| A `name` over 64 characters or an empty `description` rejects the skill. | docs-only | Source, [parser.rs](https://github.com/openai/codex/blob/main/codex-rs/skills/src/parser.rs). Fallback: Mindweaver holds names to 64 characters and requires a description when it writes. |

### Rules

| Fact | Mark | Source, and the fallback |
|---|---|---|
| Project `AGENTS.md` is read from the project root (the git root, by `project_root_markers`) down to the start directory, at most one file per folder (`AGENTS.override.md`, then `AGENTS.md`, then `project_doc_fallback_filenames`), concatenated root first. Without a project root, only the start directory. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md). Fallback: #42 warns when an `AGENTS.override.md` hides the file and when there is no git root above. |
| The global file is `$CODEX_HOME/AGENTS.override.md`, else `$CODEX_HOME/AGENTS.md`. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md). Not written by Mindweaver. |
| The project chain shares a 32 KiB budget (`project_doc_max_bytes`) and a file past it is truncated; the global file is not counted. | docs-only | Source, [agents_md.rs](https://github.com/openai/codex/blob/main/codex-rs/core/src/agents_md.rs). Fallback: #42 warns as the chain nears the budget; counting the global file too would only warn earlier. |
| There is no `@`-import. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md). Why D-03 writes rule text into `AGENTS.md` itself. |
| `.claude/rules` is not read: the instruction files are the `AGENTS.md` chain above, and Codex's own "rules" are Starlark command policy under `rules/`, not instructions. | docs-only | [agents-md](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [rules](https://learn.chatgpt.com/docs/agent-configuration/rules). Fallback: #43 warns that a path-scoped rule reaches Claude only; were Codex to read it after all, the warning would be too cautious, never wrong about Claude. |
| A project marked `trust_level = "untrusted"` skips project `AGENTS.md`; one with no trust entry still reads it. | docs-only | Source, [agents_md.rs](https://github.com/openai/codex/blob/main/codex-rs/core/src/agents_md.rs), [config_toml.rs](https://github.com/openai/codex/blob/main/codex-rs/config/src/config_toml.rs). Check 5 was not run. Fallback: `mindweaver doctor` (#96) reads Codex's config and says a project it marks `untrusted` (`doctor_codex_distrusted`). |
| An ignored `AGENTS.md` is read. | docs-only | Source search only. Fallback: shared mode, or `edit_tracked` for a tracked `AGENTS.md`, which no exclude touches. |
| `CODEX_HOME` moves `~/.codex`. | docs-only | [config-reference](https://learn.chatgpt.com/docs/config-file/config-reference). Fallback: `--codex-home` names the folder outright. |
| How an empty `CODEX_HOME` is read. | docs-only | The docs are silent. The source requires a set `CODEX_HOME` to name an existing folder ([home-dir](https://github.com/openai/codex/blob/main/codex-rs/utils/home-dir/src/lib.rs)), so an empty one may be refused rather than taken for unset; not run. Fallback: Mindweaver reads an empty variable as no folder, as it reads an empty `$MINDWEAVER_HOME`, so if the agent takes it for unset and reads its default folder, a check leaves that folder out; `--codex-home` names it outright. An [open question](architecture.md#open-questions). |

### Plugins and marketplaces

Read on 2026-11-10 in the [build plugins](https://developers.openai.com/codex/plugins/build)
page and Codex's source (`codex-rs/core-plugins/src/marketplace.rs`,
`manifest.rs` and `loader.rs`, `codex-rs/utils/plugins/src/plugin_namespace.rs`),
and the [Agent Plugins](https://agent-plugins.org/specification) 1.0.0
specification, for #104's gather of a Codex marketplace. Nothing was run.

| Fact | Mark | Source, and the fallback |
|---|---|---|
| A repository's catalog is `.agents/plugins/marketplace.json`; Codex also reads `.agents/plugins/api_marketplace.json`, `.claude-plugin/marketplace.json` and `.cursor-plugin/marketplace.json`, in that order. | docs-only | Source, `MARKETPLACE_MANIFEST_RELATIVE_PATHS`. Fallback: a gather reads `.claude-plugin/marketplace.json`, then `.agents/plugins/marketplace.json`, each in the dialect its entries are written in, so a Codex catalog at either place reads; one kept only in the other two places is said to have no catalog. |
| An entry's source is a `./` path, as a string or `{"source": "local", "path"}`, a `url` (with an optional `path`), a `git-subdir` or `npm`; any other kind, `github` among them, is skipped with a warning, and so is a source that does not resolve, a path without `./` or climbing out among them. | docs-only | Source, `RawMarketplaceManifestPluginSource` and `resolve_plugin_source`; the build page says an entry that cannot be resolved is skipped "instead of failing the whole marketplace". Fallback: a gather skips the same with a warning of its code (`marketplace_entry_unresolved`), so were Codex to fetch more, a gather would only take less than it. |
| `policy` and `category` are required of every entry for a catalog Codex lists, and read with defaults when missing. | docs-only | The build page; source, `#[serde(default)]`. Fallback: a gather reads neither, so their absence or their values change nothing. |
| A plugin's manifest is a root `plugin.json` declaring an Agent Plugins `$schema`, else the first of `.codex-plugin/plugin.json`, `.claude-plugin/plugin.json` and `.cursor-plugin/plugin.json`. | docs-only | Source, `find_plugin_manifest_path` and `DISCOVERABLE_PLUGIN_MANIFEST_PATHS`. Fallback: a gather reads the first three in that order, which is where the skills a gather takes are said; Cursor's is read by none. |
| A Codex manifest's `skills` is a `./` path or a list of them, each walked whole for a `SKILL.md`, and `skills/` when it lists none; its MCP servers are in `.mcp.json`, or the file its `mcpServers` names. | docs-only | Source, `resolve_manifest_paths`, `plugin_skill_roots` and `default_mcp_config_paths`. Fallback: a gather takes the folders directly in each, a folder holding a `SKILL.md` a skill with all it holds, which reads every plugin of openai/plugins as Codex does; a skill nested deeper under a folder holding none is not taken (an [open question](architecture.md#open-questions)). |
| An Agent Plugins package's skills are each folder directly in its `skills/` holding a `SKILL.md`, never deeper, and no manifest moves them; its MCP servers are in `mcp.json`. | docs-only | The specification, sections 6.1, 7.1 and 7.2.1; Codex's source, `SkillDiscoveryMode::DirectChildren`. Fallback: a gather reads it so, and refuses a version of the format it does not know (`marketplace_plugin_schema_unknown`). |

## OpenCode

Nothing here was run, neither the installed V2 nor V1 through
`npx opencode-ai@1.19.4`. OpenCode has two maintained lines that both install
a binary named `opencode`: V1, 1.19.x (latest 1.19.4, 2026-10-22, docs at
opencode.ai/docs), and V2, 2.1.x (latest tag 2.1.4, 2026-10-23, docs at
opencode.ai/v2/docs) ([releases](https://github.com/anomalyco/opencode/releases)).

### Skills

| Fact | Mark | Source, and the fallback |
|---|---|---|
| V1 reads `.opencode/skills`, `.claude/skills` and `.agents/skills` from the start directory up to the git worktree, plus `~/.config/opencode/skills`, `~/.claude/skills`, `~/.agents/skills` and config `skills.paths`/`skills.urls`. | docs-only | [V1 skills](https://opencode.ai/docs/skills/); source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: it reads both folders D-07 fills, so either copy reaches it. |
| V1 finds skills with a recursive `skills/**/SKILL.md` glob that includes hidden folders. | docs-only | Source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: Mindweaver stages in `.mind/tmp`, never inside a skills folder. |
| V1 keys a skill by its front matter `name`, and 1.19.4 does not enforce the documented name rules. | docs-only | Source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: Mindweaver requires the name to match the folder, so V1 and V2 see one ID. |
| V1 given one name twice logs `duplicate skill name`, and the winner is whichever copy finishes parsing last. | docs-only | Source, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Check 2 was not run. Fallback: D-07 writes both copies identically in one staged step, so either winner is the same skill. |
| `OPENCODE_DISABLE_CLAUDE_CODE` or `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS` turns `.claude/skills` off in V1. | docs-only | [V1 rules](https://opencode.ai/docs/rules/). Fallback: the `.agents/skills` copy still reaches it. |
| V2 reads `~/.config/opencode/skills`, `~/.claude/skills`, `~/.agents/skills`, `.opencode/skills`, `.claude/skills` and `.agents/skills` from the start directory up to the project root, plus `skills` config entries. A skill's ID is its folder name. Later sources win: built-in, `.claude/skills`, `.agents/skills`, `~/.config/opencode/skills`, project `.opencode/skills`, config; within a group the global folder comes first, then the farthest ancestor down. Hidden folders and `node_modules` are skipped, six levels deep at most. | docs-only | [V2 skills](https://opencode.ai/v2/docs/skills/). Fallback: the copies are identical, so which wins does not matter. `~/.agents/skills` beating a project's `.claude/skills` is why doctor reports shadowing (D-22). |
| Whether `$CLAUDE_CONFIG_DIR` moves the `~/.claude/skills` either line reads. | docs-only | The [V1 skills](https://opencode.ai/docs/skills/) and [V2 skills](https://opencode.ai/v2/docs/skills/) docs name `~/.claude/skills` and are silent on `$CLAUDE_CONFIG_DIR`; not run. Fallback: Mindweaver takes `~/.claude/skills` whatever `$CLAUDE_CONFIG_DIR` says, as it takes `~/.config/opencode` whatever `$XDG_CONFIG_HOME` says (`AgentHomes::claude_dir_in_home` while the variable moves Claude's folder), so if OpenCode follows it, `mindweaver doctor` (#96) compares a project's skills with the wrong folder while it is set; `--claude-config-dir` names the one folder outright, for both agents. |
| Ignored files are read: V1's scan has no gitignore option. | docs-only | Source search only, [skill/index.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/skill/index.ts). Fallback: shared mode (D-02). |

### Rules

| Fact | Mark | Source, and the fallback |
|---|---|---|
| V1 reads every `AGENTS.md` from the start directory up to the worktree, and a project `CLAUDE.md` (then `CONTEXT.md`) only when there is none; globally `~/.config/opencode/AGENTS.md`, else `~/.claude/CLAUDE.md`. `@` references are not followed. | docs-only | [V1 rules](https://opencode.ai/docs/rules/); source, [instruction.ts](https://github.com/anomalyco/opencode/blob/v1.19.4/packages/opencode/src/session/instruction.ts). Fallback: none needed; a project with a block has an `AGENTS.md`, so V1 never falls back to its `CLAUDE.md`. |
| V2 reads only `AGENTS.md`: the global `~/.config/opencode/AGENTS.md` and every `AGENTS.md` from the start directory up toward home (up to the project root outside home), nested ones when it reads below them; `instructions` after them. | docs-only | [V2 instructions](https://opencode.ai/v2/docs/instructions/), [migration](https://opencode.ai/v2/docs/migrate-v1/). Fallback: #42 leaves no block of rules in an `AGENTS.md` above a linked project, or in a workspace's root where no linked project is, since V2, and Claude too, would read it in every project below. |
| `OPENCODE_DISABLE_PROJECT_CONFIG=1` makes V2 skip project `AGENTS.md`. | docs-only | [V2 instructions](https://opencode.ai/v2/docs/instructions/). Not relied on. |
| Neither line reads `.claude/rules`: V1's instruction files are `AGENTS.md`, `CLAUDE.md` in its place and `instructions`, V2's `AGENTS.md` and `instructions`. | docs-only | [V1 rules](https://opencode.ai/docs/rules/), [V2 instructions](https://opencode.ai/v2/docs/instructions/). Fallback: as for Codex, #43's "reaches Claude only" warning would at worst be too cautious. |
| An ignored `AGENTS.md` is read. | docs-only | Source search only. Fallback: shared mode. |

### OpenCode's own folders

| Fact | Mark | Source, and the fallback |
|---|---|---|
| `OPENCODE_CONFIG_DIR` adds a folder loaded after `~/.config/opencode` and the project's `.opencode`; it does not replace `~/.config/opencode`. | docs-only | [config](https://opencode.ai/docs/config/). Fallback: still an [open question](architecture.md#open-questions); `AgentHomes` takes both folders since `mindweaver doctor` (#96), the first check to read OpenCode's, read them: `~/.config/opencode` beneath the folder the variable names (`opencode_dir_in_home`), each read for skills and servers, and an option naming the one folder outright. Were the variable to replace it after all, doctor would read a folder too many, a finding too cautious, never one missed. |
| Whether `$XDG_CONFIG_HOME` moves `~/.config/opencode`. | docs-only | The [config](https://opencode.ai/docs/config/) docs name `~/.config/opencode` and are silent on `$XDG_CONFIG_HOME`; not run. Fallback: Mindweaver takes `~/.config/opencode` whatever `$XDG_CONFIG_HOME` says, so if OpenCode follows it, a check reads the wrong folder while it is set; `--opencode-config-dir` names the right one outright. An [open question](architecture.md#open-questions). |
| Its global config is `config.json`, `opencode.json` and `opencode.jsonc` in its own folder, and a project's config is merged over it key by key, its `mcp` servers among it, so a server of one name in both is read as one made of both. | docs-only | [config](https://opencode.ai/docs/config/), which merges config files rather than replacing one with another; the three names are the ones the probe's guards keep out of the temp home ([MCP servers](#how-they-are-run)). Fallback: `mindweaver doctor` (#96) reports a server of an installed one's name in any of them; were OpenCode to take the project's whole, the finding would be too cautious, never wrong about what reaches OpenCode. |
| How an empty `OPENCODE_CONFIG_DIR` is read. | docs-only | The [config](https://opencode.ai/docs/config/) docs are silent; not run. Fallback: Mindweaver reads an empty variable as no folder, as it reads an empty `$MINDWEAVER_HOME`, so if the agent takes it for unset and reads its default folder, a check leaves that folder out; `--opencode-config-dir` names it outright. An [open question](architecture.md#open-questions). |

## MCP servers

**Codex's pending.** The probe's MCP checks (#18) were written on 2026-11-04,
and Claude Code's and OpenCode's ran on 2026-11-05, at the owner's word
([the MCP runs](#the-mcp-runs)). Codex's were not run: they are paid calls
on a Codex login, and the owner has no Codex subscription. Its results below
read *pending*, each of its facts stands as its source says, and #91 is
built on them as they stand, its fallbacks in place. A run fills in the
*Result* column of each table and the record of the run, and moves a fact's
mark to verified.

[D-10](architecture.md#d-10-an-mcp-server-is-one-owned-entry-per-agent) puts
each server in an agent's project config as one entry of Mindweaver's, and
[D-11](architecture.md#d-11-mcp-secrets-are-environment-references) writes
every secret as a reference to an environment variable, in the agent's own
syntax. The checks ask the agents what those rest on: that each project
config loads, and what each agent makes of a reference in each place a
renderer would put one. Both decisions say what each result would change in
#87 and #90 to #94.

### What a definition is held to for each agent

What #87 holds an MCP server's definition to, per agent, and where each rule
stands now that #18's runs have been made:

| Agent | Rule | Mark | Source, and what would move it |
|---|---|---|---|
| Every agent | A server is keyed by letters, digits, hyphens and underscores, `^[A-Za-z0-9_-]+$`, in its project's config. | docs-only | #87 and [D-10](architecture.md#d-10-an-mcp-server-is-one-owned-entry-per-agent); none of #18's checks covers it |
| Claude Code | `workspace`, `claude-in-chrome` and `computer-use` are names it keeps for servers of its own, refused as a server's. | docs-only | #87; none of #18's checks covers it |
| Claude Code | A name colliding case-insensitively with `Claude Preview` or `Claude Browser` is refused as a server's. No key holds the space, so Mindweaver takes it to cover each in any case with the space written as `_`, `-` or nothing (`claude_preview`, `claude-preview`, `claudepreview`). | docs-only | #87; none of #18's checks covers it, nor which spelling Claude compares |
| Claude Code | `anthropic-skills` names the skills Anthropic provides, and is not refused as a server's: Mindweaver warns of it. | docs-only | #87; none of #18's checks covers it |
| OpenCode | `servers` and `timeout` are keys its `mcp` table holds itself, beside the servers its V1 shape keys there, so neither keys a server. | docs-only | #87 and [D-12](architecture.md#d-12-opencode-gets-the-v2-mcp-shape); none of #18's checks covers it |
| Claude Code | An environment reference is written `${VAR}`. | verified | [D-11](architecture.md#d-11-mcp-secrets-are-environment-references); `claude-mcp` below, expanded in `args`, `env`, `url` and `headers` |
| Codex | There is no interpolation: a variable is named only by `env_vars`, `env_http_headers` and `bearer_token_env_var`. | docs-only | [D-11](architecture.md#d-11-mcp-secrets-are-environment-references); #18's Codex runs were not made (no Codex subscription) |
| OpenCode | An environment reference is written `{env:VAR}`. | verified | [D-11](architecture.md#d-11-mcp-secrets-are-environment-references); the OpenCode runs below, expanded in the command, the environment, the URL and the headers |
| Codex | A secret argument cannot be expressed. | docs-only | [D-11](architecture.md#d-11-mcp-secrets-are-environment-references) and #87; #18's Codex runs were not made (no Codex subscription) |
| Codex | A secret inside a URL cannot be expressed. | docs-only | [D-11](architecture.md#d-11-mcp-secrets-are-environment-references) and #87; #18's Codex runs were not made (no Codex subscription) |
| Codex | A secret header cannot be expressed other than as its whole value (`env_http_headers`) or as `Authorization: Bearer ${VAR}` (`bearer_token_env_var`). | docs-only | [D-11](architecture.md#d-11-mcp-secrets-are-environment-references) and #87; #18's Codex runs were not made (no Codex subscription) |
| Codex | A secret in a program's environment cannot be expressed other than as the whole value of the variable named as the one it is read from: `env_vars` passes a variable on under its own name. | docs-only | [config-reference](https://learn.chatgpt.com/docs/config-file/config-reference) and #91, by D-11's reasoning; #18's Codex runs were not made (no Codex subscription), and `codex-mcp-trusted`'s `PROBE_MCP_ENV_SOURCE` would show the name a variable is passed on under |
| Codex | A server over sse cannot be expressed. | docs-only | #87; #18's Codex runs were not made (no Codex subscription) |
| Codex | A server of a name `$CODEX_HOME/config.toml` holds under `mcp_servers` is merged with the project's key by key, so the project's is refused. | docs-only | [config-reference](https://learn.chatgpt.com/docs/config-file/config-reference) and #91; `codex-mcp-collision` below was not run, and would say whether a warning would do |
| Codex | `experimental_environment` and `environment_id`, by their names the environment it runs a server in, are never written, an extra for Codex giving one refused. | docs-only | #91; none of #18's checks covers them |
| OpenCode | A server over sse cannot be expressed in its V2 shape, the one Mindweaver writes. | docs-only | #87 and [D-12](architecture.md#d-12-opencode-gets-the-v2-mcp-shape); none of #18's checks covers it |
| Claude Code | A program's `cwd` cannot be expressed: its entry is written without it. | docs-only | #87; none of #18's checks covers it |
| Claude Code | A variable it keeps from a remote server, a credential of its own or of a tool on the machine (`ANTHROPIC_API_KEY`, `NPM_TOKEN`, `SONAR_TOKEN`, `HTTPS_PROXY`, `OTEL_*`, `GIT_CONFIG_*` and more, each under `INPUT_` too where Claude looks there), is read as empty where `${VAR}` is expanded in a remote server's `url` or `headers`, so an input of an address read from one cannot be expressed. | docs-only | #90, read from Claude Code 2.1.318's binary, in any case as it compares them, and listed in `apps/core/src/mcp/blanked.rs`; none of #18's checks names one. A run putting one of them in `claude-mcp`'s header would move it. A name Mindweaver lists that Claude passes on refuses Claude a server it could reach, and one it misses reaches the server empty, which Claude's log says ("credential variable(s) that are never expanded toward a remote server"). |
| Claude Code | `${` written as it is where no input stands, in the command, the arguments, the environment, the URL or the headers, cannot be expressed: Claude reads it as a reference (`${VAR}`, `${VAR:-default}`) and gives the server a variable's value, a default or nothing in its place. | verified, the command docs-only | #90; `claude-mcp` below expands a `${VAR}` the file holds in `args`, `env`, `url` and `headers`, and the [mcp](https://code.claude.com/docs/en/mcp) docs say the command too. Claude Code 2.1.318's binary takes `${`, a name and `}`, or `:-`, a default and `}`; Mindweaver refuses any `${`, since a reference it writes for an input after one may close it. A run with a `${VAR}` in the command would move that part. |
| OpenCode | `{env:` written as it is where no input stands, in the command, the arguments, the environment, the folder, the URL or the headers, cannot be expressed in either line: both fill `{env:NAME}` in over the config file's text before they parse it, a variable's value or nothing in its place. Nor can `{file:`, which they fill in with a file's contents. | verified for `{env:}` on 2.2.5, 2.1.3 and 1.19.4; the folder and `{file:` docs-only | #92; the OpenCode runs below expand a `{env:}` the file holds in the command, its first word too, the environment, the URL and the headers, inside a longer value too, and the [config](https://opencode.ai/docs/config/) docs say `{file:path}` is filled in the same way. Mindweaver refuses any `{env:` or `{file:`, rather than only what OpenCode's pattern takes, since a reference it writes for an input after one may close it. A run with a `{file:}` in a header would move that part. |
| OpenCode | V1's shape, `mcp.<name>`, has no key for a program's folder, where V2's has `cwd`: an entry written in V1's, into a file that keeps its servers so, is written without it. | docs-only | #92; V1's [MCP servers](https://opencode.ai/docs/mcp-servers/) docs give a local server `type`, `command`, `environment`, `enabled` and `timeout`, and none of #18's checks writes a folder. Mindweaver says what it leaves out (`opencode_mcp_v1_cwd`); were V1 to take `cwd`, a server it could be given whole starts where OpenCode starts it. |


### What an agent asks before it starts one

What an install says of each agent once it has written a server's entry
(#95, `Agent::starts_a_server`), from what the runs and the docs below
say:

| Agent | Before it starts a project's server | Mark | Source |
|---|---|---|---|
| Claude Code | Asks somebody working in a session to approve the project's servers; a `-p` run starts every one the file names, approved or not. | docs-only for the session, verified for `-p` | [Claude Code's `.mcp.json`](#claude-codes-mcpjson); `claude-mcp` on 2.1.318 started `probe-unapproved` |
| Codex | Reads `.codex/config.toml` only in a project its user's config marks trusted, which an install says as `codex_mcp_untrusted` when nothing marks it. | docs-only | [Codex's `.codex/config.toml`](#codexs-codexconfigtoml); `codex-mcp-trusted` and `codex-mcp-untrusted` pending |
| OpenCode | Asks nothing: it connects to every server the project's file names. | verified for `opencode mcp list` | [OpenCode's `opencode.json`](#opencodes-opencodejson); the OpenCode runs on 2.2.5, 2.1.3 and 1.19.4 connected each stand-in, nothing approved |

### Running them

```bash
PROBE_ONLY='claude-mcp claude-mcp-untyped claude-mcp-comment claude-mcp-comma
  codex-mcp-trusted codex-mcp-untrusted codex-mcp-collision
  opencode-mcp-v2 opencode-mcp-v1-shape opencode-mcp-service opencode-mcp-1-19' \
  scripts/agent-probe/probe.sh claude codex opencode
```

That is seven paid calls at most. Claude's four are `claude -p` on haiku with
a cap of 0.10 USD each, where #17's runs cost about 0.002 USD apiece.
Codex's three are `codex exec` on `gpt-6-luna` at low effort, and the third
makes none if Codex refuses its configuration before the session starts.
OpenCode's four call no model. Without `PROBE_ONLY`, the same command also
runs Claude's seven runs of #17 again, six of them paid.

That command runs the V2 on `PATH`, 2.2.5 at the time of writing. #18 and
D-12 name 2.1.x, and the owner's 2.1.3 is installed beside it, so a second
pass runs OpenCode's three V2 runs on it, at no cost, since none calls a
model:

```bash
PROBE_ONLY='opencode-mcp-v2 opencode-mcp-v1-shape opencode-mcp-service' \
  PROBE_OPENCODE_BIN=~/.local/share/mise/installs/opencode/2.1.3/node_modules/.bin/opencode \
  scripts/agent-probe/probe.sh opencode
```

Its results go in the 2.1.3 half of each V2 cell below.

Beyond what #17 needs:

- `python3` in `/usr/bin` or `/bin`, for the stand-in server. Each prepare
  asks for it before anything is paid for.
- Codex's native binary, on `PATH` or named by `PROBE_CODEX_BIN` (`mise which
  codex` prints it), logged in with the login kept in
  `$CODEX_HOME/auth.json` (by default `~/.codex/auth.json`), not the
  keyring. Only that file is copied in. A ChatGPT login whose access token
  has less than 15 minutes left, or that was last refreshed seven days ago
  or more, is refused, as Claude's close to expiry is: a refresh inside the
  run could rotate the refresh token the real session holds.
  `PROBE_CODEX_MODEL` names another model if the account does not offer
  `gpt-6-luna`.
- OpenCode's V2 binary, on `PATH` or named by `PROBE_OPENCODE_BIN`. The
  launcher script of an npm or mise install is followed to the binary its
  `@opencode/cli` package ships; a version manager's shim is refused. No
  OpenCode login is needed or copied.
- `npm` on `PATH`, for 1.19.4: the script fetches `opencode-linux-x64@1.19.4`
  (about 80 MB) with `npm pack` into `target/agent-probe-run`, with npm's
  cache there too, and runs its `package/bin/opencode`, which is what `npx
  opencode-ai@1.19.4` runs once its postinstall has copied it into place.
  The baseline package is taken on a CPU without AVX2, and the musl one on
  musl, as that postinstall does.

### How they are run

- **A stand-in server.** `scripts/agent-probe/mcp_server.py`, standard
  library Python run as `python3 -I`, answers as much MCP as an agent needs
  to load it: `initialize`, with the protocol version the client asked for,
  `tools/list` with one tool, `ping`, and empty lists of prompts and
  resources. Over stdio it reads one message a line. Over HTTP it binds
  127.0.0.1 on a port the kernel picks and answers each POST as JSON, which
  Streamable HTTP allows; a GET for an event stream is refused with 405, and
  one under `/.well-known/` with 404, so no client takes the server for an
  OAuth one. It writes down, one JSON object a line, the arguments and
  environment it was started with, every method it was sent, and the path
  and headers of each request. A value is kept whole only when it is the
  probe's own, all of which say "probe", or empty; anything else, a variable
  or a header the agent added, is kept as its length, so no credential
  reaches the results.
- **References with values of their own.** A run puts a reference in each
  place it is about, naming one of these variables, which are set for the
  agent's command alone:

  | Variable | Value | Referred to from |
  |---|---|---|
  | `PROBE_MCP_ARG` | `probe-arg-value` | the stdio server's `--arg` argument |
  | `PROBE_MCP_ENV_SOURCE` | `probe-env-value` | the stdio server's `PROBE_MCP_ENV` variable; in Codex also named in `env_vars` |
  | `PROBE_MCP_URL_PART` | `probe-url-value` | the http server's URL, as `/probe-http/<reference>` |
  | `PROBE_MCP_HEADER` | `probe-header-value` | the `X-Probe-Header` header |
  | `PROBE_MCP_TOKEN` | `probe-token-value` | `Authorization: Bearer <reference>` |
  | `PROBE_MCP_PYTHON` | the `python3` path | OpenCode only: the command's first word |

  So the stand-in's record says, for each place, whether the agent expanded
  the reference (the value), passed it on as written (the reference), or
  dropped it (empty, or unset), and whether it started the server at all.
- **What loaded.** *loaded* is a stand-in that saw `initialize` and then
  `tools/list`, *initialized* one that saw only the first, *none* one that
  saw neither or never started. Where an agent says what it loaded without a
  model's help, that is kept and held too: Claude's init message, whose
  `mcp_servers` gives each server a status, written once Claude has waited
  for every server to connect, since each run sets
  `MCP_CONNECTION_NONBLOCKING=0` (2.1.318 connects without waiting
  otherwise, and lists a server still connecting as `pending`); OpenCode's
  `opencode mcp list`, which connects to each server and calls no model; and
  Codex's `codex mcp list --json`, which lists the configuration, kept as a
  record only.
- **Nothing from the model.** Claude and Codex are asked for `OK` alone, and
  each run is held to calling no tool: Claude with `--tools ''
  --max-turns 1`, Codex with the sandbox read-only.
- **Guards.** Besides #17's, before each run: no MCP configuration at user
  scope in the temp home (`~/.mcp.json`, OpenCode's user `opencode.json`,
  `opencode.jsonc` or `config.json`, or a server at user or local scope in
  Claude's `.claude.json`), and no MCP config file above a scenario but the
  ones it wrote. What an agent puts at user scope of its own accord, where
  a guard would refuse it, is taken out after each of its commands, so that
  the next run's guard holds. So far that is only the system skills Codex
  0.166 carries and writes to `$CODEX_HOME/skills/.system` when it starts,
  with their marker. Only those names go, and anything else in that folder
  is still refused.
- **Verdicts.** `ok` or `MISMATCH` against the expected result in the tables
  below. A `record` line is a result the issue asks to find out rather than
  confirm, with nothing expected of it, and is never a mismatch.
- **What is kept.** Each run's config files as given (`<run>.given.mcp.json`,
  `<run>.given.codex.config.toml`, `<run>.given.home.config.toml`,
  `<run>.given.opencode.json`), the stand-ins' logs (`<run>.<server>.jsonl`,
  `<run>.http.jsonl`) and what the agent printed, in
  `target/agent-probe-results/<time>/`.

Where a stdio entry's arguments read `<stdio>` below, they are, for a run
named `<run>`:

```
-I /mnt/agent-probe-scripts/mcp_server.py stdio --name probe-stdio \
  --log /mnt/agent-probe/results/<run>.probe-stdio.jsonl
```

with `--arg <reference>` after them where a table checks the argument. Each
http entry's URL is `http://127.0.0.1:<port>/<server>/...`, the port the
run's stand-in was given.

### Claude Code's `.mcp.json`

Claude reads a project's servers from `.mcp.json` at its root, a strict JSON
file with a `mcpServers` map ([mcp](https://code.claude.com/docs/en/mcp)),
and, in a session somebody works in, starts one only once it is approved
(docs-only). A `-p` run starts every server the file names, approved or not
(verified on 2.1.318, `claude-mcp` below). Each run approves its servers by
name in the temp home's user settings, `enabledMcpjsonServers` in
`$CLAUDE_CONFIG_DIR/settings.json`, beside the `InstructionsLoaded` hook. Not
in the project's own `.claude/settings.local.json`, where the approval
dialog writes it: Claude 2.1.318 counts an approval there only in a
workspace trusted explicitly, which a `-p` run's never is, and its own
message says to put it in `~/.claude/settings.json` instead (read from the
binary, docs-only). The same settings set `disableClaudeAiConnectors`, so a
logged-in Claude does not fetch and connect the account's claude.ai
connectors beside the project's servers. #90 writes none of this
(approval is out of its scope); a person approves the servers when Claude
asks.

#### `claude-mcp`: entries with a type, and a reference in each place

A fresh repository with this `.mcp.json`, `probe-stdio` and `probe-http`
approved and `probe-unapproved` not:

```json
{
  "mcpServers": {
    "probe-stdio": {
      "type": "stdio",
      "command": "/usr/bin/python3",
      "args": ["<stdio>", "--arg", "${PROBE_MCP_ARG}"],
      "env": { "PROBE_MCP_ENV": "${PROBE_MCP_ENV_SOURCE}" }
    },
    "probe-http": {
      "type": "http",
      "url": "http://127.0.0.1:<port>/probe-http/${PROBE_MCP_URL_PART}",
      "headers": {
        "X-Probe-Header": "${PROBE_MCP_HEADER}",
        "Authorization": "Bearer ${PROBE_MCP_TOKEN}"
      }
    },
    "probe-unapproved": {
      "type": "stdio",
      "command": "/usr/bin/python3",
      "args": ["<stdio, named probe-unapproved>"],
      "env": { "PROBE_MCP_ENV": "${PROBE_MCP_ENV_SOURCE}" }
    }
  }
}
```

| Check | Expected | Result |
|---|---|---|
| Tools called, turns | none, 1 | none, 1 |
| `probe-stdio` in the init message | `connected` | `connected` |
| `probe-stdio`'s stand-in | loaded | loaded |
| Its `--arg`: `${VAR}` in `args` | `probe-arg-value`, expanded | `probe-arg-value` |
| Its `PROBE_MCP_ENV`: `${VAR}` in `env` | `probe-env-value`, expanded | `probe-env-value` |
| `probe-http` in the init message | `connected` | `connected` |
| `probe-http`'s stand-in | loaded | loaded |
| The path: `${VAR}` in `url` | `/probe-http/probe-url-value` | `/probe-http/probe-url-value` |
| `X-Probe-Header`: `${VAR}` in `headers` | `probe-header-value` | `probe-header-value` |
| `Authorization`: a reference inside a longer value | `Bearer probe-token-value` | `Bearer probe-token-value` |
| `probe-unapproved`'s stand-in | none: not started | **loaded**: started, though nobody approved it |
| `probe-unapproved` in the init message | recorded | `connected` |

Expected from the [mcp](https://code.claude.com/docs/en/mcp) docs, which say
`${VAR}` and `${VAR:-default}` expand in `command`, `args`, `env`, `url` and
`headers`. The last rows say what `-p` makes of an entry nobody approved:
2.1.318 started it and listed it `connected` with the others, its debug log
saying nothing of an approval, so the one result that differs is the
expectation, which the probe now holds the other way. #90 writes no
approval, and in a `-p` session an entry it writes runs as soon as the
file holds it.

#### `claude-mcp-untyped`: an http entry with no type

The same `probe-stdio`, without `--arg`, beside
`"probe-untyped": {"url": "http://127.0.0.1:<port>/probe-untyped/"}`, both
approved. Recorded, not expected: #90 writes `type` always, and needs to
know what one somebody else wrote without it does to the file.

| Record | Result |
|---|---|
| `probe-untyped` in the init message: read as http, refused alone, or absent | absent |
| `probe-untyped`'s stand-in: whether its URL was reached anyway | none: never reached |
| `probe-stdio` in the init message: whether the untyped entry costs the file its other servers | `connected`: it costs nothing else |
| `probe-stdio`'s stand-in | loaded |

#### `claude-mcp-comment` and `claude-mcp-comma`: what strict JSON refuses

The same `probe-stdio`, approved, in a file with a comment:

```
{
  // A comment, which JSON does not allow and JSONC does.
  "mcpServers": {
    "probe-stdio": <the entry>
  }
}
```

and, in the other run, in a file with a trailing comma after it:

```
{
  "mcpServers": {
    "probe-stdio": <the entry>,
  }
}
```

| Record | `claude-mcp-comment` | `claude-mcp-comma` |
|---|---|---|
| `probe-stdio` in the init message: whether the file breaks whole | absent: the file breaks whole | absent: the file breaks whole |
| `probe-stdio`'s stand-in | none | none |

The debug log's lines about the probe's servers and `.mcp.json` are kept
beside each run, with whatever Claude said of the file: for both, `[ERROR]
MCP config is not valid JSON` with the file's path, `scope=project`, and
no server of the file started. A file somebody wrote with a comment or a
trailing comma has none of its servers loaded, which #90 says rather than
repairs.

#### What Mindweaver writes there

`claude_mcp` (#90) renders a definition as one entry, `mcpServers.<name>`,
on what the runs above and the [mcp](https://code.claude.com/docs/en/mcp)
docs say, through `config_entry`, every byte outside the entry kept:

- A program as `{type, command, args, env}` and an address as `{type,
  url, headers}`, `type` always, as `claude-mcp` wrote them, so no entry
  of Mindweaver's is one Claude drops for having none
  (`claude-mcp-untyped`). One somebody else wrote without it costs the
  file nothing else, and refuses nothing.
- Each input as `${VAR}`, the variable it is read from, where its
  placeholder stands in `args`, `env`, `url` or `headers`, whole or inside
  a longer value (`claude-mcp`, verified). The `command` is written as the
  definition holds it: the docs say `${VAR}` expands there too, and the
  binary expands it as it does the arguments, but no input's kind is
  written there and no run tried it, so nothing rests on it (docs-only).
- An input of an address read from a variable Claude keeps from a remote
  server refuses the entry (`variable_blanked`), as the table above says
  (docs-only), and a program's `cwd` is left out (`cwd_unsupported`).
- A `${` the definition writes as it is, where no input stands, refuses
  the entry (`text_expanded`): Claude would read it as a reference
  (`claude-mcp`, and the docs for the command), where the definition
  means the text.
- sse is written, with a warning that the [mcp](https://code.claude.com/docs/en/mcp)
  docs call it deprecated in favour of streamable HTTP (docs-only).
- The file is read as strict JSON, up to 2 MiB. One holding a comment or a
  trailing comma, which loses Claude every server it names
  (`claude-mcp-comment`, `claude-mcp-comma`), is refused
  (`mcp_json_not_strict`) and left as it is, never repaired.
- No approval is written, neither `enabledMcpjsonServers` nor anything
  else: a person approves a project's servers when Claude asks, and a `-p`
  run starts every server the file names, approved or not (`claude-mcp`).

### Codex's `.codex/config.toml`

Not run (no Codex subscription): every result in this section is pending,
and what it expects is what Codex's documentation and binary say.

Codex reads `[mcp_servers.<name>]` from a project's `.codex/config.toml`
only in a trusted project, merges it over `$CODEX_HOME/config.toml`, and has
no interpolation: secrets go through keys that name a variable, `env_vars`
for a stdio server's environment, `env_http_headers` for a header's whole
value and `bearer_token_env_var` for `Authorization: Bearer`
([config-reference](https://learn.chatgpt.com/docs/config-file/config-reference),
docs-only; the 0.166 binary's strings hold every one of those keys). Each
run writes the temp home's `config.toml` whole: `cli_auth_credentials_store
= "file"`, so the copied `auth.json` is what Codex reads, and the project's
trust entry, `[projects."<repo>"] trust_level = ...`. Every run's project
holds:

```toml
[mcp_servers.probe-stdio]
command = "/usr/bin/python3"
args = ["<stdio>", "--arg", "${PROBE_MCP_ARG}"]
env = { PROBE_MCP_ENV = "${PROBE_MCP_ENV_SOURCE}" }
env_vars = ["PROBE_MCP_ENV_SOURCE"]

[mcp_servers.probe-http]
url = "http://127.0.0.1:<port>/probe-http/${PROBE_MCP_URL_PART}"
env_http_headers = { "X-Probe-Header" = "PROBE_MCP_HEADER" }
bearer_token_env_var = "PROBE_MCP_TOKEN"
```

Each run asks `codex mcp list --json` from the project and then, once:

```
codex exec --json --skip-git-repo-check --sandbox read-only \
  --model gpt-6-luna -c 'model_reasoning_effort="low"' <prompt asking for OK>
```

`--skip-git-repo-check`, though each scenario is a repository, so that
exec's own refusal of a folder it does not trust cannot stand in for what
trust does to the servers.

#### `codex-mcp-trusted`

The project marked `trusted`.

| Check | Expected | Result |
|---|---|---|
| The session reached the model | yes | pending |
| Items besides messages | none | pending |
| `probe-stdio`'s stand-in | loaded | pending |
| Its `--arg`: `${VAR}` in `args` | `${PROBE_MCP_ARG}`, as written | pending |
| Its `PROBE_MCP_ENV`: `${VAR}` in `env` | `${PROBE_MCP_ENV_SOURCE}`, as written | pending |
| Its `PROBE_MCP_ENV_SOURCE`, named in `env_vars` | `probe-env-value` | pending |
| Its `PROBE_MCP_HEADER`, named nowhere: whether Codex passes a server only what it names | recorded | pending |
| `probe-http`'s stand-in | loaded | pending |
| The path: `${VAR}` in `url` | `/probe-http/${PROBE_MCP_URL_PART}`, as written (sent percent-encoded) | pending |
| `X-Probe-Header`, from `env_http_headers` | `probe-header-value` | pending |
| `Authorization`, from `bearer_token_env_var` | `Bearer probe-token-value` | pending |
| `codex mcp list --json` | recorded | pending |

#### `codex-mcp-untrusted`

The same project, first with no trust entry at all, for the list alone, then
marked `untrusted`, for the list and the session.

| Check | Expected | Result |
|---|---|---|
| The session reached the model | yes | pending |
| Items besides messages | none | pending |
| `probe-stdio`'s stand-in | none: not started | pending |
| `probe-http`'s stand-in | none: not started | pending |
| `codex mcp list --json`, untrusted | recorded | pending |
| `codex mcp list --json`, no trust entry: what #91's warning is about | recorded | pending |

#### `codex-mcp-collision`

The project marked `trusted`, and the temp home's own `config.toml` naming
the project's stdio server too, as an http one:

```toml
[mcp_servers.probe-stdio]
url = "http://127.0.0.1:<port>/probe-stdio-user/"
```

Codex merges config layers key by key, so the two may make one entry with
both a `command` and a `url`. Recorded, not expected: this is the result
that decides whether #91 refuses a name a lower layer holds or only warns.

| Record | Result |
|---|---|
| Outcome: `whole config refused` (the list and the session both fail before any server starts), `session refused`, `other servers lost`, `project layer wins`, `user layer wins`, `that name dropped` or `both started` | pending |
| `codex mcp list --json` | pending |
| The session's exit status, and whether it reached the model | pending |
| `probe-stdio` as the project's stdio server | pending |
| `probe-stdio` as the user's http server | pending |
| `probe-http`, which collides with nothing: whether the collision costs the other servers | pending |

The first three outcomes break more than the one name, so #91 refuses it,
as it does while no run has said otherwise; the others leave the rest of the
configuration working, so a warning would do.

#### What Mindweaver writes there

`codex_mcp` (#91) renders a definition as one table,
`[mcp_servers.<name>]`, on what the
[config-reference](https://learn.chatgpt.com/docs/config-file/config-reference)
says, through `config_entry` in TOML, every byte outside it kept. None of it
was run, so all of it is docs-only, and every result above stays pending:

- A program as `command`, `args`, `env`, `env_vars` and `cwd`, and an
  address as `url`, `http_headers`, `env_http_headers` and
  `bearer_token_env_var`, each only when it holds something, the
  definition's extras for Codex beside them.
- Each input only where a key names its variable: `env_vars` for a
  program's environment variable that is the input whole under the
  variable's own name, `env_http_headers` for a header that is the input
  whole, and `bearer_token_env_var` for the first `Authorization` header
  holding `Bearer` and the input. Every other value is written as the
  definition holds it, `${VAR}` among it, which Codex reads as the text it
  is (`codex-mcp-trusted` expects `${PROBE_MCP_ARG}` as written).
- A secret written anywhere else refuses the server, as the table above
  says (`secret_in_argument`, `secret_in_url`, `secret_header_composite`,
  `secret_env_composite`, `secret_env_renamed`), and so does sse
  (`sse_unsupported`). An input that is no secret written there refuses it
  too (`codex_mcp_input_in_text`): it could be given only as its value,
  which no install hands the renderer yet (#93).
- Never `experimental_environment` nor `environment_id`: an extra giving
  one refuses the server (`extra_withheld`).
- A name `$CODEX_HOME/config.toml` holds under `mcp_servers` refuses the
  server (`codex_mcp_collision`), docs-only: a run of `codex-mcp-collision`
  would decide whether a warning would do. The Codex home is the one
  Mindweaver is given (`--codex-home`, then `$CODEX_HOME`, then
  `~/.codex`); with none given there is no user's config to read, and
  nothing collides.
- A project that `$CODEX_HOME/config.toml` does not mark `trusted` is
  written all the same, and said (`codex_mcp_untrusted`), since Codex reads
  its file only in a trusted project (`codex-mcp-untrusted` expects neither
  server to start otherwise). Mindweaver looks up `[projects."<path>"]
  trust_level` under the project's root as it is linked and as its links
  resolve to; which path Codex keys a project by, the folder it starts in
  or the repository's root above it, is docs-only, so a project trusted
  under another is warned of. With no Codex home given, nothing marks it
  trusted, and that is said.
- The file is read up to 2 MiB, made with `.codex` when it is not there,
  and taken away with its last server only when Mindweaver made it and
  nothing else is in it; `.codex` stays. A project whose
  `.codex/config.toml` is Codex's own, one at the user's home while
  `$CODEX_HOME` is `~/.codex`, is refused (`codex_mcp_user_scope`).

### OpenCode's `opencode.json`

OpenCode reads `mcp` from the project's `opencode.json`, and fills
`{env:NAME}` in the file's text before it parses it, an unset variable as
empty ([config](https://opencode.ai/docs/config/); 2.2.5's binary does it
with one replace over the text; verified on 2.2.5, 2.1.3 and 1.19.4 by the
runs below). V2 writes servers under
`mcp.servers.<name>`, V1 straight under `mcp.<name>` with `enabled`, and V1
has read V2's shape since 1.18.24 (D-12). Each run gives the project one of
these, V2's shape:

```json
{
  "mcp": {
    "servers": {
      "probe-stdio": {
        "type": "local",
        "command": ["{env:PROBE_MCP_PYTHON}", "<stdio>", "--arg", "{env:PROBE_MCP_ARG}"],
        "environment": { "PROBE_MCP_ENV": "{env:PROBE_MCP_ENV_SOURCE}" }
      },
      "probe-http": {
        "type": "remote",
        "url": "http://127.0.0.1:<port>/probe-http/{env:PROBE_MCP_URL_PART}",
        "headers": {
          "X-Probe-Header": "{env:PROBE_MCP_HEADER}",
          "Authorization": "Bearer {env:PROBE_MCP_TOKEN}"
        }
      }
    }
  }
}
```

or V1's, the same two entries straight under `mcp`, each with
`"enabled": true`. Neither has `oauth`, `timeout` or anything else #92 does
not write. Each run asks `opencode mcp list` from the project, with
`OPENCODE_DISABLE_AUTOUPDATE=1` and `OPENCODE_DISABLE_MODELS_FETCH=1`:

| Run | Binary | Shape | How |
|---|---|---|---|
| `opencode-mcp-v2` | V2: 2.2.5, then 2.1.3 | V2 | a private server in the list's environment: `opencode serve` on a port of its own, asked with `--server`; in 2.1.3, whose list takes no `--server`, the temp home's background server, which the list starts on a port of its own and which is stopped after |
| `opencode-mcp-v1-shape` | V2: 2.2.5, then 2.1.3 | V1 | the same |
| `opencode-mcp-service` | V2: 2.2.5, then 2.1.3 | V2 | through V2's background server, the way a person runs OpenCode, read twice: below |
| `opencode-mcp-1-19` | 1.19.4 | V2 | in process, with `--print-logs --log-level DEBUG`, its data, state and cache folders its own |

A V2 list is asked again, a few seconds apart and five times at most, while
it says `pending` or that no server is configured. The first list a V2
server gives for a folder says none is, in 2.2.5 and 2.1.3 alike: that
request is what boots the folder's services, and their MCP servers are read
from the configuration in the background while it is answered; the second
lists them. So `--standalone` is not used: its server lives for one list,
and every answer it gives is a first one. 1.19.4 gets folders of its own
because V2's runs leave their database in the temp home's, which 1.19.4
refuses to open ("Database is not empty and has no session table").

The background server is the question in `opencode-mcp-service`: whether it
fills `{env:}` from the environment of the client that asks, or from its
own, which V2's `opencode service set env` suggests it keeps. 2.2.5's binary
suggests the second: the server is spawned with the environment of
whichever command starts it, and fills a config file's references from its
own. A server the asking list starts itself holds every variable either
way, so the run reads it twice, each reading in a project of its own:

- **Started apart** (`opencode-mcp-service`). `opencode service start`,
  from an environment holding the two switches and `PROBE_MCP_PYTHON`
  alone, as a server started at login or from another shell would be; then
  the list, from the project with all five variables, asked again as every
  V2 list is (above); then `opencode service stop`. What each reference
  became says whose environment filled it: the values if the list's, empty
  if the server's own. A start that fails skips the list, which would start
  a server from its own environment in its place.
- **Started by the list** (`opencode-mcp-service.inherited`). The list,
  asked the same way, starts the server itself, which then holds the list's
  variables, and the server is stopped after. Every reference is expected to
  expand, as in a private server.

Every `opencode service` command runs without the five variables. Before
anything starts the server, the run gives it a port of its own, one the
kernel has just found free, with `opencode service set port`, which writes
only the temp home's service settings (`~/.config/opencode/service.json`
there). The namespace shares the host's network, and a V2 server otherwise
takes its channel's fixed port, 49374, where the operator's own OpenCode
keeps its server: the probe's could not start, or the operator's could not
while the probe's ran. A `service set port` that fails is a mismatch, and
the run stops there.

Each V2 run's cell holds 2.2.5's result, from the first command under
[Running them](#running-them), then 2.1.3's, from the second.

| Check | Expected | `opencode-mcp-v2` | `opencode-mcp-v1-shape` | `opencode-mcp-service.inherited` | `opencode-mcp-1-19` |
|---|---|---|---|---|---|
| `probe-stdio`'s stand-in, so `{env:}` in the command's first word too | loaded | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| Its `--arg`: `{env:}` in `command` | `probe-arg-value` | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| Its `PROBE_MCP_ENV`: `{env:}` in `environment` | `probe-env-value` | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| `probe-http`'s stand-in | loaded | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| The path: `{env:}` in `url` | `/probe-http/probe-url-value` | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| `X-Probe-Header`: `{env:}` in `headers` | `probe-header-value` | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| `Authorization`: inside a longer value | `Bearer probe-token-value` | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | 2.2.5 ok, 2.1.3 ok | ok |
| Each server's word in the list | recorded | 2.2.5 `connected`, 2.1.3 `connected` | 2.2.5 `connected`, 2.1.3 `connected` | 2.2.5 `connected`, 2.1.3 `connected` | `connected` |

The reading with the server started apart, `opencode-mcp-service`, records
what it is asked to find out rather than expecting it:

| Check | Expected | 2.2.5 | 2.1.3 |
|---|---|---|---|
| `opencode service set port` | exit 0 | ok | ok |
| `opencode service start`, without the five variables | exit 0 | ok | ok |
| `opencode service stop`, before the second reading | exit 0 | ok | ok |
| `probe-stdio`'s stand-in, its first word in either environment | loaded | ok | ok |
| Its `--arg`: `{env:}` in `command` | recorded: `probe-arg-value` from the list's environment, empty from the server's | empty | empty |
| Its `PROBE_MCP_ENV`: `{env:}` in `environment` | recorded: `probe-env-value`, or empty | empty | empty |
| `probe-http`'s stand-in | loaded | ok | ok |
| The path: `{env:}` in `url` | recorded: `/probe-http/probe-url-value`, or `/probe-http/` | `/probe-http/` | `/probe-http/` |
| `X-Probe-Header`: `{env:}` in `headers` | recorded: `probe-header-value`, or empty | empty | empty |
| `Authorization`: inside a longer value | recorded: `Bearer probe-token-value`, or `Bearer` with nothing after it, which the stand-in keeps as its length alone | `Bearer` with nothing after it (6 characters) | `Bearer` with nothing after it (6 characters) |
| Whose environment filled them | recorded: `the list's: 5 of 5 expanded`, `not the list's: 0 of 5 expanded` (the server's own when the five read empty), or `mixed` | `not the list's: 0 of 5 expanded` | `not the list's: 0 of 5 expanded` |
| Each server's word in the list | recorded | `connected`, both | `connected`, both |

#### What Mindweaver writes there

`opencode_mcp` (#92) renders a definition as one entry under `mcp` of the
project's OpenCode config, on what the runs above and the
[config](https://opencode.ai/docs/config/) docs say, through
`config_entry`, every byte outside the entry kept:

- The file is the first of `opencode.json`, `opencode.jsonc`,
  `.opencode/opencode.json` and `.opencode/opencode.jsonc` there is at the
  project's root, and `opencode.json` is made when none is. Each is read
  as JSONC, its comments and trailing commas kept, since OpenCode reads
  JSON and JSONC alike, a `.json` holding a comment included (the
  [config](https://opencode.ai/docs/config/) docs; docs-only, since every
  run above wrote a strict `opencode.json`), up to 2 MiB. An entry
  spliced among trailing commas is written with its own.
- V2's shape, `mcp.servers.<name>`, which V2 and 1.19.4 load
  (`opencode-mcp-v2`, `opencode-mcp-1-19`): a program as `{type: "local",
  command: [command, ...args], environment, cwd}` and an address as
  `{type: "remote", url, headers}`, each key after the command or the
  address only when it holds something, and none of `codemode`,
  `protocol` or `timeout.startup`, nor a key of Mindweaver's.
- V1's shape, `mcp.<name>` with `enabled`, which V2 loads too
  (`opencode-mcp-v1-shape`), only in a file that keeps a server so
  already, and a server of the name is replaced in the shape it stands in.
  V1's has no key for a program's folder, so its `cwd` is left out and
  said (`opencode_mcp_v1_cwd`, docs-only, as the table above says). A name
  written in both shapes is refused (`opencode_mcp_twice`).
- Each input as `{env:VAR}`, the variable it is read from, where its
  placeholder stands in the arguments, the environment, the URL or the
  headers, whole or inside a longer value (the runs above, in each place).
  The command's first word and the folder are written as the definition
  holds them: OpenCode fills a `{env:}` in the first word too, but no
  input's kind is written there.
- An `{env:` or a `{file:` the definition writes as it is, where no input
  stands, refuses the entry (`text_substituted`), as the table above says:
  OpenCode would fill it in, where the definition means the text. So does
  sse (`sse_unsupported`, docs-only).
- The file goes with its last server only when it is the `opencode.json`
  Mindweaver made and nothing else is left in it, a comment somebody wrote
  included. A project whose file is inside an agent's own folder, one
  whose root is `~/.config/opencode` while that is OpenCode's, is refused
  (`opencode_mcp_user_scope`).
- Nothing is said of whose environment fills the references, though V2's
  background server fills them from its own (`opencode-mcp-service`):
  whether Mindweaver says so, and doctor checks the server's variables, is
  the owner's to decide ([D-11](architecture.md#d-11-mcp-secrets-are-environment-references)).
  `mindweaver doctor` (#96) checks the variables set where it runs, and
  says nothing of a V2 server's own.

### Assumptions the run confirms

Nothing was read from the web: what the checks assume beyond each agent's
documentation was read from the installed packages, and a run that
disagrees fails or records it. Claude Code's and Codex's have not run yet;
OpenCode's ran on 2026-11-05 ([the MCP runs](#the-mcp-runs)), and what is
said of it below is as corrected by that run.

- Claude Code 2.1.318: approvals in the user settings count in a `-p`
  run, as they turned out not to need to (a `-p` run starts every server,
  approved or not); the init message lists `.mcp.json` servers in `mcp_servers` with a
  status; `MCP_CONNECTION_NONBLOCKING=0` has it wait for each to connect
  before it is written, as the binary reads the variable and sets it so
  itself where it needs its servers connected first; `--tools ''` turns
  every tool off.
- Codex 0.166.0: `codex exec` takes `--json`, `--skip-git-repo-check`,
  `--sandbox read-only`, `--model` and `-c`, as its help says; `codex mcp
  list --json` exists ("Output the configured servers as JSON"); the
  account offers `gpt-6-luna`, the smallest model the binary's own catalog
  lists; a session starts its servers whether or not the model is reached;
  `cli_auth_credentials_store = "file"` has it read the copied `auth.json`;
  a ChatGPT login is refreshed past about eight days.
- OpenCode 2.2.5, 2.1.3 and 1.19.4: `opencode mcp list` connects to each
  server, as 1.19.4's handler and 2.2.5's status values say (confirmed: both
  stand-ins saw `initialize` and `tools/list`); `opencode serve --port` with
  `OPENCODE_SERVER_PASSWORD` runs a private server a 2.2.5 list reaches with
  `--server`, where 2.1.3's list has neither `--server` nor `--standalone`,
  and both have the `opencode service` commands; 1.19.4 turns `mcp.servers`
  into its own entries, as its binary's conversion does. In V2, `opencode
  service start` returns once the background server answers, `opencode
  service stop` returns once it is gone, and `opencode service set port`
  keeps the port in the service settings under `$XDG_CONFIG_HOME/opencode`,
  which the server reads when it starts; a list finds the server through
  the registration under `$XDG_STATE_HOME/opencode`, both in the temp home,
  so it never reaches the operator's.

### The MCP runs

**OpenCode's**, which call no model, ran by themselves (`probe.sh opencode`),
two passes the same day, as [Running them](#running-them) gives them:

- **When:** 2026-11-05; 2.2.5 and 1.19.4 from 14:07:31 UTC, 2.1.3 from
  14:07:13 UTC.
- **Binaries:** OpenCode 2.2.5 and 2.1.3, the native Linux x86-64 builds of
  the npm package mise installed
  (`~/.local/share/mise/installs/opencode/<version>/.../@opencode/cli/bin/opencode.exe`),
  and `opencode-linux-x64@1.19.4`, fetched from the npm registry into the
  run's temp dir.
- **Host:** Linux 7.2.15, bubblewrap 0.13.1, git 2.56.0, python 3 in
  `/usr/bin` for the stand-ins.
- **Cost:** none. **Logins:** none copied in.
- **Environment:** as below, without the Claude and Codex variables.
- **Results:** every expected one met, in both passes, as the tables under
  [OpenCode's `opencode.json`](#opencodes-opencodejson) say. Every list was
  asked twice in V2, the first answer saying no server was configured, and
  once in 1.19.4.
- **The real home, before and after:** every path the probe watches
  unchanged in both passes.

An earlier pass that day, before the runs were changed to what is described
above, found every server missing: V2's lists were each a first answer
(`--standalone`, and a background server asked once), and 1.19.4 would not
open the database V2 had left in the temp home.

**Claude Code's**, the four MCP runs alone (`PROBE_ONLY='claude-mcp
claude-mcp-untyped claude-mcp-comment claude-mcp-comma' probe.sh claude`):

- **When:** 2026-11-05, from 14:21:33 UTC.
- **Binary:** Claude Code 2.1.318, the native Linux x86-64 build
  (`~/.local/share/mise/installs/claude/2.1.318/claude`).
- **Model:** `--model haiku`, which resolved to `claude-haiku-5-5`.
- **Host:** as OpenCode's.
- **Cost:** four `claude -p` runs, 0.0029 USD in all (0.0007 to 0.0008
  each), against a cap of 0.10 USD each.
- **Login:** `~/.claude/.credentials.json` copied in, valid for another 422
  minutes, and not refreshed.
- **Results:** every expected one met but one: `probe-unapproved`, which
  no setting approved, was started and listed `connected` with the others,
  so a `-p` run gates nothing on approval. The probe now expects that. The
  three runs that record found an http entry without a type dropped alone
  and never reached, and a comment or a trailing comma losing the whole
  file, which Claude's debug log calls `MCP config is not valid JSON`.
- **The real home, before and after:** every path the probe watches
  unchanged.

**Codex's** were not run: the owner has no Codex subscription. A run would
take Codex 0.166.0, the one on `PATH` at the time of writing, with
`gpt-6-luna` at low effort and the owner's `auth.json` copied in, three
`codex exec` calls at most.

**The environment** of every run, cleared first: #17's, plus
`PROBE_CODEX_BIN`, `PROBE_CODEX_MODEL`, `PROBE_OPENCODE_BIN` and
`PROBE_OPENCODE_V1_BIN` where their agent runs; for each agent's command
the five `PROBE_MCP_*` values above, for Claude's
`MCP_CONNECTION_NONBLOCKING=0`, and for OpenCode's `PROBE_MCP_PYTHON`,
`OPENCODE_DISABLE_AUTOUPDATE=1` and `OPENCODE_DISABLE_MODELS_FETCH=1`.

## What says an agent works in a project

`mindweaver scan --suggest` ranks a git repository holding something an agent
reads at its root above one holding nothing (#58), by name alone:
`Agent::reads_at_root` in `apps/core/src/agent.rs`. Each name is the folder
of a place the tables above say the agent reads a kind from, an instruction
file they list, or a file [D-10](architecture.md#d-10-an-mcp-server-is-one-owned-entry-per-agent)
may put the agent's project MCP servers in. The fallback for every docs-only row
is the same: a name wrongly counted ranks a repository a little high, and
one missed ranks it a little low, but the suggestion still lists every
repository it finds, so neither hides one.

| Agent | Names at a project's root | Mark | Source, and the fallback |
|---|---|---|---|
| Claude Code | `.claude`, `CLAUDE.md`, `CLAUDE.local.md`, `AGENTS.md` | verified | [Skills](#skills) and [rules](#rules-and-instruction-files) above: `.claude/skills`, the instruction files, and `AGENTS.md` read natively when no `CLAUDE.md` is there. |
| Claude Code | `.mcp.json`, its project's MCP servers | verified | [Claude Code's `.mcp.json`](#claude-codes-mcpjson) above: Claude Code 2.1.318 started every server it named (`claude-mcp`); D-10 writes into it. |
| Codex | `.agents`, `AGENTS.md`, `AGENTS.override.md` | docs-only | [Skills](#skills-1) and [rules](#rules) above. |
| Codex | `.codex`, which holds a trusted project's `config.toml` and `skills` | docs-only | [config-reference](https://learn.chatgpt.com/docs/config-file/config-reference), and `.codex/skills` above; D-10 writes into `.codex/config.toml`. Counted though `.codex/skills` is never targeted: a folder of Codex's own shows Codex is used there, trusted or not. |
| OpenCode | `.agents`, `.claude`, `.opencode`, `AGENTS.md`, and `CLAUDE.md`, which V1 reads when there is no `AGENTS.md` | docs-only | [Skills](#skills-2) and [rules](#rules-1) above. |
| OpenCode | `opencode.json` and `opencode.jsonc`, its project's configuration, whose two in `.opencode` count as `.opencode` | docs-only | [config](https://opencode.ai/docs/config/), and [OpenCode's `opencode.json`](#opencodes-opencodejson) above; D-10 writes into the first of the four there is, as JSONC, or a new `opencode.json`. |

## Facts the roadmap cites

Where every fact #28, #34, #42 to #44, #87, #90 to #94, #96 and #104 build on stands. A
docs-only one carries its fallback in the tables above, one of #87's what
would move it, and a pending one the check that will settle it, under [MCP
servers](#mcp-servers).

| Issue | Fact | Mark |
|---|---|---|
| #28 | Claude reads `.claude/skills` and not `.agents/skills`. | verified |
| #28 | Codex reads `.agents/skills`. | docs-only |
| #28 | OpenCode V1 and V2 read `.agents/skills`, and `.claude/skills` too. | docs-only |
| #28 | `.codex/skills` and `~/.codex/skills` are deprecated or undocumented. | docs-only |
| #34 | An excluded copy in `.claude/skills` loads in Claude. | verified |
| #34 | An excluded copy in `.agents/skills` loads in Codex and OpenCode. | docs-only |
| #34 | A `.claude-plugin/` folder in a skill makes Claude load it as a plugin. | docs-only |
| #34 | OpenCode given identical copies in both folders shows one skill without an error. | docs-only |
| #34, D-07 | A relative symlink in `.claude/skills` loads once in Claude; its exclude entry must not end in `/`. | verified |
| #42 | The root `AGENTS.md` reaches Claude (when #44 says so), Codex and OpenCode. | verified for Claude; docs-only for Codex and OpenCode |
| #42 | An excluded `AGENTS.md` is read by Claude. | verified |
| #42 | Rule text inside the marked block reaches Claude, natively or imported, and the markers, HTML comments, are stripped. | verified |
| #42 | Codex's 32 KiB budget covers the project chain only. | docs-only |
| #42 | An `AGENTS.override.md` hides `AGENTS.md` from Codex. | docs-only |
| #42 | Without a git root, Codex reads only the start directory. | docs-only |
| #42 | OpenCode V2 reads `AGENTS.md` up to home. | docs-only |
| #42 | Claude reads every `AGENTS.md` from the start directory up, so one above linked projects reaches each of them. | docs-only; the search of those folders is seen in Claude's debug log |
| #43 | An excluded `.claude/rules/mindflayer/<route>.md` loads in Claude, with and without `paths:`. | verified |
| #43 | A rule with `paths:` loads only once Claude reads a matching file; one without loads at start. | verified |
| #43 | `.claude/rules` is searched recursively, and its files do not count as a `CLAUDE.md`. | verified |
| #43 | Claude strips a rule's front matter before injecting it. | docs-only |
| #43 | Only Claude honours path-scoped rules: Codex and OpenCode read no `.claude/rules`. | verified for Claude; docs-only for Codex and OpenCode |
| #44 | Claude reads `AGENTS.md` only with no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` in the start directory or above. | verified for `CLAUDE.md` and `CLAUDE.local.md`; `.claude/CLAUDE.md` seen only among the names the check looks for |
| #44 | A `CLAUDE.md` above the git root counts. | verified |
| #44 | A `CLAUDE.md` at `/` itself does not count, and the predicate's walk stops below `/` to match. | verified in part: read from the debug log's directory counts, not tried |
| #44 | The user memory file does not count, also for a project under the home. | verified |
| #44 | `@AGENTS.md` in a `CLAUDE.md`, between Mindweaver's markers, loads it once. | verified |
| #44 | `@../AGENTS.md` in `.claude/CLAUDE.md` loads the project's `AGENTS.md`, and both files are read. | docs-only |
| #44 | An `@AGENTS.md` line of its own, outside a fence, is followed in any of Claude's files, `CLAUDE.local.md` included. | verified in `CLAUDE.md` outside the markers; docs-only for the fence and `CLAUDE.local.md` |
| #44 | A `CLAUDE.local.md` alone stops `AGENTS.md` being read. | verified |
| #44 | Native `AGENTS.md` reading needs Claude 2.1.277 or later. | docs-only |
| #87 | Every agent keys an MCP server by `^[A-Za-z0-9_-]+$`. | docs-only: none of #18's checks covers it |
| #87 | Claude Code keeps `workspace`, `claude-in-chrome`, `computer-use`, and names colliding case-insensitively with `Claude Preview` and `Claude Browser`, for servers of its own. | docs-only: none of #18's checks covers it |
| #87 | Claude Code names Anthropic's own skills `anthropic-skills`, and refuses no server by it. | docs-only: none of #18's checks covers it |
| #87 | OpenCode holds `servers` and `timeout` in its `mcp` table itself, so neither keys a server. | docs-only: none of #18's checks covers it |
| #87 | Codex reads a secret only through `env_vars`, `env_http_headers` and `bearer_token_env_var`: no secret argument, none inside a URL, and no secret header but a whole value or `Authorization: Bearer`. | docs-only: none of #18's checks covers it |
| #87 | Neither Codex nor OpenCode's V2 shape takes sse. | docs-only: none of #18's checks covers it |
| #87 | Claude Code's entry for a program has no `cwd`. | docs-only: none of #18's checks covers it |
| #87, #90 | Claude expands `${VAR}` in `args`, `env`, `url` and `headers`, also inside a longer value. | verified (`claude-mcp`) |
| #87, #91 | Codex passes `${VAR}` in `args`, `env` and `url` on as written, so it cannot express a secret argument or a secret inside a URL. | pending (`codex-mcp-trusted`) |
| #90 | An approved `.mcp.json` entry with a `type` loads; what an http entry without one, a comment and a trailing comma do to the file. | verified (`claude-mcp`): it loads, and so does one nobody approved, in `-p`; an http entry without a type is dropped alone, never reached; a comment or a trailing comma loses the whole file (the three runs after it) |
| #90 | Claude expands `${VAR}` in a program's `command`. | docs-only: [mcp](https://code.claude.com/docs/en/mcp) and the binary; no run tried it, and no input is written there |
| #90 | Claude reads a credential it keeps from a remote server as empty in its `url` and `headers` (`apps/core/src/mcp/blanked.rs`). | docs-only: read from 2.1.318's binary; none of #18's checks names one |
| #90 | Claude's docs call the sse transport deprecated. | docs-only: [mcp](https://code.claude.com/docs/en/mcp) |
| #91 | A trusted project's `.codex/config.toml` loads, and an untrusted one's does not. | pending (`codex-mcp-trusted`, `codex-mcp-untrusted`) |
| #91 | `env_vars`, `env_http_headers` and `bearer_token_env_var` fill a value from a variable. | pending (`codex-mcp-trusted`) |
| #91 | A server of the same name in `$CODEX_HOME/config.toml`, with another transport, breaks the whole configuration or leaves one layer's standing. | pending, recorded (`codex-mcp-collision`) |
| #91 | `env_vars` passes a variable on to a program under its own name, so a secret in a program's environment is given only whole, under its variable's name. | pending (`codex-mcp-trusted`, its `PROBE_MCP_ENV_SOURCE`) |
| #91 | Codex keys a trusted project in `[projects."<path>"]` by its root, as it is linked or as its links resolve to. | docs-only: none of #18's checks trusts a project under another path, and the warning that leans on it costs no entry |
| #92 | V2's `mcp.servers` loads in V2 (2.2.5 and 2.1.3) and in 1.19.4, and V1's `mcp.<name>` loads in V2. | verified (`opencode-mcp-v2`, `opencode-mcp-1-19`, `opencode-mcp-v1-shape`) |
| #92 | `{env:}` expands in the command, the environment, the URL and the headers, through V2's background server too when that server holds the variable. | verified (all four OpenCode runs, the V2 ones on 2.2.5 and 2.1.3) |
| #92, #96 | Whose environment V2's background server fills `{env:}` from: the asking client's, or its own. | verified: its own, on 2.2.5 and 2.1.3; a server started apart filled none of the five from the asking list (`opencode-mcp-service`) |
| #92 | Both lines read a project's config from `opencode.json`, `opencode.jsonc`, `.opencode/opencode.json` or `.opencode/opencode.jsonc`, each as JSONC, comments and trailing commas included. | docs-only: [config](https://opencode.ai/docs/config/); every run wrote a strict `opencode.json` |
| #92 | Both lines fill `{file:path}` in over the config's text, as they fill `{env:}`. | docs-only: [config](https://opencode.ai/docs/config/); no run wrote one |
| #92 | V1's shape has no key for a program's folder; V2's has `cwd`. | docs-only: V1's [MCP servers](https://opencode.ai/docs/mcp-servers/) docs, and V2's shape as #92 gives it; no run wrote a folder |
| #93, #94 | Each agent loads its project's servers from the one file the target table names (`Agent::mcp_file`): `.mcp.json`, `.codex/config.toml` (trusted), OpenCode's first of four there is, `opencode.json` when none is. | verified for `.mcp.json` and `opencode.json`; docs-only for `.codex/config.toml`, whose runs were not made (every MCP run) |
| #96 | Codex reads a project's `.codex/config.toml` only in a project its user's config marks trusted, and one marked `untrusted` loses its `AGENTS.md` too. | docs-only: `codex-mcp-trusted` and `codex-mcp-untrusted` pending, and Codex's check 5 not run |
| #96 | An `AGENTS.override.md` hides `AGENTS.md` from Codex, Codex reads only the start directory without a git root, and cuts the chain off at 32 KiB. | docs-only, as #42's rows above |
| #96 | A `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` in or above a project keeps Claude from its `AGENTS.md`. | verified for `CLAUDE.md` and `CLAUDE.local.md`, as #44's rows above |
| #96 | OpenCode reads `.agents/skills`, `.claude/skills` and `.opencode/skills`, and given one name twice loads one of them. | docs-only |
| #96, D-22 | At user scope Claude reads `<config dir>/skills`, Codex `~/.agents/skills`, and OpenCode `~/.config/opencode/skills`, the one in the folder `$OPENCODE_CONFIG_DIR` names, `~/.claude/skills` whatever `$CLAUDE_CONFIG_DIR` says, and `~/.agents/skills`, and one name at both scopes resolves differently in each. | verified in part for Claude's folder; docs-only for the rest |
| #96 | Codex reads a project's servers over `$CODEX_HOME/config.toml`'s key by key, and OpenCode merges a project's config over its global config key by key. | docs-only |
| #104 | Codex keeps a catalog at `.agents/plugins/marketplace.json` and reads one at `.claude-plugin/marketplace.json` too, and skips an entry it cannot resolve with a warning. | docs-only, as [plugins and marketplaces](#plugins-and-marketplaces) above |
| #104 | Codex reads a plugin by a root `plugin.json` declaring an Agent Plugins `$schema`, else `.codex-plugin/plugin.json`, else `.claude-plugin/plugin.json`, and an Agent Plugins package's skills only directly in its `skills/`. | docs-only, as above |
