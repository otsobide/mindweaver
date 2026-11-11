import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import type { APIRequestContext, Page } from '@playwright/test'
import type { Snapshot } from '../src/api/generated/Snapshot'
import { runApart, succeed, type Serving } from './environment.ts'
import { GATHERED, repositoryBeside } from './fixture.ts'
import { drawn, expect, test, tree } from './panel.ts'

/*
 * Adding a source from the panel, held to the disk, to the CLI and to the
 * snapshot: a workspace of the test's own (`gatherable`), nothing gathered,
 * beside a repository holding one skill, gathered by its `file://` URL,
 * what a gather would take said first; and a gather waiting on a server,
 * stopped, leaving the shelf and the sources as they were. Its twin, a copy
 * made beforehand, is given the same line on the command line, and both
 * shelves hold the same files.
 */
test.use({ building: 'gatherable' })

/** The snapshot `served` answers a script with, the token as Bearer, as the API's guide asks it. */
async function snapshot(request: APIRequestContext, served: Serving): Promise<Snapshot> {
  const answer = await request.get(`${served.origin}/api/v1/snapshot`, { headers: { Authorization: `Bearer ${served.token}` } })
  expect(answer.status()).toBe(200)
  return await answer.json() as Snapshot
}

/** A server on loopback that holds each request it reads until told to answer, then answers 404, as the TUI's suite has one. */
interface Holding {
  /** Its address, where a repository would be. */
  url: string
  /** From now on, hold each request until `answer`; until then each is answered at once. */
  hold(): void
  /** Wait until a request is held: a gather is waiting on the answer. */
  asked(): Promise<void>
  /** Answer what is held, and anything asked from now on. */
  answer(): void
  close(): Promise<void>
}

async function holding(): Promise<Holding> {
  let held = false
  const waiting: (() => void)[] = []
  let told: (() => void) | undefined
  let read = 0
  const server: Server = createServer((request, response) => {
    const answer = () => response.writeHead(404, { 'Content-Length': '0', 'Connection': 'close' }).end()
    if (!held) {
      answer()
      return
    }
    read += 1
    told?.()
    waiting.push(answer)
  })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  return {
    url: `http://127.0.0.1:${port}/acme/skills.git`,
    hold: () => {
      held = true
    },
    asked: () => new Promise((resolve) => {
      if (read > 0) {
        resolve()
        return
      }
      told = resolve
    }),
    answer: () => {
      held = false
      waiting.splice(0).forEach(answer => answer())
    },
    close: () => new Promise<void>((resolve) => {
      server.closeAllConnections()
      server.close(() => resolve())
    })
  }
}

/** The source the page lists by `url`. */
function source(page: Page, url: string) {
  return page.getByTestId('source').filter({ has: page.getByTestId('url').getByText(url, { exact: true }) })
}

test('a repository on this machine, what a gather would take said first, is gathered from the panel, and its skill is on the shelf as the same line leaves it', async ({ page, own, request }) => {
  const url = pathToFileURL(repositoryBeside(own.at)).href
  const dialog = page.getByRole('dialog')
  await own.open('/sources')
  await expect(page.getByTestId('empty')).toBeVisible()

  await page.getByTestId('source-url').fill(url)
  await page.getByTestId('source-dry').click()
  await expect(dialog.getByTestId('command')).toHaveText([`$ mindweaver -W ${own.at.workspace} gather git --dry-run ${url}`])
  await expect(dialog.getByTestId('caution')).toHaveCount(0)
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()
  const said = page.getByTestId('source-said')
  await expect(said).toContainText(`would add  ${GATHERED.name}`)
  await expect(said).toContainText('nothing gathered: run it without --dry-run to gather')
  expect((await snapshot(request, own.served)).sources).toEqual([])

  await page.getByTestId('source-gather').click()
  await expect(dialog.getByTestId('command')).toHaveText([`$ mindweaver -W ${own.at.workspace} gather git ${url}`])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()
  await expect(page.getByTestId('summary-output')).toContainText(`added  ${GATHERED.name}  ${GATHERED.description}`)
  const gathered = source(page, url)
  await expect(gathered.getByTestId('shelved')).toHaveText([`skill:${GATHERED.name}`])
  await expect(gathered.getByTestId('licence-said')).toHaveText([GATHERED.licence])

  await page.getByTestId('sidebar').getByRole('link', { name: 'Shelf', exact: true }).click()
  await expect(page.getByTestId('shelf')).toBeVisible()
  await drawn(page)
  await expect(page.getByTestId('row').getByTestId('name')).toHaveText([GATHERED.name])
  await expect(page.getByTestId('row').getByTestId('origin')).toHaveText([url])

  succeed(own.twin, 'mindweaver', own.twin.workspace, ['gather', 'git', url])
  const shelf = (at: string) => tree(join(at, '.mindweaver', 'skills'))
  expect(shelf(own.at.workspace)).toEqual(shelf(own.twin.workspace))
})

test('a gather waiting on its server, stopped from the panel, leaves the shelf and the sources as they were, as the snapshot says', async ({ page, own, request }) => {
  const server = await holding()
  try {
    // The repository beside it gathered, and one on the server registered
    // by a gather the server turned away, as a source whose every gather
    // failed is: nothing of it on the shelf.
    succeed(own.at, 'mindweaver', own.at.workspace, ['gather', 'git', repositoryBeside(own.at)])
    // Apart, so this process's server answers it.
    const failed = await runApart(own.at, 'mindweaver', own.at.workspace, ['gather', 'git', server.url])
    expect(failed.status, failed.stderr).not.toBe(0)
    const before = await snapshot(request, own.served)
    expect(before.sources.map(each => each.url)).toContain(server.url)
    await own.open('/sources')

    server.hold()
    const name = before.sources.find(each => each.url === server.url)!.name
    const dialog = page.getByRole('dialog')
    await source(page, server.url).getByTestId('again').click()
    await expect(dialog.getByTestId('command')).toHaveText([`$ mindweaver -W ${own.at.workspace} gather update ${name}`])
    await dialog.getByTestId('run').click()
    await server.asked()
    await expect(dialog.getByTestId('stop')).toBeEnabled()
    await dialog.getByTestId('stop').click()
    await expect(dialog.getByTestId('stop-state')).toHaveText('Asked to stop: it stops before it changes anything, and says so when it is over.')
    server.answer()

    // What the run said opens by itself, the stop said by the gather's
    // code, and stays on the page once it is closed.
    const said = page.getByTestId('said')
    await expect(said.getByTestId('line').filter({ hasText: 'cancelled' })).toContainText('gather_cancelled')
    await page.keyboard.press('Escape')
    await expect(said).toBeHidden()
    await expect(page.getByTestId('summary-line').filter({ hasText: 'gather_cancelled' })).toHaveCount(1)
    const after = await snapshot(request, own.served)
    expect({ shelf: after.shelf, sources: after.sources }).toEqual({ shelf: before.shelf, sources: before.sources })
  } finally {
    await server.close()
  }
})
