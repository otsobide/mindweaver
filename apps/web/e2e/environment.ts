/*
 * What every process the end-to-end suite starts runs in: the binaries under
 * test, the temporary folder holding the fixture workspace and the home, and
 * an environment that leads to no workspace or agent home of the
 * developer's. The driver (run.ts), the config and the specs all read it
 * from here, so none of them can start a command some other way.
 */
import { spawn, spawnSync, type ChildProcess } from 'node:child_process'
import { existsSync, lstatSync, mkdirSync, readdirSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'

/**
 * The variables that lead a command to a real workspace or a real agent
 * home, as the CLI's own tests name them (`common::CLEARED`): an activated
 * workspace, the default one's home, each under the name a Mindflayer build
 * read it by too, and the agents' folders. The driver
 * starts Playwright without them, so neither the server nor any command a
 * spec runs, nor a worker, ever sees them.
 */
export const CLEARED = [
  'MINDWEAVER_HOME',
  'MINDWEAVER_WORKSPACE',
  'MINDFLAYER_HOME',
  'MINDFLAYER_WORKSPACE',
  'CLAUDE_CONFIG_DIR',
  'CODEX_HOME',
  'OPENCODE_CONFIG_DIR'
] as const

/**
 * Every agent home empty, so nothing the server or a command runs reads a
 * real one, whatever the environment says: the four options the CLI's tests
 * pin (`common::PINNED`) beside `--home` and `--workspace`.
 */
export const NO_AGENT_HOMES = [
  '--claude-config-dir=',
  '--codex-home=',
  '--opencode-config-dir=',
  '--agents-dir='
] as const

/** The variables the driver hands the config and the workers. */
const SCRATCH = 'MINDWEAVER_E2E_SCRATCH'
const BIN = 'MINDWEAVER_E2E_BIN'

/** Where a run keeps what it builds, and what it runs. */
export interface Places {
  /** The temporary folder all of it is in, which the driver removes. */
  scratch: string
  /**
   * The home every command and the server get, as `HOME` and `--home`: made
   * empty, and held to stay that way (run.ts).
   */
  home: string
  /** The fixture workspace, holding `.mindweaver` and the projects it links. */
  workspace: string
  /** The folder holding the `mindweaver` and `mind` under test. */
  bin: string
}

/** The places for a run in `scratch`, of the binaries in `bin`. */
export function places(scratch: string, bin: string): Places {
  return { scratch, home: join(scratch, 'home'), workspace: join(scratch, 'workspace'), bin }
}

/** What the driver hands every process after it, in the environment. */
export function handed(at: Places): Record<string, string> {
  return { [SCRATCH]: at.scratch, [BIN]: at.bin }
}

/**
 * The places the driver chose, read back from the environment it started
 * Playwright with. Without them the suite was started some other way, which
 * would serve no fixture, or one nobody checks the home of afterwards.
 */
export function given(): Places {
  const scratch = process.env[SCRATCH]
  const bin = process.env[BIN]
  if (!scratch || !bin) {
    throw new Error('the end-to-end suite runs through its driver, which builds the fixture first: run `make web/e2e`')
  }
  for (const variable of CLEARED) {
    if (variable in process.env) {
      throw new Error(`${variable} is set, and it could lead to a real workspace or agent home: run the suite through \`make web/e2e\`, which clears it`)
    }
  }
  return places(scratch, bin)
}

/**
 * What makes a folder a project or a workspace to a command walking up the
 * tree: a `.mind`, a `.mindweaver`, or the `.mindflayer` a Mindflayer build
 * made a workspace in, which resolving still finds.
 */
const MARKS = ['.mind', '.mindweaver', '.mindflayer']

/**
 * The folder above `folder`, itself included, that holds any of `MARKS`, if
 * one does: resolving walks up the tree before it looks at a home, so a
 * workspace built under such a folder could reach it. Here rather than in
 * the driver, so that whatever else builds a workspace in the temporary
 * folder refuses the same.
 */
export function marked(folder: string): string | undefined {
  for (let at = folder; ; at = dirname(at)) {
    if (MARKS.some(mark => existsSync(join(at, mark)))) {
      return at
    }
    if (dirname(at) === at) {
      return undefined
    }
  }
}

/**
 * Everything under `folder` and the folder itself, each with its size and
 * when it last changed: a file written and removed again still changes the
 * folder it was in. What a home is held to once the server has stopped,
 * the run's by the driver and one of a test's own by its fixture
 * (panel.ts).
 */
export function look(folder: string): string[] {
  const seen = [`. ${lstatSync(folder).mtimeMs}`]
  for (const entry of readdirSync(folder, { recursive: true, withFileTypes: true })) {
    const path = join(entry.parentPath, entry.name)
    const stat = lstatSync(path)
    seen.push(`${relative(folder, path)} ${entry.isDirectory() ? 'folder' : `${stat.size} bytes`} ${stat.mtimeMs}`)
  }
  return seen.sort()
}

/** The path of the binary called `name` in `bin`. */
export function binary(bin: string, name: 'mindweaver' | 'mind'): string {
  return join(bin, process.platform === 'win32' ? `${name}.exe` : name)
}

/** Whether both binaries are in `bin`, to run. */
export function built(bin: string): boolean {
  return existsSync(binary(bin, 'mindweaver')) && existsSync(binary(bin, 'mind'))
}

/**
 * This process's environment without any variable `CLEARED` names: what the
 * driver starts Playwright with, and what every binary is started with
 * besides its home, cleared again there so a helper used some other way
 * cannot let one through.
 */
export function cleared(): Record<string, string> {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined && !(CLEARED as readonly string[]).includes(entry[0])
    )
  )
}

/**
 * The environment a binary runs in: `cleared`, with the temporary home as
 * `HOME`. Not an empty one, with which the standard library falls back to
 * the account's real home.
 */
export function environment(at: Places): Record<string, string> {
  return { ...cleared(), HOME: at.home, USERPROFILE: at.home }
}

/**
 * The options every line names its home, its workspace and the agent homes
 * by: `--home` the temporary one, `--workspace` the fixture, or, before
 * there is one to name, empty, which names none and ignores
 * `$MINDWEAVER_WORKSPACE` too, and every agent home empty. Named on every
 * line even though `-C` is under the temporary folder: resolving walks up
 * from there, and only a line that names its workspace cannot be led to
 * another.
 */
export function pinned(at: Places, workspace: 'fixture' | 'none'): string[] {
  return [`--home=${at.home}`, `--workspace=${workspace === 'fixture' ? at.workspace : ''}`, ...NO_AGENT_HOMES]
}

/**
 * Who the repositories a spec or the demo makes commit as, and when: fixed,
 * so a regenerated shelf shows the same commits. Neither the person running
 * it nor their git configuration: no system or global file is read.
 */
function author(date: string): Record<string, string> {
  return {
    GIT_AUTHOR_NAME: 'Demo',
    GIT_AUTHOR_EMAIL: 'demo@example.com',
    GIT_AUTHOR_DATE: date,
    GIT_COMMITTER_NAME: 'Demo',
    GIT_COMMITTER_EMAIL: 'demo@example.com',
    GIT_COMMITTER_DATE: date,
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_CONFIG_GLOBAL: '/dev/null'
  }
}

/**
 * Run git with `args` in `directory`, as that author at `date`, in the
 * environment every binary runs in, failing unless it exits 0: how a
 * repository somebody keeps is made, which only git makes, for the demo's
 * shelf and for a spec gathering from one.
 */
export function git(at: Places, directory: string, args: string[], date = '2026-10-01T09:00:00Z'): void {
  const ran = spawnSync('git', ['-c', 'init.defaultBranch=main', '-c', 'commit.gpgsign=false', ...args], {
    cwd: directory,
    env: { ...environment(at), ...author(date) },
    encoding: 'utf8',
    timeout: 60_000
  })
  if (ran.error) {
    throw ran.error
  }
  if (ran.status !== 0) {
    throw new Error(`\`git ${args.join(' ')}\` in ${directory} exited ${ran.status}:\n${ran.stdout}${ran.stderr}`)
  }
}

/** A repository at `folder`, empty, as `git init` makes one. */
export function repository(at: Places, folder: string): void {
  mkdirSync(folder, { recursive: true })
  git(at, folder, ['init', '--quiet'])
}

/** What a line printed, and how it exited. */
export interface Ran {
  line: string
  status: number | null
  stdout: string
  stderr: string
}

/**
 * Run `name` with `args` in `directory`, which it is also given as `-C`, and
 * every option `pinned` names, and say what it printed.
 */
export function run(at: Places, name: 'mindweaver' | 'mind', directory: string, args: string[], workspace: 'fixture' | 'none' = 'fixture'): Ran {
  const line = [...args, '-C', directory, ...pinned(at, workspace)]
  const ran = spawnSync(binary(at.bin, name), line, {
    cwd: directory,
    env: environment(at),
    encoding: 'utf8',
    timeout: 60_000
  })
  if (ran.error) {
    throw ran.error
  }
  return { line: [name, ...line].join(' '), status: ran.status, stdout: ran.stdout, stderr: ran.stderr }
}

/**
 * `run`, without holding this process up while the line runs: for a line
 * that reaches a server this process serves, which `run`'s wait would keep
 * from answering.
 */
export function runApart(at: Places, name: 'mindweaver' | 'mind', directory: string, args: string[]): Promise<Ran> {
  const line = [...args, '-C', directory, ...pinned(at, 'fixture')]
  const child = spawn(binary(at.bin, name), line, { cwd: directory, env: environment(at), stdio: ['ignore', 'pipe', 'pipe'] })
  let stdout = ''
  let stderr = ''
  child.stdout!.setEncoding('utf8').on('data', (chunk: string) => (stdout += chunk))
  child.stderr!.setEncoding('utf8').on('data', (chunk: string) => (stderr += chunk))
  return new Promise((resolve, refuse) => {
    child.on('error', refuse)
    child.on('close', status => resolve({ line: [name, ...line].join(' '), status, stdout, stderr }))
  })
}

/** `run`, failing unless the line exits 0 and says no warning. */
export function succeed(at: Places, name: 'mindweaver' | 'mind', directory: string, args: string[], workspace: 'fixture' | 'none' = 'fixture'): Ran {
  const ran = run(at, name, directory, args, workspace)
  if (ran.status !== 0 || /^warning:/m.test(ran.stderr)) {
    throw new Error(`\`${ran.line}\` exited ${ran.status}:\n${ran.stdout}${ran.stderr}`)
  }
  return ran
}

/**
 * `mindweaver ui` as a person starts it over the workspace `at` names, on a
 * port the system picks, with nothing opened: what the config starts the
 * suite's server with, and `serve` a spec's own. With `workspace` `none` it
 * names none, and finds the one it is started in, as a person's does: what
 * the screenshots show, from a folder no other workspace is above. With
 * `log`, it says on stderr every request it answers and every run, with how
 * long each took (`--log`).
 */
export function ui(at: Places, workspace: 'fixture' | 'none' = 'fixture', log = false): string[] {
  return ['ui', '--no-open', '--port', '0', ...(log ? ['--log'] : []), '-C', at.workspace, ...pinned(at, workspace)]
}

/** A `mindweaver ui` a spec started, and how to stop it. */
export interface Serving {
  /** Where it listens: `http://127.0.0.1:<port>`. */
  origin: string
  token: string
  /** The link it printed, which trades the token for the cookie. */
  link: string
  /**
   * All it has said on stderr so far: the note saying where it serves, then
   * its log, a line for every request answered and for every run, each
   * after the time of day (`--log`), never holding the token.
   */
  said(): string
  /** Ctrl-C, as a person stops it, and wait until it has. */
  stop(): Promise<void>
}

/** The link `mindweaver ui` prints first on stdout, and the only line there while it serves. */
const LINK = /^(?<origin>http:\/\/127\.0\.0\.1:\d+)\/\?token=(?<token>[0-9a-f]+)$/m

/**
 * Start `mindweaver ui` over the workspace `at` names, as the config starts the
 * suite's, from that workspace with the environment `environment` gives,
 * and answer once it printed its link: for a spec that changes a workspace
 * of its own, which no other spec reads. Its stdout is read, never
 * printed, so the token stays out of the log.
 *
 * With its log (`--log`), kept with the rest of its stderr for as long as
 * it runs, and after (`Serving.said`): what a test that failed keeps
 * beside its trace (panel.ts), to say where a run's time went, which a
 * trace, timing each request from the browser, cannot (#240). Not the
 * config's server, whose stderr Playwright prints, a line for every
 * request of every test: no spec runs a change on it.
 *
 * With `variables` set in its environment beside what `environment` gives,
 * for that server alone and every line it runs: what a spec sets where the
 * agents of its projects would run, an MCP server's secret among it.
 */
export function serve(at: Places, workspace: 'fixture' | 'none' = 'fixture', variables: Record<string, string> = {}): Promise<Serving> {
  const child = spawn(binary(at.bin, 'mindweaver'), ui(at, workspace, true), {
    cwd: at.workspace,
    env: { ...environment(at), ...variables },
    stdio: ['ignore', 'pipe', 'pipe']
  })
  return new Promise((resolve, refuse) => {
    let printed = ''
    let said = ''
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      refuse(new Error(`mindweaver ui printed no link within a minute:\n${said}`))
    }, 60_000)
    child.stderr!.setEncoding('utf8').on('data', (chunk: string) => (said += chunk))
    child.stdout!.setEncoding('utf8').on('data', (chunk: string) => {
      printed += chunk
      const link = LINK.exec(printed)
      if (link?.groups) {
        clearTimeout(timer)
        resolve({ origin: link.groups.origin!, token: link.groups.token!, link: link[0], said: () => said, stop: () => stopped(child) })
      }
    })
    child.on('error', (error) => {
      clearTimeout(timer)
      refuse(error)
    })
    child.on('exit', (code) => {
      clearTimeout(timer)
      refuse(new Error(`mindweaver ui exited ${code} before it printed a link:\n${said}`))
    })
  })
}

/**
 * `child` stopped with the Ctrl-C a person stops `mindweaver ui` with, which
 * lets it close every event stream and finish what it runs, and killed if
 * it has not stopped ten seconds later.
 */
function stopped(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve()
  }
  return new Promise((done) => {
    const timer = setTimeout(() => child.kill('SIGKILL'), 10_000)
    child.on('exit', () => {
      clearTimeout(timer)
      done()
    })
    child.kill('SIGINT')
  })
}
