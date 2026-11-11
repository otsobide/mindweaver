/*
 * The README's screenshots, which `make docs/screenshots` takes:
 *
 *   MINDWEAVER_E2E_BIN=../../target/debug node e2e/screenshots.ts
 *
 * Taken, never drawn by hand, so they show the panel as it is: the demo
 * workspace (demo.ts) built with the binaries under test, served by their
 * `mindweaver ui`, and every page of the tour opened in Chromium at one size
 * and written into docs/images. It runs as the end-to-end suite does
 * (environment.ts): no variable leading to a real workspace or agent home,
 * a temporary home holding only the default workspace the demo makes
 * there, every command pinned. Not a spec, so `make
 * web/e2e` never runs it, and Playwright's library rather than its runner:
 * the pages are visited once, in order, some after another has changed the
 * workspace.
 *
 * At a fixed place under the system's temporary folder rather than one
 * made fresh, since every page shows where the workspace is and a path
 * that changed with each run would change every image. Nowhere under a
 * home: an image shows the paths it was taken over, and before one is
 * written the page it shows is read for a home's path, the account's name
 * and the launch token, any of which fails the run.
 */
import { existsSync, mkdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { homedir, tmpdir, userInfo } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium, type Browser, type Page } from '@playwright/test'
import { ARTIFACT, LOADED, REMOVED, SHOWN, TEAM, WORKSPACE, build } from './demo.ts'
import { built, look, marked, serve, type Places, type Serving } from './environment.ts'

const web = fileURLToPath(new URL('..', import.meta.url))

/** Where the images go, which the README and the reference name. */
const IMAGES = resolve(web, '../../docs/images')

/**
 * The size every page is taken at: wide enough for the sidebar beside a
 * project's table with every column it has, which 1280 cut short.
 */
const VIEWPORT = { width: 1440, height: 900 }

/**
 * Image pixels to a CSS pixel. One blurs once GitHub scales the image down
 * to its column and a screen with dense pixels scales it up again; two
 * sharpens that, and doubles what each image weighs, close to the budget
 * for the busiest pages. A quarter more is sharp enough, and keeps every
 * image near half of it.
 */
const SCALE = 1.25

/**
 * The language and the time zone every date is written in. The panel
 * writes one in the browser's own (src/time.ts), which Playwright takes
 * from the machine: left to it, a run under another language would put
 * its words into the English README, and every run would tell the time
 * zone of whoever took it.
 */
const LOCALE = 'en-US'
const ZONE = 'UTC'

/** Above this an image is refused: regenerating them must not bloat the history. */
const BUDGET = 300 * 1024

/** What says this folder is the demo's, so a run left behind may be removed. */
const OURS = '.mindweaver-screenshots'

/** Say what stopped the run, and stop. */
function refuse(message: string): never {
  console.error(`error: ${message}`)
  process.exit(2)
}

/** `text` read as a pattern of itself. */
function literally(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * Everything a page may not show: the home this was run from, a path under
 * any other home, the account's name as a folder of a path, and the launch
 * token. `/home/you` alone is let through, the stand-in home the reference
 * writes every example in, which the panel's own placeholders use too. The
 * name only as a folder: as a bare word it would be found in a page that
 * says `root` or `admin` of something else, and outside a path the panel
 * has nowhere to show it.
 */
function leaks(text: string, token: string): string[] {
  const found: string[] = []
  const home = homedir()
  const user = userInfo().username
  if (home.length > 1 && text.includes(home)) {
    found.push(`this account's home, ${home}`)
  }
  for (const path of text.matchAll(/\/(?:home|Users)\/(?!you\b)[^/\s"'<>`]+/g)) {
    found.push(`a path under a home, ${path[0]}`)
  }
  if (user !== '' && new RegExp(`[/\\\\]${literally(user)}(?=[/\\\\\\s"'<>\`]|$)`, 'm').test(text)) {
    found.push(`this account's name as a folder, ${user}`)
  }
  if (text.includes(token)) {
    found.push('the launch token')
  }
  return found
}

/** One image of the tour: the page it shows, made ready by `ready`. */
interface Shot {
  /** Its file in docs/images, without `.png`. */
  name: string
  /** The panel's path to open, or none to stay where the last left off. */
  path?: string
  /** What to wait for, or do, before it is taken. */
  ready: (page: Page) => Promise<void>
}

/**
 * Wait for the toast saying a line of `binary` ending in `words` ran to
 * show, then to go, so the image is taken without it over what it shows.
 * Matched whichever workspace the line names with `-W`, which the panel
 * puts right after the binary: the line spelled out would be a wait that
 * passes at once, for a toast that is never there, the day its spelling
 * changes; and waited for to show first, so a pattern that misses fails
 * the run rather than leave the toast in the image.
 */
async function toastGone(page: Page, binary: string, words: string): Promise<void> {
  const toast = page.getByText(new RegExp(`^Ran ${literally(binary)} (?:-W .+ )?${literally(words)}$`))
  await toast.waitFor({ timeout: 30_000 })
  await toast.waitFor({ state: 'hidden', timeout: 30_000 })
}

/** Wait until the page is drawn, as the suite's `drawn` does, and every animation settled. */
async function drawn(page: Page): Promise<void> {
  await page.getByRole('main').first().waitFor()
  await page.getByTestId('reading').waitFor({ state: 'detached' })
  await page.evaluate(async () => {
    await document.fonts.ready
  })
}

const at: Places = (() => {
  const bin = resolve(web, process.env.MINDWEAVER_E2E_BIN ?? '../../target/debug')
  if (!built(bin)) {
    refuse(`no mindweaver and mind in ${bin}: \`make docs/screenshots\` builds them there`)
  }
  const temporary = tmpdir()
  const holder = marked(temporary)
  if (holder) {
    refuse(`the temporary folder ${temporary} is inside ${holder}, which holds a .mind, a .mindweaver or a .mindflayer a command run from the demo could reach`)
  }
  if (!existsSync(chromium.executablePath())) {
    refuse('no Chromium of Playwright\'s to take them in: `make web/e2e/browsers BROWSERS=chromium` downloads it, into node_modules')
  }
  const scratch = join(temporary, 'mindweaver-demo')
  return { scratch, home: join(scratch, 'home'), workspace: join(scratch, WORKSPACE), bin }
})()

const workspace = at.workspace
const project = `/project?${new URLSearchParams({ entry: SHOWN })}`

/**
 * The boxes the matrix's image is taken with marked, each by the start of
 * what it says it is: one ticked, one unticked.
 */
const ASSIGNED = [
  `skill release-notes from ${join(at.scratch, TEAM)} in data-pipeline`,
  'skill api-conventions from workspace in admin-console'
]

/**
 * What is typed into the command palette's image: a word the shelf's
 * `release-notes` starts with, which some projects hold and others could.
 */
const PALETTE = 'release'

/**
 * The tour, in the order it is taken: of the last five, three run a command
 * and ask about another, the fourth runs a dry run, which changes nothing,
 * after the transcript is taken, and the last loads a workspace, after
 * every other page is taken without it.
 */
const TOUR: Shot[] = [
  {
    name: 'overview',
    path: '/',
    ready: async page => page.getByTestId('cards').waitFor()
  },
  {
    // The header's switcher open over the overview: the workspace in use,
    // found above where the panel was launched, and every one this machine
    // knows to switch to. Nothing is switched.
    name: 'switcher',
    path: '/',
    ready: async (page) => {
      await page.getByTestId('cards').waitFor()
      await page.getByTestId('workspace-switcher').click()
      await page.getByRole('menuitemcheckbox', { checked: true }).waitFor()
    }
  },
  {
    // The command palette over the overview, opened with its keys and a
    // word typed: what it finds of every kind, grouped, an install among
    // it. Nothing is chosen.
    name: 'palette',
    path: '/',
    ready: async (page) => {
      await page.getByTestId('cards').waitFor()
      await page.keyboard.press('ControlOrMeta+K')
      const palette = page.getByRole('dialog', { name: 'Command palette' })
      await palette.getByRole('textbox').waitFor()
      await page.keyboard.type(PALETTE)
      await palette.getByRole('option', { name: new RegExp(`^Install ${literally(PALETTE)}`) }).waitFor()
    }
  },
  {
    name: 'projects',
    path: '/projects',
    ready: async page => page.getByTestId('row').first().waitFor()
  },
  {
    // The explorer over the projects page, opened from the Link form at
    // the workspace's root: what each folder is, and which the workspace
    // links. Nothing is chosen.
    name: 'explorer',
    path: '/projects',
    ready: async (page) => {
      await page.getByTestId('row').first().waitFor()
      await page.getByTestId('link-browse').click()
      await page.getByRole('dialog', { name: 'Choose a folder to link' }).getByTestId('folder').first().waitFor()
    }
  },
  {
    name: 'project',
    path: project,
    ready: async page => page.getByTestId('artifact').first().waitFor()
  },
  {
    name: 'library',
    path: '/library?kind=skill',
    ready: async page => page.getByTestId('row').first().waitFor()
  },
  {
    name: 'artifact',
    path: `/artifact?${new URLSearchParams({ kind: 'skill', name: ARTIFACT })}`,
    ready: async page => page.getByTestId('markdown').waitFor()
  },
  {
    // Its SKILL.md in the editor, a line typed into its front matter that
    // validate warns of, judged as core judges it, and not saved.
    name: 'editor',
    path: `/edit?${new URLSearchParams({ kind: 'skill', name: ARTIFACT })}`,
    ready: async (page) => {
      const text = page.getByRole('textbox', { name: `The text of ${ARTIFACT}'s SKILL.md` })
      await text.click()
      // Below the fence that opens it: a line further down may wrap.
      await page.keyboard.press('ControlOrMeta+Home')
      await page.keyboard.press('End')
      await page.keyboard.press('Enter')
      await page.keyboard.type('compatibility: ""')
      await page.getByTestId('verdict').getByTestId('issue').first().waitFor()
      await page.getByTestId('judging').waitFor({ state: 'detached' })
    }
  },
  {
    // One added from the `full` template, typed and judged, not yet added.
    name: 'new',
    path: '/new?kind=skill',
    ready: async (page) => {
      await page.getByLabel('Name').fill('release-checklist')
      await page.getByLabel('Description').fill('Walk through what every release needs before it ships')
      await page.getByRole('radio', { name: /^full/ }).click()
      await page.getByTestId('writes').waitFor()
      await page.getByTestId('judging').waitFor({ state: 'detached' })
    }
  },
  {
    // The library's MCP servers: what each agent would run of each, what
    // it asks before it starts one, and what doctor finds in the project
    // holding one.
    name: 'mcp',
    path: '/mcp',
    ready: async page => page.getByTestId('server').first().waitFor()
  },
  {
    name: 'shelf',
    path: '/shelf',
    ready: async page => page.getByTestId('row').first().waitFor()
  },
  {
    // Two boxes marked, a skill from the shelf into a project and one of
    // the library's own out of another, with what the plan runs for them.
    name: 'assign',
    path: '/assign',
    ready: async (page) => {
      await page.getByTestId('matrix').waitFor()
      for (const box of ASSIGNED) {
        await page.getByRole('checkbox', { name: new RegExp(`^${literally(box)}(;|$)`) }).click()
      }
      await page.getByTestId('line').first().waitFor()
      // Clicking scrolls a box out from under the headings: back to the top.
      await page.getByTestId('matrix').evaluate((matrix) => {
        matrix.scrollTo(0, 0)
      })
    }
  },
  {
    // What an update would do: what is outdated, and the edited copy it
    // leaves, with why.
    name: 'updates',
    path: '/updates',
    ready: async page => page.getByTestId('state-edited').getByTestId('row').first().waitFor()
  },
  {
    name: 'discover',
    path: `/discover?${new URLSearchParams({ root: workspace })}`,
    ready: async (page) => {
      await page.getByTestId('scanned').waitFor()
      for (const mark of await page.getByTestId('mark').all()) {
        await mark.click()
      }
    }
  },
  {
    // What linking the two marked runs, shown before it runs.
    name: 'preview',
    ready: async (page) => {
      await page.getByTestId('link-marked').click()
      await page.getByRole('dialog').getByTestId('preview').waitFor()
    }
  },
  {
    // Run, then a line that takes something away, which is a question:
    // asked here, and cancelled below.
    name: 'confirm',
    ready: async (page) => {
      const dialog = page.getByRole('dialog')
      await dialog.getByTestId('run').click()
      await dialog.waitFor({ state: 'hidden' })
      await page.goto(new URL(project, page.url()).href)
      await drawn(page)
      await page.getByTestId('unlink').click()
      await dialog.getByTestId('caution').first().waitFor()
    }
  },
  {
    // What the link ran, each line ok: an image of a failure is no tour.
    name: 'transcript',
    ready: async (page) => {
      const dialog = page.getByRole('dialog')
      await dialog.getByTestId('cancel').click()
      await dialog.waitFor({ state: 'hidden' })
      await page.getByTestId('open-transcript').first().click()
      const transcript = page.getByTestId('transcript')
      await transcript.getByTestId('line').first().waitFor()
      const said = await transcript.getByTestId('ok').allInnerTexts()
      if (said.length === 0 || said.some(word => word.trim() !== 'ok')) {
        throw new Error(`the panel's link did not run cleanly: ${said.join(', ')}`)
      }
    }
  },
  {
    // Removing one of the library's own everywhere, its dry run read
    // first: what goes from each project, the edited copy it leaves, and
    // the library's copy. Never run.
    name: 'remove',
    path: `/artifact?${new URLSearchParams({ kind: 'skill', name: REMOVED })}`,
    ready: async (page) => {
      await page.getByTestId('remove-dry').click()
      const dialog = page.getByRole('dialog')
      await dialog.getByTestId('run').click()
      await dialog.waitFor({ state: 'hidden' })
      await page.getByTestId('remove-said').waitFor()
      // The toast saying it ran would cover what it said: gone first.
      await toastGone(page, 'mindweaver', `remove --everywhere skill:${REMOVED}`)
      await page.getByTestId('remove-yes').scrollIntoViewIfNeeded()
    }
  },
  {
    // The team's repository typed into the form adding a source, and what
    // a gather of it would take, its dry run read first, above the source
    // it is already: never gathered. After the transcript, which the dry
    // run would otherwise join.
    name: 'sources',
    path: '/sources',
    ready: async (page) => {
      const team = join(at.scratch, TEAM)
      await page.getByTestId('source').first().waitFor()
      await page.getByTestId('source-url').fill(team)
      await page.getByTestId('source-dry').click()
      const dialog = page.getByRole('dialog')
      await dialog.getByTestId('run').click()
      await dialog.waitFor({ state: 'hidden' })
      await page.getByTestId('source-said').waitFor()
      // The toast saying it ran would cover what it said: gone first.
      await toastGone(page, 'mindweaver', `gather git --dry-run ${team}`)
    }
  },
  {
    // The platform team's workspace, loaded from the list of those this
    // machine knows: what it offers, and how each it knows stands.
    name: 'loads',
    path: '/loads',
    ready: async (page) => {
      await page.getByRole('button', { name: `Load ${LOADED}` }).click()
      const dialog = page.getByRole('dialog')
      await dialog.getByTestId('run').click()
      await dialog.waitFor({ state: 'hidden' })
      await page.getByTestId('load').getByTestId('offer').first().waitFor()
      // The toast saying it ran would cover what the page shows: gone first.
      await toastGone(page, 'mindweaver', `load ${join(at.scratch, LOADED)}`)
    }
  }
]

/**
 * Open the launch link in a page of its own, in `mode`, and take `shots`
 * of it, each read for a leak before it is written.
 */
async function take(browser: Browser, served: Serving, mode: 'light' | 'dark', shots: Shot[]): Promise<string[]> {
  const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: SCALE, locale: LOCALE, timezoneId: ZONE, colorScheme: mode, reducedMotion: 'reduce' })
  // The panel is dark until somebody picks, and keeps what they picked in
  // a cookie of its own (src/preferences.ts): picked here before it opens.
  await context.addCookies([{ name: 'mindweaver-vueuse-color-scheme', value: mode, url: served.origin }])
  const page = await context.newPage()
  const thrown: string[] = []
  page.on('pageerror', error => thrown.push(error.message))
  const written: string[] = []
  try {
    await page.goto(served.link)
    await page.waitForURL(`${served.origin}/`)
    for (const shot of shots) {
      if (shot.path) {
        await page.goto(new URL(shot.path, served.origin).href)
        await drawn(page)
      }
      await shot.ready(page)
      await page.evaluate(async () => {
        await document.fonts.ready
        await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)))
      })
      // The page as the image will show it, every word on it and every
      // attribute, a placeholder's or a link's included, and what each
      // field holds: neither the markup nor the text carries a value typed
      // or bound into a field, as v-model binds Discover's folders, which
      // start as the workspace's own when the address names none.
      const fields = await page.evaluate(() => [...document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>('input, textarea, select')].map(field => field.value))
      const shown = [page.url(), await page.content(), await page.evaluate(() => document.body.innerText), ...fields].join('\n')
      const found = leaks(shown, served.token)
      if (found.length > 0) {
        throw new Error(`the ${shot.name} page shows ${found.join(', ')}, so it is not written`)
      }
      // A page that threw, could not read the workspace or says it cannot
      // watch it would be an image of something going wrong.
      if (thrown.length > 0) {
        throw new Error(`the ${shot.name} page threw: ${thrown.join('; ')}`)
      }
      for (const trouble of ['unread', 'unwatched']) {
        if (await page.getByTestId(trouble).count() > 0) {
          throw new Error(`the ${shot.name} page shows \`${trouble}\`: ${await page.getByTestId(trouble).first().innerText()}`)
        }
      }
      const file = join(IMAGES, `${shot.name}.png`)
      await page.screenshot({ path: file, animations: 'disabled', caret: 'hide' })
      written.push(file)
    }
  } finally {
    await context.close()
  }
  return written
}

// A run stopped part way, by a Ctrl-C say, leaves its folder: the mark
// says it is one, which the next run may take away.
if (existsSync(at.scratch)) {
  if (!existsSync(join(at.scratch, OURS))) {
    refuse(`${at.scratch} is there already, and not as an earlier run left it: remove it, or move it out of the way`)
  }
  rmSync(at.scratch, { recursive: true, force: true })
}

let status = 1
let served: Serving | undefined
let browser: Browser | undefined
try {
  mkdirSync(at.scratch)
  writeFileSync(join(at.scratch, OURS), '')
  mkdirSync(at.home)
  console.error(`note: the demo workspace is ${at.workspace}, and every command's home ${at.home}`)
  build(at)
  const made = look(at.home)
  served = await serve(at, 'none')
  browser = await chromium.launch()
  mkdirSync(IMAGES, { recursive: true })
  // Dark, the owner's choice for the README, every page of the tour:
  // one mode, so one image a page and no `<picture>` to pick between two.
  const written = await take(browser, served, 'dark', TOUR)
  const heavy = written.filter(file => statSync(file).size > BUDGET)
  for (const file of written) {
    console.error(`wrote ${file.slice(IMAGES.length + 1)}, ${Math.round(statSync(file).size / 1024)} KB`)
  }
  if (heavy.length > 0) {
    throw new Error(`over ${BUDGET / 1024} KB, which regenerating would add to the history each time: ${heavy.join(', ')}`)
  }
  await browser.close()
  await served.stop()
  // As the suite's driver holds its home: nothing the tour ran, nor the
  // server, may have written there since the demo made the default
  // workspace in it.
  const now = look(at.home)
  const changed = now.filter(line => !made.includes(line))
  if (changed.length > 0 || now.length !== made.length) {
    throw new Error(`the temporary home ${at.home} is not as the demo made it, so something the tour ran wrote there: ${changed.join(', ')}`)
  }
  status = 0
} catch (error) {
  console.error(`error: ${error instanceof Error ? error.message : String(error)}`)
} finally {
  await browser?.close()
  await served?.stop()
  rmSync(at.scratch, { recursive: true, force: true })
}
process.exit(status)
