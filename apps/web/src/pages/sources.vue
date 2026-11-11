<script setup lang="ts">
import { ref } from 'vue'
import type { ShelfSource } from '@/api/generated/ShelfSource'
import type { Shelved } from '@/api/generated/Shelved'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useRunner } from '@/api/commands'
import type { Ran } from '@/api/transport'
import ActivityFeed from '@/components/ActivityFeed.vue'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { weaver, making } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import SourceCard from '@/components/SourceCard.vue'
import SourceForm from '@/components/SourceForm.vue'

/*
 * Where the shelf is filled from, and every change of it, each a task the
 * runner shows before it runs: a source added, what it would take said
 * first (`SourceForm`), each source gathered again, renamed or forgotten
 * (`SourceCard`), and every one gathered again. A gather is followed while
 * it runs, its steps and Stop in the runner's dialog; once it is over, what
 * it said is shown here, stopped or not, the toast having gone by then.
 */

const runner = useRunner()
const { busy } = runner

/**
 * What a gather, an update or a removal from this page said once it was
 * over, and what it was: kept until another says something, or it is let
 * go, so it is there to read after the toast has gone.
 */
const said = ref<{ what: string, ran: Ran }>()

function told(what: string, ran: Ran) {
  said.value = { what, ran }
}

/**
 * What `source` supplied of the shelf, in the shelf's order: the rows that
 * came from it, told by the folder it owns, which no two sources share.
 */
function supplied(snapshot: Snapshot, source: ShelfSource): Shelved[] {
  return snapshot.shelf.filter(shelved => shelved.source.directory === source.directory)
}

/**
 * The sources listed at the top, in the snapshot's order: each but a
 * plugin whose marketplace the snapshot lists, which is drawn under it.
 */
function tops(snapshot: Snapshot): ShelfSource[] {
  const names = new Set(snapshot.sources.map(source => source.name))
  return snapshot.sources.filter(source => source.parent === null || !names.has(source.parent))
}

/** The plugins gathered of `parent`, in the snapshot's order: the sources naming it as theirs. */
function children(snapshot: Snapshot, parent: ShelfSource): ShelfSource[] {
  return snapshot.sources.filter(source => source.parent === parent.name)
}

/** `mindweaver gather update --all`: every source gathered again, one after the other. */
async function everyOne() {
  const ran = await runner.ask({ type: 'update_sources', names: [] })
  if (ran) {
    told('gathering every source again', ran)
  }
}
</script>

<template>
  <UDashboardPanel
    id="sources"
    role="main"
    data-testid="sources"
  >
    <template #header>
      <AppHeader
        title="Sources"
        icon="i-lucide-git-branch"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <SourceForm
          :gatherable="snapshot.gatherable"
          @ran="told"
        />

        <section
          v-if="said"
          aria-labelledby="said-heading"
          class="flex flex-col gap-2 rounded-lg border border-default p-4"
          data-testid="summary"
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h2
              id="said-heading"
              class="text-base font-semibold text-highlighted"
            >
              What {{ said.what }} said
            </h2>
            <UButton
              label="Dismiss"
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              data-testid="summary-dismiss"
              @click="said = undefined"
            />
          </div>
          <div
            v-for="(run, index) in said.ran.outcome.runs"
            :key="index"
            class="flex flex-col gap-1.5"
            data-testid="summary-run"
          >
            <code
              class="font-mono text-xs text-muted"
              data-testid="summary-command"
            ><span
              class="select-none text-dimmed"
              aria-hidden="true"
            >$ </span>{{ run.command }}</code>
            <pre
              v-if="run.stdout"
              class="w-full overflow-x-auto rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
              :aria-label="`What ${run.command} printed`"
              data-testid="summary-output"
            >{{ run.stdout }}</pre>
            <ul
              v-if="run.stderr.length"
              class="flex flex-col gap-1"
              :aria-label="`What ${run.command} said beside it`"
            >
              <li
                v-for="(line, at) in run.stderr"
                :key="at"
                class="flex items-start gap-2 text-sm"
                data-testid="summary-line"
              >
                <UIcon
                  :name="line.code === null ? 'i-lucide-info' : 'i-lucide-triangle-alert'"
                  class="mt-0.5 size-4 shrink-0"
                  :class="line.code === null ? 'text-muted' : 'text-warning'"
                  aria-hidden="true"
                />
                <span class="min-w-0">
                  <span
                    v-if="line.code !== null"
                    class="me-2 font-mono text-xs font-medium text-warning"
                  >{{ line.code }}<span class="sr-only">:</span></span>
                  {{ line.text }}
                </span>
              </li>
            </ul>
            <p
              v-if="run.error"
              class="flex items-start gap-2 text-sm"
              data-testid="summary-error"
            >
              <UIcon
                name="i-lucide-circle-x"
                class="mt-0.5 size-4 shrink-0 text-error"
                aria-hidden="true"
              />
              <span class="min-w-0">
                <span class="me-2 font-mono text-xs font-medium text-error">{{ run.error.code }}<span class="sr-only">:</span></span>
                {{ run.error.message }}
              </span>
            </p>
          </div>
        </section>

        <section
          aria-labelledby="sources-heading"
          class="flex flex-col gap-3"
          data-testid="list"
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h2
              id="sources-heading"
              class="text-base font-semibold text-highlighted"
            >
              Where the shelf is filled from
            </h2>
            <UButton
              v-if="snapshot.sources.length > 0"
              label="Gather every one again"
              icon="i-lucide-refresh-cw"
              color="neutral"
              variant="outline"
              size="sm"
              :disabled="busy"
              data-testid="again-all"
              @click="everyOne"
            />
          </div>

          <div
            v-if="snapshot.sources.length === 0"
            class="flex flex-col items-start gap-2"
            data-testid="empty"
          >
            <p class="text-muted">
              Nothing has been gathered from anywhere yet.
            </p>
            <CommandHint
              :commands="[
                ...making(snapshot.workspace),
                { line: weaver(snapshot.workspace, 'gather git <URL>'), does: 'skills from a repository, onto the shelf' }
              ]"
            />
          </div>

          <ul
            v-else
            class="flex flex-col gap-3"
            aria-label="Every source, by its name"
          >
            <SourceCard
              v-for="source in tops(snapshot)"
              :key="source.directory"
              :source="source"
              :supplied="supplied(snapshot, source)"
              @ran="told"
            >
              <ul
                v-if="children(snapshot, source).length"
                class="flex flex-col gap-3"
                :aria-label="`The plugins gathered of ${source.name}`"
                data-testid="children"
              >
                <SourceCard
                  v-for="child in children(snapshot, source)"
                  :key="child.directory"
                  :source="child"
                  :supplied="supplied(snapshot, child)"
                  @ran="told"
                />
              </ul>
            </SourceCard>
          </ul>
        </section>

        <section
          aria-labelledby="activity-heading"
          class="flex flex-col gap-3"
          data-testid="history"
        >
          <h2
            id="activity-heading"
            class="text-base font-semibold text-highlighted"
          >
            What happened lately
          </h2>
          <ActivityFeed :entries="snapshot.history" />
        </section>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
