import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory } from 'vue-router'
import App from '@/App.vue'
import { createPanel } from '@/panel'
import { useShell } from '@/stores/shell'
import type { Transport } from '@/api/transport'

/** A transport that writes down every call, and answers none of them. */
function recording() {
  const calls: string[] = []
  const transport: Transport = {
    query: () => {
      calls.push('query')
      return new Promise(() => {})
    },
    send: () => {
      calls.push('send')
      return new Promise(() => {})
    },
    transcript: () => {
      calls.push('transcript')
      return new Promise(() => {})
    },
    subscribe: () => {
      calls.push('subscribe')
      return () => {}
    }
  } as unknown as Transport
  return { calls, transport }
}

/** Where a launch serves the panel; each takes a port of its own. */
const LAUNCH = 'http://127.0.0.1:41001/'
const NEXT_LAUNCH = 'http://127.0.0.1:41002/'

/**
 * Open the page at another address, as the next `mindweaver ui` would. A browser
 * keeps localStorage per origin, so a new port opens on an empty one; happy-dom
 * keeps one for every address, so it is emptied here.
 */
function openAt(url: string) {
  (window as unknown as { happyDOM: { setURL(url: string): void } }).happyDOM.setURL(url)
  localStorage.clear()
}

/** The value of a cookie the page can read, or undefined. */
function cookie(name: string): string | undefined {
  const pair = document.cookie.split(/;\s*/).find(each => each.startsWith(`${name}=`))
  return pair === undefined ? undefined : decodeURIComponent(pair.slice(name.length + 1))
}

/** Forget every choice this browser made on 127.0.0.1. */
function forgetChoices() {
  for (const pair of document.cookie.split(/;\s*/)) {
    const name = pair.split('=')[0]
    if (name) {
      document.cookie = `${name}=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT`
    }
  }
}

let wrapper: VueWrapper | undefined
let styles: Element[]
let watching: MutationObserver

async function render(path: string, transport: Transport = recording().transport) {
  const panel = createPanel({ transport, history: createMemoryHistory() })
  await panel.router.push(path)
  wrapper = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  openAt(LAUNCH)
  forgetChoices()
  document.documentElement.className = 'dark'
  // Every <style> element anything adds, for as long as the test runs.
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

function close() {
  wrapper?.unmount()
  wrapper = undefined
  document.body.innerHTML = ''
}

afterEach(() => {
  watching.disconnect()
  close()
})

describe('the shell', () => {
  it('draws the sidebar and the header around the overview, which waits for the workspace', async () => {
    const page = await render('/')

    const sidebar = page.get('[data-testid="sidebar"]')
    expect(sidebar.text()).toContain('Mindweaver')
    expect(sidebar.findAll('nav a').map(link => link.text())).toEqual(['Overview', 'Projects', 'Discover', 'Assign', 'Updates', 'Library', 'MCP servers', 'Loads', 'Shelf', 'Sources', 'Catalogs'])
    expect(sidebar.get('nav a[aria-current="page"]').text()).toBe('Overview')
    expect(sidebar.get('[data-testid="address"]').text()).toBe(window.location.host)

    expect(page.get('[data-testid="overview"] h1').text()).toBe('Overview')
    expect(page.get('[data-testid="reading"]').text()).toBe('Reading the workspace')
  })

  it('answers a path it has no page for with one that says so', async () => {
    const page = await render('/projects/nowhere')

    expect(page.get('[data-testid="not-found"] h1').text()).toBe('Not found')
    expect(page.text()).toContain('The panel has nothing at /projects/nowhere.')
    expect(page.get('[data-testid="not-found"] a[href="/"]').text()).toContain('Back to the overview')
  })

  it('collapses the sidebar to its icons from the header', async () => {
    const page = await render('/')

    await page.get('[data-testid="overview"] button[aria-label="Collapse sidebar"]').trigger('click')
    await flushPromises()

    expect(useShell().sidebarCollapsed).toBe(true)
    expect(page.get('[data-testid="sidebar"]').text()).not.toContain('Mindweaver')
    expect(page.get('[data-testid="address"]').text()).toBe('')
  })

  it('keeps the sidebar under a name nothing rendered before it can move', async () => {
    const page = await render('/')

    await page.get('[data-testid="overview"] button[aria-label="Collapse sidebar"]').trigger('click')
    await flushPromises()

    expect(JSON.parse(cookie('mindweaver-dashboard-sidebar-default') ?? '{}')).toMatchObject({ collapsed: true })
  })

  it('opens the next launch, on a port of its own, with the sidebar as it was left', async () => {
    const page = await render('/')
    await page.get('[data-testid="overview"] button[aria-label="Collapse sidebar"]').trigger('click')
    await flushPromises()
    close()

    openAt(NEXT_LAUNCH)
    const next = await render('/')

    expect(useShell().sidebarCollapsed).toBe(true)
    expect(next.get('[data-testid="sidebar"]').text()).not.toContain('Mindweaver')
  })

  it('adds no style element, which the CSP would refuse', async () => {
    await render('/')

    // Not the palette, which Nuxt UI writes into one at run time, and not
    // the colour mode, which adds one for each switch.
    expect(styles).toEqual([])
  })

  it('listens to the server once, asks it for the snapshot, and for nothing else', async () => {
    const { calls, transport } = recording()

    await render('/', transport)

    expect(calls).toEqual(['subscribe', 'query'])
  })

  it('draws every icon it names from the bundle', async () => {
    const page = await render('/')

    // Iconify draws an icon it holds as an <svg>, and nothing at all for one
    // it would have had to fetch.
    const icons = page.findAllComponents({ name: 'Icon' })
    expect(icons.length).toBeGreaterThan(0)
    for (const icon of icons) {
      expect(icon.element).toBeInstanceOf(SVGElement)
      expect((icon.element as SVGElement).innerHTML).not.toBe('')
    }
  })
})

describe('the colour mode', () => {
  it('starts dark when this browser has chosen nothing', async () => {
    await render('/')

    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(cookie('mindweaver-vueuse-color-scheme')).toBe('dark')
  })

  it('keeps the light mode this browser chose', async () => {
    document.cookie = 'mindweaver-vueuse-color-scheme=light; Path=/'

    await render('/')

    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(document.documentElement.classList.contains('light')).toBe(true)
  })

  it('keeps the mode chosen under a Mindflayer build, moved to the new name once', async () => {
    document.cookie = 'mindflayer-vueuse-color-scheme=light; Path=/'

    await render('/')

    expect(document.documentElement.classList.contains('light')).toBe(true)
    expect(cookie('mindweaver-vueuse-color-scheme')).toBe('light')
    expect(cookie('mindflayer-vueuse-color-scheme')).toBeUndefined()
  })

  it('switches from the header without adding a style element', async () => {
    const page = await render('/')

    await page.get('[data-testid="overview"] button[aria-label="Switch to light mode"]').trigger('click')
    await flushPromises()

    expect(document.documentElement.classList.contains('light')).toBe(true)
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(cookie('mindweaver-vueuse-color-scheme')).toBe('light')
    expect(styles).toEqual([])
  })

  it('opens the next launch, on a port of its own, in the mode chosen in the last', async () => {
    const page = await render('/')
    await page.get('[data-testid="overview"] button[aria-label="Switch to light mode"]').trigger('click')
    await flushPromises()
    close()
    document.documentElement.className = 'dark'

    openAt(NEXT_LAUNCH)
    await render('/')

    expect(document.documentElement.classList.contains('light')).toBe(true)
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })

  it('keeps no choice in localStorage, which the next port would not see', async () => {
    const page = await render('/')
    await page.get('[data-testid="overview"] button[aria-label="Switch to light mode"]').trigger('click')
    await page.get('[data-testid="overview"] button[aria-label="Collapse sidebar"]').trigger('click')
    await flushPromises()

    expect(Object.keys(localStorage)).toEqual([])
  })
})
