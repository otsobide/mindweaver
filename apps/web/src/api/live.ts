import { computed, inject, reactive, readonly, type ComputedRef, type InjectionKey } from 'vue'
import type { QueryClient, QueryKey } from '@tanstack/vue-query'
import type { Holder } from './generated/Holder'
import type { LiveEvent, Transport, Unsubscribe } from './transport'

/** How the server watches the workspace, as far as the panel was told. */
export interface Live {
  /**
   * `waiting` until the server says, then `watching`, when a change on
   * disk is told as it happens, or `unwatched`, when one may not be.
   */
  state: 'waiting' | 'watching' | 'unwatched'
  /** Why not, when not. */
  reason?: string
  /**
   * How many times the server switched workspace since the tab opened, as
   * far as the tab knows: what whatever keeps something of the workspace
   * on screen, a task shown or boxes marked, watches, to let it go.
   */
  switches: number
}

export const liveKey: InjectionKey<Readonly<Live>> = Symbol('live')

/** How the server watches, for a view to say so. */
export function useLive(): Readonly<Live> {
  const live = inject(liveKey)
  if (!live) {
    throw new Error('the panel was started without its live updates')
  }
  return live
}

/**
 * How long an answer from the server stays fresh: for ever while the server
 * tells each change, since every change it tells marks what it reached as
 * stale; not at all otherwise, as before there was a stream, so a page asks
 * again on opening and the window on coming back into focus.
 */
export function useFreshness(): ComputedRef<number> {
  const live = useLive()
  return computed(() => (live.state === 'watching' ? Infinity : 0))
}

/**
 * The queries the server's events mark stale, by the first word of their
 * key (`src/api/snapshot.ts`, `src/api/artifact.ts`, `src/api/plan.ts`,
 * `src/api/updates.ts`, `src/api/draft.ts`, `src/api/workspaces.ts`): the
 * snapshot, what was read of an artifact, the plan of the boxes marked with
 * the lines it previews, what an update would do, what core says of a
 * draft, and what the header may switch to, which are of the disk as it
 * stands.
 * Named rather than every query, for one asked for by hand: a scan (#77)
 * reads the disk far past what is watched, and an event asking for it
 * again would run one on every change, every command and every tab shown
 * again. A query added later is followed only once it is named here.
 * Spelled here rather than imported, since every one of those modules
 * imports this one.
 */
const FOLLOWED: readonly unknown[] = ['snapshot', 'files', 'contents', 'plan', 'preview', 'updates', 'validate', 'workspaces']

/**
 * The query only a run changes, by the first word of its key
 * (`src/api/transcript.ts`): the server's transcript, asked for again once a
 * run finishes and on a resync, which may follow one missed, and never for
 * a change on disk, which runs nothing.
 */
const RAN: readonly unknown[] = ['transcript']

/**
 * How long a run waits to hear on the stream that it finished before it
 * asks for what it changed itself: far past when the server tells it, just
 * before it answers the run, and short enough that a run sent while the
 * stream is let go, or lost, is not long left waiting.
 */
export const PATIENCE = 2000

/**
 * Send a run (`send`), and answer what the server answered once the views
 * have what it changed: once the stream has told it finished and what that
 * marked stale has been asked for again, or, when the stream tells nothing
 * within `PATIENCE`, once it has been asked for again regardless.
 */
export type Settle = <Answer extends { lines: string[] }>(send: () => Promise<Answer>) => Promise<Answer>

export const settleKey: InjectionKey<Settle> = Symbol('settle')

/** How a run waits for its views, for whatever sends one. */
export function useSettle(): Settle {
  const settle = inject(settleKey)
  if (!settle) {
    throw new Error('the panel was started without its live updates')
  }
  return settle
}

/**
 * Send a switch (`send`), and answer what the server answered once the
 * views have the new workspace's answers: once the stream has told it
 * `switched` and the tab has let go of the workspace before, or, when the
 * stream tells nothing within `PATIENCE`, once it has let go regardless,
 * as a run is settled (`Settle`). Not settled on a refusal, which switched
 * nothing.
 */
export type SettleSwitch = <Answer>(send: () => Promise<Answer>) => Promise<Answer>

export const settleSwitchKey: InjectionKey<SettleSwitch> = Symbol('settle-switch')

/** How a switch waits for its views, for whatever sends one. */
export function useSettleSwitch(): SettleSwitch {
  const settle = inject(settleSwitchKey)
  if (!settle) {
    throw new Error('the panel was started without its live updates')
  }
  return settle
}

/**
 * Mark stale what the server's events follow: all of it, or what a change
 * of `scopes` reached. The views on screen then ask for it again.
 */
export function markStale(client: QueryClient, scopes?: Holder[]): Promise<void> {
  return client.invalidateQueries({
    predicate: ({ queryKey }) => FOLLOWED.includes(queryKey[0]) && (scopes === undefined || reached(queryKey, scopes))
  })
}

/**
 * Mark stale everything a run can change: what the events follow, all of
 * it, and the transcript. Settled once the views on screen have it again.
 */
function markRan(client: QueryClient): Promise<void> {
  return Promise.all([
    markStale(client),
    client.invalidateQueries({ predicate: ({ queryKey }) => RAN.includes(queryKey[0]) })
  ]).then(() => undefined)
}

/**
 * Listen to the server's events for as long as the panel is open, one
 * subscription for the tab, and turn each into what Vue Query is to ask for
 * again, never a read of its own and never a scan: a change marks the
 * snapshot stale, and what was read of an artifact in a holder it reached,
 * which the views on screen then ask for again.
 *
 * And settle each run the panel sends (`Settle`) on the finish the stream
 * tells of it, so a run is over for the panel once its views are drawn
 * from what it changed, never from an answer guessed beforehand: the
 * stream says when, and a read is what the views are drawn again from.
 *
 * The subscription is let go while the tab is hidden and taken again when it
 * is shown, which the server answers with `resync`: a browser opens at most
 * six HTTP/1.1 connections to one server, and every tab holding a stream
 * holds one of them, so a few tabs left open behind would leave the one in
 * front none to ask with.
 */
export function listen(transport: Transport, client: QueryClient, page: Document = document, patience: number = PATIENCE) {
  const live = reactive<Live>({ state: 'waiting', switches: 0 })
  let unsubscribe: Unsubscribe | undefined
  /** Each run waiting on its finish, told every finish and what it set off. */
  const waiting = new Set<(lines: string[], asked: Promise<void>) => void>()
  /** Each switch waiting on the word it was taken, told what letting go set off. */
  const switching = new Set<(asked: Promise<void>) => void>()
  /**
   * How many switches the server had taken as the stream last said, by the
   * count each `switched` and `resync` carries: none before it first says.
   */
  let heard: number | undefined

  function told(event: LiveEvent) {
    switch (event.type) {
      case 'changed':
        void markStale(client, event.scopes)
        break
      // A command can change anything, and what it changed may lie where
      // nothing is watched: everything an event follows is asked for again,
      // and the transcript, which it is now in.
      case 'command_finished': {
        const asked = markRan(client)
        for (const finished of waiting) {
          finished(event.lines, asked)
        }
        break
      }
      // A count other than the last one heard is a switch this tab missed:
      // its stream was let go while it was hidden, or fell behind. Let go
      // of the workspace before as on hearing `switched`, or a task it
      // shows would be run in the new one under lines naming the old one,
      // for as long as it stayed hidden.
      case 'resync': {
        const missed = heard !== undefined && event.switches !== heard
        heard = event.switches
        if (missed) {
          tellSwitched()
        } else {
          void markRan(client)
        }
        break
      }
      // One a resync counted already is not let go of twice: one the
      // stream still held behind the resync of a tab that fell behind.
      case 'switched':
        if (heard === undefined || event.switches > heard) {
          heard = event.switches
          tellSwitched()
        }
        break
      // A run under way, how it goes, and that it was asked to stop or is
      // what a run just sent waits behind: nothing on disk is told by any
      // of them, so nothing is asked for again. What a view draws of them
      // is the page following a run's (#113).
      case 'command_started':
      case 'progress':
      case 'cancelling':
      case 'busy':
        break
      case 'watching':
        live.state = 'watching'
        live.reason = undefined
        break
      case 'unwatched':
        live.state = 'unwatched'
        live.reason = event.reason
        break
    }
  }

  // Let go of the workspace before, as `switched` says, and tell every
  // switch this tab waits on that it was taken.
  function tellSwitched() {
    const asked = switched()
    for (const taken of switching) {
      taken(asked)
    }
  }

  // Let go of the workspace before a switch, from this tab or another:
  // every answer reset rather than marked stale, since an answer of the
  // workspace before is no answer of this one, and a view drawing it while
  // the new one comes would show one workspace's projects under the
  // other's name; and `switches` counted, for what keeps a task shown or
  // boxes marked to let them go.
  function switched(): Promise<void> {
    live.switches += 1
    return client.resetQueries()
  }

  function start() {
    unsubscribe ??= transport.subscribe(told)
  }

  function pause() {
    unsubscribe?.()
    unsubscribe = undefined
  }

  function shown() {
    if (page.visibilityState === 'hidden') {
      pause()
    } else {
      start()
    }
  }

  // Waiting from before the run is sent: the server tells the finish just
  // before it answers, so the stream may well say it first.
  async function settle<Answer extends { lines: string[] }>(send: () => Promise<Answer>): Promise<Answer> {
    const heard: { lines: string[], asked: Promise<void> }[] = []
    let expecting: ((lines: string[], asked: Promise<void>) => boolean) | undefined
    const finished = (lines: string[], asked: Promise<void>) => {
      if (!expecting?.(lines, asked)) {
        heard.push({ lines, asked })
      }
    }
    waiting.add(finished)
    try {
      const answer = await send()
      const before = heard.find(each => sameLines(each.lines, answer.lines))
      await (before?.asked ?? new Promise<void>((resolve) => {
        const timer = setTimeout(() => resolve(markRan(client)), patience)
        expecting = (lines, asked) => {
          if (!sameLines(lines, answer.lines)) {
            return false
          }
          clearTimeout(timer)
          resolve(asked)
          return true
        }
      }))
      return answer
    } finally {
      waiting.delete(finished)
    }
  }

  // Waiting from before the switch is sent, as a run waits: the server
  // tells it taken just before it answers. Any word of one will do, there
  // being nothing in it to tell two apart: either way the tab lets go once.
  async function settleSwitch<Answer>(send: () => Promise<Answer>): Promise<Answer> {
    let heard: Promise<void> | undefined
    let expecting: ((asked: Promise<void>) => void) | undefined
    const taken = (asked: Promise<void>) => {
      heard ??= asked
      expecting?.(asked)
    }
    switching.add(taken)
    try {
      const answer = await send()
      await (heard ?? new Promise<void>((resolve) => {
        const timer = setTimeout(() => resolve(switched()), patience)
        expecting = (asked) => {
          clearTimeout(timer)
          resolve(asked)
        }
      }))
      return answer
    } finally {
      switching.delete(taken)
    }
  }

  page.addEventListener('visibilitychange', shown)
  shown()
  return {
    live: readonly(live),
    settle: settle as Settle,
    settleSwitch: settleSwitch as SettleSwitch,
    stop() {
      page.removeEventListener('visibilitychange', shown)
      pause()
    }
  }
}

/** Whether two runs' lines are the same lines, in the same order. */
function sameLines(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((line, index) => line === b[index])
}

/**
 * Whether a change of `scopes` reaches what was asked under `key`: what was
 * read of an artifact, by the holder it was read from, and the snapshot,
 * which holds every holder.
 */
function reached(key: QueryKey, scopes: Holder[]): boolean {
  const named = key[1]
  if (typeof named !== 'object' || named === null || !('holder' in named)) {
    return true
  }
  const holder = named.holder as Holder
  return scopes.some(scope => same(scope, holder))
}

/**
 * Whether two holders are one: the workspace, or one project by its entry,
 * whether a task names it as its own or as one elsewhere.
 */
function same(a: Holder, b: Holder): boolean {
  return a.type === 'workspace' ? b.type === 'workspace' : b.type !== 'workspace' && b.entry === a.entry
}
