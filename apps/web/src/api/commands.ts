import { computed, inject, provide, ref, watch, type ComputedRef, type InjectionKey } from 'vue'
import { useMutation, useMutationState } from '@tanstack/vue-query'
import { useLive, useSettle, type Underway } from './live'
import { useTransport, type Preview, type Ran, type Task } from './transport'

/** A task shown before it runs: what was asked for, and the lines it is. */
export interface Asking {
  task: Task
  preview: Preview
  /** Whether the page that asked draws what the run printed itself. */
  drawn: boolean
}

/** How a page asks for a task, beside the task. */
export interface Asked {
  /**
   * The page draws what the run printed itself, a search's JSON read back
   * and listed, so how it ended is said without it: what a terminal would
   * show of it is the page's to lay out, never a toast's to repeat.
   */
  drawn?: boolean
}

/**
 * How the panel changes anything (D-18): a task, previewed by the server,
 * shown, and run only once somebody says so.
 *
 * Every action of every page goes through `ask`, so none runs a line it did
 * not show first, and none runs one the backend says takes something away
 * without the question being one: which lines those are is the preview's
 * `confirm`, the CLI's list, and nothing here decides it. The lines shown
 * are the preview's, and the panel never puts one together itself.
 */
export interface Runner {
  /**
   * Preview `task` and show it, its lines and what they take away. Answers
   * what its run was answered with, once the views are drawn from what it
   * changed, or nothing when it was let go, refused, or something else is
   * already under way.
   */
  ask(task: Task, asked?: Asked): Promise<Ran | undefined>
  /** What is shown now, waiting for a yes or a no. */
  asking: ComputedRef<Asking | undefined>
  /** Whether what is shown takes something away, which makes it a confirmation. */
  confirming: ComputedRef<boolean>
  /** Whether a run of it is under way, its views not yet drawn again. */
  running: ComputedRef<boolean>
  /** Whether anything is: a preview asked for, one shown, or a run. */
  busy: ComputedRef<boolean>
  /** Yes: run what is shown. */
  run(): Promise<void>
  /** No: let it go, nothing run. */
  cancel(): void
  /**
   * The run of what is shown, while it is under way, as the stream tells
   * it: its latest step, whether it takes a cancel and whether one was
   * asked for. None for a run the stream has not told started yet, and
   * none for another tab's.
   */
  underway: ComputedRef<Underway | undefined>
  /**
   * Ask the run of what is shown to stop, while it is under way and takes
   * a cancel. Whether it stopped is its outcome's to say: a gather stops
   * before it places anything, a backup before it pushes, and one past
   * that finishes.
   */
  stop(): Promise<void>
  /**
   * What the last run that answered was answered with, for the warnings
   * drawer: not the run mutation's own answer, which Vue Query clears as the
   * next run starts and leaves cleared when that one fails, so a later
   * refusal would leave the drawer an earlier run's toast opens saying
   * nothing has run.
   */
  said: ComputedRef<Ran | undefined>
}

export const runnerKey: InjectionKey<Runner> = Symbol('runner')

/** The panel's runner, for a page to ask with. */
export function useRunner(): Runner {
  const runner = inject(runnerKey)
  if (!runner) {
    throw new Error('the panel was started without its runner')
  }
  return runner
}

/**
 * What a front end makes of how a task ended, which the runner leaves to
 * whatever draws: the panel's toasts and drawers (`CommandRunner.vue`).
 */
export interface Say {
  /**
   * It ran, and its views are drawn from what it changed; `drawn` when the
   * page that asked draws what it printed itself.
   */
  ran(ran: Ran, drawn: boolean): void
  /**
   * It did not run, or its answer never came: refused when previewed or when
   * run (`ran` false, or true and refused with a 4xx, nothing ran either
   * way), or the server failed or stopped while it ran, which may have
   * changed something.
   */
  refused(error: unknown, ran: boolean): void
}

/**
 * Make the panel's one runner and hand it to everything under the caller:
 * one, so two pages cannot show two tasks at once, and made where Vue Query
 * and the transport are at hand, telling `say` how each task ended.
 *
 * The preview and the run are Vue Query mutations, so what the server
 * answered stays in Vue Query; what is shown is the preview's answer, held
 * until it is answered. A run is settled on the stream's word that it
 * finished (`useSettle`), so what says it is over comes once the views show
 * what it did.
 */
export function provideRunner(say: Say): Runner {
  const transport = useTransport()
  const settle = useSettle()
  const live = useLive()

  const previewing = useMutation({
    mutationKey: ['preview'],
    mutationFn: (task: Task) => transport.send(task, 'preview')
  })
  const running = useMutation({
    mutationKey: ['run'],
    mutationFn: (task: Task) => settle(() => transport.send(task, 'run')),
    // Each run's answer is kept for as long as the tab is open, not the five
    // minutes after the next run starts that Vue Query keeps one by default:
    // the drawer may be opened on the last that answered at any time, and a
    // tab runs few enough for every answer to cost nothing kept.
    gcTime: Infinity
  })
  const answered = useMutationState({
    filters: { mutationKey: ['run'], status: 'success' },
    select: mutation => mutation.state.data as Ran
  })

  /** Who is waiting on what is shown, to be told how it ended. */
  let answer: ((ran: Ran | undefined) => void) | undefined

  /** Whether the page that asked for what is shown draws what it prints. */
  const drawn = ref(false)

  const asking = computed<Asking | undefined>(() => {
    const preview = previewing.data.value
    const task = previewing.variables.value
    return preview && task ? { task, preview, drawn: drawn.value } : undefined
  })

  function end(ran: Ran | undefined) {
    previewing.reset()
    const waiting = answer
    answer = undefined
    waiting?.(ran)
  }

  const busy = computed(() => previewing.isPending.value || asking.value !== undefined || running.isPending.value)

  // The run the stream tells of that is the one shown, by its lines, which
  // the server previewed and runs alike: its number is the stream's to
  // tell, the run's answer coming only once it is over.
  const underway = computed<Underway | undefined>(() => {
    const now = live.underway
    const shown = asking.value
    if (!running.isPending.value || !now || !shown) {
      return undefined
    }
    const same = now.lines.length === shown.preview.lines.length && now.lines.every((line, index) => line === shown.preview.lines[index])
    return same ? now : undefined
  })

  const runner: Runner = {
    async ask(task, asked = {}) {
      if (busy.value) {
        return undefined
      }
      drawn.value = asked.drawn ?? false
      try {
        await previewing.mutateAsync(task)
      } catch (error) {
        previewing.reset()
        say.refused(error, false)
        return undefined
      }
      return new Promise<Ran | undefined>((resolve) => {
        answer = resolve
      })
    },
    asking,
    confirming: computed(() => (asking.value?.preview.confirm.length ?? 0) > 0),
    running: computed(() => running.isPending.value),
    busy,
    async run() {
      const asked = asking.value
      if (!asked || running.isPending.value) {
        return
      }
      try {
        const ran = await running.mutateAsync(asked.task)
        say.ran(ran, asked.drawn)
        end(ran)
      } catch (error) {
        say.refused(error, true)
        end(undefined)
      }
    },
    cancel() {
      if (!running.isPending.value) {
        end(undefined)
      }
    },
    underway,
    async stop() {
      const now = underway.value
      if (!now?.cancellable || now.cancelling || now.step?.cancellable === false) {
        return
      }
      // A cancel the server will not take, its run over already or past
      // stopping, changes nothing, and the run's own outcome says how it
      // ended: nothing to tell beside that.
      await transport.cancel(now.run).catch(() => undefined)
    },
    said: computed(() => answered.value.at(-1))
  }
  // A task shown is let go once the server switched workspace, from this
  // tab or another: its lines name the workspace before with `-W`, which
  // is not where a yes would now run it, and it may name what the next one
  // has not. One under way is the server's to finish, queued before the
  // switch, and says how it ended where it ran.
  watch(() => live.switches, () => runner.cancel())

  provide(runnerKey, runner)
  return runner
}
