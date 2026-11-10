<script setup lang="ts">
import { computed, ref, useTemplateRef, watch, type ComponentPublicInstance } from 'vue'
import type { Caution } from '@/api/generated/Caution'
import type { Confirm } from '@/api/generated/Confirm'
import { useRunner, type Asking } from '@/api/commands'

const runner = useRunner()
const { asking, running, underway } = runner

// What is drawn: the task asked about, kept as it was while the dialog
// closes, so its lines do not empty out as it fades.
const shown = ref<Asking>()
watch(asking, (value) => {
  if (value) {
    shown.value = value
  }
}, { immediate: true })

const confirming = computed(() => (shown.value?.preview.confirm.length ?? 0) > 0)

// Closed by Escape, the overlay or its close button, which is a no; never
// while the run is under way, which nobody can take back.
const open = computed({
  get: () => asking.value !== undefined,
  set: (value: boolean) => {
    if (!value) {
      runner.cancel()
    }
  }
})

/** Each line the server previewed, with what the backend says it takes away. */
interface Line {
  line: string
  cautions: Confirm[]
}

const lines = computed<Line[]>(() => {
  const preview = shown.value?.preview
  if (!preview) {
    return []
  }
  return preview.lines.map((line, index) => ({ line, cautions: preview.confirm.filter(each => each.line === index) }))
})

const title = computed(() => {
  if (confirming.value) {
    return 'Confirm what this takes away'
  }
  return lines.value.length === 1 ? 'Run this command?' : `Run these ${lines.value.length} commands?`
})

const description = computed(() =>
  confirming.value
    ? 'Read what each line takes away: nothing runs until you confirm.'
    : 'The panel runs exactly these lines, as you could type them in the workspace\'s folder.'
)

// How each caution looks, and nothing it means: the word and its words are
// the backend's, both always said, so the icon only repeats them. Partial,
// with a plain look for the rest, so a caution the CLI adds is drawn by its
// words before anybody gives it an icon of its own.
const ICONS: Partial<Record<Caution, string>> = {
  removes: 'i-lucide-trash',
  uninstalls: 'i-lucide-package-minus',
  unlinks: 'i-lucide-unlink',
  unloads: 'i-lucide-unplug',
  forces: 'i-lucide-pencil-off'
}

const PLAIN = 'i-lucide-triangle-alert'

// While a run that takes a cancel is under way, a gather or a backup: what
// it is doing, in the backend's words, and whether a stop still stops it,
// as the backend says of each step. Drawn from what the stream told, so a
// step comes in as the run reports it.
const following = computed(() => running.value && (shown.value?.preview.cancellable ?? false))

/** Whether a stop now still stops it: asked for once, and never past its last check. */
const stoppable = computed(() => {
  const now = underway.value
  return now !== undefined && now.cancellable && !now.cancelling && now.step?.cancellable !== false
})

/**
 * How far the step under way has got, when it counts what it does and has
 * said of how much: what the bar draws, as the step's own numbers. Otherwise
 * none, and the bar says only that something is under way, as a step that
 * counts nothing, connecting, or does not know its total yet, would.
 */
const counted = computed(() => {
  const step = underway.value?.step
  return step && step.done !== null && step.total !== null ? { done: step.done, total: step.total } : undefined
})

/** What a stop can do now, in words, so the state is never said by the button alone. */
const stopping = computed(() => {
  const now = underway.value
  if (now?.cancelling) {
    return 'Asked to stop: it stops before it changes anything, and says so when it is over.'
  }
  if (now?.step?.cancellable === false) {
    return 'Too late to stop: it finishes what it started.'
  }
  return 'It can be stopped until it starts to change anything.'
})

const cancelButton = useTemplateRef<ComponentPublicInstance>('cancel')
const runButton = useTemplateRef<ComponentPublicInstance>('run')

// Focus on what a slip of the Enter key would do least harm with: cancel,
// when the lines take something away; run, when they only add.
function focus(event: Event) {
  event.preventDefault()
  const button = confirming.value ? cancelButton.value : runButton.value
  ;(button?.$el as HTMLElement | undefined)?.focus()
}
</script>

<template>
  <UModal
    v-model:open="open"
    :title="title"
    :description="description"
    :dismissible="!running"
    :close="!running"
    :content="{ onOpenAutoFocus: focus }"
  >
    <template #body>
      <ol
        class="space-y-3"
        aria-label="The lines it runs"
        data-testid="preview"
      >
        <li
          v-for="(each, index) in lines"
          :key="index"
          data-testid="line"
        >
          <code
            class="block rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs break-all text-highlighted"
            data-testid="command"
          ><span
            class="select-none text-dimmed"
            aria-hidden="true"
          >$ </span>{{ each.line }}</code>
          <ul
            v-if="each.cautions.length > 0"
            class="mt-2 space-y-1.5"
          >
            <li
              v-for="caution in each.cautions"
              :key="caution.caution"
              class="flex items-start gap-2 text-sm"
              data-testid="caution"
            >
              <UIcon
                :name="ICONS[caution.caution] ?? PLAIN"
                class="mt-0.5 size-4 shrink-0 text-error"
                aria-hidden="true"
                :data-icon="ICONS[caution.caution] ?? PLAIN"
              />
              <span class="min-w-0">
                <span class="me-2 font-mono text-xs font-medium text-error">{{ caution.caution }}<span class="sr-only">:</span></span>
                {{ caution.message }}
              </span>
            </li>
          </ul>
        </li>
      </ol>

      <div
        v-if="following"
        class="mt-4 space-y-1 rounded-md border border-default px-3 py-2"
        role="status"
        aria-live="polite"
        data-testid="underway"
      >
        <p
          class="flex items-center gap-2 font-mono text-sm text-highlighted"
          data-testid="step"
        >
          <UIcon
            name="i-lucide-loader-circle"
            class="size-4 shrink-0 text-muted motion-safe:animate-spin"
            aria-hidden="true"
          />
          {{ underway?.step?.said ?? 'starting' }}
        </p>
        <!--
          The browser's own bar, its value and maximum the step's numbers,
          which it says to a screen reader as they are, drawn with no style
          attribute for the CSP to refuse. Given neither, it is the bar of a
          step that has said no count yet.
        -->
        <progress
          :value="counted?.done"
          :max="counted?.total"
          aria-label="How far it has got"
          class="block h-1.5 w-full appearance-none overflow-hidden rounded-full bg-accented [&::-moz-progress-bar]:bg-primary [&::-webkit-progress-bar]:bg-accented [&::-webkit-progress-value]:bg-primary"
          data-testid="progress"
        />
        <p
          class="text-xs text-muted"
          data-testid="stop-state"
        >
          {{ stopping }}
        </p>
      </div>
    </template>

    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton
          v-if="following"
          label="Stop"
          icon="i-lucide-circle-stop"
          color="error"
          variant="outline"
          :disabled="!stoppable"
          data-testid="stop"
          @click="runner.stop()"
        />
        <UButton
          ref="cancel"
          label="Cancel"
          color="neutral"
          variant="outline"
          :disabled="running"
          data-testid="cancel"
          @click="runner.cancel()"
        />
        <UButton
          ref="run"
          :label="confirming ? 'Confirm and run' : 'Run'"
          :color="confirming ? 'error' : 'primary'"
          :icon="confirming ? 'i-lucide-triangle-alert' : 'i-lucide-play'"
          :loading="running"
          data-testid="run"
          @click="runner.run()"
        />
      </div>
    </template>
  </UModal>
</template>
