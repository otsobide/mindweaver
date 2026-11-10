import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, type VueWrapper } from '@vue/test-utils'
import type { Planned } from '@/api/generated/Planned'
import type { Snapshot } from '@/api/generated/Snapshot'
import { TransportError, type Query, type Transport } from '@/api/transport'
import { UPDATE_ALL } from '@/api/updates'
import { VIEWS } from '@/components/AppSidebar.vue'
import { useShell } from '@/stores/shell'
import { answering, answers, applying, commanded, commanding, drafts, fixture, plans, render, unmount, updates, updatings, type Answered } from './panel'

afterEach(() => {
  unmount()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

/**
 * Press `key` where the focus is, or on the window when nothing holds it, as
 * a keyboard would; the event, whose default a key the panel takes is kept
 * from the browser by.
 */
function press(key: string, modifiers: KeyboardEventInit = {}, at: EventTarget = document.activeElement ?? window): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, composed: true, cancelable: true, ...modifiers })
  at.dispatchEvent(event)
  return event
}

/** Wait for what a key or a choice set off: the palette drawn, gone, or a page opened. */
async function settled() {
  await flushPromises()
  await new Promise(resolve => setTimeout(resolve, 0))
  await flushPromises()
}

/** The palette, while it is open. */
function palette(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[role="dialog"]:has([data-testid="palette"])')
}

/** Type `term` into the palette's search, as a person types it. */
async function search(term: string) {
  const input = palette()!.querySelector<HTMLInputElement>('input')!
  input.value = term
  input.dispatchEvent(new Event('input', { bubbles: true }))
  // Fuse answers a frame later, its results throttled to one a frame.
  await new Promise(resolve => setTimeout(resolve, 40))
  await flushPromises()
}

/** Each group the palette lists, by its heading, with the label of each entry in it. */
function listed(): { group: string, entries: string[] }[] {
  return [...palette()!.querySelectorAll('[role="group"]')].map(group => ({
    group: document.getElementById(group.getAttribute('aria-labelledby')!)!.textContent!.trim(),
    entries: [...group.querySelectorAll('[role="option"]')].map(entry => [...entry.querySelectorAll('[data-slot="itemLabelBase"], [data-slot="itemLabelSuffix"]')]
      .map(part => part.textContent!.trim())
      .join(' '))
  }))
}

/** The entries listed under `heading`, none when it is not listed. */
function group(heading: string): string[] {
  return listed().find(each => each.group === heading)?.entries ?? []
}

/** Every entry's label, in the order listed. */
function entries(): string[] {
  return listed().flatMap(group => group.entries)
}

/**
 * Have the palette leave as a browser draws it, which happy-dom does not:
 * its dialog says it animates in while open and out once closed, so Reka
 * keeps it while it leaves, until `left` says that has ended.
 */
function animating() {
  const computed = window.getComputedStyle.bind(window)
  vi.stubGlobal('getComputedStyle', (element: Element, pseudo?: string | null) => {
    const style = computed(element, pseudo)
    if (!(element instanceof HTMLElement) || element.getAttribute('role') !== 'dialog') {
      return style
    }
    return new Proxy(style, {
      get(target, key) {
        if (key === 'animationName') {
          return element.dataset.state === 'closed' ? 'leave' : 'enter'
        }
        const value: unknown = Reflect.get(target, key, target)
        return typeof value === 'function' ? value.bind(target) : value
      }
    })
  })
}

/** End the palette's leave, as a browser says it once it has drawn it. */
function left() {
  const ended = new Event('animationend')
  Object.defineProperty(ended, 'animationName', { value: 'leave' })
  palette()!.dispatchEvent(ended)
}

/** Open the palette as a keyboard does, over whatever has the focus. */
async function opened() {
  press('k', { ctrlKey: true })
  await settled()
  expect(palette(), 'the palette, opened by Ctrl+K').not.toBeNull()
}

/**
 * Have the document tell of its selection changing as a browser does: in a
 * task of its own, once whatever changed it has returned, and once for
 * however many changes came before that task ran
 * (https://w3c.github.io/selection-api/#selectionchange-event). happy-dom
 * 21.0.4 tells of each at once, inside the call that made it,
 * `Selection.collapse` among them, and CodeMirror collapses the selection
 * in the middle of an update. Told then, it reads the caret it is placing
 * as one somebody moved, and starts a second update inside the first,
 * which it refuses ("Calls to EditorView.update are not allowed while an
 * update is in progress") from a timer of its own, outside the test, an
 * error Vitest fails the run for (#233). The update it makes 10ms after
 * the focus moves in did so whenever an earlier test had left the
 * selection in its own editor, in nodes happy-dom keeps it in once they
 * are taken away, and so does that update when it comes more than 200ms
 * after the focus, once CodeMirror no longer takes a caret at the start of
 * the text for the one a browser puts there as it focuses. Taking the
 * selection out between tests was the other way, and would have left the
 * late update throwing. Undone after each test, by `vi.restoreAllMocks`.
 */
function selecting() {
  const dispatch = document.dispatchEvent.bind(document)
  let scheduled = false
  vi.spyOn(document, 'dispatchEvent').mockImplementation((event: Event) => {
    // happy-dom hands an event on from target to target through this same
    // method, its phase set: only a change told anew waits.
    if (event.type !== 'selectionchange' || event.eventPhase !== Event.NONE) {
      return dispatch(event)
    }
    if (!scheduled) {
      scheduled = true
      setTimeout(() => {
        scheduled = false
        dispatch(event)
      })
    }
    return true
  })
}

/**
 * The editor's text on `page`, focused as a click there focuses it, its
 * selection told of as a browser tells of it (`selecting`), once CodeMirror
 * has taken the focus in, which it does 10ms after the focus moves, in an
 * update a person sees as the editor's outline. A key pressed at once left
 * that update to the machine's speed: it ran before the test took the
 * editor down, or after, so a run went through it only by chance.
 */
async function editorFocused(page: VueWrapper): Promise<HTMLElement> {
  selecting()
  const content = await vi.waitFor(() => {
    const found = page.get('[data-testid="editor"]').element.shadowRoot?.querySelector<HTMLElement>('.cm-content')
    expect(found).toBeTruthy()
    return found!
  })
  content.focus()
  await vi.waitFor(() => expect(content.closest('.cm-editor.cm-focused')).not.toBeNull())
  return content
}

describe('the command palette', () => {
  it('opens on Ctrl+K over any page, its search focused, and gives the focus back where it was on Escape', async () => {
    await render('/projects', fixture('workspace'))
    const before = document.querySelector<HTMLElement>('[data-testid="sidebar"] nav a[href="/assign"]')!
    before.focus()

    await opened()
    expect(useShell().paletteOpen).toBe(true)
    expect(document.activeElement).toBe(palette()!.querySelector('input'))

    press('Escape')
    await vi.waitFor(() => expect(palette()).toBeNull())
    await settled()
    expect(useShell().paletteOpen).toBe(false)
    expect(document.activeElement).toBe(before)
  })

  it('opens on Cmd+K on a Mac, and on Ctrl+K there too, and closes on Cmd+K from its search', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15')
    await render('/', fixture('workspace'))

    press('k', { metaKey: true })
    await settled()
    expect(palette()).not.toBeNull()

    press('Escape')
    await vi.waitFor(() => expect(palette()).toBeNull())
    press('k', { ctrlKey: true })
    await settled()
    expect(palette()).not.toBeNull()

    expect(press('k', { metaKey: true }).defaultPrevented).toBe(true)
    await vi.waitFor(() => expect(palette()).toBeNull())
  })

  it('closes on Ctrl+K from its own search, the key kept from the browser, and gives the focus back where it was', async () => {
    await render('/projects', fixture('workspace'))
    const before = document.querySelector<HTMLElement>('[data-testid="sidebar"] nav a[href="/assign"]')!
    before.focus()
    await opened()
    expect(document.activeElement).toBe(palette()!.querySelector('input'))

    // Chromium would put the focus in its own search bar on a Ctrl+K it
    // was not kept from, and leave the palette open.
    expect(press('k', { ctrlKey: true }).defaultPrevented).toBe(true)
    await vi.waitFor(() => expect(palette()).toBeNull())
    await settled()
    expect(useShell().paletteOpen).toBe(false)
    expect(document.activeElement).toBe(before)

    // And opens again on it, its field gone with the focus let go of.
    await opened()
  })

  it('opens from the sidebar, which says its keys', async () => {
    const { page } = await render('/', fixture('workspace'))
    const button = page.get('[data-testid="open-palette"]')

    // Off a Mac, Nuxt UI reads Cmd+K as Ctrl+K: Meta+K opens nothing.
    expect(button.attributes('aria-keyshortcuts')).toBe('Control+K')
    await button.trigger('click')
    await settled()

    expect(palette()).not.toBeNull()
  })

  it('says both its keys from the sidebar on a Mac, where it takes both', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15')
    const { page } = await render('/', fixture('workspace'))

    expect(page.get('[data-testid="open-palette"]').attributes('aria-keyshortcuts')).toBe('Meta+K Control+K')
  })

  it('never opens while a field or the editor has the focus, whose keys they stay', async () => {
    const { page } = await render('/projects', fixture('workspace'))
    page.get<HTMLInputElement>('input[data-testid="filter-text"]').element.focus()
    press('k', { ctrlKey: true })
    await settled()
    expect(palette()).toBeNull()
    unmount()

    const editing = await render('/edit?kind=skill&name=review', answering(fixture('workspace'), [...answers(), ...drafts()]).transport)
    const content = await editorFocused(editing.page)
    press('k', { ctrlKey: true }, content)
    await settled()
    expect(palette()).toBeNull()
  })

  it('never opens while a task is shown, whose question is answered first', async () => {
    const { transport } = commanding()
    const { page } = await render('/project?entry=gone', transport)
    await page.get('[data-testid="unlink"]').trigger('click')
    await settled()
    expect(document.querySelector('[data-testid="preview"]')).not.toBeNull()

    press('k', { ctrlKey: true })
    await settled()

    expect(useShell().paletteOpen).toBe(false)
    expect(palette()).toBeNull()
  })

  it('lists every page and its actions, then each offer to install, project, the library\'s own, the shelf and the sources, each under its heading', async () => {
    const snapshot = fixture('workspace')
    await render('/', snapshot)
    await opened()

    expect(listed()).toEqual([
      { group: 'Pages', entries: VIEWS.map(view => view.label) },
      { group: 'Actions', entries: ['Update all', 'Keyboard shortcuts'] },
      // Every offer some project's box of is neither ticked nor closed:
      // `review`, which every project holds, is not, and the library's MCP
      // server is, of every kind installed now.
      { group: 'Install', entries: ['Install lint skill', 'Install format skill', 'Install triage skill', 'Install git/commits rule', 'Install style/comments rule', 'Install github mcp'] },
      { group: 'Projects', entries: snapshot.projects.map(project => `${project.name ?? project.entry} ${project.health}`) },
      { group: 'Library', entries: snapshot.library.artifacts.map(held => `${held.name} ${held.kind}`) },
      { group: 'Shelf', entries: snapshot.shelf.map(shelved => `${shelved.name} ${shelved.kind}`) },
      { group: 'Sources', entries: snapshot.sources.map(source => `${source.origin} ${source.kind}`) }
    ])
    expect(palette()!.querySelector('[role="listbox"]')!.getAttribute('aria-label')).toBeTruthy()
    expect(palette()!.querySelector('input')!.getAttribute('aria-label')).toBe('Search the panel')
    expect(palette()!.getAttribute('aria-labelledby')).toBeTruthy()
  })

  it('searches fuzzily, every word narrowing what the others found, the group of the best match first', async () => {
    await render('/', fixture('workspace'))
    await opened()

    await search('revew')
    expect(listed()[0]).toEqual({ group: 'Library', entries: ['review skill'] })

    await search('projets')
    expect(entries()).toContain('Projects')

    // Not so near as to find `comments`; and every skill, which the first
    // word finds nothing of, left out, but what is typed offered as a
    // folder to link, last, whatever it is.
    await search('rule commits')
    expect(group('Library')).toEqual(['git/commits rule'])
    expect(entries().filter(entry => !entry.endsWith(' rule'))).toEqual(['Link rule commits'])

    // The source, whose origin it is all of, before what it put on the
    // shelf and offers to install, whose descriptions name it.
    await search('acme')
    expect(listed()[0]).toEqual({ group: 'Sources', entries: ['https://github.com/acme/skills git'] })
    expect([...group('Shelf')].sort()).toEqual(['format skill', 'triage skill'])

    await search('nothing here at all')
    expect(entries()).toEqual(['Link nothing here at all'])
  })

  // Five pages drawn, each opened from the palette: about a second alone,
  // but beside every other suite, a worker on each core, every step took
  // five times as long, waiting on nothing but the CPU, and the whole
  // outlasted Vitest's five seconds in most runs on a 16-thread machine.
  it('goes where a link on a page would, once it has closed', async () => {
    const { router } = await render('/', fixture('workspace'))

    for (const [term, path] of [
      ['docs broken', '/project?entry=docs'],
      ['review', '/artifact?kind=skill&name=review'],
      ['triage', '/shelf?kind=skill&q=triage'],
      ['acme git', '/sources'],
      ['assign', '/assign']
    ]) {
      await opened()
      await search(term!)
      press('Enter')
      await vi.waitFor(() => expect(router.currentRoute.value.fullPath).toBe(path))
      expect(palette()).toBeNull()
      await settled()
    }
  }, 20_000)

  it('lets a choice go when it is opened again before it has gone, rather than doing it at the next close, and opens anew', async () => {
    animating()
    const { router } = await render('/', fixture('workspace'))

    // Assign chosen, then the palette opened again while it leaves.
    await opened()
    await search('assign')
    press('Enter')
    await settled()
    expect(palette()!.dataset.state, 'the palette, still leaving').toBe('closed')
    await opened()
    expect(palette()!.dataset.state).toBe('open')
    expect(palette()!.querySelector('input')!.value).toBe('')
    expect(document.activeElement).toBe(palette()!.querySelector('input'))

    // Closed with nothing chosen, and gone: Assign is not opened then.
    press('Escape')
    await settled()
    left()
    await vi.waitFor(() => expect(palette()).toBeNull())
    await settled()
    expect(router.currentRoute.value.fullPath).toBe('/')

    // An offer's projects, left by closing, are not what it opens on again.
    await opened()
    await search('install lint')
    await enter()
    expect(listed().map(each => each.group)).toEqual(['Install lint'])
    press('Escape')
    await settled()
    await opened()
    expect(listed().map(each => each.group).slice(0, 2)).toEqual(['Pages', 'Actions'])
  })

  it('draws what it lists as text, never as HTML', async () => {
    const snapshot: Snapshot = fixture('workspace')
    snapshot.library.artifacts[0]!.summary = '<img src="x" alt="planted"> a <b>bold</b> summary'
    await render('/', snapshot)
    await opened()

    await search('bold')

    const found = palette()!.querySelector('[role="option"]')!
    expect(found.textContent).toContain('<img src="x" alt="planted"> a <b>bold</b> summary')
    expect(found.querySelector('img, b, mark')).toBeNull()
  })

  it('asks the server for nothing, the snapshot read from what is kept while the server watches', async () => {
    const { queries, tell, transport } = commanding(fixture('workspace'))
    await render('/', transport)
    tell({ type: 'watching' })
    await flushPromises()
    const asked = queries.length

    await opened()
    await search('api')

    expect(queries.slice(asked)).toEqual([])
  })
})

/** Choose, by Enter, the entry highlighted: the first, or the nearest to what was typed. */
async function enter() {
  press('Enter')
  await settled()
}

/** The lines the dialog shows, as `$ line`, once it shows a task. */
async function shown(): Promise<string[]> {
  return vi.waitFor(() => {
    const lines = [...document.querySelectorAll('[data-testid="preview"] [data-testid="command"]')].map(line => line.textContent!.trim())
    expect(lines).not.toEqual([])
    return lines
  })
}

/** Press Run in the dialog, and wait until it has gone. */
async function run() {
  document.querySelector<HTMLElement>('[role="dialog"] [data-testid="run"]')!.click()
  await vi.waitFor(() => expect(document.querySelector('[data-testid="preview"]')).toBeNull())
}

/** Each toast's title and description, as shown. */
function toasts(): string[] {
  return [...document.querySelectorAll('li[data-state="open"] [data-slot="wrapper"]')]
    .map(toast => [...toast.children].map(part => part.textContent!.trim()).join(' '))
}

describe('acting from the command palette', () => {
  it('installs one offer into one project as ticking its box on the matrix does: core plans the box, and its tasks are previewed, then run', async () => {
    const [, applied] = applying()
    const [, oneBox] = plans()
    const { queries, sent, transport } = commanding(fixture('assigning'), applying(), plans())
    await render('/', transport)
    await opened()

    await search('install deploy workspace')
    expect(group('Install')).toEqual(['Install deploy skill'])
    await enter()
    // `api` holds the team's `deploy`, whose box the library's replaces;
    // `bare` and `gone` take no skill.
    expect(listed()).toEqual([{ group: 'Install deploy', entries: ['api claude, codex', 'web claude, opencode', 'docs codex'] }])
    await search('api')
    await enter()

    expect(await shown()).toEqual(applied!.preview.lines.map(line => `$ ${line}`))
    expect(queries.filter(query => query.type === 'plan')).toEqual([oneBox!.query])
    expect(sent).toEqual([{ task: applied!.task, mode: 'preview' }])
    expect(applied!.task).toEqual({ type: 'apply_install', changes: (oneBox!.answer as Planned).tasks })

    await run()
    expect(sent.map(each => each.mode)).toEqual(['preview', 'run'])
    expect(sent[1]!.task).toEqual(applied!.task)
  })

  it('offers an offer only into the projects whose box of it is neither ticked nor closed', async () => {
    await render('/', fixture('assigning'))
    await opened()

    // `docs` holds the team's `notes`; `bare` and `gone` take no skill.
    await search('install notes')
    await enter()

    expect(listed()).toEqual([{ group: 'Install notes', entries: ['api claude, codex', 'web claude, opencode'] }])
  })

  it('says what holds an install back, with its code, and runs nothing', async () => {
    const cell = { project: 'web', reference: 'skill:format', origin: 'https://github.com/acme/skills', wanted: true }
    const why = { code: 'foreign', message: '`format` in web is somebody\'s own, at .claude/skills/format' }
    const held: Answered = { query: { type: 'plan', selection: [cell] }, answer: { tasks: [], conflicts: [], locked: [{ cell, why }] } }
    const { sent, transport } = commanding(fixture('assigning'), applying(), [held])
    await render('/', transport)
    await opened()

    await search('install format')
    await enter()
    await search('web')
    await enter()

    await vi.waitFor(() => expect(toasts()).toEqual([`Nothing ran ${why.message} (${why.code})`]))
    expect(sent).toEqual([])
  })

  it('says so when the plan of a box runs nothing, holding nothing back, and runs nothing', async () => {
    const cell = { project: 'web', reference: 'skill:format', origin: 'https://github.com/acme/skills', wanted: true }
    const none: Answered = { query: { type: 'plan', selection: [cell] }, answer: { tasks: [], conflicts: [], locked: [] } }
    const { sent, transport } = commanding(fixture('assigning'), applying(), [none])
    await render('/', transport)
    await opened()

    await search('install format')
    await enter()
    await search('web')
    await enter()

    await vi.waitFor(() => expect(toasts()).toEqual(['Nothing ran The plan of skill:format from https://github.com/acme/skills into web runs nothing.']))
    expect(sent).toEqual([])
  })

  it('says why a plan was refused, with its code, and runs nothing', async () => {
    const { sent, transport } = commanding(fixture('assigning'), applying(), [])
    const refusing: Transport = {
      ...transport,
      query: query => (query.type === 'plan'
        ? Promise.reject(new TransportError(422, '`api` is no entry of the workspace', 'not_linked'))
        : transport.query(query))
    } as Transport
    await render('/', refusing)
    await opened()

    await search('install format')
    await enter()
    await search('api')
    await enter()

    await vi.waitFor(() => expect(toasts()).toEqual(['Nothing ran `api` is no entry of the workspace (not_linked)']))
    expect(sent).toEqual([])
  })

  it('updates every project with the task Update all sends on the page of updates', async () => {
    const [all] = updatings()
    const { sent, transport } = commanding(fixture('updating'), updatings(), updates())
    await render('/', transport)
    await opened()

    await search('update all')
    await enter()

    expect(await shown()).toEqual(all!.preview.lines.map(line => `$ ${line}`))
    expect(sent).toEqual([{ task: UPDATE_ALL, mode: 'preview' }])
    expect(all!.task).toEqual(UPDATE_ALL)
  })

  it('links the folder typed, with the task the link form sends', async () => {
    const [link] = commanded()
    const path = (link!.task as { path: string }).path
    const { sent, transport } = commanding(fixture('workspace'), commanded())
    await render('/', transport)
    await opened()

    await search(path)
    expect(listed().at(-1)).toEqual({ group: 'Link', entries: [`Link ${path}`] })
    await enter()

    expect(await shown()).toEqual(link!.preview.lines.map(line => `$ ${line}`))
    expect(sent).toEqual([{ task: { type: 'link', path }, mode: 'preview' }])
  })

  it('fetches nothing itself: each action reaches the server through the transport the pages use', async () => {
    const reached = vi.fn()
    for (const name of ['fetch', 'XMLHttpRequest', 'EventSource', 'WebSocket']) {
      vi.stubGlobal(name, vi.fn((...args: unknown[]) => {
        reached(name, ...args)
        throw new Error(`${name} reached past the transport`)
      }))
    }
    try {
      const { queries, sent, transport } = commanding(fixture('assigning'), [...applying(), ...updatings(), ...commanded()], plans())
      const { router } = await render('/', transport)
      const asked = queries.length

      // Each change shown, then let go, so the next can be asked.
      const letGo = async () => {
        await shown()
        document.querySelector<HTMLElement>('[role="dialog"] [data-testid="cancel"]')!.click()
        await vi.waitFor(() => expect(document.querySelector('[data-testid="preview"]')).toBeNull())
      }
      for (const [steps, then] of [
        [['install deploy workspace', 'api'], letGo],
        [['update all'], letGo],
        [['/home/you/Projects/blog'], letGo],
        [['assign'], async () => undefined]
      ] as [string[], () => Promise<void>][]) {
        await opened()
        for (const step of steps) {
          await search(step)
          await enter()
        }
        await vi.waitFor(() => expect(useShell().paletteOpen).toBe(false))
        await then()
        await settled()
      }

      await vi.waitFor(() => expect(router.currentRoute.value.path).toBe('/assign'))
      expect(reached).not.toHaveBeenCalled()
      expect(queries.slice(asked).filter((query: Query) => query.type !== 'snapshot').map(query => query.type)).toEqual(['plan'])
      expect(sent.map(each => [each.task.type, each.mode])).toEqual([['apply_install', 'preview'], ['update', 'preview'], ['link', 'preview']])
    } finally {
      vi.unstubAllGlobals()
    }
  })
})

/** The sheet of shortcuts, while it is open. */
function sheet(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[role="dialog"]:has([data-testid="shortcuts"])')
}

describe('the sheet of shortcuts', () => {
  it('opens on ?, saying the keys of the palette, a change asked about, the matrix and the editor, and gives the focus back on Escape', async () => {
    await render('/projects', fixture('workspace'))
    const before = document.querySelector<HTMLElement>('[data-testid="sidebar"] nav a[href="/assign"]')!
    before.focus()

    press('?', { shiftKey: true })
    await settled()

    expect(useShell().shortcutsOpen).toBe(true)
    const titles = [...sheet()!.querySelectorAll('[data-testid="shortcut-section"] h3')].map(title => title.textContent!.trim())
    expect(titles).toEqual(['Anywhere but a field or the editor', 'In the command palette', 'Asked about a change', 'On the matrix, in Assign', 'In the editor'])
    const keys = [...sheet()!.querySelectorAll('[data-testid="keys"] .sr-only')].map(said => said.textContent!.trim())
    expect(keys.slice(0, 2)).toEqual(['Control K', 'Question mark'])
    expect(keys).toContain('Control S')
    expect(sheet()!.getAttribute('aria-labelledby')).toBeTruthy()
    // What a field keeps is the two that open something, not every key
    // listed: the palette's and the editor's are pressed in a field.
    expect(document.getElementById(sheet()!.getAttribute('aria-describedby')!)!.textContent).toBe('What the keys do in the panel. Neither Ctrl+K nor ? is taken while you type in a field or in the editor, whose keys they stay, but in the palette\'s own search, where Ctrl+K closes it.')

    press('Escape')
    await vi.waitFor(() => expect(sheet()).toBeNull())
    await settled()
    expect(document.activeElement).toBe(before)
  })

  it('never opens while a field or the editor has the focus, where ? is typed', async () => {
    const { page } = await render('/projects', fixture('workspace'))
    page.get<HTMLInputElement>('input[data-testid="filter-text"]').element.focus()
    press('?', { shiftKey: true })
    await settled()
    expect(sheet()).toBeNull()
    unmount()

    const editing = await render('/edit?kind=skill&name=review', answering(fixture('workspace'), [...answers(), ...drafts()]).transport)
    const content = await editorFocused(editing.page)
    press('?', { shiftKey: true }, content)
    await settled()
    expect(sheet()).toBeNull()
  })

  it('never opens while a task is shown', async () => {
    const { transport } = commanding()
    const { page } = await render('/project?entry=gone', transport)
    await page.get('[data-testid="unlink"]').trigger('click')
    await settled()

    press('?', { shiftKey: true })
    await settled()

    expect(useShell().shortcutsOpen).toBe(false)
  })

  it('opens from the palette, and gives way to it on Ctrl+K', async () => {
    await render('/', fixture('workspace'))
    await opened()

    await search('keyboard')
    await enter()
    await vi.waitFor(() => expect(sheet()).not.toBeNull())
    expect(palette()).toBeNull()

    press('k', { ctrlKey: true })
    await vi.waitFor(() => expect(palette()).not.toBeNull())
    await vi.waitFor(() => expect(sheet()).toBeNull())
    expect(useShell().shortcutsOpen).toBe(false)
  })
})
