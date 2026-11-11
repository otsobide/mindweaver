/*
 * What the specs share: the server the config started, a page opened
 * through its launch link as a person opens it, and what the CLI says of the
 * same workspace, to hold the page to; and what a test that fails keeps
 * beside its trace, to say where its time went.
 */
import { cpSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { test as base, expect, type Page, type TestInfo } from '@playwright/test'
import type { Envelope } from '../src/api/generated/Envelope'
import type { Listing } from '../src/api/generated/Listing'
import { given, look, run, serve, type Places, type Serving } from './environment.ts'
import { assignable, authorable, changeable, gatherable, loadable, searchable, serveable, updatable, type Launched } from './fixture.ts'

/** The server under test, as the link it printed says. */
export interface Served {
  /** Where it listens, as a browser writes the origin: `http://127.0.0.1:<port>`. */
  origin: string
  token: string
  /** The link it printed, which trades the token for the cookie. */
  link: string
}

/**
 * The server the webServer started, from what the config's regex read from
 * its link and Playwright put in the environment before it started any
 * worker.
 */
function served(): Served {
  const origin = process.env.MINDWEAVER_E2E_URL
  const token = process.env.MINDWEAVER_E2E_TOKEN
  if (!origin || !token) {
    throw new Error('no server said where it listens: run the suite with `make web/e2e`')
  }
  return { origin, token, link: `${origin}/?token=${token}` }
}

/** What a test's page said that the suite fails on. */
export interface Watch {
  /** Each CSP violation it logged, in its own words or the browser's. */
  violations: string[]
  /** Each error it threw that nothing caught. */
  thrown: string[]
}

/**
 * How a CSP violation reads in a console: as `report` says it, or as the
 * browser does, each engine in words of its own that all name the policy.
 */
const VIOLATION = /csp violation|content security policy/i

/**
 * Run in the page before any script of its own: every violation said in the
 * console in one form. Each engine fires the event, while what it writes in
 * the console by itself differs, and Playwright may not pass on what a
 * browser rather than a script logs. Playwright's own way in, beside the
 * page's scripts, so the CSP that refuses an inline script does not refuse
 * this one.
 */
function report(): void {
  document.addEventListener('securitypolicyviolation', (event) => {
    console.error(`csp violation: ${event.effectiveDirective} refused ${event.blockedURI || 'inline'} on ${location.pathname}`)
  })
}

/** A page of the panel, opened as a person opens it. */
export interface Panel {
  /**
   * Open the launch link, which sets the cookie and lands on the overview,
   * then `path`, and wait for the page to draw what the snapshot says.
   */
  open(path?: string): Promise<void>
}

/** Open `served`'s launch link in `page`, then `path`, as `Panel.open` says. */
async function open(page: Page, served: Served, path: string): Promise<void> {
  const landed = await page.goto(served.link)
  expect(landed?.status(), 'the server serves the panel, which a binary built before it answers with a 503').toBe(200)
  await expect(page, 'the launch link sends the browser on to the overview').toHaveURL(`${served.origin}/`)
  if (path !== '/') {
    await page.goto(new URL(path, served.origin).href)
  }
  await drawn(page)
}

/**
 * A workspace of a test's own, served by a `mindweaver ui` of its own, for a
 * test that changes what it serves: built by `changeable`, or what the
 * spec's `building` names, beside a copy of
 * it made before anything else touched it (`twin`), on which the test runs
 * the command the panel ran, to hold what each left byte for byte.
 *
 * Of its own, so neither the specs that read the suite's fixture while it
 * changes, nor the same test in the other browser, ever see the change.
 */
export interface Own extends Panel {
  /** Where it is, as every command a test runs on it names it. */
  at: Places
  /** The copy, as every command names it. */
  twin: Places
  /** Its `mindweaver ui`, which the fixture stops once the test is done (`close`). */
  served: Serving
  /**
   * What its builder launched its `mindweaver ui` with beside the run's
   * options, when it did: the catalogs on loopback a search asks, and what
   * each was asked. None for a builder that launches nothing.
   */
  launched?: Launched
  /**
   * What its `mindweaver ui` has said on stderr so far, a line at a time:
   * where it serves, then its log, every request it answered and every
   * run, each after the time of day and with how long it took. What a
   * test that fails keeps beside its trace (`close`).
   */
  log(): string[]
}

/**
 * What a test's own workspace is built as: `changeable`, for the links and
 * unlinks, unless the spec says `assignable`, for the matrix
 * (`test.use({ building: 'assignable' })`), `updatable`, for updating and
 * removing, `authorable`, for editing, adding and renaming in the
 * library, `loadable`, for loading and unloading, `serveable`, for
 * adding an MCP server and assigning it, `searchable`, for searching
 * the catalogs on loopback its builder starts and launches the server
 * asking, or `gatherable`, for adding sources from a repository beside it.
 */
const BUILDERS: Record<'changeable' | 'assignable' | 'updatable' | 'authorable' | 'loadable' | 'serveable' | 'searchable' | 'gatherable', (at: Places) => void | Promise<Launched>> = {
  changeable,
  assignable,
  updatable,
  authorable,
  loadable,
  serveable,
  searchable,
  gatherable
}

/**
 * The builders whose workspace is served with a home of its own, beside
 * it, rather than the run's: a default workspace there is what remembers
 * the workspaces this machine knows, and the run's home is held to stay
 * empty.
 */
const HOMED: readonly (keyof typeof BUILDERS)[] = ['loadable']

export const test = base.extend<{ served: Served, panel: Panel, watch: Watch, own: Own, building: keyof typeof BUILDERS, variables: Record<string, string>, stalls: void }>({
  building: ['changeable', { option: true }],

  // What a test's own `mindweaver ui` has set in its environment beside the
  // run's, for that server and the lines it runs alone
  // (`test.use({ variables })`): none unless a spec says.
  variables: [{}, { option: true }],

  // How long the machine stalled while each test ran, kept beside the
  // trace of one that failed, from its first fixture to its last: what
  // says whether a run that took seconds waited on the machine rather
  // than on anything of the panel's (`stalled`).
  stalls: [async ({}, use, testInfo) => {
    const before = pressure()
    const started = Date.now()
    await use()
    await keepStalls(testInfo, before, started)
  }, { auto: true }],

  served: async ({}, use) => {
    await use(served())
  },

  // Every test's page, watched from before it opens anything to after the
  // test is done: a violation or an uncaught error on any page fails the
  // test it happened in, whatever that test was about.
  watch: [async ({ page }, use) => {
    const watch: Watch = { violations: [], thrown: [] }
    await page.addInitScript(report)
    page.on('console', (message) => {
      if (VIOLATION.test(message.text())) {
        watch.violations.push(message.text())
      }
    })
    page.on('pageerror', error => watch.thrown.push(error.message))
    await use(watch)
    expect(watch.violations, 'the CSP violations the page logged').toEqual([])
    expect(watch.thrown, 'what the page threw').toEqual([])
  }, { auto: true }],

  panel: async ({ page, served }, use) => {
    await use({ open: (path = '/') => open(page, served, path) })
  },

  // Under the run's temporary folder, which the driver removes, and served
  // with the run's home, which the driver holds to stay as it was made, or
  // one of its own beside it, held here the same way once the server has
  // stopped. Its log is kept beside the trace of a test that failed, once
  // the server has stopped and said all it will.
  own: async ({ page, building, variables }, use, testInfo) => {
    const at = given()
    const folder = mkdtempSync(join(at.scratch, 'own-'))
    const homed = HOMED.includes(building)
    const mine: Places = { ...at, workspace: join(folder, 'workspace'), ...(homed ? { home: join(folder, 'home') } : {}) }
    const twin: Places = { ...mine, workspace: join(folder, 'twin') }
    const launched = (await BUILDERS[building](mine)) ?? undefined
    cpSync(mine.workspace, twin.workspace, { recursive: true })
    const made = homed ? look(mine.home) : undefined
    const server = await serve(mine, 'fixture', variables, launched?.options)
    const log = () => server.said().split('\n').filter(line => line !== '')
    try {
      await use({ at: mine, twin, served: server, launched, log, open: (path = '/') => open(page, server, path) })
    } finally {
      await close(testInfo, server)
      await launched?.stop()
    }
    if (made) {
      expect(look(mine.home), `the home of its own, ${mine.home}, is as it was made`).toEqual(made)
    }
  }
})

export { expect }

/** What Linux says tasks stall on, each in a file of `/proc/pressure`. */
const RESOURCES = ['cpu', 'memory', 'io'] as const

/** What `/proc/pressure` says of each resource. */
export type Pressure = Partial<Record<typeof RESOURCES[number], string>>

/**
 * Each of `/proc/pressure`'s files as it reads now, and none that cannot be
 * read: none at all on another system, or under a kernel built without it.
 * Read by any account.
 */
export function pressure(): Pressure {
  const read: Pressure = {}
  for (const resource of RESOURCES) {
    try {
      read[resource] = readFileSync(`/proc/pressure/${resource}`, 'utf8')
    } catch {
      // Not there to read, which says nothing of the test.
    }
  }
  return read
}

/** The microseconds a pressure file's `some` or `full` line says tasks have stalled in all. */
function total(file: string | undefined, line: 'some' | 'full'): number | undefined {
  const found = file?.match(new RegExp(String.raw`^${line} .*\btotal=(\d+)\s*$`, 'm'))
  return found ? Number(found[1]) : undefined
}

/**
 * How long tasks stalled on each resource between two readings, `took`
 * milliseconds apart, as a failed test keeps it in `pressure.txt`: a line
 * for each, `io: some 4120ms, full 3870ms of 6213ms`, then each file as it
 * read the second time, its averages over the last ten seconds, minute
 * and five minutes among them. `some` is how long at least one task was
 * kept waiting for it, and `full` how long every task that would run was.
 *
 * From the totals rather than the averages, so it is the test's own time,
 * not the minutes around it. An unload once took five and a half seconds
 * on CI, whose runner a whole suite and two browsers share, while reads
 * beside it took their usual milliseconds (#240): how long its disk kept
 * writers waiting then is what would have said whether the machine was
 * the cause.
 */
export function stalled(before: Pressure, after: Pressure, took: number): string {
  const lines: string[] = []
  for (const resource of RESOURCES) {
    const said = (['some', 'full'] as const).flatMap((line) => {
      const from = total(before[resource], line)
      const to = total(after[resource], line)
      return from === undefined || to === undefined ? [] : [`${line} ${Math.round((to - from) / 1000)}ms`]
    })
    if (said.length > 0) {
      lines.push(`${resource}: ${said.join(', ')} of ${took}ms`)
    }
  }
  for (const resource of RESOURCES) {
    if (after[resource] !== undefined) {
      lines.push('', `/proc/pressure/${resource}:`, after[resource].trimEnd())
    }
  }
  return `${lines.join('\n')}\n`
}

/**
 * What keeping a file beside a test's trace reads of its `TestInfo`: how
 * it ended, how it was expected to, where its files go and how one is
 * attached to the report. No more, so `log.spec.ts` can hand `keep` a
 * test that failed, which no test that passes is, and which `test.fail()`
 * cannot make either: a test marked so that fails ended as expected.
 */
export type Ending = Pick<TestInfo, 'status' | 'expectedStatus' | 'outputPath' | 'attach'>

/**
 * Keep what `said` gives as `name` beside the trace of a test that ended
 * other than it was expected to, in its folder of `test-results`, which CI
 * uploads, and attached to the report; nothing, and `said` never asked,
 * for one that ended as expected. Whether a test failed is decided here
 * alone, for every fixture that keeps something, so the one decision is
 * the one `log.spec.ts` holds.
 */
export async function keep(ending: Ending, name: string, said: () => string): Promise<void> {
  if (ending.status === ending.expectedStatus) {
    return
  }
  const path = ending.outputPath(name)
  writeFileSync(path, said())
  await ending.attach(name, { path, contentType: 'text/plain' })
}

/**
 * Stop a test's own `mindweaver ui`, then keep all it said beside the trace
 * of a test that failed, as `mindweaver-ui.log` (`keep`): every request it
 * answered with how long the answer took, and every run with how long it
 * waited its turn behind the runs before it, how long each of its lines
 * took and how much of that it waited for a workspace's lock. Once it has
 * stopped, so it has said all it will. The trace times each request from
 * the browser, which says that a run took seconds but not where they went
 * (#240). Never the token, which the server keeps out of its log. The own
 * fixture's teardown.
 */
export async function close(ending: Ending, server: Serving): Promise<void> {
  await server.stop()
  await keep(ending, 'mindweaver-ui.log', () => server.said())
}

/**
 * Keep how long the machine stalled since `before` was read, at `started`,
 * beside the trace of a test that failed, as `pressure.txt` (`keep`,
 * `stalled`); nothing where Linux said nothing to read. The `stalls`
 * fixture's teardown.
 */
export async function keepStalls(ending: Ending, before: Pressure, started: number): Promise<void> {
  if (Object.keys(before).length > 0) {
    await keep(ending, 'pressure.txt', () => stalled(before, pressure(), Date.now() - started))
  }
}

/**
 * Wait until the page is drawn: its panel there, the snapshot answered, and
 * every font it loads loaded, so a font the CSP refused has been refused by
 * the time a spec looks.
 */
export async function drawn(page: Page): Promise<void> {
  await expect(page.getByRole('main')).toBeVisible()
  await expect(page.getByTestId('reading')).toHaveCount(0)
  await page.evaluate(async () => {
    await document.fonts.ready
  })
}

/**
 * What `mindweaver list --json` says of the fixture workspace, run as every
 * command of the suite is (environment.ts), in the envelope it prints:
 * version 2, and no warning, since the fixture is built clean.
 */
export function listing(): Listing {
  const at = given()
  const ran = run(at, 'mindweaver', at.workspace, ['list', '--json'])
  expect(ran.status, `${ran.line}\n${ran.stderr}`).toBe(0)
  const envelope = JSON.parse(ran.stdout) as Envelope<Listing>
  expect(envelope.version).toBe(2)
  expect(envelope.warnings).toEqual([])
  return envelope.data
}

/**
 * Every file under `root` but its `.mind`, by its route, with what it
 * holds: what a project holds, to hold what the panel left in one to what
 * the same lines left in its twin.
 */
export function tree(root: string): Record<string, string> {
  const found: Record<string, string> = {}
  const walk = (folder: string) => {
    for (const name of readdirSync(folder)) {
      const path = join(folder, name)
      if (name === '.mind') {
        continue
      }
      if (statSync(path).isDirectory()) {
        walk(path)
      } else {
        found[relative(root, path)] = readFileSync(path, 'utf8')
      }
    }
  }
  walk(root)
  return found
}
