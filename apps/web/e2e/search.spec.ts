import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { SERVER, SKILL } from './catalogs.ts'
import { run, succeed, succeedApart } from './environment.ts'
import { HOUSE_RULES, SKILL_FILE } from './fixture.ts'
import { expect, test, tree } from './panel.ts'

/*
 * The page of catalogs, searching the skill directories and the MCP
 * Registry, held to the disk and to the CLI: a workspace of the test's own
 * (`searchable`), whose `weaver.toml` lists skills.sh, served by a
 * `mindweaver ui` launched asking skills.sh and the registry on loopback,
 * which its builder starts: nothing reaches the network. Each line the
 * dialog shows is the one the CLI runs; the twin, a copy made beforehand,
 * is given the same lines on the command line, and holds what the panel
 * left.
 */

test.use({ building: 'searchable' })

test('a skill a search of the directories finds is taken onto the shelf by the line gather search --take amounts to', async ({ page, own }) => {
  const workspace = own.at.workspace
  const repository = join(dirname(workspace), HOUSE_RULES)
  const options = own.launched!.options
  const finder = options[options.indexOf('--finder-url') + 1]!
  await own.open('/catalogs')

  // In the section's form, by its name, and its label inside it.
  const form = page.getByRole('form', { name: 'Search the skill directories' })
  await form.getByLabel('Skills about').fill('commit')
  await form.getByRole('button', { name: 'Search' }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([`$ mindweaver -W ${workspace} gather search --finder-url ${finder} --json commit`])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  const hit = page.getByTestId('hit').filter({ hasText: SKILL })
  await expect(hit.getByTestId('hit-repository')).toHaveText(repository)
  await hit.getByRole('button', { name: `Take ${SKILL} onto the shelf` }).click()
  const take = `mindweaver -W ${workspace} gather git ${repository}`
  await expect(dialog.getByTestId('command')).toHaveText([`$ ${take}`])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()
  await expect(hit.getByTestId('taken')).toContainText('see it on the shelf')

  // On the shelf, as the CLI lists it, and nothing asked but the
  // directory on loopback, the repository read from this disk.
  const listed = run(own.at, 'mindweaver', workspace, ['gather', 'list', '--json'])
  expect(listed.status, listed.stderr).toBe(0)
  expect(listed.stdout).toContain(`"${SKILL}"`)
  expect(own.launched!.asked()).toEqual(['/api/search'])
  await page.getByRole('link', { name: 'see it on the shelf' }).click()
  await expect(page.getByTestId('shelf')).toContainText(SKILL)

  // The twin given the same line holds the same shelf, the skill as its
  // repository holds it.
  const twin = own.twin
  succeed(twin, 'mindweaver', twin.workspace, ['gather', 'git', repository])
  const shelf = (root: string) => tree(join(root, '.mindweaver', 'skills'))
  expect(shelf(workspace)).toEqual(shelf(twin.workspace))
  expect(Object.entries(shelf(workspace)).find(([route]) => route.endsWith(`${SKILL}/SKILL.md`))?.[1]).toBe(SKILL_FILE)
})

test('a server the registry lists is added to the library from a search, once its dry run was read', async ({ page, own }) => {
  const workspace = own.at.workspace
  const options = own.launched!.options
  const registry = options[options.indexOf('--registry') + 1]!
  const definition = join(workspace, '.mindweaver', 'library', 'mcp', `${SERVER.definition}.toml`)
  await own.open('/catalogs')

  const form = page.getByRole('form', { name: 'Search the MCP Registry' })
  await form.getByLabel('Servers named').fill('clawfetch')
  await form.getByRole('button', { name: 'Search' }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByTestId('command')).toHaveText([`$ mindweaver -W ${workspace} gather mcp search --registry ${registry} --json clawfetch`])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  const listed = page.getByTestId('listed-server').filter({ hasText: SERVER.name })
  await expect(listed.getByTestId('server-version')).toHaveText(SERVER.version)
  await listed.getByRole('button', { name: `See what adding ${SERVER.name} writes` }).click()
  const dry = `mindweaver -W ${workspace} gather mcp add --registry ${registry} --version ${SERVER.version} ${SERVER.name}`
  await expect(dialog.getByTestId('command')).toHaveText([`$ ${dry}`])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  // What an agent would run, said before anything is written.
  await expect(listed.getByTestId('add-said')).toContainText(`runs: npx -y @clawfetch/mcp@${SERVER.version}`)
  await expect(listed.getByTestId('add-said')).toContainText('nothing written: add --yes to go ahead')
  expect(existsSync(definition), 'the dry run wrote the definition').toBe(false)

  await listed.getByRole('button', { name: `Add ${SERVER.name} to the library` }).click()
  const yes = `mindweaver -W ${workspace} gather mcp add --registry ${registry} --version ${SERVER.version} --yes ${SERVER.name}`
  await expect(dialog.getByTestId('command')).toHaveText([`$ ${yes}`])
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()
  await expect(listed.getByTestId('added')).toContainText('see it among the MCP servers')

  // In the library, which the page of MCP servers draws, and written as
  // the same line writes it into the twin, but for when it was fetched.
  await page.getByRole('link', { name: 'see it among the MCP servers' }).click()
  await expect(page.getByTestId('server').filter({ has: page.getByRole('heading', { name: SERVER.definition }) })).toBeVisible()
  // Run while this process goes on answering for the registry on loopback.
  const twin = own.twin
  await succeedApart(twin, 'mindweaver', twin.workspace, ['gather', 'mcp', 'add', '--registry', registry, '--version', SERVER.version, '--yes', SERVER.name])
  const written = (path: string) => readFileSync(path, 'utf8').replace(/^fetched_at = .*$/m, '')
  expect(written(definition)).toEqual(written(join(twin.workspace, '.mindweaver', 'library', 'mcp', `${SERVER.definition}.toml`)))
  expect(own.launched!.asked().every(path => path.startsWith('/v0.1/'))).toBe(true)
})
