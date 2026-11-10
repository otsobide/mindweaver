<script setup lang="ts">
import { useToast } from '@nuxt/ui/composables'
import { provideRunner, type Say } from '@/api/commands'
import { TransportError, type Ran } from '@/api/transport'
import { useShell } from '@/stores/shell'
import CommandDialog from './CommandDialog.vue'
import TranscriptDrawer from './TranscriptDrawer.vue'
import WarningsDrawer from './WarningsDrawer.vue'

/*
 * The panel's one runner (`src/api/commands.ts`), handed to everything in
 * its slot, which is every page, and what it draws beside them: the task it
 * shows before it runs, a toast for how each ended, what the last run said,
 * and the transcript of every run. Here rather than in the runner, which holds no look: and here
 * rather than in App.vue, inside UApp, whose toasts and their limit it uses.
 */

const toast = useToast()
const shell = useShell()

/** Whether a run said anything worth reading beside its output: a warning, a note or an error. */
function said(ran: Ran): boolean {
  return ran.outcome.runs.some(run => run.stderr.length > 0 || run.error !== null)
}

/**
 * Whether what a run said asks to be read at once: it failed, or a line it
 * said carries a code, which a note or a hint never does.
 */
function asksToBeRead(ran: Ran): boolean {
  return !ran.outcome.ok || ran.outcome.runs.some(run => run.error !== null || run.stderr.some(line => line.code !== null))
}

const transcript = { label: 'Transcript', icon: 'i-lucide-scroll-text', onClick: () => (shell.transcriptOpen = true) }

const say: Say = {
  // What a line printed, unless the page that asked draws it itself, as a
  // search's results are: then only why a line did not run.
  ran(ran, drawn) {
    const failed = ran.outcome.runs.find(run => !run.ok)
    const output = ran.outcome.runs.map(run => (run.error?.message ?? (drawn ? '' : run.stdout)).trim()).filter(Boolean)
    let title = `Ran ${ran.lines.length} commands`
    if (failed) {
      title = `Failed: ${failed.command}`
    } else if (ran.lines.length === 1) {
      title = `Ran ${ran.lines[0]}`
    }
    toast.add({
      title,
      description: output.join('\n') || undefined,
      // What a command printed a line at a time, as the terminal shows it.
      ui: { description: 'whitespace-pre-line' },
      color: failed ? 'error' : 'success',
      icon: failed ? 'i-lucide-circle-x' : 'i-lucide-circle-check',
      actions: [
        ...(said(ran) ? [{ label: 'What it said', icon: 'i-lucide-list-x', onClick: () => (shell.warningsOpen = true) }] : []),
        transcript
      ]
    })
    if (asksToBeRead(ran)) {
      shell.warningsOpen = true
    }
  },

  // A refusal ran nothing, whenever it came; anything else that stopped a
  // run's answer may have come after the run changed something, which the
  // transcript then says.
  refused(error, ran) {
    const nothing = !ran || (error instanceof TransportError && error.status >= 400 && error.status < 500)
    const message = error instanceof Error ? error.message : String(error)
    const code = error instanceof TransportError ? error.code : undefined
    toast.add({
      title: nothing ? 'Nothing ran' : 'The run did not answer',
      description: code ? `${message} (${code})` : message,
      color: 'error',
      icon: nothing ? 'i-lucide-ban' : 'i-lucide-circle-x',
      actions: nothing ? undefined : [transcript]
    })
  }
}

provideRunner(say)
</script>

<template>
  <slot />
  <CommandDialog />
  <WarningsDrawer />
  <TranscriptDrawer />
</template>
