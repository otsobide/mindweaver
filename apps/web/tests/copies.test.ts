import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { useShell } from '@/stores/shell'
import { commanding, fixture, kept, render, steps, unmount, type Commanded } from './panel'

afterEach(unmount)

/*
 * The library's forms for a copy of it, an export into a folder and a
 * backup to a git repository, drawn on the workspace holding something of
 * everything (tests/fixtures/workspace.json), each task sent and answered
 * as the CLI's backend answered it on that workspace (kept.json), and a
 * backup's steps as the backend tells them (steps.json).
 */

const [EVERY, REPLACE, TOKEN, STOPPED] = kept() as [Commanded, Commanded, Commanded, Commanded]
const [CHECKING, EXPORTING, , PUSHING] = steps()

/** A checkbox of the forms, by its test id. */
function box(testid: string): HTMLElement {
  return document.body.querySelector<HTMLElement>(`[data-testid="${testid}"] [role="checkbox"], [role="checkbox"][data-testid="${testid}"]`)!
}

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

/** What the dialog says of the run under way, or nothing while none is followed. */
function following() {
  const underway = document.body.querySelector('[data-testid="underway"]')
  return underway
    ? {
        step: underway.querySelector('[data-testid="step"]')?.textContent?.trim(),
        stop: underway.querySelector('[data-testid="stop-state"]')?.textContent?.trim(),
        stoppable: !document.body.querySelector('[data-testid="stop"]')?.hasAttribute('disabled')
      }
    : undefined
}

describe('a copy of the library', () => {
  it('offers an export and a backup on the library\'s page, a tick for each kind core lists, in its order', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/library', snapshot)

    const copies = page.get('[data-testid="copies"]')
    for (const form of ['export', 'backup']) {
      expect(copies.find(`[data-testid="${form}-form"]`).exists()).toBe(true)
      expect(snapshot.kinds.map(kind => box(`${form}-kind-${kind}`).getAttribute('aria-checked'))).toEqual(snapshot.kinds.map(() => 'false'))
    }
    expect(page.get('[data-testid="export"]').attributes('disabled')).toBeDefined()
    expect(page.get('[data-testid="backup"]').attributes('disabled')).toBeDefined()
  })

  it('exports into the folder as typed, naming no kind when none is ticked, the line shown before it runs', async () => {
    const { sent, transport } = commanding(fixture('workspace'), kept())
    const { page } = await render('/library', transport)
    const folder = EVERY.task.type === 'export' ? EVERY.task.folder : ''

    await page.get('input[data-testid="export-folder"]').setValue(folder)
    await page.get('[data-testid="export-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: EVERY.task, mode: 'preview' }])
    expect(shown()).toEqual({ lines: EVERY.preview.lines.map(line => `$ ${line}`), cautions: [] })
    // It takes no cancel: nothing is said of a run under way.
    expect(document.body.querySelector('[data-testid="stop"]')).toBeNull()

    clicked('run')
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: EVERY.task, mode: 'run' }])
    expect(following()).toBeUndefined()
  })

  it('asks to confirm an export told to replace, for what the backend says it takes away, the kinds ticked in core\'s order', async () => {
    const { sent, transport } = commanding(fixture('workspace'), kept())
    const { page } = await render('/library', transport)
    const folder = REPLACE.task.type === 'export' ? REPLACE.task.folder : ''

    await page.get('input[data-testid="export-folder"]').setValue(folder)
    box('export-kind-skill').click()
    box('export-replace').click()
    await flushPromises()
    await page.get('[data-testid="export-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: REPLACE.task, mode: 'preview' }])
    expect(shown()).toEqual({
      lines: REPLACE.preview.lines.map(line => `$ ${line}`),
      cautions: REPLACE.preview.confirm.map(each => `${each.caution}: ${each.message}`)
    })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('cancel')
  })

  it('backs up naming a token by the variable that holds it, leaving out what is left empty', async () => {
    const { sent, transport } = commanding(fixture('workspace'), kept())
    const { page } = await render('/library', transport)
    expect(TOKEN.task).toMatchObject({ type: 'backup', token_env: 'GITHUB_TOKEN', branch: null, message: null })
    const url = TOKEN.task.type === 'backup' ? TOKEN.task.url : ''

    await page.get('input[data-testid="backup-url"]').setValue(url)
    await page.get('input[data-testid="backup-branch"]').setValue('  ')
    await page.get('input[data-testid="backup-token-env"]').setValue('GITHUB_TOKEN')
    await page.get('[data-testid="backup-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: TOKEN.task, mode: 'preview' }])
    expect(shown()).toEqual({ lines: TOKEN.preview.lines.map(line => `$ ${line}`), cautions: [] })
    expect(TOKEN.preview.cancellable).toBe(true)
  })

  it('follows a backup step by step and stops it when asked, until it says it is too late', async () => {
    const { sent, cancels, control, tell, transport } = commanding(fixture('workspace'), kept())
    let finish!: () => void
    control.finishing = new Promise<void>((resolve) => {
      finish = resolve
    })
    const { page } = await render('/library', transport)
    const url = STOPPED.task.type === 'backup' ? STOPPED.task.url : ''
    await page.get('input[data-testid="backup-url"]').setValue(url)
    await page.get('[data-testid="backup-form"]').trigger('submit')
    await flushPromises()
    expect(sent).toEqual([{ task: STOPPED.task, mode: 'preview' }])

    clicked('run')
    await flushPromises()

    // Told started, and nothing reported yet.
    expect(following()).toEqual({
      step: 'starting',
      stop: 'It can be stopped until it starts to change anything.',
      stoppable: true
    })
    tell({ type: 'progress', run: 1, progress: CHECKING! })
    tell({ type: 'progress', run: 1, progress: EXPORTING! })
    await flushPromises()
    expect(following()).toMatchObject({ step: EXPORTING!.said, stoppable: true })

    clicked('stop')
    await flushPromises()

    expect(cancels).toEqual([1])
    expect(following()).toEqual({
      step: EXPORTING!.said,
      stop: 'Asked to stop: it stops before it changes anything, and says so when it is over.',
      stoppable: false
    })
    // Asked once, however often pressed.
    clicked('stop')
    await flushPromises()
    expect(cancels).toEqual([1])

    finish()
    await flushPromises()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: STOPPED.task, mode: 'run' }])
    const error = STOPPED.ran.outcome.runs[0]!.error!
    expect(error.code).toBe('backup_cancelled')
    await vi.waitFor(() => expect(useShell().warningsOpen).toBe(true))
    expect(document.body.querySelector('[data-testid="said"]')?.textContent).toContain(error.code)
  })

  it('offers no stop once a backup pushes, and says why in words', async () => {
    const { cancels, control, tell, transport } = commanding(fixture('workspace'), kept())
    control.finishing = new Promise<void>(() => {})
    const { page } = await render('/library', transport)
    const url = STOPPED.task.type === 'backup' ? STOPPED.task.url : ''
    await page.get('input[data-testid="backup-url"]').setValue(url)
    await page.get('[data-testid="backup-form"]').trigger('submit')
    await flushPromises()
    clicked('run')
    await flushPromises()

    tell({ type: 'progress', run: 1, progress: PUSHING! })
    await flushPromises()

    expect(PUSHING!.cancellable).toBe(false)
    expect(following()).toEqual({
      step: PUSHING!.said,
      stop: 'Too late to stop: it finishes what it started.',
      stoppable: false
    })
    clicked('stop')
    await flushPromises()
    expect(cancels).toEqual([])
  })
})
