<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { PluginEntry } from '@/api/generated/PluginEntry'
import type { PluginPlace } from '@/api/generated/PluginPlace'
import { useRunner } from '@/api/commands'
import type { Ran } from '@/api/transport'
import type { Listed } from '@/plugins'

/*
 * The plugins a Claude Code marketplace's catalog lists, as core read it
 * (`MarketplaceListing`), for those to gather to be ticked: `mindweaver
 * gather marketplace --plugin <name>... <source>`, each ticked named in the
 * order the catalog lists them, never `--all`, so the line says what it
 * gathers. Which a gather takes is core's (`gatherable`), and what each is
 * called once gathered (`gathered`); a plugin no gather takes is listed,
 * never ticked.
 */

const props = defineProps<{
  /** What the listing printed, read back. */
  listed: Listed
  /** The marketplace as the line that listed it named it, and its branch, which the gather names again. */
  source: string
  reference: string | null
}>()

const emit = defineEmits<{
  /** The plugins ticked were gathered, however it ended. */
  ran: [what: string, ran: Ran]
}>()

const runner = useRunner()
const { busy } = runner

/** The plugins ticked, by name. */
const ticked = ref<string[]>([])

watch(() => props.listed, () => {
  ticked.value = []
})

const entries = computed(() => props.listed.listing.catalog.entries)

function tick(entry: PluginEntry, on: boolean | 'indeterminate') {
  ticked.value = on === true ? [...ticked.value, entry.name] : ticked.value.filter(name => name !== entry.name)
}

/** A commit as a person refers to it; the whole of it is its title. */
function short(revision: string): string {
  return revision.slice(0, 7)
}

/**
 * Where a plugin is, as its entry's `source` says it: the word it is tagged
 * by, then each of its fields that holds something, as written.
 */
function where(place: PluginPlace): string {
  const fields = Object.entries(place).filter(([key, value]) => key !== 'type' && value !== null && value !== '')
  return [place.type, ...fields.map(([key, value]) => `${key} ${String(value)}`)].join(', ')
}

async function gather() {
  const plugins = entries.value.filter(entry => ticked.value.includes(entry.name)).map(entry => entry.name)
  if (plugins.length === 0) {
    return
  }
  const ran = await runner.ask({ type: 'gather_plugins', source: props.source, reference: props.reference, plugins })
  if (ran) {
    emit('ran', `gathering ${props.listed.listing.name}'s plugins`, ran)
  }
}
</script>

<template>
  <form
    class="flex flex-col gap-3 rounded-md border border-default p-3"
    :aria-label="`The plugins ${listed.listing.name} lists`"
    data-testid="plugins"
    @submit.prevent="gather"
  >
    <div class="flex flex-col gap-0.5">
      <p class="text-sm">
        <span
          class="font-mono font-medium text-highlighted"
          data-testid="marketplace-name"
        >{{ listed.listing.name }}</span>
        <span class="text-muted">: </span>
        <code
          class="break-all font-mono text-xs"
          data-testid="marketplace-url"
        >{{ listed.listing.url }}</code>
        <template v-if="listed.listing.revision">
          <span class="text-muted"> at </span>
          <code
            class="font-mono text-xs"
            :title="listed.listing.revision"
          >{{ short(listed.listing.revision) }}</code>
        </template>
      </p>
      <p
        v-if="listed.listing.catalog.description"
        class="text-xs text-muted"
        data-testid="marketplace-description"
      >
        {{ listed.listing.catalog.description }}<template v-if="listed.listing.catalog.owner">
          , kept by {{ listed.listing.catalog.owner }}
        </template>
      </p>
    </div>

    <fieldset class="flex flex-col gap-2">
      <legend class="mb-1 text-xs text-muted">
        Plugins to gather, each a source of its own, its skills alone
      </legend>
      <p
        v-if="entries.length === 0"
        class="text-sm text-muted"
      >
        Its catalog lists no plugin that reads.
      </p>
      <div
        v-for="entry in entries"
        :key="entry.name"
        class="flex flex-col gap-1"
        data-testid="plugin"
      >
        <UCheckbox
          :model-value="ticked.includes(entry.name)"
          :label="entry.name"
          :description="entry.description ?? undefined"
          :disabled="busy || !entry.gatherable"
          :ui="{ label: 'font-mono text-sm' }"
          :data-testid="`plugin-${entry.name}`"
          @update:model-value="tick(entry, $event)"
        />
        <ul class="ms-6 flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-muted">
          <li data-testid="plugin-where">
            {{ where(entry.source) }}
          </li>
          <li
            v-if="entry.version"
            data-testid="plugin-version"
          >
            version {{ entry.version }}
          </li>
          <li
            v-if="entry.licence"
            data-testid="plugin-licence"
          >
            licence <span class="font-mono">{{ entry.licence }}</span>
          </li>
          <li
            v-if="entry.gathered"
            data-testid="plugin-gathered"
          >
            gathered as <span class="font-mono text-toned">{{ entry.gathered }}</span>
          </li>
          <li
            v-if="!entry.gatherable"
            class="text-toned"
            data-testid="plugin-not-gathered"
          >
            not gathered: no gather takes it
          </li>
          <li
            v-if="entry.declares.length"
            data-testid="plugin-declares"
          >
            its {{ entry.declares.join(', ') }} not gathered
          </li>
          <li
            v-if="entry.headers_helper"
            data-testid="plugin-headers-helper"
          >
            its headersHelper never run
          </li>
        </ul>
      </div>
    </fieldset>

    <ul
      v-if="listed.said.length"
      class="flex flex-col gap-1 text-sm"
      aria-label="What the listing said beside it"
    >
      <li
        v-for="(line, at) in listed.said"
        :key="at"
        class="flex items-start gap-2"
        data-testid="plugins-said"
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

    <UButton
      type="submit"
      :label="ticked.length === 1 ? 'Gather the plugin ticked' : `Gather the ${ticked.length} plugins ticked`"
      icon="i-lucide-download"
      size="sm"
      class="self-start"
      :disabled="busy || ticked.length === 0"
      data-testid="plugins-gather"
    />
  </form>
</template>
