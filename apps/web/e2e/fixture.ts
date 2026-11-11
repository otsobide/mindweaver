/*
 * The workspace the panel is served over, built as somebody would build it:
 * a command at a time, with the binaries under test, so what the panel draws
 * is what those commands made and `mindweaver list --json` says of it. Nothing is
 * written by hand but the folders `mind init` is run in, as `mkdir` would,
 * and, outside the workspace, the text an `--editor` copies in, as an editor
 * would save it.
 */
import { spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { catalogs, SKILL } from './catalogs.ts'
import { environment, succeed, type Places } from './environment.ts'

/** The workspace's own, each as `mindweaver add` takes it: kind, name, description. */
export const LIBRARY = [
  ['skill', 'review', 'Read a change before it lands'],
  ['skill', 'lint', 'Tidy the code before a commit'],
  ['rule', 'git/commits', 'Write commit messages that say why']
] as const

/** The projects it links, each by its entry. */
export const PROJECTS = ['api', 'web'] as const

/**
 * `workspace`, holding something of each thing the views compare with the
 * listing:
 *
 * - its library holds the skills `review` and `lint` and the rule
 *   `git/commits`;
 * - `api`, for Claude and Codex and local, authors the skill `deploy` and
 *   the rule `style/comments`, and received `review`;
 * - `web`, for Claude and OpenCode and shared, received `review` and `lint`.
 *
 * Neither project is a repository: a local one says, in a note, that nothing
 * keeps its copies out of git, which is no failure. No rule is installed,
 * since a rule's block in an `AGENTS.md` outside a work tree is a warning,
 * that Codex reads it only when started there.
 */
export function build(at: Places): void {
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  for (const [kind, name, description] of LIBRARY) {
    succeed(at, 'mindweaver', at.workspace, ['add', kind, name, description])
  }

  project(at, 'api', ['--agents', 'claude,codex'])
  const api = join(at.workspace, 'api')
  succeed(at, 'mind', api, ['add', 'skill', 'deploy', 'Ship a release'])
  succeed(at, 'mind', api, ['add', 'rule', 'style/comments', 'Comment the why, not the what'])
  succeed(at, 'mindweaver', at.workspace, ['install', '-p', 'api', 'skill:review'])

  project(at, 'web', ['--agents', 'claude,opencode', '--shared'])
  for (const name of ['review', 'lint']) {
    succeed(at, 'mindweaver', at.workspace, ['install', '-p', 'web', `skill:${name}`])
  }
}

/** `name`, a folder of the workspace, made a project with `options` and linked. */
function project(at: Places, name: string, options: string[]): void {
  const folder = join(at.workspace, name)
  mkdirSync(folder)
  succeed(at, 'mind', folder, ['init', ...options])
  succeed(at, 'mindweaver', at.workspace, ['link', name])
}

/**
 * A workspace for a spec that changes it, which no other spec reads: two
 * projects beside it, `drafts` linked and `notes` not, each made by `mind
 * init` and nothing else, so that a link or an unlink from the panel can be
 * held byte for byte to what the same command leaves in a copy of it.
 */
export function changeable(at: Places): void {
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  for (const name of ['drafts', 'notes']) {
    const folder = join(at.workspace, name)
    mkdirSync(folder)
    succeed(at, 'mind', folder, ['init'])
  }
  succeed(at, 'mindweaver', at.workspace, ['link', 'drafts'])
}

/** The skill `assignable`'s loaded workspace offers, and where from as `--from` takes it. */
export const OFFERED = { name: 'notes', origin: 'load:../team' } as const

/** The projects `assignable` links, each for Claude and Codex. */
export const ASSIGNED = ['alpha', 'beta'] as const

/**
 * A workspace for the spec that assigns from the matrix, which no other
 * spec reads: `team`, a workspace beside it whose own skill `notes` it
 * loads, and `alpha` and `beta`, each made by `mind init` for Claude and
 * Codex and linked, holding nothing yet.
 *
 * Offered from a loaded workspace rather than from the shelf, which only a
 * gather from a git source fills, and the suite builds nothing a git
 * source makes: a load is the other place an offer comes from beside the
 * workspace's own, and is installed with `--from` as one from the shelf is.
 */
export function assignable(at: Places): void {
  const team: Places = { ...at, workspace: join(dirname(at.workspace), 'team') }
  succeed(at, 'mindweaver', at.scratch, ['init', team.workspace], 'none')
  succeed(team, 'mindweaver', team.workspace, ['add', 'skill', OFFERED.name, 'Keep notes as the team does'])
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  succeed(at, 'mindweaver', at.workspace, ['load', '../team'])
  for (const name of ASSIGNED) {
    const folder = join(at.workspace, name)
    mkdirSync(folder)
    succeed(at, 'mind', folder, ['init', '--agents', 'claude,codex'])
    succeed(at, 'mindweaver', at.workspace, ['link', name])
  }
}

/** The skill `updatable`'s library holds, and both its projects received. */
export const UPDATED = 'lint' as const

/**
 * The projects `updatable` links: `alpha`, for Claude and Codex, whose copy
 * is as it was installed, and `beta`, for Claude, whose copy was edited
 * since.
 */
export const UPDATING = ['alpha', 'beta'] as const

/** What `mindweaver add` writes for the skill `lint`, a line added below. */
function lint(line: string): string {
  return `---\nname: ${UPDATED}\ndescription: Tidy the code before a commit\n---\n\n# ${UPDATED}\n\n${line}\n`
}

/** The library's `lint` once the team learned something, which leaves what was installed of it behind. */
export const LINT_SINCE = lint('Run the formatter before the linter.')

/**
 * `text` saved where an `--editor` copies it from, beside the workspace
 * rather than in it, under `name`: what an editor would save, and the one
 * file a spec or a builder writes by hand. Its path, for the `cp` an
 * `--editor` runs.
 */
export function draft(at: Places, name: string, text: string): string {
  const path = join(dirname(at.workspace), 'drafts', name)
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, text)
  return path
}

/**
 * A workspace for the spec that updates and removes from the panel, which
 * no other spec reads: its library holds the skill `lint`, installed into
 * `alpha` and `beta`, and `beta`, which authors its skills in Claude's
 * folder, where its copy is installed too, had that copy edited since with
 * `mind edit`, an `--editor` copying a draft in, as an editor saves one.
 *
 * Edited with the command rather than written into by hand: `mind edit`
 * reaches only what a project authors, so `beta` authors where Claude
 * reads, and the edit keeps the copy's record, which it then stands as
 * edited against.
 */
export function updatable(at: Places): void {
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  succeed(at, 'mindweaver', at.workspace, ['add', 'skill', UPDATED, 'Tidy the code before a commit'])
  const [alpha, beta] = UPDATING
  project(at, alpha, ['--agents', 'claude,codex'])
  project(at, beta, ['--agents', 'claude', '--skills', '.claude/skills'])
  succeed(at, 'mindweaver', at.workspace, ['install', '-p', alpha, '-p', beta, `skill:${UPDATED}`])
  const edited = draft(at, 'beta-lint.md', lint('Our own way: lint before every push.'))
  succeed(at, 'mind', join(at.workspace, beta), ['edit', `skill:${UPDATED}`, '--editor', `cp ${edited}`])
}

/** The skill `authorable`'s library holds, which the editor changes. */
export const AUTHORED = 'review' as const

/**
 * A workspace for the spec that authors in the library from the panel,
 * which no other spec reads: its library holds the skill `review`, as
 * `mindweaver add` writes it, and nothing else.
 */
export function authorable(at: Places): void {
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  succeed(at, 'mindweaver', at.workspace, ['add', 'skill', AUTHORED, 'Read a change before it lands'])
}

/** The workspace `loadable` loads from the panel, by its entry once loaded, and the skill it offers. */
export const LOADED = { name: 'notes', entry: '../team', description: 'Keep notes as the team does' } as const

/** The skill of its own `loadable` offers, whatever it loads. */
export const OWN = 'review' as const

/**
 * A workspace for the spec that loads and unloads from the panel, which no
 * other spec reads, in a home of its own, made the default workspace first,
 * so that each `mindweaver init` after it remembers the workspace it makes:
 * `team`, beside the workspace, holding the skill `notes` of its own, then
 * the workspace, holding `review` and linking `alpha`, made by `mind init`
 * for Claude, so the matrix has an offer and a project to draw before
 * anything is loaded. Nothing loads `team` yet.
 *
 * A home of its own rather than the run's, which the driver holds to stay
 * as it was made, empty: only a default workspace remembers the workspaces
 * this machine knows, and this one is held to stay as it was made in turn.
 */
export function loadable(at: Places): void {
  succeed(at, 'mindweaver', at.scratch, ['init', at.home], 'none')
  const team: Places = { ...at, workspace: join(dirname(at.workspace), 'team') }
  succeed(at, 'mindweaver', at.scratch, ['init', team.workspace], 'none')
  succeed(team, 'mindweaver', team.workspace, ['add', 'skill', LOADED.name, LOADED.description])
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  succeed(at, 'mindweaver', at.workspace, ['add', 'skill', OWN, 'Read a change before it lands'])
  project(at, 'alpha', ['--agents', 'claude'])
}

/** The project `serveable` links, for every agent. */
export const SERVED = 'alpha' as const

/**
 * A workspace for the spec that adds an MCP server from the panel and
 * assigns it, which no other spec reads: its library holds nothing, and it
 * links `alpha`, made by `mind init` for Claude, Codex and OpenCode, so
 * the server is written into each agent's own config file.
 */
export function serveable(at: Places): void {
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  project(at, SERVED, ['--agents', 'claude,codex,opencode'])
}

/**
 * What a builder hands back beside what it built, when its `mindweaver ui`
 * is launched with options of its own: those options, each request what
 * they name was sent, and what stops it once the test is done.
 */
export interface Launched {
  options: string[]
  asked(): string[]
  stop(): Promise<void>
}

/** The repository `searchable`'s skill directory names, beside the workspace, by its folder's name. */
export const HOUSE_RULES = 'house-rules' as const

/** What `searchable`'s repository holds of the skill it offers. */
export const SKILL_FILE = `---\nname: ${SKILL}\ndescription: How this repo writes commit messages\n---\n\n# ${SKILL}\n\nSay why, in the imperative.\n`

/**
 * A git repository at `folder` holding `files`, committed once, as its
 * author would make it: by a fixed author at a fixed time, none of the
 * git configuration of the person running the suite read.
 */
function committed(at: Places, folder: string, files: Record<string, string>): void {
  for (const [route, text] of Object.entries(files)) {
    mkdirSync(dirname(join(folder, route)), { recursive: true })
    writeFileSync(join(folder, route), text)
  }
  const author = {
    GIT_AUTHOR_NAME: 'Fixture',
    GIT_AUTHOR_EMAIL: 'fixture@example.com',
    GIT_AUTHOR_DATE: '2026-10-01T09:00:00Z',
    GIT_COMMITTER_NAME: 'Fixture',
    GIT_COMMITTER_EMAIL: 'fixture@example.com',
    GIT_COMMITTER_DATE: '2026-10-01T09:00:00Z',
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_CONFIG_GLOBAL: '/dev/null'
  }
  for (const args of [['init', '--quiet'], ['add', '.'], ['commit', '--quiet', '-m', 'Add the house rules']]) {
    const ran = spawnSync('git', ['-c', 'init.defaultBranch=main', '-c', 'commit.gpgsign=false', ...args], {
      cwd: folder,
      env: { ...environment(at), ...author },
      encoding: 'utf8',
      timeout: 60_000
    })
    if (ran.error || ran.status !== 0) {
      throw ran.error ?? new Error(`\`git ${args.join(' ')}\` in ${folder} exited ${ran.status}:\n${ran.stdout}${ran.stderr}`)
    }
  }
}

/**
 * A workspace for the spec that searches the catalogs from the panel,
 * which no other spec reads, and the catalogs its `mindweaver ui` asks:
 *
 * - the workspace, made by `mindweaver init`, its `weaver.toml` listing
 *   skills.sh in `finders`, the one key no command writes, written in by
 *   hand as a person writes it once they have read the directory's terms
 *   (D-15);
 * - `house-rules`, a repository beside it holding the skill
 *   `commit-style`, made with git as its author would make it;
 * - skills.sh and the MCP Registry on loopback (catalogs.ts), the first
 *   naming that repository as the skill's source, which a finder on this
 *   machine may, and `mindweaver ui` launched asking them.
 */
export async function searchable(at: Places): Promise<Launched> {
  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  const marker = join(at.workspace, '.mindweaver', 'weaver.toml')
  const text = readFileSync(marker, 'utf8')
  const listed = text.replace('loaded = []\n', 'loaded = []\nfinders = ["skills.sh"]\n')
  if (listed === text) {
    throw new Error(`${marker} holds no \`loaded = []\` to list a finder after:\n${text}`)
  }
  writeFileSync(marker, listed)
  const repository = join(dirname(at.workspace), HOUSE_RULES)
  committed(at, repository, { [`skills/${SKILL}/SKILL.md`]: SKILL_FILE })
  const served = await catalogs(repository)
  return { options: served.options, asked: served.asked, stop: served.stop }
}
