import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { listen } from '@/api/live'
import { TransportError, type LiveEvent, type Query, type Transport } from '@/api/transport'
import { createQueryClient } from '@/query'
import { answering, fixture, render, steps, unmount } from './panel'

afterEach(() => {
  unmount()
  shown('visible')
})

/**
 * A transport answering the workspace fixture as `answering` does, which
 * hands the test what the panel subscribed with, so it can tell the panel
 * what the server's stream would.
 */
function streaming() {
  const { queries, transport } = answering(fixture('workspace'))
  const listeners: ((event: LiveEvent) => void)[] = []
  const subscriptions = { taken: 0, let: 0 }
  const streamed = {
    ...transport,
    subscribe(listener: (event: LiveEvent) => void) {
      subscriptions.taken += 1
      listeners.push(listener)
      return () => {
        subscriptions.let += 1
        listeners.splice(listeners.indexOf(listener), 1)
      }
    }
  } as Transport
  return {
    queries,
    subscriptions,
    transport: streamed,
    async tell(event: LiveEvent) {
      for (const listener of [...listeners]) {
        listener(event)
      }
      await flushPromises()
    }
  }
}

/** How many times `queries` holds one of `type`. */
function asked(queries: Query[], type: Query['type']): number {
  return queries.filter(query => query.type === type).length
}

/** Show or hide the tab, as the browser does when another is chosen. */
function shown(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true })
  document.dispatchEvent(new Event('visibilitychange'))
}

const workspace = { type: 'workspace' } as const

describe('live updates', () => {
  it('take one subscription for the tab, let go when the panel closes', async () => {
    const { subscriptions, transport } = streaming()

    const { router } = await render('/', transport)
    await router.push('/projects')
    await flushPromises()

    expect(subscriptions).toEqual({ taken: 1, let: 0 })
    unmount()
    expect(subscriptions).toEqual({ taken: 1, let: 1 })
  })

  it('ask for the snapshot again when the server says the workspace changed', async () => {
    const { queries, transport, tell } = streaming()
    await render('/', transport)
    const before = asked(queries, 'snapshot')

    await tell({ type: 'changed', scopes: [workspace] })

    expect(asked(queries, 'snapshot')).toBe(before + 1)
  })

  it('ask for nothing on moving between pages while the server watches, and on each as before when it does not', async () => {
    const { queries, transport, tell } = streaming()
    const { router } = await render('/', transport)
    await tell({ type: 'watching' })
    const before = asked(queries, 'snapshot')

    await router.push('/projects')
    await flushPromises()
    await router.push('/shelf')
    await flushPromises()
    expect(asked(queries, 'snapshot')).toBe(before)

    await tell({ type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' })
    await router.push('/sources')
    await flushPromises()
    expect(asked(queries, 'snapshot')).toBe(before + 1)
  })

  it('ask for everything again on a resync and once a command finished, and nothing when one starts', async () => {
    const { queries, transport, tell } = streaming()
    await render('/', transport)
    const before = asked(queries, 'snapshot')

    await tell({ type: 'command_started', run: 1, lines: ['mindweaver link beta'], cancellable: false })
    expect(asked(queries, 'snapshot')).toBe(before)
    await tell({ type: 'command_finished', run: 1, lines: ['mindweaver link beta'] })
    expect(asked(queries, 'snapshot')).toBe(before + 1)
    await tell({ type: 'resync', switches: 0 })
    expect(asked(queries, 'snapshot')).toBe(before + 2)
  })

  // A run under way says nothing of the disk until it finishes, however
  // long a gather takes, so none of what it tells as it goes asks again.
  it('ask for nothing again as a run goes, is asked to stop or keeps the server busy', async () => {
    const { queries, transport, tell } = streaming()
    await render('/', transport)
    const before = asked(queries, 'snapshot')
    const lines = ['mindweaver gather git https://example.com/skills']
    const step = { stage: 'connecting', done: null, total: null, said: 'connecting', cancellable: true } as const

    for (const event of [
      { type: 'command_started', run: 2, lines, cancellable: true },
      { type: 'progress', run: 2, progress: step },
      { type: 'busy', run: 2, lines, cancellable: true, cancelling: false },
      { type: 'cancelling', run: 2 }
    ] as LiveEvent[]) {
      await tell(event)

      expect(asked(queries, 'snapshot'), event.type).toBe(before)
    }
  })

  it('ask for what an artifact holds again only when a change reaches its holder', async () => {
    const { queries, transport, tell } = streaming()
    await render('/artifact?kind=skill&name=review', transport)
    await tell({ type: 'watching' })
    const files = asked(queries, 'files')
    const contents = asked(queries, 'contents')
    const snapshots = asked(queries, 'snapshot')
    expect(files).toBeGreaterThan(0)

    await tell({ type: 'changed', scopes: [{ type: 'project', entry: 'api' }] })
    expect(asked(queries, 'snapshot')).toBe(snapshots + 1)
    expect(asked(queries, 'files')).toBe(files)
    expect(asked(queries, 'contents')).toBe(contents)

    await tell({ type: 'changed', scopes: [workspace] })
    expect(asked(queries, 'files')).toBe(files + 1)
    expect(asked(queries, 'contents')).toBe(contents + 1)
  })

  it('mark stale the snapshot and what was read of an artifact, never a query of another kind', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { stop } = listen(transport, client)
    // A scan (#77) is asked for by hand, and reads far past what is watched.
    const scan = ['scan', { roots: ['/work'], depth: 2 }]
    const files = ['files', { holder: workspace, reference: 'skill:review' }]
    const stale = (key: unknown[]) => client.getQueryState(key)?.isInvalidated

    for (const event of [
      { type: 'changed', scopes: [workspace] },
      { type: 'command_finished', run: 1, lines: ['mindweaver link beta'] },
      { type: 'resync', switches: 0 }
    ] as LiveEvent[]) {
      for (const key of [['snapshot'], files, scan]) {
        client.setQueryData(key, {})
      }
      await tell(event)

      expect([stale(['snapshot']), stale(files), stale(scan)], event.type).toEqual([true, true, false])
    }
    stop()
  })

  it('mark the transcript stale once a run finishes and on a resync, never for a change on disk', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { stop } = listen(transport, client)
    const stale = () => client.getQueryState(['transcript'])?.isInvalidated

    for (const [event, expected] of [
      [{ type: 'changed', scopes: [workspace] }, false],
      [{ type: 'command_started', run: 1, lines: ['mindweaver link beta'], cancellable: false }, false],
      [{ type: 'command_finished', run: 1, lines: ['mindweaver link beta'] }, true],
      [{ type: 'resync', switches: 0 }, true]
    ] as [LiveEvent, boolean][]) {
      client.setQueryData(['transcript'], [])
      await tell(event)

      expect(stale(), event.type).toBe(expected)
    }
    stop()
  })

  it('let the stream go while the tab is hidden, and take it again once shown', async () => {
    const { subscriptions, transport } = streaming()
    await render('/', transport)

    shown('hidden')
    expect(subscriptions).toEqual({ taken: 1, let: 1 })
    shown('visible')
    expect(subscriptions).toEqual({ taken: 2, let: 1 })
  })
})

describe('a workspace the server cannot watch all of', () => {
  it('says so above every page, and asks for everything again on a refresh by hand', async () => {
    const { queries, transport, tell } = streaming()
    const { page, router } = await render('/', transport)
    expect(page.find('[data-testid="unwatched"]').exists()).toBe(false)

    await tell({ type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' })

    const banner = page.get('[data-testid="unwatched"]')
    expect(banner.text()).toContain('the system\'s limit on watched folders is reached: refresh to see them.')
    const before = asked(queries, 'snapshot')
    await banner.get('button').trigger('click')
    await flushPromises()
    expect(asked(queries, 'snapshot')).toBe(before + 1)

    await router.push('/projects')
    await flushPromises()
    expect(page.find('[data-testid="unwatched"]').exists()).toBe(true)
  })

  it('stops saying so once the server watches everything again', async () => {
    const { transport, tell } = streaming()
    const { page } = await render('/', transport)
    await tell({ type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' })

    await tell({ type: 'watching' })

    expect(page.find('[data-testid="unwatched"]').exists()).toBe(false)
  })
})

/** What a run of `mindweaver link /work/blog` is answered with. */
const RAN = { lines: ['mindweaver link /work/blog'], outcome: { ok: true, runs: [] } }

describe('a run the panel sends', () => {
  it('is over once the stream tells it finished, a finish of other lines aside', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { settle, stop } = listen(transport, client, document, 60_000)
    client.setQueryData(['snapshot'], {})
    let over = false

    const settled = settle(() => Promise.resolve(RAN)).then((answer) => {
      over = true
      return answer
    })
    await flushPromises()
    expect(over).toBe(false)
    await tell({ type: 'command_finished', run: 1, lines: ['mindweaver link /work/other'] })
    expect(over).toBe(false)
    expect(client.getQueryState(['snapshot'])?.isInvalidated).toBe(true)
    client.setQueryData(['snapshot'], {})
    await tell({ type: 'command_finished', run: 1, lines: RAN.lines })

    expect(await settled).toBe(RAN)
    expect(over).toBe(true)
    expect(client.getQueryState(['snapshot'])?.isInvalidated).toBe(true)
    stop()
  })

  it('is over at once when the stream told it finished before the answer came, as the server tells it', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { settle, stop } = listen(transport, client, document, 60_000)

    const settled = settle(async () => {
      await tell({ type: 'command_finished', run: 1, lines: RAN.lines })
      return RAN
    })

    expect(await settled).toBe(RAN)
    stop()
  })

  it('asks for what it changed itself when the stream tells nothing in time', async () => {
    const { transport } = streaming()
    const client = createQueryClient()
    const { settle, stop } = listen(transport, client, document, 10)
    client.setQueryData(['snapshot'], {})
    client.setQueryData(['transcript'], [])

    expect(await settle(() => Promise.resolve(RAN))).toBe(RAN)

    expect(client.getQueryState(['snapshot'])?.isInvalidated).toBe(true)
    expect(client.getQueryState(['transcript'])?.isInvalidated).toBe(true)
    stop()
  })

  it('refused, marks nothing stale and says why', async () => {
    const { transport } = streaming()
    const client = createQueryClient()
    const { settle, stop } = listen(transport, client, document, 10)
    client.setQueryData(['snapshot'], {})
    const refusal = new TransportError(422, 'the path is relative', 'path_relative')

    await expect(settle(() => Promise.reject(refusal))).rejects.toBe(refusal)

    await new Promise(resolve => setTimeout(resolve, 20))
    expect(client.getQueryState(['snapshot'])?.isInvalidated).toBe(false)
    stop()
  })
})

describe('a switch the panel sends', () => {
  const SWITCHED = { path: '/home/you/kit' }

  it('lets go of every answer once the stream says it was taken, and counts it', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { live, settleSwitch, stop } = listen(transport, client, document, 60_000)
    client.setQueryData(['snapshot'], { workspace: 'before' })
    client.setQueryData(['scan', {}], { found: [] })
    let over = false

    const settled = settleSwitch(() => Promise.resolve(SWITCHED)).then((answer) => {
      over = true
      return answer
    })
    await flushPromises()
    expect(over).toBe(false)
    expect(live.switches).toBe(0)
    await tell({ type: 'switched', switches: 1 })

    expect(await settled).toBe(SWITCHED)
    expect(live.switches).toBe(1)
    expect(client.getQueryData(['snapshot'])).toBeUndefined()
    expect(client.getQueryData(['scan', {}])).toBeUndefined()
    stop()
  })

  it('lets go once, when the stream said it was taken before the answer came', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { live, settleSwitch, stop } = listen(transport, client, document, 60_000)

    const settled = settleSwitch(async () => {
      await tell({ type: 'switched', switches: 1 })
      return SWITCHED
    })

    expect(await settled).toBe(SWITCHED)
    expect(live.switches).toBe(1)
    stop()
  })

  it('lets go itself when the stream says nothing in time', async () => {
    const { transport } = streaming()
    const client = createQueryClient()
    const { live, settleSwitch, stop } = listen(transport, client, document, 10)
    client.setQueryData(['snapshot'], { workspace: 'before' })

    expect(await settleSwitch(() => Promise.resolve(SWITCHED))).toBe(SWITCHED)

    expect(live.switches).toBe(1)
    expect(client.getQueryData(['snapshot'])).toBeUndefined()
    stop()
  })

  it('refused, lets go of nothing', async () => {
    const { transport } = streaming()
    const client = createQueryClient()
    const { live, settleSwitch, stop } = listen(transport, client, document, 10)
    client.setQueryData(['snapshot'], { workspace: 'before' })
    const refusal = new TransportError(422, 'none of the workspaces this machine knows', 'switch_unlisted')

    await expect(settleSwitch(() => Promise.reject(refusal))).rejects.toBe(refusal)

    await new Promise(resolve => setTimeout(resolve, 20))
    expect(live.switches).toBe(0)
    expect(client.getQueryData(['snapshot'])).toEqual({ workspace: 'before' })
    stop()
  })
})

describe('a switch the stream did not tell', () => {
  it('is let go of once the tab is shown again, by the count its resync carries', async () => {
    const { subscriptions, transport, tell } = streaming()
    const client = createQueryClient()
    const { live, stop } = listen(transport, client)
    await tell({ type: 'resync', switches: 2 })
    client.setQueryData(['snapshot'], { workspace: 'before' })

    shown('hidden')
    // Another tab switches, told on every stream open: none of this tab's.
    await tell({ type: 'switched', switches: 3 })
    expect(live.switches).toBe(0)
    shown('visible')
    await tell({ type: 'resync', switches: 3 })

    expect(subscriptions).toEqual({ taken: 2, let: 1 })
    expect(live.switches).toBe(1)
    expect(client.getQueryData(['snapshot'])).toBeUndefined()
    stop()
  })

  it('is let go of once, when the stream still held it behind the resync that counted it', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { live, stop } = listen(transport, client)
    await tell({ type: 'resync', switches: 0 })

    // Fallen behind past a switch, then told what the stream still held.
    await tell({ type: 'resync', switches: 1 })
    await tell({ type: 'switched', switches: 1 })
    expect(live.switches).toBe(1)
    await tell({ type: 'switched', switches: 2 })

    expect(live.switches).toBe(2)
    stop()
  })

  it('is no switch when the resync counts what the tab heard: only marked stale', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { live, stop } = listen(transport, client)
    await tell({ type: 'resync', switches: 1 })
    client.setQueryData(['snapshot'], { workspace: 'same' })

    await tell({ type: 'resync', switches: 1 })

    expect(live.switches).toBe(0)
    expect(client.getQueryData(['snapshot'])).toEqual({ workspace: 'same' })
    expect(client.getQueryState(['snapshot'])?.isInvalidated).toBe(true)
    stop()
  })
})

describe('the run under way', () => {
  const lines = ['mindweaver backup https://github.com/you/library-backup.git']
  const [checking, exporting] = steps()

  it('is kept as the stream tells it: started, each step, asked to stop, and gone once it finishes', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { live, stop } = listen(transport, client)

    await tell({ type: 'command_started', run: 3, lines, cancellable: true })
    expect(live.underway).toEqual({ run: 3, lines, cancellable: true, cancelling: false })
    await tell({ type: 'progress', run: 3, progress: checking! })
    await tell({ type: 'progress', run: 3, progress: exporting! })
    expect(live.underway?.step).toEqual(exporting)
    // Another run's word is not this one's.
    await tell({ type: 'progress', run: 2, progress: checking! })
    await tell({ type: 'cancelling', run: 2 })
    expect(live.underway).toEqual({ run: 3, lines, cancellable: true, cancelling: false, step: exporting })
    await tell({ type: 'cancelling', run: 3 })
    expect(live.underway?.cancelling).toBe(true)
    await tell({ type: 'command_finished', run: 3, lines })

    expect(live.underway).toBeUndefined()
    stop()
  })

  it('is told again as busy on a stream opened while it runs, its step after it, and forgotten on a resync until then', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { live, stop } = listen(transport, client)
    await tell({ type: 'command_started', run: 4, lines, cancellable: true })

    await tell({ type: 'resync', switches: 0 })
    expect(live.underway).toBeUndefined()
    await tell({ type: 'busy', run: 4, lines, cancellable: true, cancelling: true })
    await tell({ type: 'progress', run: 4, progress: exporting! })
    // A busy of the same run, as one queued behind it tells, keeps its step.
    await tell({ type: 'busy', run: 4, lines, cancellable: true, cancelling: true })

    expect(live.underway).toEqual({ run: 4, lines, cancellable: true, cancelling: true, step: exporting })
    stop()
  })
})
