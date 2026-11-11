import { LIBRARY, PROJECTS } from './fixture.ts'
import { drawn, expect, test } from './panel.ts'

/*
 * The CSP the server sends (`default-src 'self'`) against the panel as a
 * browser draws it: no page of it may need what the policy refuses, a font
 * or a style from elsewhere, an inline style, a `data:` URL. The build is
 * checked for each of them (tests/dist.test.ts), and the libraries' run-time
 * habits in happy-dom (tests/shell.test.ts); this is the one place a real
 * engine loads the page under the real header. Every test fails on a
 * violation (panel.ts); these visit every page to have none.
 */
const PAGES = [
  '/',
  '/projects',
  ...PROJECTS.map(entry => `/project?${new URLSearchParams({ entry })}`),
  '/discover',
  '/assign',
  '/assign?by=project',
  '/updates',
  '/library',
  '/library?kind=rule',
  ...LIBRARY.map(([kind, name]) => `/artifact?${new URLSearchParams({ kind, name })}`),
  '/new?kind=skill',
  '/mcp',
  '/loads',
  '/shelf',
  '/sources',
  '/catalogs',
  '/nowhere'
]

for (const path of PAGES) {
  test(`${path} loads without a CSP violation`, async ({ panel, watch }) => {
    await panel.open(path)
    expect(watch.violations).toEqual([])
  })
}

// The editor, CodeMirror's styles adopted into its shadow root as a
// constructed sheet, never a <style> element. Drawn only once its file is
// read, after the snapshot a page is drawn by, so each waits for it.
for (const [kind, name] of LIBRARY) {
  const path = `/edit?${new URLSearchParams({ kind, name })}`
  test(`${path} draws its editor without a CSP violation`, async ({ page, panel, watch }) => {
    await panel.open(path)
    await expect(page.getByRole('textbox', { name: new RegExp(`^The text of ${name}'s `) })).toBeVisible()
    await drawn(page)
    expect(watch.violations).toEqual([])
  })
}

test('switching the colour mode adds nothing the CSP refuses', async ({ page, panel, watch }) => {
  await panel.open('/')
  await page.getByTestId('overview').getByRole('button', { name: 'Switch to light mode' }).click()
  await expect(page.locator('html')).toHaveClass(/\blight\b/)
  await page.getByTestId('overview').getByRole('button', { name: 'Switch to dark mode' }).click()
  await expect(page.locator('html')).toHaveClass(/\bdark\b/)
  await drawn(page)
  expect(watch.violations).toEqual([])
})

// The watch itself, which would pass every test above if it saw nothing: a
// style written into the page, as Nuxt UI's colours plugin would write one
// were it not stubbed out, is refused, and the refusal is seen.
test('a violation is seen when a page has one', async ({ page, panel, watch }) => {
  await panel.open('/')
  await page.evaluate(() => {
    const style = document.createElement('style')
    style.textContent = 'body { outline: 1px solid red }'
    document.head.append(style)
  })
  await expect.poll(() => watch.violations.length).toBeGreaterThan(0)
  // Seen, so it fails nothing.
  watch.violations.length = 0
})
