import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, type VueWrapper } from '@vue/test-utils'
import type { Envelope } from '@/api/generated/Envelope'
import type { ServerListing } from '@/api/generated/ServerListing'
import type { SkillsFound } from '@/api/generated/SkillsFound'
import type { Task } from '@/api/transport'
import { commanding, fixture, render, searched, texts, unmount, type Commanded } from './panel'

afterEach(() => {
  unmount()
  vi.restoreAllMocks()
})

/** The change of tests/fixtures/searched.json whose task is of `type`, told yes or not where it says. */
function change(type: Task['type'], yes?: boolean): Commanded {
  const found = searched().find(each => each.task.type === type && (yes === undefined || (each.task as { yes?: boolean }).yes === yes))
  if (!found) {
    throw new Error(`no fixture of ${type}`)
  }
  return found
}

/** What the one line of `change` printed, read back as the envelope `--json` prints. */
function printed<T>(change: Commanded): Envelope<T> {
  return JSON.parse(change.ran.outcome.runs[0]!.stdout) as Envelope<T>
}

/** The lines the dialog shows, as the preview gave them. */
function shown(): (string | null)[] {
  return [...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)
}

/** Say yes to what the dialog shows, and wait for the run and the views it changed. */
async function run() {
  document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
  await flushPromises()
  await vi.waitFor(() => expect(document.body.querySelector('[data-testid="preview"]')).toBeNull())
  await flushPromises()
}

/** Type `query` into the search `of` names and send it, then say yes to its line. */
async function search(page: VueWrapper, of: 'skills' | 'servers', query: string) {
  await page.get(`input[data-testid="${of}-query"]`).setValue(query)
  await page.get(`[data-testid="${of}-form"]`).trigger('submit')
  await flushPromises()
  await run()
}

describe('the page of catalogs', () => {
  it('lists what the skill directories found for a search, as core listed it, read back from what its line printed', async () => {
    const skills = change('search_skills')
    const { sent, transport } = commanding(fixture('workspace'), [skills])
    const { page } = await render('/catalogs', transport)

    await page.get('input[data-testid="skills-query"]').setValue('  commit\n')
    await page.get('[data-testid="skills-form"]').trigger('submit')
    await flushPromises()
    // The line the server previewed, the address it asks among it, never
    // one the panel put together; then the run, once said yes to.
    expect(shown()).toEqual(skills.preview.lines.map(line => `$ ${line}`))
    await run()

    expect(sent.map(each => each.mode)).toEqual(['preview', 'run'])
    const found = printed<SkillsFound>(skills).data
    expect(texts(page, '[data-testid="hit-name"]')).toEqual(found.hits.map(hit => hit.name))
    expect(texts(page, '[data-testid="hit-repository"]')).toEqual(found.hits.map(hit => hit.repository))
    expect(texts(page, '[data-testid="hit-path"]')).toEqual(['none named: the whole repository is gathered', 'none named: the whole repository is gathered'])
    expect(texts(page, '[data-testid="hit-reference"]')).toEqual(['the repository\'s own', 'the repository\'s own'])
    expect(texts(page, '[data-testid="hit-installs"]')).toEqual(found.hits.map(hit => String(hit.installs)))
    expect(texts(page, '[data-testid="hit-finder"]')).toEqual(['skills.sh', 'skills.sh'])
  })

  it('takes a result by sending it back whole as the search printed it, and never an address to fetch', async () => {
    const fetched = vi.spyOn(globalThis, 'fetch')
    const skills = change('search_skills')
    const take = change('take_skill')
    const { queries, sent, transport } = commanding(fixture('workspace'), [skills, take])
    const { page } = await render('/catalogs', transport)

    await search(page, 'skills', 'commit')
    await page.findAll('[data-testid="take"]')[0]!.trigger('click')
    await flushPromises()
    expect(shown()).toEqual(take.preview.lines.map(line => `$ ${line}`))
    await run()

    // The only requests: the snapshot, and each task previewed, then run.
    expect(fetched).not.toHaveBeenCalled()
    expect(new Set(queries.map(query => query.type))).toEqual(new Set(['snapshot']))
    expect(sent).toEqual([
      { task: { type: 'search_skills', query: 'commit' }, mode: 'preview' },
      { task: { type: 'search_skills', query: 'commit' }, mode: 'run' },
      { task: take.task, mode: 'preview' },
      { task: take.task, mode: 'run' }
    ])
    // The result taken is the one listed, as its line printed it: a field
    // more or less would be the panel's own saying of where it is.
    const [hit] = printed<SkillsFound>(skills).data.hits
    expect(sent[2]!.task).toEqual({ type: 'take_skill', hit })
    // No task names where the server asks a directory or a registry, nor
    // a token: those are the launch's, and the line's to say.
    const said = JSON.stringify(sent)
    for (const address of skills.preview.lines[0]!.split(' ').filter(word => word.includes('://'))) {
      expect(said).not.toContain(address.replace(/^[^=]*=/, ''))
    }
    expect(said).not.toMatch(/registry|finder_url|token/)
    expect(page.get('[data-testid="taken"]').text()).toContain('see it on the shelf')
    // Nothing on the page loads or links to another site: a repository is
    // shown as text, never as something a browser would follow.
    for (const element of page.element.querySelectorAll('[href], [src]')) {
      const address = element.getAttribute('href') ?? element.getAttribute('src')!
      expect(address, element.outerHTML).toMatch(/^\/(?!\/)|^#/)
    }
  })

  it('says why a search ran nothing, in its words and by its code, and lists nothing', async () => {
    const skills = change('search_skills')
    skills.task = { type: 'search_skills', query: 'pdf' }
    skills.preview.lines = skills.preview.lines.map(line => line.replace(/commit$/, 'pdf'))
    skills.ran.lines = skills.preview.lines
    const message = 'nothing was searched: github needs a token, handed over with --token-env or --gh, and no finder that needs none is enabled'
    skills.ran.outcome = {
      ok: false,
      runs: [{ ...skills.ran.outcome.runs[0]!, stdout: '', error: { code: 'finder_none_asked', message }, ok: false }]
    }
    const { page } = await render('/catalogs', commanding(fixture('workspace'), [skills]).transport)

    await search(page, 'skills', 'pdf')

    expect(page.get('[data-testid="skills-refused"]').text()).toBe(`Nothing was searched: ${message} finder_none_asked`)
    expect(page.find('[data-testid="hit"]').exists()).toBe(false)
  })

  it('says what a search said beside what it found, each line by its code, a hint by none', async () => {
    const skills = change('search_skills')
    const hint = 'hint: github was not asked, since its search needs a token: --token-env or --gh hands one over'
    const dropped = 'warning: skills.sh: result 3 (`odd`): it names no repository, so it is not listed'
    skills.ran.outcome.runs[0]!.stderr = [{ text: hint, code: null }, { text: dropped, code: 'finder_result_dropped' }]
    const { page } = await render('/catalogs', commanding(fixture('workspace'), [skills]).transport)

    await search(page, 'skills', 'commit')

    expect(texts(page, '[data-testid="skills-said"] [data-testid="said-line"]')).toEqual([hint, `${dropped} finder_result_dropped`])
    expect(page.findAll('[data-testid="hit"]')).toHaveLength(2)
  })

  it('says how a search ended without what its line printed, which the page lists itself', async () => {
    const skills = change('search_skills')
    const { page } = await render('/catalogs', commanding(fixture('workspace'), [skills]).transport)

    await search(page, 'skills', 'commit')

    await vi.waitFor(() => expect(document.body.textContent).toContain(`Ran ${skills.ran.lines[0]}`))
    expect(document.body.textContent).not.toContain('"version": 2')
  })

  it('lists what the registry lists for a search, each server by the words core gave it', async () => {
    const servers = change('search_servers')
    const { page } = await render('/catalogs', commanding(fixture('workspace'), [servers]).transport)

    await search(page, 'servers', 'mcp')

    const listed = printed<ServerListing>(servers).data.servers
    expect(texts(page, '[data-testid="server-name"]')).toEqual(listed.map(server => server.name))
    expect(texts(page, '[data-testid="server-version"]')).toEqual(listed.map(server => server.version))
    // A status by its word, never by a colour alone.
    expect(texts(page, '[data-testid="server-status"]')).toEqual(listed.map(server => server.status))
    expect(texts(page, '[data-testid="server-description"]')).toEqual(listed.map(server => server.description))
    expect(texts(page, '[data-testid="server-way"]')).toEqual(listed.flatMap(server => server.ways))
  })

  it('adds a server only once its dry run was read, by the same task told yes', async () => {
    const fetched = vi.spyOn(globalThis, 'fetch')
    const servers = change('search_servers')
    const dry = change('import_server', false)
    const yes = change('import_server', true)
    const { sent, transport } = commanding(fixture('workspace'), [servers, dry, yes])
    const { page } = await render('/catalogs', transport)

    await search(page, 'servers', 'mcp')
    expect(page.find('[data-testid="add-yes"]').exists()).toBe(false)
    await page.findAll('[data-testid="add-dry"]')[0]!.trigger('click')
    await flushPromises()
    expect(shown()).toEqual(dry.preview.lines.map(line => `$ ${line}`))
    await run()

    // What the add would write, as its line said it: the program an agent
    // would run, the variable it reads and where it goes.
    expect(page.get('[data-testid="add-said"]').text()).toBe(dry.ran.outcome.runs[0]!.stdout.trim())
    expect(page.get('[data-testid="add-said"]').text()).toContain('runs: npx -y @clawfetch/mcp@0.4.0')
    await page.get('[data-testid="add-yes"]').trigger('click')
    await flushPromises()
    expect(shown()).toEqual(yes.preview.lines.map(line => `$ ${line}`))
    await run()

    expect(sent.map(each => [each.task, each.mode])).toEqual([
      [servers.task, 'preview'],
      [servers.task, 'run'],
      [dry.task, 'preview'],
      [dry.task, 'run'],
      [yes.task, 'preview'],
      [yes.task, 'run']
    ])
    // The yes is the dry run's own task, told yes, and nothing else.
    expect({ ...yes.task, yes: false }).toEqual(dry.task)
    expect(page.find('[data-testid="add-said"]').exists()).toBe(false)
    expect(page.get('[data-testid="added"]').text()).toContain('see it among the MCP servers')
    expect(fetched).not.toHaveBeenCalled()
  })

  it('is in the sidebar, which lights it', async () => {
    const { page } = await render('/catalogs', fixture('workspace'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Catalogs')
  })
})
