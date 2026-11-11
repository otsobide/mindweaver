/*
 * The catalogs the panel's page of them searches, on loopback: one server
 * a spec starts on 127.0.0.1, on a port the system picks, answering as
 * skills.sh answers a search (`/api/search`) and as the MCP Registry
 * answers its read API (`/v0.1`), which `mindweaver ui --finder-url
 * skills.sh=` and `--registry` point the panel's lines at. Nothing a spec
 * runs reaches the network, a real skill directory or the real registry.
 *
 * skills.sh lists one skill, in a repository on this disk, which a finder
 * on this machine may name; the registry serves what the real one sent for
 * `ai.clawfetch/mcp` 0.4.0 on 2026-11-10, as #106 recorded it into core's
 * fixtures, to a search whose name it holds and to a request of that
 * version or the latest, and a 404 as the registry's own to anything else.
 */
import { readFileSync } from 'node:fs'
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http'
import type { AddressInfo } from 'node:net'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

/** Where core's suite keeps the registry's recorded answers. */
const RECORDED = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'core', 'tests', 'fixtures', 'mcp-registry')

/** The server the registry is asked for, by the name it gives it, and its recorded version. */
export const SERVER = { name: 'ai.clawfetch/mcp', version: '0.4.0', definition: 'clawfetch-mcp' } as const

/** The skill skills.sh lists, by its name. */
export const SKILL = 'commit-style' as const

/** The catalogs on loopback, as a spec reads them. */
export interface Catalogs {
  /** Where they listen: `http://127.0.0.1:<port>`. */
  origin: string
  /** What `mindweaver ui` is launched with to ask them: `--registry` and `--finder-url`. */
  options: string[]
  /** Each request they were sent, by its path, in order. */
  asked(): string[]
  /** Stop listening. */
  stop(): Promise<void>
}

/** A recorded answer of the registry's, as it was recorded. */
function recorded(file: string): string {
  return readFileSync(join(RECORDED, file), 'utf8').trimEnd()
}

/** Answer `response` with `status` and `body`, JSON. */
function answer(response: ServerResponse, status: number, body: string): void {
  response.writeHead(status, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body) })
  response.end(body)
}

/**
 * The catalogs on loopback, skills.sh naming `repository`, a folder on
 * this disk, as the source of the one skill it lists.
 */
export function catalogs(repository: string): Promise<Catalogs> {
  const asked: string[] = []
  const server = createServer((request: IncomingMessage, response: ServerResponse) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1')
    asked.push(url.pathname)
    if (url.pathname === '/api/search') {
      const skill = { id: `${repository}/${SKILL}`, source: repository, skillId: SKILL, name: SKILL, installs: 56 }
      return answer(response, 200, JSON.stringify({ searchType: 'fuzzy', skills: [skill], count: 1 }))
    }
    if (url.pathname === '/v0.1/servers') {
      const search = (url.searchParams.get('search') ?? '').toLowerCase()
      const servers = SERVER.name.includes(search) ? [recorded('clawfetch-0.4.0.json')] : []
      return answer(response, 200, `{"servers":[${servers.join(',')}],"metadata":{"count":${servers.length}}}`)
    }
    const encoded = encodeURIComponent(SERVER.name)
    const versions = [SERVER.version, 'latest'].map(version => `/v0.1/servers/${encoded}/versions/${version}`)
    if (versions.includes(request.url?.split('?')[0] ?? '')) {
      return answer(response, 200, recorded('clawfetch-0.4.0.json'))
    }
    return answer(response, 404, recorded('loice-telegram-1.0.1.json'))
  })
  return new Promise((resolve, refuse) => {
    server.once('error', refuse)
    server.listen(0, '127.0.0.1', () => {
      const origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`
      resolve({
        origin,
        options: ['--registry', origin, '--finder-url', `skills.sh=${origin}`],
        asked: () => [...asked],
        stop: () => new Promise(done => server.close(() => done()))
      })
    })
  })
}
