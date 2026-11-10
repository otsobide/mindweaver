/*
 * The one file allowed to use `fetch` and `EventSource` (see eslint.config.ts).
 */
import type { Holder } from './generated/Holder'
import { TransportError, type Answer, type Cancelling, type LiveEvent, type Preview, type Query, type Ran, type Switch, type Switchable, type Task, type Transport } from './transport'

/**
 * Sent with every request.
 *
 * `X-Mindweaver` is a header no form and no plain link can add, so a request
 * carrying it was made by a script on a page the browser let set it: the
 * server's own, since it answers no CORS preflight. JSON, because that is
 * all the server reads, and it too forces the preflight on any other origin.
 */
const HEADERS = {
  'Accept': 'application/json',
  'Content-Type': 'application/json',
  'X-Mindweaver': '1'
}

/**
 * Where the server serves its API, the version in the path: the panel is
 * its first client, shipped in the same binary, and asks for the version
 * that binary serves.
 */
const API = '/api/v1'

/**
 * The transport for a panel the local server serves, over HTTP: the
 * server's reads, each at its own route, its tasks, previewed and run, its
 * switch, its transcript and its event stream, all under `/api/v1`.
 *
 * Relative URLs, so the panel talks to whichever server sent it, on
 * whatever port it took, or to the Vite dev server that proxies `/api` to
 * one. The launch token travels in the cookie the launch link set, which is
 * `HttpOnly`: the panel never holds the token, and only the browser sends it.
 */
export function httpTransport(): Transport {
  return {
    query<Asked extends Query>(query: Asked) {
      const { method, path, body } = route(query)
      return ask<Answer<Asked>>(method, path, body)
    },

    send(task: Task, mode: 'preview' | 'run') {
      return mode === 'preview'
        ? post<Preview>(`${API}/tasks/preview`, task)
        : post<Ran>(`${API}/tasks`, task)
    },

    cancel(run: number) {
      return post<Cancelling>(`${API}/tasks/cancel`, { run })
    },

    switch(to: Switch) {
      return post<Switchable>(`${API}/switch`, to)
    },

    transcript() {
      return ask<Ran[]>('GET', `${API}/transcript`)
    },

    subscribe(listener) {
      // Same origin, so the cookie goes with it; a GET changes nothing, so
      // it needs neither the header nor the JSON body EventSource cannot send.
      // One source, which the browser connects again by itself when the
      // connection drops, and the server then starts again with `resync`.
      const source = new EventSource(`${API}/events`)
      source.onmessage = (message: MessageEvent<string>) => {
        const event = parse(message.data)
        if (typeof event === 'object' && event !== null && 'type' in event) {
          listener(event as LiveEvent)
        }
      }
      return () => source.close()
    }
  } as Transport
}

/** How a read is asked for: its method, its path and query, and a body. */
interface Routed {
  method: 'GET' | 'POST'
  path: string
  body?: unknown
}

/**
 * Where the server serves `query`, and how it is asked for there: a GET
 * naming what it reads in its parameters, or, for a read holding more than
 * a parameter could, a POST whose body holds the query's own fields.
 *
 * The routes are the backend's, each the query it becomes (the CLI's
 * `routes.rs`): this only spells each query as its route takes it, a
 * holder as the `project` its entry names, or none for the workspace's
 * own. One case for every query, so one Rust adds is a function with no
 * answer for it, which the panel does not compile until it is spelled.
 */
function route(query: Query): Routed {
  switch (query.type) {
    case 'snapshot':
    case 'projects':
    case 'library':
    case 'updates':
    case 'workspaces':
      return read(query.type)
    case 'project':
      return read('project', { entry: query.entry })
    case 'files':
      return read('files', { reference: query.reference, project: entry(query.holder) })
    case 'contents':
      return read('contents', { reference: query.reference, file: query.file, project: entry(query.holder) })
    case 'scan':
      return read('scan', { root: query.roots, depth: query.depth })
    case 'folders':
      return read('folders', { path: query.path, hidden: query.hidden })
    case 'plan':
      return { method: 'POST', path: `${API}/plan`, body: { selection: query.selection } }
    case 'validate':
      return { method: 'POST', path: `${API}/validate`, body: { holder: query.holder, draft: query.draft } }
  }
}

/** A value a parameter is sent, a list once for each of its values, or none. */
type Parameter = string | number | boolean | string[] | null | undefined

/**
 * A GET of the read at `name`, its parameters in its query string, encoded
 * as a form is, which the server decodes: a list as the parameter once for
 * each value, and one with no value left out, as the route reads it.
 */
function read(name: string, parameters: Record<string, Parameter> = {}): Routed {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(parameters)) {
    for (const one of value === null || value === undefined ? [] : [value].flat()) {
      search.append(key, String(one))
    }
  }
  const query = search.toString()
  return { method: 'GET', path: query ? `${API}/${name}?${query}` : `${API}/${name}` }
}

/** The entry of the project a holder names, or none for the workspace's own. */
function entry(holder: Holder): string | undefined {
  return holder.type === 'workspace' ? undefined : holder.entry
}

function post<Reply>(path: string, body: unknown): Promise<Reply> {
  return ask<Reply>('POST', path, body)
}

/**
 * A request to the server, and its answer as JSON; a GET carries no body.
 * Every one with the same headers: a read needs neither `X-Mindweaver` nor
 * the JSON type, but one way to ask leaves no request without them.
 */
async function ask<Reply>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<Reply> {
  let response: Response
  try {
    response = await fetch(path, {
      method,
      // The default, spelled out: the cookie is the panel's credential, and
      // it must never be offered to another origin.
      credentials: 'same-origin',
      headers: HEADERS,
      body: body === undefined ? undefined : JSON.stringify(body)
    })
  } catch {
    throw new TransportError(0, 'the server cannot be reached: is `mindweaver ui` still running?')
  }
  // The status came first, so the body can still be lost to a server that
  // stops halfway through it: that is no answer either.
  let text: string
  try {
    text = await response.text()
  } catch {
    throw new TransportError(0, 'the server stopped before it finished answering: is `mindweaver ui` still running?')
  }
  const reply = parse(text)
  if (!response.ok) {
    throw new TransportError(response.status, refusal(response, reply), code(reply))
  }
  // Every answer the server gives is JSON, so one that is not came from
  // something else on the way, and passing it on as `undefined` would read
  // as an empty answer rather than a broken one.
  if (reply === undefined) {
    throw new TransportError(response.status, `the server answered ${response.status} with something that is not JSON`)
  }
  return reply as Reply
}

/**
 * What the server said was wrong, which it says as `{"error": "...",
 * "code": "..."}`, the code read apart (`code`).
 */
function refusal(response: Response, reply: unknown): string {
  if (response.status === 401) {
    return 'this browser does not hold the launch token: open the link `mindweaver ui` printed'
  }
  if (typeof reply === 'object' && reply !== null && 'error' in reply && typeof reply.error === 'string') {
    return reply.error
  }
  return `the server answered ${response.status} ${response.statusText}`.trimEnd()
}

/** The code a refusal says beside its words, when it says one. */
function code(reply: unknown): string | undefined {
  if (typeof reply === 'object' && reply !== null && 'code' in reply && typeof reply.code === 'string') {
    return reply.code
  }
  return undefined
}

/** JSON, or nothing for a body that is not. */
function parse(text: string): unknown {
  try {
    return JSON.parse(text) as unknown
  } catch {
    return undefined
  }
}
