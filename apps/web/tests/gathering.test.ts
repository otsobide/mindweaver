import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, type VueWrapper } from '@vue/test-utils'
import type { Task } from '@/api/transport'
import { useShell } from '@/stores/shell'
import { instant, when } from '@/time'
import { commanding, fixture, gathered, gatheringSteps, render, texts, unmount, type Commanded } from './panel'

afterEach(unmount)

/*
 * The page of sources changing what it lists, drawn on a workspace whose
 * shelf is filled from repositories beside it (tests/fixtures/sources.json),
 * each task sent and answered as the CLI's backend answered it on that
 * workspace (gathered.json), and each step a gather reports told on the
 * stream as the server tells it (gathering.json).
 */

const [DRY, GATHER, STOPPED, AGAIN, EVERY, RENAMED, TAKEN, LOOKED, FORGOT, HALTED] = gathered() as [
  Commanded, Commanded, Commanded, Commanded, Commanded, Commanded, Commanded, Commanded, Commanded, Commanded
]
const [CONNECTING, RECEIVING, RESOLVING, , , PLACING] = gatheringSteps()

/** The repository `DRY` and `GATHER` add, as the form is typed. */
const TOOLS = DRY.task.type === 'gather' ? DRY.task.url : ''

/** What the dialog shows: each line, and each caution with its words. */
function shown() {
  return {
    lines: [...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent),
    cautions: [...document.body.querySelectorAll('[data-testid="caution"]')].map(each => each.textContent?.replace(/\s+/g, ' ').trim())
  }
}

function clicked(testid: string) {
  document.body.querySelector<HTMLElement>(`[data-testid="${testid}"]`)!.click()
}

/** What the dialog says of the run under way, its bar among it, or nothing while none is followed. */
function following() {
  const underway = document.body.querySelector('[data-testid="underway"]')
  const bar = underway?.querySelector('progress')
  return underway
    ? {
        step: underway.querySelector('[data-testid="step"]')?.textContent?.trim(),
        stop: underway.querySelector('[data-testid="stop-state"]')?.textContent?.trim(),
        value: bar?.getAttribute('value') ?? null,
        max: bar?.getAttribute('max') ?? null,
        stoppable: !document.body.querySelector('[data-testid="stop"]')?.hasAttribute('disabled')
      }
    : undefined
}

/** `task`'s run as the fixture holds it: what its one line printed. */
function printed(commanded: Commanded): string {
  return commanded.ran.outcome.runs[0]!.stdout
}

/** Type the repository `DRY` names into the form, and ask what it would take. */
async function lookAtTools(page: VueWrapper) {
  await page.get('input[data-testid="source-url"]').setValue(TOOLS)
  await page.get('[data-testid="add-source"]').trigger('submit')
  await flushPromises()
}

describe('the sources', () => {
  it('lists each source by its name, with its pin, its folder and the licences what it supplied says, as the snapshot counts them', async () => {
    const snapshot = fixture('sources')
    const [skills, rules] = snapshot.sources

    const { page } = await render('/sources', snapshot)

    const [first, second] = page.findAll('[data-testid="source"]')
    expect(texts(page, '[data-testid="source"] [data-testid="name"]')).toEqual(snapshot.sources.map(source => source.name))
    expect(first!.get('[data-testid="url"]').text()).toBe(skills!.url)
    expect(first!.find('[data-testid="pinned"]').exists()).toBe(false)
    expect(first!.get('[data-testid="pin"]').text()).toBe('none: it takes whatever its branch is at')
    expect(texts(first!, '[data-testid="licence-said"]')).toEqual(skills!.licences.map(each => each.licence))
    expect(texts(first!, '[data-testid="licence-count"]')).toEqual(skills!.licences.map(each => String(each.count)))
    expect(first!.get('[data-testid="gathered"] time').attributes('datetime')).toBe(instant(skills!.gathered_at!))
    expect(first!.get('[data-testid="gathered"]').text()).toBe(when(skills!.gathered_at!))

    expect(second!.get('[data-testid="pinned"]').text()).toBe('pinned')
    expect(second!.get('[data-testid="pin"] code').text()).toBe(rules!.pin!.slice(0, 7))
    expect(second!.get('[data-testid="pin"] code').attributes('title')).toBe(rules!.pin)
    expect(second!.get('[data-testid="subdirectory"]').text()).toBe(rules!.subdirectory)
    expect(texts(second!, '[data-testid="licence-said"]')).toEqual(['MIT License'])
    expect(texts(second!, '[data-testid="shelved"]')).toEqual(['rule:git/commits', 'rule:style/comments'])
  })

  it('says a source whose shelf says no licence says none, and one with nothing on the shelf was never gathered there', async () => {
    const snapshot = fixture('sources')
    Object.assign(snapshot.sources[0]!, { licences: [], gathered_at: null })

    const { page } = await render('/sources', snapshot)

    const [first] = page.findAll('[data-testid="source"]')
    expect(first!.get('[data-testid="licences"]').text()).toBe('none recorded: nothing on the shelf from it says one yet')
    expect(first!.get('[data-testid="gathered"]').text()).toBe('never put anything on the shelf')
  })
})

describe('adding a source', () => {
  it('offers to take each kind a gather takes, as core lists them, and nothing more', async () => {
    const snapshot = fixture('sources')
    expect(snapshot.gatherable.map(each => each.kind)).toEqual(['skill', 'rule'])

    const { page } = await render('/sources', snapshot)

    const offered = [...document.body.querySelectorAll('[data-testid="source-kind"] [role="radio"]')]
    expect(offered).toHaveLength(snapshot.gatherable.length)
    expect(texts(page, '[data-testid="source-kind"] label')).toEqual(snapshot.gatherable.map(each => each.kind))
    expect(offered.map(each => each.getAttribute('aria-checked'))).toEqual(['true', 'false'])
  })

  it('names a folder for a kind core looks for in none unsaid before it asks anything', async () => {
    const { page } = await render('/sources', fixture('sources'))

    await page.get('input[data-testid="source-url"]').setValue(TOOLS)
    expect(page.get('[data-testid="source-dry"]').attributes('disabled')).toBeUndefined()
    document.body.querySelectorAll<HTMLElement>('[data-testid="source-kind"] [role="radio"]')[1]!.click()
    await flushPromises()

    expect(page.get('[data-testid="source-dry"]').attributes('disabled')).toBeDefined()
    await page.get('input[data-testid="source-folder"]').setValue('rules')
    expect(page.get('[data-testid="source-dry"]').attributes('disabled')).toBeUndefined()
  })

  it('says what a gather would take first, then gathers exactly that, and says what it did once it is over', async () => {
    const { sent, transport } = commanding(fixture('sources'), [DRY, GATHER])
    const { page } = await render('/sources', transport)
    expect(page.find('[data-testid="source-gather"]').exists()).toBe(false)

    await lookAtTools(page)

    expect(sent).toEqual([{ task: DRY.task, mode: 'preview' }])
    expect(shown()).toEqual({ lines: DRY.preview.lines.map(line => `$ ${line}`), cautions: [] })
    clicked('run')
    await flushPromises()

    expect(page.get('[data-testid="source-said"]').text()).toBe(printed(DRY).trim())
    expect(printed(DRY)).toContain('would add')
    await page.get('[data-testid="source-gather"]').trigger('click')
    await flushPromises()
    expect(sent.at(-1)).toEqual({ task: GATHER.task, mode: 'preview' })
    expect({ ...GATHER.task, dry_run: true }).toEqual(DRY.task)
    clicked('run')
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run').map(each => each.task)).toEqual([DRY.task, GATHER.task])
    expect(page.find('[data-testid="source-said"]').exists()).toBe(false)
    const summary = page.get('[data-testid="summary"]')
    expect(summary.get('h2').text()).toBe('What the gather said')
    expect(summary.get('[data-testid="summary-command"]').text()).toBe(`$ ${GATHER.ran.lines[0]}`)
    expect(summary.get('[data-testid="summary-output"]').text()).toBe(printed(GATHER).trim())
    expect(summary.find('[data-testid="summary-error"]').exists()).toBe(false)

    await page.get('[data-testid="summary-dismiss"]').trigger('click')
    expect(page.find('[data-testid="summary"]').exists()).toBe(false)
  })

  it('names the source and pins it as typed, the white space around each left out, and an empty field not at all', async () => {
    const { sent, transport } = commanding(fixture('sources'), [])
    const { page } = await render('/sources', transport)
    const pin = '4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3'

    await page.get('input[data-testid="source-url"]').setValue(`  ${TOOLS} `)
    await page.get('input[data-testid="source-name"]').setValue(' tools ')
    await page.get('input[data-testid="source-pin"]').setValue(pin)
    await page.get('input[data-testid="source-reference"]').setValue('   ')
    await page.get('[data-testid="add-source"]').trigger('submit')
    await flushPromises()

    const asked: Task = { ...(DRY.task as Task & { type: 'gather' }), name: 'tools', pin }
    expect(sent).toEqual([{ task: asked, mode: 'preview' }])
  })

  it('offers no gather once the fields say another than the one its dry run was of', async () => {
    const { transport } = commanding(fixture('sources'), [DRY, GATHER])
    const { page } = await render('/sources', transport)
    await lookAtTools(page)
    clicked('run')
    await flushPromises()
    expect(page.find('[data-testid="source-gather"]').exists()).toBe(true)

    await page.get('input[data-testid="source-reference"]').setValue('main')

    expect(page.find('[data-testid="source-said"]').exists()).toBe(false)
    expect(page.find('[data-testid="source-gather"]').exists()).toBe(false)
    await page.get('input[data-testid="source-reference"]').setValue('')
    expect(page.find('[data-testid="source-gather"]').exists()).toBe(true)
  })

  it('draws a gather\'s progress as a bar of each step\'s counts, stops it when asked, and says it stopped', async () => {
    const { sent, cancels, control, tell, transport } = commanding(fixture('sources'), [DRY, STOPPED])
    const { page } = await render('/sources', transport)
    await lookAtTools(page)
    clicked('run')
    await flushPromises()
    let finish!: () => void
    control.finishing = new Promise<void>((resolve) => {
      finish = resolve
    })
    await page.get('[data-testid="source-gather"]').trigger('click')
    await flushPromises()
    expect(STOPPED.preview.cancellable).toBe(true)
    clicked('run')
    await flushPromises()

    // Told started, nothing counted yet: a bar of no value, as of a step
    // that says no count.
    expect(following()).toEqual({ step: 'starting', stop: 'It can be stopped until it starts to change anything.', value: null, max: null, stoppable: true })
    tell({ type: 'progress', run: 2, progress: CONNECTING! })
    await flushPromises()
    expect(following()).toMatchObject({ step: CONNECTING!.said, value: null, max: null })
    tell({ type: 'progress', run: 2, progress: RECEIVING! })
    await flushPromises()
    expect(following()).toMatchObject({ step: RECEIVING!.said, value: String(RECEIVING!.done), max: String(RECEIVING!.total), stoppable: true })
    // A count with no total yet draws no value it could not place.
    tell({ type: 'progress', run: 2, progress: RESOLVING! })
    await flushPromises()
    expect(RESOLVING!.total).toBeNull()
    expect(following()).toMatchObject({ step: RESOLVING!.said, value: null, max: null })

    clicked('stop')
    await flushPromises()
    expect(cancels).toEqual([2])
    expect(following()).toMatchObject({ stop: 'Asked to stop: it stops before it changes anything, and says so when it is over.', stoppable: false })

    finish()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run').map(each => each.task)).toEqual([DRY.task, STOPPED.task])
    const error = STOPPED.ran.outcome.runs[0]!.error!
    expect(error.code).toBe('gather_cancelled')
    const summary = page.get('[data-testid="summary"]')
    expect(summary.get('[data-testid="summary-error"]').text().replace(/\s+/g, ' ')).toBe(`${error.code}: ${error.message}`)
    expect(summary.find('[data-testid="summary-output"]').exists()).toBe(false)
    await vi.waitFor(() => expect(useShell().warningsOpen).toBe(true))
  })

  it('offers no stop once a gather places what it read, and says why in words', async () => {
    const { cancels, control, tell, transport } = commanding(fixture('sources'), [DRY, GATHER])
    const { page } = await render('/sources', transport)
    await lookAtTools(page)
    clicked('run')
    await flushPromises()
    control.finishing = new Promise<void>(() => {})
    await page.get('[data-testid="source-gather"]').trigger('click')
    await flushPromises()
    clicked('run')
    await flushPromises()

    tell({ type: 'progress', run: 2, progress: PLACING! })
    await flushPromises()

    expect(PLACING!.cancellable).toBe(false)
    expect(following()).toEqual({
      step: PLACING!.said,
      stop: 'Too late to stop: it finishes what it started.',
      value: String(PLACING!.done),
      max: String(PLACING!.total),
      stoppable: false
    })
    clicked('stop')
    await flushPromises()
    expect(cancels).toEqual([])
  })
})

describe('changing a source', () => {
  it('gathers one again by its name, and every one, each followed and said once it is over', async () => {
    const { sent, transport } = commanding(fixture('sources'), [AGAIN, EVERY])
    const { page } = await render('/sources', transport)

    await page.findAll('[data-testid="source"]')[0]!.get('[data-testid="again"]').trigger('click')
    await flushPromises()
    expect(sent).toEqual([{ task: AGAIN.task, mode: 'preview' }])
    expect(AGAIN.task).toEqual({ type: 'update_sources', names: ['src-acme-skills'] })
    expect(document.body.querySelector('[data-testid="underway"]')).toBeNull()
    clicked('run')
    await flushPromises()
    expect(page.get('[data-testid="summary"] h2').text()).toBe('What gathering src-acme-skills again said')
    expect(page.get('[data-testid="summary-output"]').text()).toBe(printed(AGAIN).trim())

    await page.get('[data-testid="again-all"]').trigger('click')
    await flushPromises()
    expect(sent.at(-1)).toEqual({ task: EVERY.task, mode: 'preview' })
    expect(EVERY.preview.cancellable).toBe(true)
    clicked('run')
    await flushPromises()
    expect(page.get('[data-testid="summary"] h2').text()).toBe('What gathering every source again said')
    expect(page.get('[data-testid="summary-output"]').text()).toBe(printed(EVERY).trim())
  })

  it('says a source a gather again stopped at by the gather\'s code, beside what else it said', async () => {
    const { tell, control, transport } = commanding(fixture('sources'), [HALTED])
    let finish!: () => void
    control.finishing = new Promise<void>((resolve) => {
      finish = resolve
    })
    const { page } = await render('/sources', transport)

    await page.findAll('[data-testid="source"]')[0]!.get('[data-testid="again"]').trigger('click')
    await flushPromises()
    clicked('run')
    await flushPromises()
    tell({ type: 'progress', run: 1, progress: CONNECTING! })
    await flushPromises()
    clicked('stop')
    finish()
    await flushPromises()

    const said = HALTED.ran.outcome.runs[0]!.stderr
    expect(said.map(line => line.code)).toContain('gather_cancelled')
    const summary = page.get('[data-testid="summary"]')
    expect(texts(summary, '[data-testid="summary-line"]')).toEqual(said.map(line => (line.code === null ? line.text : `${line.code}: ${line.text}`)))
    expect(summary.find('[data-testid="summary-error"]').exists()).toBe(false)
  })

  it('renames one as typed, and keeps the field open when the line refuses the name, saying why by its code', async () => {
    const { sent, transport } = commanding(fixture('sources'), [RENAMED, TAKEN])
    const { page } = await render('/sources', transport)
    const card = () => page.findAll('[data-testid="source"]')[0]!
    expect(card().find('[data-testid="rename"]').exists()).toBe(false)

    await card().get('[data-testid="rename-open"]').trigger('click')
    expect(card().get('[data-testid="rename-open"]').attributes('aria-expanded')).toBe('true')
    expect((card().get('input[data-testid="rename-to"]').element as HTMLInputElement).value).toBe('src-acme-skills')
    expect(card().get('[data-testid="rename-run"]').attributes('disabled')).toBeDefined()
    await card().get('input[data-testid="rename-to"]').setValue('team-rules')
    await card().get('[data-testid="rename"]').trigger('submit')
    await flushPromises()
    expect(sent).toEqual([{ task: TAKEN.task, mode: 'preview' }])
    clicked('run')
    await flushPromises()

    expect(TAKEN.ran.outcome.runs[0]!.error!.code).toBe('source_name_taken')
    expect(card().find('[data-testid="rename"]').exists()).toBe(true)
    await vi.waitFor(() => expect(useShell().warningsOpen).toBe(true))
    useShell().warningsOpen = false
    await flushPromises()

    await card().get('input[data-testid="rename-to"]').setValue(' acme-skills ')
    await card().get('[data-testid="rename"]').trigger('submit')
    await flushPromises()
    expect(sent.at(-1)).toEqual({ task: RENAMED.task, mode: 'preview' })
    clicked('run')
    await flushPromises()
    expect(card().find('[data-testid="rename"]').exists()).toBe(false)
  })

  it('says what forgetting one does first, then forgets it once that is read, confirmed for what it takes away', async () => {
    const { sent, transport } = commanding(fixture('sources'), [LOOKED, FORGOT])
    const { page } = await render('/sources', transport)
    const card = () => page.findAll('[data-testid="source"]')[1]!
    expect(card().find('[data-testid="forget-yes"]').exists()).toBe(false)

    await card().get('[data-testid="forget-dry"]').trigger('click')
    await flushPromises()
    expect(sent).toEqual([{ task: LOOKED.task, mode: 'preview' }])
    expect(shown().cautions).toEqual([])
    clicked('run')
    await flushPromises()
    expect(card().get('[data-testid="forget-said"]').text()).toBe(printed(LOOKED).trim())

    await card().get('[data-testid="forget-yes"]').trigger('click')
    await flushPromises()
    expect(sent.at(-1)).toEqual({ task: FORGOT.task, mode: 'preview' })
    expect(shown()).toEqual({
      lines: FORGOT.preview.lines.map(line => `$ ${line}`),
      cautions: FORGOT.preview.confirm.map(each => `${each.caution}: ${each.message}`)
    })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('cancel')
    clicked('run')
    await flushPromises()

    expect(page.get('[data-testid="summary"] h2').text()).toBe('What forgetting team-rules said')
    expect(page.get('[data-testid="summary-output"]').text()).toBe(printed(FORGOT).trim())
  })
})
