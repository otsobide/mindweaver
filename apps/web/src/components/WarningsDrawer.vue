<script setup lang="ts">
import { storeToRefs } from 'pinia'
import type { Code } from '@/api/generated/Code'
import { useRunner } from '@/api/commands'
import { useShell } from '@/stores/shell'

const { said } = useRunner()
const { warningsOpen } = storeToRefs(useShell())

// The icon of what a line says, by the code beside it: how it looks, and
// nothing it means, since the code and the words are always said beside
// it. Partial, by the codes the panel knows a look for, keyed by the
// generated `Code` so a code spelled wrong does not compile; any other
// takes the plain look of a warning, or of an error for why a line did not
// run, and a line with no code, a note or a hint, the look of a note.
const ICONS: Partial<Record<Code, string>> = {
  // What a front end without a terminal does not run.
  opens_screen: 'i-lucide-app-window',
  opens_editor: 'i-lucide-square-pen',
  prints_shell_code: 'i-lucide-terminal',
  show_unheld: 'i-lucide-eye-off',
  serves_panel: 'i-lucide-monitor',
  line_unparsed: 'i-lucide-text-cursor-input',
  // What a browser may not name.
  path_relative: 'i-lucide-folder-x',
  path_unnormalized: 'i-lucide-folder-x',
  path_url: 'i-lucide-folder-x',
  path_in_mindflayer: 'i-lucide-folder-x',
  not_registered: 'i-lucide-unlink',
  not_loaded: 'i-lucide-unlink',
  not_a_project: 'i-lucide-folder-search',
  not_a_workspace: 'i-lucide-folder-search',
  not_found: 'i-lucide-search-x',
  // Somebody's work, left alone.
  foreign: 'i-lucide-user-round',
  edited: 'i-lucide-pencil',
  rule_copy_edited: 'i-lucide-pencil',
  claude_rule_edited: 'i-lucide-pencil',
  // Another writer, or a file that does not read.
  workspace_busy: 'i-lucide-lock',
  front_matter_missing: 'i-lucide-file-warning',
  front_matter_unterminated: 'i-lucide-file-warning',
  front_matter_invalid: 'i-lucide-file-warning',
  front_matter_unparsed: 'i-lucide-file-warning'
}

/** How a line looks: its icon and its colour. */
interface Look {
  icon: string
  tone: string
}

function look(code: string | null, error: boolean): Look {
  const tone = error ? 'text-error' : code === null ? 'text-info' : 'text-warning'
  if (code === null) {
    return { icon: 'i-lucide-info', tone }
  }
  const icon = ICONS[code as Code] ?? (error ? 'i-lucide-circle-x' : 'i-lucide-triangle-alert')
  return { icon, tone }
}
</script>

<template>
  <USlideover
    v-model:open="warningsOpen"
    title="What the run said"
    description="Each line's warnings, notes and errors, as the terminal shows them, each with its code."
  >
    <template #body>
      <p
        v-if="!said"
        class="text-muted"
        data-testid="nothing-ran"
      >
        Nothing has run yet.
      </p>
      <ol
        v-else
        class="space-y-4"
        data-testid="said"
      >
        <li
          v-for="(run, index) in said.outcome.runs"
          :key="index"
          data-testid="run"
        >
          <div class="flex items-start justify-between gap-2">
            <code class="min-w-0 font-mono text-xs break-all text-highlighted"><span
              class="select-none text-dimmed"
              aria-hidden="true"
            >$ </span>{{ run.command }}</code>
            <UBadge
              :label="run.ok ? 'ok' : 'failed'"
              :color="run.ok ? 'success' : 'error'"
              :icon="run.ok ? 'i-lucide-circle-check' : 'i-lucide-circle-x'"
              variant="subtle"
              size="sm"
              class="shrink-0 font-mono"
              data-testid="ok"
            />
          </div>
          <ul
            v-if="run.stderr.length > 0 || run.error"
            class="mt-2 space-y-1.5"
          >
            <li
              v-for="(line, at) in run.stderr"
              :key="at"
              class="flex items-start gap-2 text-sm"
              data-testid="line"
            >
              <UIcon
                :name="look(line.code, false).icon"
                :class="look(line.code, false).tone"
                class="mt-0.5 size-4 shrink-0"
                aria-hidden="true"
                :data-icon="look(line.code, false).icon"
              />
              <span class="min-w-0 break-words">
                <span
                  v-if="line.code"
                  class="me-2 font-mono text-xs text-dimmed"
                >{{ line.code }}<span class="sr-only">:</span></span>
                {{ line.text }}
              </span>
            </li>
            <li
              v-if="run.error"
              class="flex items-start gap-2 text-sm"
              data-testid="line"
            >
              <UIcon
                :name="look(run.error.code, true).icon"
                :class="look(run.error.code, true).tone"
                class="mt-0.5 size-4 shrink-0"
                aria-hidden="true"
                :data-icon="look(run.error.code, true).icon"
              />
              <span class="min-w-0 break-words">
                <span class="me-2 font-mono text-xs text-dimmed">{{ run.error.code }}<span class="sr-only">:</span></span>
                error: {{ run.error.message }}
              </span>
            </li>
          </ul>
          <p
            v-else
            class="mt-2 text-sm text-muted"
          >
            It said nothing beside its output.
          </p>
        </li>
      </ol>
    </template>
  </USlideover>
</template>
