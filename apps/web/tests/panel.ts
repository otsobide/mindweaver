/*
 * What the suites that draw the views share: the snapshots and the other
 * answers a test of the CLI wrote, a transport that answers with them, and
 * the panel mounted as a browser would mount it.
 */
import { useToast } from '@nuxt/ui/composables'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory, type Router, type RouteRecordRaw } from 'vue-router'
import App from '@/App.vue'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { Step } from '@/api/generated/Step'
import { TransportError, type LiveEvent, type Preview, type Query, type Ran, type Switch, type Switchable, type Task, type Transport } from '@/api/transport'
import { createPanel } from '@/panel'
import applied from './fixtures/applied.json'
import assigning from './fixtures/assigning.json'
import authoredJson from './fixtures/authored.json'
import candidates from './fixtures/candidates.json'
import commands from './fixtures/commands.json'
import draftsJson from './fixtures/drafts.json'
import foldersJson from './fixtures/folders.json'
import gatheredJson from './fixtures/gathered.json'
import gatheringJson from './fixtures/gathering.json'
import keptJson from './fixtures/kept.json'
import library from './fixtures/library.json'
import loadsJson from './fixtures/loads.json'
import planned from './fixtures/plans.json'
import scan from './fixtures/scan.json'
import searchedJson from './fixtures/searched.json'
import servedJson from './fixtures/served.json'
import serversJson from './fixtures/servers.json'
import serving from './fixtures/serving.json'
import sourcesJson from './fixtures/sources.json'
import stepsJson from './fixtures/steps.json'
import unmade from './fixtures/unmade.json'
import updated from './fixtures/updated.json'
import surveyed from './fixtures/updates.json'
import switchesJson from './fixtures/switches.json'
import updating from './fixtures/updating.json'
import validatedJson from './fixtures/validated.json'
import workspace from './fixtures/workspace.json'

/**
 * The snapshots in tests/fixtures, which `make web/fixtures` writes from
 * workspaces a test of the CLI builds by running the commands
 * (apps/cli/tests/fixtures_test.rs): `workspace` holds something of
 * everything a view shows, `unmade` is a home where nothing is made, and
 * `candidates` is `workspace` with more beside its projects, the one the
 * scan in tests/fixtures/scan.json looked through: two projects and two
 * repositories nobody linked, `old`, linked and moved since, and `legacy`,
 * linked, which a `mind.toml` marks; `assigning`, a workspace to assign
 * in, which the matrix draws: one name offered from two places, the shelf,
 * a copy of each state an install stands in, a project nothing can go into
 * and an entry that opens none; and `updating`, a workspace to update,
 * which the page of updates draws beside tests/fixtures/updates.json: an
 * installation outdated, edited, missing and orphaned, and an entry that
 * opens none; and `serving`, a workspace keeping MCP servers, which their
 * page draws beside tests/fixtures/servers.json: a program, an address and
 * one reached over server-sent events, a loaded workspace's that waits for
 * a yes, and a project for every agent holding one; and `sources`, a
 * workspace whose shelf is filled from repositories beside it, which the
 * page of sources draws beside tests/fixtures/gathered.json: one of skills,
 * gathered as it is, whose two say two licences, and one of rules, named
 * and pinned.
 */
const FIXTURES = { workspace, unmade, candidates, assigning, updating, serving, sources: sourcesJson }

export type Fixture = keyof typeof FIXTURES

/**
 * A fixture as the server would answer it, copied, so a test may change its
 * copy. Imported as JSON, whose words TypeScript reads as any string, so it
 * is said to be the snapshot it was written as.
 */
export function fixture(name: Fixture): Snapshot {
  return structuredClone(FIXTURES[name]) as unknown as Snapshot
}

/** A query, and what the server answers it with. */
export interface Answered {
  query: Query
  answer: unknown
}

/**
 * What the workspace fixture's server answers when each of its library's
 * own is opened (tests/fixtures/library.json): the files of each, and each
 * file of them, written as the snapshots are. Copied, so a test may change
 * its copy.
 */
export function answers(): Answered[] {
  return structuredClone(library) as unknown as Answered[]
}

/**
 * What the workspace fixture's server answers when its page of what could
 * be linked scans it (tests/fixtures/scan.json): the workspace's folder, to
 * core's own depth, with what there is to find beside its projects, written
 * as the snapshots are. Copied, so a test may change its copy.
 */
export function scans(): Answered[] {
  return structuredClone(scan) as unknown as Answered[]
}

/**
 * What the candidates fixture's server, launched in its home, answers as
 * the explorer walks its folders (tests/fixtures/folders.json): the
 * workspace's root asked for by no path, as the Link form opens it, and by
 * its own, as Discover does; the home above it; `archive`, holding `old`;
 * and `notes`, a repository, its hidden folder left out, then asked for
 * (`walked`). Beside them, `Projects` typed, which a request may not name,
 * refused with its code (`refused`). Copied, so a test may change its copy.
 */
export interface Walk {
  walked: Answered[]
  refused: { query: Query, error: string, code: string }
}

export function walks(): Walk {
  return structuredClone(foldersJson) as unknown as Walk
}

/**
 * What the assigning fixture's server answers when its matrix plans a few
 * selections of boxes (tests/fixtures/plans.json), each in the order the
 * boxes were marked: `format` into `api` and `docs`, the library's `deploy`
 * in place of the team's in `api`, and a selection that clashes and is
 * locked beside a change that goes ahead. Copied, so a test may change its
 * copy.
 */
export function plans(): Answered[] {
  return structuredClone(planned) as unknown as Answered[]
}

/**
 * What the updating fixture's server answers when its page of updates asks
 * what updating every project would do (tests/fixtures/updates.json).
 * Copied, so a test may change its copy.
 */
export function updates(): Answered[] {
  return structuredClone(surveyed) as unknown as Answered[]
}

/**
 * What the workspace fixture's server answers when its editor and its page
 * adding one of the library's own ask core to judge what is typed
 * (tests/fixtures/drafts.json): `review` as it is, with two things
 * `validate` warns of, and with its description emptied; then a skill to
 * add from nothing and from `full`, one whose name is refused and one whose
 * name is taken. Copied, so a test may change its copy.
 */
export function drafts(): Answered[] {
  return structuredClone(draftsJson) as unknown as Answered[]
}

/**
 * What `mindweaver validate` said of the workspace fixture's library once
 * `review` held `text`, the draft it warns of (tests/fixtures/
 * validated.json): what the editor's verdict is held to, word for word.
 */
export const validated: { text: string, said: string } = validatedJson

/** `value` with every object's keys in one order, to compare as text. */
function sorted(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sorted)
  }
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, each]) => [key, sorted(each)]))
  }
  return value
}

/**
 * A transport answering the snapshot with `snapshot`, and any other query
 * with the answer `answered` holds for it, what opening the library's own
 * and scanning are answered with unless a test says otherwise, refusing
 * one it holds none for as the server refuses what it cannot answer; each
 * query written down.
 */
export function answering(snapshot: Snapshot, answered: Answered[] = [...answers(), ...scans()]) {
  const queries: Query[] = []
  const transport = {
    query: (query: Query) => {
      queries.push(query)
      if (query.type === 'snapshot') {
        return Promise.resolve(structuredClone(snapshot))
      }
      const asked = JSON.stringify(sorted(query))
      const found = answered.find(each => JSON.stringify(sorted(each.query)) === asked)
      return found
        ? Promise.resolve(structuredClone(found.answer))
        : Promise.reject(new TransportError(422, `not_found: nothing answers ${asked}`))
    },
    send: () => new Promise(() => {}),
    transcript: () => Promise.resolve([]),
    subscribe: () => () => {}
  } as unknown as Transport
  return { queries, transport }
}

/** A task, its preview, and what its run is answered with. */
export interface Commanded {
  task: Task
  preview: Preview
  ran: Ran
}

/**
 * What the workspace fixture's server answers when each of a few tasks is
 * previewed, then run, in turn (tests/fixtures/commands.json, written as the
 * snapshots are): a link, an unlink to confirm, a validate that fails with
 * a code, a removal everywhere. Copied, so a test may change its copy.
 */
export function commanded(): Commanded[] {
  return structuredClone(commands) as unknown as Commanded[]
}

/**
 * What the assigning fixture's server answers when a plan is applied, its
 * tasks sent back as one batch, each on the workspace as it was built
 * (tests/fixtures/applied.json): its preview, and its run. The plan of
 * `format` into `api` and `docs`, which the matrix applies, then of the
 * library's `deploy` into `api`, one box, which the command palette
 * applies. Copied, so a test may change its copy.
 */
export function applying(): Commanded[] {
  return structuredClone(applied) as unknown as Commanded[]
}

/**
 * What the updating fixture's server answers when each change its page of
 * updates offers is previewed and run, each on the workspace as it was
 * built (tests/fixtures/updated.json): every project updated, one, one
 * installation in one, an edited one forced, an orphaned one uninstalled,
 * an orphaned one edited since uninstalled forced, and every orphaned one
 * taken out. Copied, so a test may change its copy.
 */
export function updatings(): Commanded[] {
  return structuredClone(updated) as unknown as Commanded[]
}

/**
 * What the workspace fixture's server answers when each change its pages
 * of the library and of a project send is previewed and run, each on the
 * workspace as it was built (tests/fixtures/authored.json): `review` saved
 * from the editor, then saved again once its file changed on disk since it
 * was read, which is refused (`edit_stale`); a skill added from `full`;
 * `lint` renamed `tidy`; and `web`'s `legacy` adopted. Copied, so a test
 * may change its copy.
 */
export function authored(): Commanded[] {
  return structuredClone(authoredJson) as unknown as Commanded[]
}

/**
 * What the assigning fixture's server, in the home whose default workspace
 * knows it, answers when each change its page of loads sends is previewed
 * and run, each on the workspace as it was built (tests/fixtures/
 * loads.json): `kit` loaded by the path the snapshot lists it by, `team`
 * unloaded by its entry, and the workspace itself loaded, which the line
 * refuses (`loads_itself`). Copied, so a test may change its copy.
 */
export function loadings(): Commanded[] {
  return structuredClone(loadsJson) as unknown as Commanded[]
}

/**
 * What the serving fixture's server answers when its page of MCP servers
 * and its matrix ask (tests/fixtures/servers.json): a server to add judged
 * fit, one whose secret is read from a variable no agent can refer to,
 * `github` given another transport, and plans of the team's `sentry`, which
 * waits for a yes, and the library's `linear`, into `web`. Copied, so a
 * test may change its copy.
 */
export function servers(): Answered[] {
  return structuredClone(serversJson) as unknown as Answered[]
}

/**
 * What the serving fixture's server answers when each change its page of
 * MCP servers and its matrix send is previewed and run, each on the
 * workspace as it was built (tests/fixtures/served.json): a server added,
 * `github` given another transport, and `sentry` into `web`, by the plan's
 * task, which installs nothing, then by its confirmation's. Copied, so a
 * test may change its copy.
 */
export function served(): Commanded[] {
  return structuredClone(servedJson) as unknown as Commanded[]
}

/**
 * What the workspace fixture's server answers when each change the
 * library's forms for a copy of it send is previewed and run, each on the
 * workspace as it was built (tests/fixtures/kept.json): an export of every
 * kind into `/home/you/backup`; one of skills alone told to replace,
 * confirmed for what it takes away; a backup to GitHub naming the variable
 * a token is read from, previewed alone, its run reading the environment of
 * whoever wrote the fixture; and a backup naming none, run and stopped at
 * its first step, before it reached any repository (`backup_cancelled`).
 * The third's `ran` is null. Copied, so a test may change its copy.
 */
export function kept(): Commanded[] {
  return structuredClone(keptJson) as unknown as Commanded[]
}

/**
 * What the workspace fixture's server answers when its page of catalogs
 * sends each of its tasks in turn, its `weaver.toml` listing skills.sh and
 * the server started naming a registry and skills.sh on loopback
 * (tests/fixtures/searched.json): a search of the skill directories for
 * `commit`, which finds `commit-style`, in a repository on the server's
 * disk, and `pdf`, on GitHub; the first taken, sent back as the search
 * printed it; a search of the registry for `mcp`, listing three servers;
 * and `ai.clawfetch/mcp` added, its dry run, then told yes. Copied, so a
 * test may change its copy.
 */
export function searched(): Commanded[] {
  return structuredClone(searchedJson) as unknown as Commanded[]
}

/**
 * Each step a backup reports, as the backend tells the panel it
 * (tests/fixtures/steps.json): checking the repository out, exporting,
 * committing, each cancellable, and pushing, past a cancel.
 */
export function steps(): Step[] {
  return structuredClone(stepsJson) as unknown as Step[]
}

/**
 * What the sources fixture's server answers when each change its page of
 * sources sends is previewed and run, each on the workspace as it was
 * built (tests/fixtures/gathered.json): `acme-tools` added, its dry run
 * first, then gathered, then gathered and stopped at its first step
 * (`gather_cancelled`); `src-acme-skills` gathered again, then every
 * source; `src-acme-skills` renamed `acme-skills`, then `team-rules`, a
 * name another source has (`source_name_taken`); `team-rules` forgotten,
 * its dry run first; and `src-acme-skills` gathered again and stopped at
 * its first step, said by the gather's code beside nothing gathered. The
 * second and the third are one task, and so are the fourth and the last:
 * a test hands `commanding` the one it means. Copied, so a test may change
 * its copy.
 */
export function gathered(): Commanded[] {
  return structuredClone(gatheredJson) as unknown as Commanded[]
}

/**
 * Each step a gather reports, as the backend tells the panel it
 * (tests/fixtures/gathering.json): connecting, receiving 120 objects of
 * 340, resolving with no total, checking out every file, reading one
 * folder of two, each cancellable, and placing one of two, past a cancel.
 */
export function gatheringSteps(): Step[] {
  return structuredClone(gatheringJson) as unknown as Step[]
}

/** A task as `send` was handed it, and how. */
export interface Sent {
  task: Task
  mode: 'preview' | 'run'
}

/**
 * A transport answering queries as `answering` does, with `answered`
 * when given, and each task of `known` as the server answered it: its preview, and its run, told as
 * started and finished on the stream before it is answered, as the server
 * tells them, and kept in the transcript. Any other task is refused with a
 * code, as the backend refuses one. Each task sent is written down, and
 * `finishing` holds a run's finish back until the test lets it go.
 */
export function commanding(snapshot: Snapshot = fixture('workspace'), known: Commanded[] = commanded(), answered?: Answered[]) {
  const { queries, transport } = answering(snapshot, answered)
  const sent: Sent[] = []
  const ran: Ran[] = []
  const listeners: ((event: LiveEvent) => void)[] = []
  const tell = (event: LiveEvent) => [...listeners].forEach(listener => listener(event))
  const control = { finishing: undefined as Promise<void> | undefined }
  /** Each run a cancel was asked for, by its number. */
  const cancels: number[] = []
  const commanding = {
    ...transport,
    async send(task: Task, mode: 'preview' | 'run') {
      // As JSON, as the HTTP transport sends it: what the runner hands over
      // may be one of Vue's proxies, which structuredClone refuses.
      sent.push({ task: JSON.parse(JSON.stringify(task)) as Task, mode })
      const asked = JSON.stringify(sorted(task))
      const found = known.find(each => JSON.stringify(sorted(each.task)) === asked)
      if (!found) {
        throw new TransportError(422, `nothing answers ${asked}`, 'not_found')
      }
      if (mode === 'preview') {
        return structuredClone(found.preview)
      }
      // Numbered as the server numbers runs, from 1 in the order they start.
      const run = ran.length + 1
      tell({ type: 'command_started', run, lines: found.ran.lines, cancellable: found.preview.cancellable })
      await control.finishing
      ran.push(structuredClone(found.ran))
      tell({ type: 'command_finished', run, lines: found.ran.lines })
      return structuredClone(found.ran)
    },
    // Taken, and told to every tab as the server tells it; what the run
    // does of it is the test's to say, by how it lets the run finish.
    cancel(run: number) {
      cancels.push(run)
      tell({ type: 'cancelling', run })
      return Promise.resolve({ run, lines: [] })
    },
    transcript: () => Promise.resolve(structuredClone(ran)),
    subscribe(listener: (event: LiveEvent) => void) {
      listeners.push(listener)
      return () => {
        listeners.splice(listeners.indexOf(listener), 1)
      }
    }
  } as unknown as Transport
  return { queries, sent, ran, control, cancels, tell, transport: commanding }
}

/**
 * What the assigning fixture's server, in the home whose default workspace
 * knows it and others, answers as its header switches (tests/fixtures/
 * switches.json): what it may switch to; a switch to `kit`, what it is
 * answered with, the snapshot of `kit` then and what it may switch to from
 * there; and a switch to `old`, listed but gone since, refused with its
 * code.
 */
export interface Switches {
  workspaces: Answered
  switched: { to: Switch, answer: Switchable, snapshot: Snapshot, workspaces: Answered }
  refused: { to: Switch, error: string, code: string }
}

/** The answers tests/fixtures/switches.json holds, copied, so a test may change its copy. */
export function switches(): Switches {
  return structuredClone(switchesJson) as unknown as Switches
}

/**
 * A transport answering as the assigning fixture's server does as its
 * header switches: the snapshot and what it may switch to, those of the
 * workspace in use; a switch to `kit` taken, told on the stream as
 * `switched` before it is answered, as the server tells it, after which
 * both are `kit`'s; one to `old` refused as the fixture says; any other as
 * the server refuses a path no list holds. Each query and each switch
 * written down; `tell` says something on the stream.
 */
export function switching() {
  const fixtures = switches()
  let snapshot = fixture('assigning')
  let listed = fixtures.workspaces.answer
  const queries: Query[] = []
  const sent: Switch[] = []
  const listeners: ((event: LiveEvent) => void)[] = []
  const tell = (event: LiveEvent) => [...listeners].forEach(listener => listener(event))
  let taken = 0
  const transport = {
    query(query: Query) {
      queries.push(query)
      if (query.type === 'snapshot') {
        return Promise.resolve(structuredClone(snapshot))
      }
      if (query.type === 'workspaces') {
        return Promise.resolve(structuredClone(listed))
      }
      return Promise.reject(new TransportError(422, `nothing answers ${query.type}`, 'not_found'))
    },
    send: () => new Promise(() => {}),
    switch(to: Switch) {
      sent.push({ ...to })
      if (to.path === fixtures.switched.to.path) {
        snapshot = fixtures.switched.snapshot
        listed = fixtures.switched.workspaces.answer
        taken += 1
        tell({ type: 'switched', switches: taken })
        return Promise.resolve(structuredClone(fixtures.switched.answer))
      }
      if (to.path === fixtures.refused.to.path) {
        return Promise.reject(new TransportError(422, fixtures.refused.error, fixtures.refused.code))
      }
      return Promise.reject(new TransportError(422, `\`${to.path}\` is none of the workspaces this machine knows`, 'switch_unlisted'))
    },
    transcript: () => Promise.resolve([]),
    subscribe(listener: (event: LiveEvent) => void) {
      listeners.push(listener)
      return () => {
        listeners.splice(listeners.indexOf(listener), 1)
      }
    }
  } as unknown as Transport
  return { transport, queries, sent, tell }
}

/** A transport refusing every query with `error`. */
export function refusing(error: Error): Transport {
  return {
    query: () => Promise.reject(error),
    send: () => new Promise(() => {}),
    transcript: () => Promise.reject(error),
    subscribe: () => () => {}
  } as unknown as Transport
}

let mounted: VueWrapper | undefined

export interface Rendered {
  page: VueWrapper
  router: Router
}

/**
 * The panel at `path`, asking `transport`, once what it asked is answered;
 * with `routes` beside its own, for a test to mount a page of its own in
 * the panel's frame.
 */
export async function render(path: string, transport: Transport | Snapshot, routes: RouteRecordRaw[] = []): Promise<Rendered> {
  const asked = 'query' in transport ? transport : answering(transport).transport
  const panel = createPanel({ transport: asked, history: createMemoryHistory() })
  for (const route of routes) {
    panel.router.addRoute(route)
  }
  await panel.router.push(path)
  mounted = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return { page: mounted, router: panel.router }
}

/**
 * Take down what `render` mounted, and the toasts it left: Nuxt UI keeps
 * them in a state of its own, which outlives the app.
 */
export function unmount() {
  mounted?.vm.$.appContext.app.runWithContext(() => useToast().clear())
  mounted?.unmount()
  mounted = undefined
  document.body.innerHTML = ''
}

/** What can be searched for elements: a mounted page, or an element in it. */
interface Searched {
  findAll(selector: string): { text(): string }[]
}

/** The text of each element `selector` finds, trimmed and with its spaces run together. */
export function texts(within: Searched, selector: string): string[] {
  return within.findAll(selector).map(found => found.text().replace(/\s+/g, ' ').trim())
}
