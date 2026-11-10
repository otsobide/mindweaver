import { inject, type InjectionKey } from 'vue'
import type { Confirm } from './generated/Confirm'
import type { Contents } from './generated/Contents'
import type { Files } from './generated/Files'
import type { Folders } from './generated/Folders'
import type { Holder } from './generated/Holder'
import type { Judged } from './generated/Judged'
import type { Library } from './generated/Library'
import type { Outcome } from './generated/Outcome'
import type { Planned } from './generated/Planned'
import type { Project } from './generated/Project'
import type { Query } from './generated/Query'
import type { Scanned } from './generated/Scanned'
import type { Snapshot } from './generated/Snapshot'
import type { Step } from './generated/Step'
import type { Switch } from './generated/Switch'
import type { Switchable } from './generated/Switchable'
import type { Task } from './generated/Task'
import type { Updates } from './generated/Updates'

/*
 * What the panel sends and is answered with is the backend's, generated
 * from Rust into `generated/` (make web/types): a read for core to answer
 * (`Query`), a change (`Task`), what running one produced (`Outcome`), and
 * the workspace to act on from now on (`Switch`), answered with its row of
 * what may be switched to (`Switchable`).
 */
export type { Outcome, Query, Switch, Switchable, Task }

/**
 * What each query is answered with, by its `type`. Every query names its
 * answer here: one Rust adds that is not named is an index `Answer` cannot
 * take, so the panel does not compile until it is.
 */
export interface Answers {
  snapshot: Snapshot
  projects: Project[]
  library: Library
  project: Project
  files: Files
  contents: Contents
  scan: Scanned
  folders: Folders
  plan: Planned
  updates: Updates
  validate: Judged
  workspaces: Switchable[]
}

/** What `query` is answered with. */
export type Answer<Asked extends Query> = Answers[Asked['type']]

/**
 * The command lines a task is, as somebody would type them, and what of
 * them the panel has somebody confirm before it runs them, none for a task
 * it runs once asked; nothing ran. Which lines take something away is the
 * backend's to say, in the generated `Confirm`, never the panel's; and so
 * is whether a run of it can be asked to stop while it runs
 * (`cancellable`), a gather's.
 */
export interface Preview {
  lines: string[]
  confirm: Confirm[]
  cancellable: boolean
}

/**
 * A task that ran: its lines, and what running them produced, each line's
 * own run among it (the generated `Run`). What a run is answered with, and
 * each entry of the transcript.
 */
export interface Ran {
  lines: string[]
  outcome: Outcome
}

/**
 * What the server's event stream tells (`GET /api/v1/events`), each told
 * apart by its `type`:
 *
 * - `changed`: something the server watches changed on disk, so what is
 *   shown of each holder in `scopes` may be out of date;
 * - `command_started` and `command_finished`: a task's lines started to
 *   run, and stopped, however that went, each with the run's number, which
 *   a cancel names it by, and the first with whether it takes one;
 * - `progress`: a step a run took, as the backend says it (the generated
 *   `Step`), its words and whether a cancel still stops it among it;
 * - `cancelling`: a run was asked to stop, from this tab, another or a
 *   script;
 * - `busy`: what was just sent waits behind the run under way, or, on a
 *   stream just opened, a run is under way, its latest step after it;
 * - `resync`: something may have been missed, as on connecting, so
 *   everything is asked for again;
 * - `switched`: the server acts on another workspace since, from this tab
 *   or another, so everything is asked for again and nothing shown of the
 *   one before is kept;
 * - each of those two with `switches`, how many switches the server has
 *   taken since it started, by which a tab that missed one, its stream let
 *   go while it was hidden or fallen behind, learns of it on its resync;
 * - `watching` and `unwatched`: whether everything the server was told to
 *   watch is, and why not, when a change there shows only once asked for
 *   by hand.
 *
 * Spelled here, beside the server's other replies, rather than generated
 * from Rust: the stream is the server's own, and the server is a crate of
 * its own with no part in `make web/types`, as `{lines, confirm}` and
 * `{lines, outcome}` are. What a change is to is the backend's, a holder as
 * a query names it, so that much is the generated `Holder`; and so is a
 * step a run reports, the generated `Step`.
 */
export type LiveEvent
  = | { type: 'changed', scopes: Holder[] }
    | { type: 'command_started', run: number, lines: string[], cancellable: boolean }
    | { type: 'progress', run: number, progress: Step }
    | { type: 'cancelling', run: number }
    | { type: 'busy', run: number, lines: string[], cancellable: boolean, cancelling: boolean }
    | { type: 'command_finished', run: number, lines: string[] }
    | { type: 'resync', switches: number }
    | { type: 'switched', switches: number }
    | { type: 'watching' }
    | { type: 'unwatched', reason: string }

/** Stops a subscription. */
export type Unsubscribe = () => void

/**
 * How the panel reaches the front end behind it, and the only way it does.
 *
 * An interface rather than the HTTP calls themselves, so a desktop shell can
 * answer the same five calls without a server (D-17), and so authentication
 * lives in exactly one place: the implementation. Every request the panel
 * makes goes through one of these; a lint rule keeps `fetch` out of every
 * other file.
 */
export interface Transport {
  /** Ask for a read. It changes nothing, so it may be asked again freely. */
  query<Asked extends Query>(query: Asked): Promise<Answer<Asked>>

  /**
   * Hand over a task: `preview` for the lines it would run, which the panel
   * shows first; `run` to queue it behind every task sent before it and wait
   * for its outcome.
   */
  send(task: Task, mode: 'preview'): Promise<Preview>
  send(task: Task, mode: 'run'): Promise<Ran>

  /**
   * Act on another workspace from now on: one the answer to the
   * `workspaces` query lists, by its path as listed, which the server
   * holds to that list read again, refusing any other. Queued behind every
   * task sent before it, and answered with its row once it is taken, which
   * the stream then tells every tab (`switched`).
   */
  switch(to: Switch): Promise<Switchable>

  /**
   * Every task the session ran, oldest first, each with what it produced:
   * the server's transcript, which it keeps for as long as it runs.
   */
  transcript(): Promise<Ran[]>

  /**
   * Be told whenever what the panel shows may have changed, or how the
   * server watches for it, until unsubscribed. `resync` comes first, and
   * again whenever something may have been missed, a connection lost and
   * made again among them.
   */
  subscribe(listener: (event: LiveEvent) => void): Unsubscribe
}

/**
 * A request that did not get its answer: the server refused it, answered
 * with something that is not JSON, or could not be reached or stopped
 * before its answer was whole (status 0).
 *
 * With the code of the condition whenever the server answered, as every
 * answer under `/api` that is no success says one (`{"error", "code"}`),
 * the backend's for what it refused and the server's own for the rest, each
 * one of the generated `Code`: the panel acts on a refusal by its code, as
 * on a run's warnings, and never on its words. None for an answer that came
 * from something else on the way, or for no answer at all.
 */
export class TransportError extends Error {
  readonly status: number
  readonly code?: string

  constructor(status: number, message: string, code?: string) {
    super(message)
    this.name = 'TransportError'
    this.status = status
    this.code = code
  }

  /** Whether the server wants the launch token this browser does not hold. */
  get unauthorized(): boolean {
    return this.status === 401
  }
}

export const transportKey: InjectionKey<Transport> = Symbol('transport')

/** The transport the panel was started with. */
export function useTransport(): Transport {
  const transport = inject(transportKey)
  if (!transport) {
    throw new Error('the panel was started without a transport')
  }
  return transport
}
