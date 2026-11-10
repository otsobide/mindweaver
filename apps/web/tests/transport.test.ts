import { afterEach, describe, expect, it, vi } from 'vitest'
import { httpTransport } from '@/api/http'
import { TransportError, type Query, type Task } from '@/api/transport'

/** A request as `fetch` was handed it. */
interface Sent {
  url: string
  init: RequestInit
}

/** Stand in for `fetch`, answering every request with `reply`. */
function serve(reply: () => Response | Promise<Response>): Sent[] {
  const sent: Sent[] = []
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    sent.push({ url, init })
    return reply()
  })
  return sent
}

/** A task to send: one install of one skill into one project. */
const INSTALL: Task = { type: 'install', projects: ['app'], kind: 'skill', name: 'demo', from: null, yes: false }

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' }
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the HTTP transport', () => {
  it('asks for a read at its route under the version, relative, with the cookie and the header', async () => {
    const sent = serve(() => json(200, { projects: [] }))

    const answer = await httpTransport().query({ type: 'snapshot' })

    expect(answer).toEqual({ projects: [] })
    expect(sent).toHaveLength(1)
    const [{ url, init }] = sent as [Sent]
    expect(url).toBe('/api/v1/snapshot')
    expect(init.method).toBe('GET')
    expect(init.credentials).toBe('same-origin')
    expect(init.headers).toMatchObject({
      'Content-Type': 'application/json',
      'X-Mindweaver': '1'
    })
    expect(init.body).toBeUndefined()
  })

  it('names each read by its route, what it reads in its parameters, or in a body for a POST', async () => {
    const workspace = { type: 'workspace' } as const
    const api = { type: 'project', entry: 'apps/api' } as const
    const cell = { project: 'api', reference: 'skill:deploy', origin: 'workspace', wanted: true }
    const draft = { type: 'edit', reference: 'skill:deploy', text: '---\n' } as const
    const asked: [Query, string, string, unknown][] = [
      [{ type: 'snapshot' }, 'GET', '/api/v1/snapshot', undefined],
      [{ type: 'projects' }, 'GET', '/api/v1/projects', undefined],
      [{ type: 'library' }, 'GET', '/api/v1/library', undefined],
      [{ type: 'project', entry: 'apps/api' }, 'GET', '/api/v1/project?entry=apps%2Fapi', undefined],
      [{ type: 'files', holder: workspace, reference: 'skill:deploy' }, 'GET', '/api/v1/files?reference=skill%3Adeploy', undefined],
      [{ type: 'files', holder: api, reference: 'deploy' }, 'GET', '/api/v1/files?reference=deploy&project=apps%2Fapi', undefined],
      [
        { type: 'contents', holder: { type: 'elsewhere', entry: '../web' }, reference: 'rule:git/commits', file: 'commits.md' },
        'GET',
        '/api/v1/contents?reference=rule%3Agit%2Fcommits&file=commits.md&project=..%2Fweb',
        undefined
      ],
      [{ type: 'scan', roots: [], depth: null }, 'GET', '/api/v1/scan', undefined],
      [{ type: 'scan', roots: ['/home/you/My Work', '/srv'], depth: 2 }, 'GET', '/api/v1/scan?root=%2Fhome%2Fyou%2FMy+Work&root=%2Fsrv&depth=2', undefined],
      [{ type: 'folders', path: null, hidden: false }, 'GET', '/api/v1/folders?hidden=false', undefined],
      [{ type: 'folders', path: '/home/you', hidden: true }, 'GET', '/api/v1/folders?path=%2Fhome%2Fyou&hidden=true', undefined],
      [{ type: 'updates' }, 'GET', '/api/v1/updates', undefined],
      [{ type: 'workspaces' }, 'GET', '/api/v1/workspaces', undefined],
      [{ type: 'plan', selection: [cell] }, 'POST', '/api/v1/plan', { selection: [cell] }],
      [{ type: 'validate', holder: workspace, draft }, 'POST', '/api/v1/validate', { holder: workspace, draft }]
    ]
    const sent = serve(() => json(200, {}))

    for (const [query] of asked) {
      await httpTransport().query(query)
    }

    expect(sent.map(({ url, init }) => [url, init.method, init.body === undefined ? undefined : JSON.parse(init.body as string)]))
      .toEqual(asked.map(([, method, url, body]) => [url, method, body]))
  })

  it('previews a task without running it', async () => {
    const sent = serve(() => json(200, { lines: ['mindweaver install demo -p app'], confirm: [] }))

    const preview = await httpTransport().send(INSTALL, 'preview')

    expect(sent.map(request => request.url)).toEqual(['/api/v1/tasks/preview'])
    expect(preview).toEqual({ lines: ['mindweaver install demo -p app'], confirm: [] })
  })

  it('asks for the transcript with a GET, the cookie and no body', async () => {
    const ran = [{ lines: ['mindweaver link /work/blog'], outcome: { ok: true, runs: [] } }]
    const sent = serve(() => json(200, ran))

    const transcript = await httpTransport().transcript()

    expect(transcript).toEqual(ran)
    const [{ url, init }] = sent as [Sent]
    expect(url).toBe('/api/v1/transcript')
    expect(init.method).toBe('GET')
    expect(init.credentials).toBe('same-origin')
    expect(init.body).toBeUndefined()
  })

  it('keeps the code the backend refused a task with', async () => {
    serve(() => json(422, { error: 'the path is relative', code: 'path_relative' }))

    const refused = await httpTransport().send(INSTALL, 'preview').catch((error: unknown) => error)

    expect(refused).toBeInstanceOf(TransportError)
    expect(refused).toMatchObject({ status: 422, message: 'the path is relative', code: 'path_relative' })
  })

  it('runs a task and answers with its lines and outcome', async () => {
    const outcome = { ok: true, runs: [] }
    const sent = serve(() => json(200, { lines: ['mindweaver install demo -p app'], outcome }))

    const run = await httpTransport().send(INSTALL, 'run')

    expect(sent.map(request => request.url)).toEqual(['/api/v1/tasks'])
    expect(run).toEqual({ lines: ['mindweaver install demo -p app'], outcome })
  })

  it('asks the run under way to stop by its number, at the server\'s own route', async () => {
    const sent = serve(() => json(200, { run: 3, lines: ['mindweaver backup ../library.git'] }))

    const taken = await httpTransport().cancel(3)

    expect(taken).toEqual({ run: 3, lines: ['mindweaver backup ../library.git'] })
    const [{ url, init }] = sent as [Sent]
    expect(url).toBe('/api/v1/tasks/cancel')
    expect(init.method).toBe('POST')
    expect(JSON.parse(init.body as string)).toEqual({ run: 3 })
  })

  it('says what the server refused, from its error body, with the code beside it', async () => {
    serve(() => json(404, { error: 'no such endpoint: GET /api/v1/nothing', code: 'unknown_route' }))

    const refused = await httpTransport().query({ type: 'snapshot' }).catch((error: unknown) => error)

    expect(refused).toBeInstanceOf(TransportError)
    expect(refused).toMatchObject({ status: 404, message: 'no such endpoint: GET /api/v1/nothing', code: 'unknown_route' })
  })

  it('keeps the code of a read naming what is not there, a 404 as a refusal is', async () => {
    serve(() => json(404, { error: 'ghost: not registered in the workspace at /home/you/Projects', code: 'not_registered' }))

    const refused = await httpTransport().query({ type: 'project', entry: 'ghost' }).catch((error: unknown) => error)

    expect(refused).toBeInstanceOf(TransportError)
    expect(refused).toMatchObject({ status: 404, message: 'ghost: not registered in the workspace at /home/you/Projects', code: 'not_registered' })
  })

  it('says how to get the token when the server asks for it', async () => {
    serve(() => json(401, { error: 'this needs the launch token', code: 'token_required' }))

    const refused = await httpTransport().query({ type: 'snapshot' }).catch((error: unknown) => error) as TransportError

    expect(refused.unauthorized).toBe(true)
    expect(refused.message).toBe('this browser does not hold the launch token: open the link `mindweaver ui` printed')
    expect(refused.code).toBe('token_required')
  })

  it('names the status when a refusal is not JSON', async () => {
    serve(() => new Response('Bad Gateway', { status: 502, statusText: 'Bad Gateway' }))

    const refused = await httpTransport().query({ type: 'snapshot' }).catch((error: unknown) => error)

    expect(refused).toMatchObject({ status: 502, message: 'the server answered 502 Bad Gateway' })
  })

  it('reports a server it cannot reach as status 0', async () => {
    serve(() => Promise.reject(new TypeError('Failed to fetch')))

    const refused = await httpTransport().query({ type: 'snapshot' }).catch((error: unknown) => error)

    expect(refused).toBeInstanceOf(TransportError)
    expect(refused).toMatchObject({ status: 0 })
  })

  it('reports an answer cut off before its end as status 0', async () => {
    // What `fetch` resolves to when the headers arrived and the body never did.
    serve(() => ({
      ok: true,
      status: 200,
      statusText: 'OK',
      text: () => Promise.reject(new TypeError('terminated'))
    }) as unknown as Response)

    const refused = await httpTransport().query({ type: 'snapshot' }).catch((error: unknown) => error)

    expect(refused).toBeInstanceOf(TransportError)
    expect(refused).toMatchObject({
      status: 0,
      message: 'the server stopped before it finished answering: is `mindweaver ui` still running?'
    })
  })

  it('refuses a success that is not JSON rather than answer nothing', async () => {
    serve(() => new Response('<!doctype html><title>Mindweaver</title>', { status: 200 }))

    const refused = await httpTransport().send(INSTALL, 'run').catch((error: unknown) => error)

    expect(refused).toBeInstanceOf(TransportError)
    expect(refused).toMatchObject({ status: 200, message: 'the server answered 200 with something that is not JSON' })
  })

  it('listens to the event stream until told to stop', () => {
    const sources: FakeEventSource[] = []
    class FakeEventSource {
      onmessage: ((message: MessageEvent<string>) => void) | null = null
      closed = false
      constructor(readonly url: string) {
        sources.push(this)
      }

      close() {
        this.closed = true
      }
    }
    vi.stubGlobal('EventSource', FakeEventSource)
    const events: unknown[] = []

    const stop = httpTransport().subscribe(event => events.push(event))
    const [source] = sources as [FakeEventSource]
    const changed = { type: 'changed', scopes: [{ type: 'workspace' }] }
    source.onmessage?.(new MessageEvent('message', { data: JSON.stringify(changed) }))
    source.onmessage?.(new MessageEvent('message', { data: 'not json' }))
    source.onmessage?.(new MessageEvent('message', { data: '"resync"' }))
    stop()

    expect(source.url).toBe('/api/v1/events')
    expect(events).toEqual([changed])
    expect(source.closed).toBe(true)
  })
})
