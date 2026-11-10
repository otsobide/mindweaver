/*
 * The workspace the README's screenshots show, built as somebody would build
 * theirs: repositories made with git, then a command at a time with the
 * binaries under test, so every page shows what those commands made. It is
 * meant to look lived in, unlike the suite's fixture, which is meant to be
 * compared: projects with plausible names, a library and a shelf with real
 * bodies, installs from both, some of them behind where they came from,
 * and two repositories nobody linked yet.
 *
 * The only files written by hand are the repositories' contents, as a
 * person's own work would be, and the bodies `mindweaver edit` copies in through
 * `--editor`, the way an editor would save them. Everything else is a
 * command's.
 */
import { cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { git, repository, run, succeed, type Places } from './environment.ts'

/** `examples/` in this repository, whose skills and rules the demo reuses. */
const EXAMPLES = fileURLToPath(new URL('../../../examples', import.meta.url))

/** The workspace's folder, under the scratch folder, as the screenshots name it. */
export const WORKSPACE = 'Projects'

/** The workspace beside the demo's that the tour loads, by its folder's name. */
export const LOADED = 'platform'

/** The project whose page the tour shows, and whose unlink it asks about. */
export const SHOWN = 'mobile-app'

/** The skill of the library's own whose page the tour shows. */
export const ARTIFACT = 'api-conventions'

/**
 * The skill of the library's own whose removal everywhere the tour shows
 * the dry run of: installed in `data-pipeline`, whose copy would go, and in
 * `infra`, whose copy was edited, and would be left.
 */
export const REMOVED = 'deploy-checklist'

/** Write `text` to `path`, making the folders above it. */
function write(path: string, text: string): void {
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, text)
}

const API_CONVENTIONS = `---
name: api-conventions
description: How our HTTP APIs are shaped, from resource names to errors and pagination. Use when adding or changing an endpoint.
---

# API conventions

Every service here speaks the same HTTP, so a client written for one works
for the next.

## Resources

- Plural nouns for collections, \`/invoices\`, and an id below them,
  \`/invoices/{id}\`. No verbs in a path: an action is a resource of its own,
  \`POST /invoices/{id}/refunds\`.
- JSON bodies, with fields in \`snake_case\` and times in RFC 3339.

## Errors

Answer with the status that says what went wrong, and a body a program can
act on:

\`\`\`json
{ "error": { "code": "invoice_closed", "message": "A closed invoice cannot change." } }
\`\`\`

## Pagination

By cursor: \`?limit=\` up to 100, and a \`next\` cursor in the answer while
there is more. Never by offset, which skips rows as others are inserted.
`

const DEPLOY_CHECKLIST = `---
name: deploy-checklist
description: Ships a service from staging to production one checked step at a time. Use before any release.
---

# Deploy checklist

1. The main branch is green, and the release notes are written.
2. Deploy to staging and run the smoke tests.
3. Watch the dashboards for ten minutes: errors, latency, saturation.
4. Deploy to production behind the flag, then ramp it up.
5. Say so in #releases, with a link to the notes.
`

/** The checklist once the team learned something, which leaves its installs behind. */
const DEPLOY_CHECKLIST_SINCE = `${DEPLOY_CHECKLIST}6. Keep the previous build ready to roll back to for an hour.
`

/** The checklist as the infra team changed their own copy of it, by hand. */
const DEPLOY_CHECKLIST_INFRA = `${DEPLOY_CHECKLIST}6. Page the on-call engineer before the ramp starts.
`

const RELEASE_NOTES = `---
name: release-notes
description: Drafts release notes from the pull requests merged since the last tag. Use when cutting a release.
---

# Release notes

1. List what merged since the last tag: \`git log --merges <tag>..HEAD\`.
2. Group it under Added, Changed and Fixed, in the words of somebody using
   the product, not the code's.
3. Name every breaking change first, with what to do about it.
`

/** The notes as the team's repository has them a day later. */
const RELEASE_NOTES_SINCE = `${RELEASE_NOTES}4. Thank whoever contributed for the first time.
`

const SECRETS = `# Keep secrets out of the repository

- Read credentials from the environment or the secret store, never from a
  file in the tree.
- A secret that reached a commit is leaked: rotate it first, then remove it.
`

/**
 * `at.workspace`, and the repositories around it:
 *
 * - its library holds the skills `api-conventions` and `deploy-checklist`,
 *   the rules `git/commits`, `security/secrets` and `style/comments`, and
 *   the MCP servers `github`, a program started with a secret, and
 *   `linear`, an address reached with a bearer token;
 * - its shelf holds `code-review`, `commit-style` and `release-notes`,
 *   gathered from `team`, the team's repository beside it;
 * - it links six projects, `billing-api`, `storefront`, `mobile-app`,
 *   `infra`, `data-pipeline` and `admin-console`, each a repository given
 *   `mind init`, with what each works with installed from both;
 * - `release-notes` and `deploy-checklist` changed where they came from
 *   since, so their installs are outdated, but for `infra`'s copy of the
 *   checklist, which `infra` keeps where Claude reads its skills and
 *   changed by hand with `mind edit` before, so it is edited;
 * - `design-system`, a mind project, and `playground`, a repository, sit in
 *   the workspace unlinked, for Discover to find;
 * - the home is the default workspace, made first, so it remembers each
 *   workspace `mindweaver init` makes after it: `at.workspace`, and `platform`
 *   beside it, a workspace of the platform team's own skills, which nothing
 *   loads yet, for the tour to load.
 */
export function build(at: Places): void {
  const drafts = join(at.scratch, 'drafts')

  // The default workspace, before any other, which each `mindweaver init`
  // after it remembers in; then the platform team's workspace, beside the
  // demo's, with skills of its own, made by `mindweaver add`.
  succeed(at, 'mindweaver', at.scratch, ['init', at.home], 'none')
  const platform: Places = { ...at, workspace: join(at.scratch, LOADED) }
  succeed(at, 'mindweaver', at.scratch, ['init', platform.workspace], 'none')
  for (const [name, description] of [
    ['incident-response', 'Runs an incident from the first page to the write-up'],
    ['on-call-handoff', 'Hands the pager over with nothing left unsaid']
  ] as const) {
    succeed(platform, 'mindweaver', platform.workspace, ['add', 'skill', name, description])
  }

  // The team's skills, in a repository of their own: two of the examples,
  // and the release notes.
  const team = join(at.scratch, 'team')
  repository(at, team)
  for (const skill of ['code-review', 'commit-style']) {
    cpSync(join(EXAMPLES, 'skills', skill), join(team, 'skills', skill), { recursive: true })
  }
  write(join(team, 'skills/release-notes/SKILL.md'), RELEASE_NOTES)
  git(at, team, ['add', 'skills'])
  git(at, team, ['commit', '--quiet', '-m', 'Add the team\'s skills'])

  succeed(at, 'mindweaver', at.scratch, ['init', at.workspace], 'none')
  succeed(at, 'mindweaver', at.workspace, ['gather', 'git', team])

  // The library's own: each made by `mindweaver add`, then written as an
  // editor would save it.
  const own: [kind: 'skill' | 'rule', name: string, description: string, body: string][] = [
    ['skill', 'api-conventions', 'How our HTTP APIs are shaped', API_CONVENTIONS],
    ['skill', 'deploy-checklist', 'Ships a service safely', DEPLOY_CHECKLIST],
    ['rule', 'git/commits', 'Commit one coherent step at a time', readFileSync(join(EXAMPLES, 'rules/git/commits.md'), 'utf8')],
    ['rule', 'security/secrets', 'Keep secrets out of the repository', SECRETS],
    ['rule', 'style/comments', 'Comments say why, not what', readFileSync(join(EXAMPLES, 'rules/style/comments.md'), 'utf8')]
  ]
  for (const [kind, name, description, body] of own) {
    succeed(at, 'mindweaver', at.workspace, ['add', kind, name, description])
    const draft = join(drafts, `${name}.md`)
    write(draft, body)
    succeed(at, 'mindweaver', at.workspace, ['edit', `${kind}:${name}`, '--editor', `cp ${draft}`])
  }

  // The library's MCP servers, each made by `mindweaver add mcp` from how
  // an agent starts or reaches it, every secret a variable's name.
  succeed(at, 'mindweaver', at.workspace, [
    'add', 'mcp', 'github', 'GitHub\'s issues, pull requests and code',
    '--command', 'npx', '--arg=-y', '--arg', '@modelcontextprotocol/server-github',
    '--secret-env', 'GITHUB_PERSONAL_ACCESS_TOKEN'
  ])
  succeed(at, 'mindweaver', at.workspace, [
    'add', 'mcp', 'linear', 'Linear\'s issues and projects',
    '--url', 'https://mcp.linear.app/mcp', '--bearer-env', 'LINEAR_API_KEY'
  ])

  // The projects, each a repository made a project and linked, with what
  // it works with installed.
  const projects: [name: string, options: string[], installs: string[]][] = [
    ['billing-api', ['--agents', 'claude,codex'], ['skill:api-conventions', 'skill:code-review', 'rule:git/commits', 'rule:security/secrets']],
    ['storefront', ['--agents', 'claude,opencode', '--shared'], ['skill:api-conventions', 'skill:commit-style', 'skill:release-notes', 'rule:git/commits']],
    [SHOWN, [], ['skill:code-review', 'skill:commit-style', 'skill:release-notes', 'rule:git/commits', 'rule:style/comments']],
    ['infra', ['--agents', 'claude', '--skills', '.claude/skills'], ['skill:deploy-checklist', 'rule:security/secrets']],
    ['data-pipeline', ['--agents', 'codex'], ['skill:code-review', 'skill:deploy-checklist', 'rule:git/commits']],
    ['admin-console', ['--agents', 'claude,opencode', '--shared'], ['skill:api-conventions', 'skill:commit-style', 'rule:git/commits']]
  ]
  for (const [name, options, installs] of projects) {
    const folder = join(at.workspace, name)
    repository(at, folder)
    succeed(at, 'mind', folder, ['init', ...options])
    succeed(at, 'mindweaver', at.workspace, ['link', name])
    for (const install of installs) {
      if (install !== 'rule:style/comments') {
        succeed(at, 'mindweaver', at.workspace, ['install', '-p', name, install])
        continue
      }
      // A rule stating `paths` reaches Claude alone, which the install says
      // in a warning, and exits 1 for: installed all the same, and what the
      // project's page warns of too.
      const ran = run(at, 'mindweaver', at.workspace, ['install', '-p', name, install])
      if (!ran.stdout.startsWith('installed ') || !/^warning: rule `style\/comments` states `paths`/.test(ran.stderr)) {
        throw new Error(`\`${ran.line}\` exited ${ran.status}:\n${ran.stdout}${ran.stderr}`)
      }
    }
  }
  succeed(at, 'mind', join(at.workspace, 'billing-api'), ['add', 'skill', 'run-migrations', 'Applies and rolls back the database migrations'])
  // GitHub's server for `billing-api`'s agents, Claude and Codex: Codex is
  // given its entry in a project nothing marks trusted, which the install
  // warns of, and exits 1 for, installed all the same.
  const served = run(at, 'mindweaver', at.workspace, ['install', '-p', 'billing-api', 'mcp:github'])
  if (!served.stdout.startsWith('installed ') || !/^warning: Codex reads `github`/m.test(served.stderr)) {
    throw new Error(`\`${served.line}\` exited ${served.status}:\n${served.stdout}${served.stderr}`)
  }
  // `infra` keeps its skills where Claude reads them, so its copy of the
  // checklist is its own to edit, and it did: edited, which an update
  // leaves alone.
  write(join(drafts, 'infra-deploy-checklist.md'), DEPLOY_CHECKLIST_INFRA)
  succeed(at, 'mind', join(at.workspace, 'infra'), ['edit', 'skill:deploy-checklist', '--editor', `cp ${join(drafts, 'infra-deploy-checklist.md')}`])

  // A day later: the team improved its release notes, gathered again, and
  // the deploy checklist grew a step, which leaves their installs behind.
  write(join(team, 'skills/release-notes/SKILL.md'), RELEASE_NOTES_SINCE)
  git(at, team, ['commit', '--quiet', '--all', '-m', 'Thank first-time contributors'], '2026-10-02T09:00:00Z')
  succeed(at, 'mindweaver', at.workspace, ['gather', 'git', team])
  write(join(drafts, 'deploy-checklist.md'), DEPLOY_CHECKLIST_SINCE)
  succeed(at, 'mindweaver', at.workspace, ['edit', 'skill:deploy-checklist', '--editor', `cp ${join(drafts, 'deploy-checklist.md')}`])

  // Two repositories nobody linked yet.
  const design = join(at.workspace, 'design-system')
  repository(at, design)
  succeed(at, 'mind', design, ['init'])
  repository(at, join(at.workspace, 'playground'))
}
