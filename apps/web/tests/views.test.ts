import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import type { Folders } from '@/api/generated/Folders'
import type { Health } from '@/api/generated/Health'
import { answering, answers, applying, commanded, commanding, drafts, fixture, plans, render, searched, servers, unmount, updates, walks, type Answered } from './panel'

/** Every `<style>` element anything adds while a test runs, which the CSP would refuse. */
let styles: Element[]
let watching: MutationObserver

beforeEach(() => {
  styles = []
  watching = new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node instanceof Element && node.tagName === 'STYLE') {
          styles.push(node)
        }
      }
    }
  })
  watching.observe(document, { childList: true, subtree: true })
})

afterEach(() => {
  watching.disconnect()
  unmount()
})

/**
 * Whether every icon on the page was drawn from the bundle: Iconify draws an
 * icon it holds as an `<svg>`, and nothing at all for one it would have had
 * to fetch, which the CSP refuses.
 */
function drawsEveryIcon(page: VueWrapper) {
  const icons = page.findAllComponents({ name: 'Icon' })
  expect(icons.length).toBeGreaterThan(0)
  for (const icon of icons) {
    expect(icon.element).toBeInstanceOf(SVGElement)
    expect((icon.element as SVGElement).innerHTML).not.toBe('')
  }
}

/**
 * The workspace fixture with `api` holding one artifact in every state the
 * snapshot names, and the projects one in every health, so every badge's
 * icon is drawn.
 */
function everyLook() {
  const snapshot = fixture('workspace')
  const api = snapshot.projects[0]!
  const [template] = api.artifacts
  api.artifacts = [...snapshot.states.map(state => ({ ...template!, name: `in-${state}`, state })), { ...template!, name: 'none', state: null }]
  snapshot.projects.forEach((project, index) => {
    project.health = snapshot.healths[index % snapshot.healths.length] as Health
  })
  return snapshot
}

describe('the views', () => {
  it.each(['/', '/projects', '/projects?sort=name', '/projects?sort=name&order=desc', '/project?entry=api', '/project?entry=docs', '/project?entry=gone', '/library', '/library?kind=rule', '/artifact?kind=skill&name=review&file=SKILL.md', '/artifact?kind=rule&name=style/comments', '/shelf', '/shelf?kind=skill', '/sources', '/catalogs', '/discover', '/discover?root=%2Fhome%2Fyou%2FProjects', '/assign', '/assign?by=project'])(
    '%s draws every icon from the bundle and adds no style element',
    async (path) => {
      const { page } = await render(path, everyLook())

      drawsEveryIcon(page)
      expect(styles).toEqual([])
    }
  )
})

describe('the matrix, with a plan of what was marked', () => {
  it('draws every icon from the bundle, every lock and clash among them, and adds no style element', async () => {
    const { transport } = answering(fixture('assigning'), plans())
    const { page } = await render('/assign', transport)

    for (const cell of (plans()[2]!.query as { selection: { project: string, reference: string, origin: string }[] }).selection) {
      const label = `${cell.reference.replace(':', ' ')} from ${cell.origin} in ${cell.project}`
      await page.findAll('[data-testid="box"]').find(each => each.attributes('aria-label')!.split('; ')[0] === label)!.trigger('click')
    }
    await flushPromises()

    expect(page.findAll('[data-held="locked"]')).not.toHaveLength(0)
    expect(page.findAll('[data-held="conflict"]')).not.toHaveLength(0)
    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('what a project an older marker marks offers', () => {
  it('draws every icon from the bundle, its dry run shown, and adds no style element', async () => {
    const { transport } = commanding(fixture('candidates'))
    const { page } = await render('/project?entry=legacy', transport)

    await page.get('[data-testid="migrate-dry"]').trigger('click')
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="migrate-yes"]').exists()).toBe(true))

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('the page of updates', () => {
  it('draws every icon from the bundle, every state listed, and adds no style element', async () => {
    const { transport } = answering(fixture('updating'), updates())
    const { page } = await render('/updates', transport)

    expect(page.findAll('[data-testid="row"]')).not.toHaveLength(0)
    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('one of the library\'s own, about to be removed', () => {
  it('draws every icon from the bundle, its dry run shown, and adds no style element', async () => {
    const { transport } = commanding(fixture('workspace'), commanded(), answers())
    const { page } = await render('/artifact?kind=skill&name=review', transport)

    await page.get('[data-testid="remove-dry"]').trigger('click')
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="remove-yes"]').exists()).toBe(true))

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('the page of MCP servers', () => {
  it.each(['/mcp', '/mcp?edit=github'])('%s draws every icon from the bundle and adds no style element', async (path) => {
    const { transport } = answering(fixture('serving'), servers())
    const { page } = await render(path, transport)

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('the page of catalogs', () => {
  it('draws every icon from the bundle, what both searches found and an add\'s dry run shown, and adds no style element', async () => {
    const found = searched()
    found[0]!.ran.outcome.runs[0]!.stderr = [
      { text: 'hint: github was not asked, since its search needs a token: --token-env or --gh hands one over', code: null },
      { text: 'warning: skills.sh: result 3 (`odd`): it names no repository, so it is not listed', code: 'finder_result_dropped' }
    ]
    const { transport } = commanding(fixture('workspace'), found)
    const { page } = await render('/catalogs', transport)

    for (const [of, query] of [['skills', 'commit'], ['servers', 'mcp']] as const) {
      await page.get(`input[data-testid="${of}-query"]`).setValue(query)
      await page.get(`[data-testid="${of}-form"]`).trigger('submit')
      await flushPromises()
      document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
      await vi.waitFor(() => expect(document.querySelector('[data-testid="preview"]')).toBeNull())
      await flushPromises()
    }
    await page.findAll('[data-testid="take"]')[0]!.trigger('click')
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="taken"]').exists()).toBe(true))
    await page.findAll('[data-testid="add-dry"]')[0]!.trigger('click')
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="add-yes"]').exists()).toBe(true))

    expect(page.findAll('[data-testid="said-line"]')).toHaveLength(2)
    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('authoring in the library', () => {
  it.each(['/edit?kind=skill&name=review', '/new?kind=skill'])('%s draws every icon from the bundle and adds no style element', async (path) => {
    const { transport } = answering(fixture('workspace'), [...answers(), ...drafts()])
    const { page } = await render(path, transport)

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

/** Press `key` where the focus is, as a keyboard would. */
function press(key: string, modifiers: KeyboardEventInit = {}) {
  const at: EventTarget = document.activeElement ?? window
  at.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, composed: true, cancelable: true, ...modifiers }))
}

/** Type `term` into the command palette's search, and wait for what it finds. */
async function search(term: string) {
  const input = document.querySelector<HTMLInputElement>('[data-testid="palette"] input')!
  input.value = term
  input.dispatchEvent(new Event('input', { bubbles: true }))
  // Fuse answers a frame later, its results throttled to one a frame.
  await new Promise(resolve => setTimeout(resolve, 40))
  await flushPromises()
}

// Neither is drawn until it opens, so the pages above draw neither: their
// icons are Nuxt UI's (its search, close, back and an entry's chevron) and
// the entries' own, which no page draws.
describe('the command palette and the sheet of shortcuts', () => {
  it('draw every icon from the bundle, an offer\'s projects, a link and what holds an install back among them, and add no style element', async () => {
    const cell = { project: 'web', reference: 'skill:format', origin: 'https://github.com/acme/skills', wanted: true }
    const why = { code: 'foreign', message: '`format` in web is somebody\'s own, at .claude/skills/format' }
    const held: Answered = { query: { type: 'plan', selection: [cell] }, answer: { tasks: [], conflicts: [], locked: [{ cell, why }] } }
    const { transport } = commanding(fixture('assigning'), applying(), [held])
    const { page } = await render('/', transport)

    press('k', { ctrlKey: true })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="palette"] [role="option"]')).not.toBeNull())
    expect(document.querySelector('[data-testid="palette"] [data-slot="itemTrailingIcon"]'), 'an offer, which lists projects').not.toBeNull()
    drawsEveryIcon(page)

    await search('install format')
    expect(document.querySelector('[data-testid="palette"]')!.textContent).toContain('Link install format')
    drawsEveryIcon(page)

    press('Enter')
    await vi.waitFor(() => expect(document.querySelector('[data-testid="palette"] [data-slot="back"]')).not.toBeNull())
    drawsEveryIcon(page)

    await search('web')
    press('Enter')
    await vi.waitFor(() => expect(document.querySelector('li[data-state="open"]')?.textContent).toContain(why.code))
    drawsEveryIcon(page)

    press('?', { shiftKey: true })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="shortcuts"]')).not.toBeNull())
    drawsEveryIcon(page)

    expect(styles).toEqual([])
  })
})

describe('the explorer', () => {
  it('draws every icon from the bundle, each word core says of a folder, a link and what it could not read among them, and adds no style element', async () => {
    const walked = walks().walked
    const root = walked[0]!.answer as Folders
    root.folders[0]!.symlink = true
    root.failures = [{ code: 'browse_entry_unread', message: '/home/you/Projects/loop: what it is cannot be read' }]
    const { transport } = commanding(fixture('candidates'), commanded(), walked)
    const { page } = await render('/projects', transport)

    document.querySelector<HTMLElement>('[data-testid="link-browse"]')!.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="folder-failure"]')).not.toBeNull())
    expect(new Set([...document.querySelectorAll('[data-testid="folder"] [data-testid="folder-is"]')].map(each => each.textContent))).toEqual(new Set(['mind project', 'folder', 'git repository']))
    drawsEveryIcon(page)

    // The home, a workspace, and a refusal.
    document.querySelector<HTMLElement>('[data-testid="folder-home"]')!.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="folder"] [data-testid="folder-is"]')?.textContent).toBe('weaver workspace'))
    const field = document.querySelector<HTMLInputElement>('input[data-testid="folder-path"], [data-testid="folder-path"] input')!
    field.value = 'nowhere'
    field.dispatchEvent(new Event('input'))
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="folder-go"]')!.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="folder-refused"]')).not.toBeNull())
    drawsEveryIcon(page)

    expect(styles).toEqual([])
  })
})
