<script setup lang="ts">
import { ref, watch } from 'vue'
import type { ShelfSource } from '@/api/generated/ShelfSource'
import type { Shelved } from '@/api/generated/Shelved'
import { useRunner } from '@/api/commands'
import type { Ran } from '@/api/transport'
import PluginPicker from '@/components/PluginPicker.vue'
import { listed, type Listed } from '@/plugins'
import { instant, when } from '@/time'

/*
 * One source the shelf is filled from, as the snapshot lists it, and what
 * can be done to it, each by its name, the one thing every command about a
 * source takes it by (D-13): gathered again (`mindweaver gather update`),
 * renamed (`gather rename`), and forgotten (`gather remove`), its dry run
 * first, which says what it would take away and leave orphaned, then the
 * line with `--yes`, offered only then, which the preview asks about. What
 * a name may be is the command's to say, by its code. A Claude Code
 * marketplace lists its plugins too (`gather marketplace --list`), for more
 * of them to be gathered, and holds the plugins gathered of it, each a
 * source of its own, which the page hands it to draw under it.
 */

const props = defineProps<{
  source: ShelfSource
  /** What it supplied of the shelf, in the shelf's order. */
  supplied: Shelved[]
}>()

const emit = defineEmits<{
  /** A change of it ran, however it ended: what it said, for the page to show. */
  ran: [what: string, ran: Ran]
}>()

const runner = useRunner()
const { busy } = runner

/** Whether the field naming what to call it is open. */
const renaming = ref(false)

/** What to call it, as typed. */
const to = ref(props.source.name)

/**
 * What forgetting it would do, once its dry run ran: what it would take off
 * the shelf and out of the cache, and what it would leave orphaned. Of the
 * name it was asked of, and let go once it is called something else.
 */
const dry = ref<{ of: string, said: string }>()

watch(() => props.source.name, (name) => {
  to.value = name
  dry.value = undefined
})

/** What its catalog lists, once listed: shown until it is gathered from or listed again. */
const plugins = ref<Listed>()

/** `mindweaver gather marketplace --list --json`, of the marketplace it is, at its branch. */
async function list() {
  const ran = await runner.ask({ type: 'list_marketplace', source: props.source.url, reference: props.source.reference })
  plugins.value = (ran && listed(ran)) || undefined
}

/** What the plugins ticked came to, for the page; the listing is let go, since it says what was gathered before. */
function pluginsRan(what: string, ran: Ran) {
  plugins.value = undefined
  emit('ran', what, ran)
}

/** A commit as a person refers to it; the whole of it is its title. */
function short(revision: string): string {
  return revision.slice(0, 7)
}

/** `mindweaver gather update <name>`: it gathered again, as it was gathered. */
async function again() {
  const ran = await runner.ask({ type: 'update_sources', names: [props.source.name] })
  if (ran) {
    emit('ran', `gathering ${props.source.name} again`, ran)
  }
}

/** `mindweaver gather rename <name> <to>`, closed once it ran without fault. */
async function rename() {
  const ran = await runner.ask({ type: 'rename_source', from: props.source.name, to: to.value.trim() })
  if (ran?.outcome.ok) {
    renaming.value = false
  }
}

/** The dry run of forgetting it, which removes nothing and says what would go. */
async function look() {
  const of = props.source.name
  const ran = await runner.ask({ type: 'remove_source', name: of, yes: false })
  if (ran && ran.outcome.runs.every(run => run.error === null)) {
    dry.value = { of, said: ran.outcome.runs.map(run => run.stdout).join('') }
  }
}

/** Forgetting it, offered once its dry run is read, which the preview asks about. */
async function forget() {
  const name = props.source.name
  const ran = await runner.ask({ type: 'remove_source', name, yes: true })
  if (ran) {
    dry.value = undefined
    emit('ran', `forgetting ${name}`, ran)
  }
}
</script>

<template>
  <li
    class="flex flex-col gap-3 rounded-lg border border-default p-4"
    data-testid="source"
  >
    <div class="flex flex-wrap items-center gap-2">
      <h3
        class="font-mono text-sm font-semibold text-highlighted"
        data-testid="name"
      >
        {{ source.name }}
      </h3>
      <UBadge
        :label="source.kind"
        color="neutral"
        variant="outline"
        size="sm"
        class="font-mono"
        data-testid="kind"
      />
      <UBadge
        v-if="source.pin"
        label="pinned"
        icon="i-lucide-pin"
        color="neutral"
        variant="soft"
        size="sm"
        data-testid="pinned"
      />
      <code
        class="basis-full break-all font-mono text-xs text-toned"
        data-testid="url"
      >{{ source.url }}</code>
    </div>

    <dl class="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1 text-xs">
      <dt class="font-mono text-muted">
        --from
      </dt>
      <dd>
        <code
          class="break-all font-mono"
          data-testid="origin"
        >{{ source.origin }}</code>
      </dd>
      <dt class="font-mono text-muted">
        branch
      </dt>
      <dd
        class="font-mono"
        data-testid="reference"
      >
        <template v-if="source.reference">
          {{ source.reference }}
        </template>
        <span
          v-else
          class="font-sans text-muted"
        >whatever the remote's HEAD points at</span>
      </dd>
      <dt class="font-mono text-muted">
        folder
      </dt>
      <dd
        class="font-mono"
        data-testid="subdirectory"
      >
        {{ source.subdirectory }}
      </dd>
      <dt class="font-mono text-muted">
        pin
      </dt>
      <dd data-testid="pin">
        <code
          v-if="source.pin"
          class="font-mono"
          :title="source.pin"
        >{{ short(source.pin) }}</code>
        <span
          v-else
          class="text-muted"
        >none: it takes whatever its branch is at</span>
      </dd>
      <dt class="font-mono text-muted">
        revision
      </dt>
      <dd
        class="font-mono"
        data-testid="revision"
      >
        <code
          v-if="source.revision"
          :title="source.revision"
        >{{ short(source.revision) }}</code>
        <span
          v-else
          class="font-sans text-muted"
        >none resolved yet</span>
      </dd>
      <dt class="font-mono text-muted">
        licences
      </dt>
      <dd data-testid="licences">
        <ul
          v-if="source.licences.length"
          class="flex flex-wrap gap-1.5"
          aria-label="Each licence what it supplied says, with how many say it"
        >
          <li
            v-for="each in source.licences"
            :key="each.licence"
            class="flex items-center gap-1"
            data-testid="licence"
          >
            <span
              class="font-mono"
              data-testid="licence-said"
            >{{ each.licence }}</span>
            <UBadge
              :label="String(each.count)"
              color="neutral"
              variant="soft"
              size="sm"
              class="tabular-nums"
              data-testid="licence-count"
            />
          </li>
        </ul>
        <span
          v-else
          class="text-muted"
        >none recorded: nothing on the shelf from it says one yet</span>
      </dd>
      <dt class="font-mono text-muted">
        kept as
      </dt>
      <dd
        class="break-all font-mono"
        data-testid="directory"
      >
        {{ source.directory }}
      </dd>
      <dt class="font-mono text-muted">
        supplied
      </dt>
      <dd data-testid="supplied">
        <ul
          v-if="supplied.length"
          class="flex flex-wrap gap-1"
        >
          <li
            v-for="shelved in supplied"
            :key="shelved.path"
          >
            <UBadge
              :label="`${shelved.kind}:${shelved.name}`"
              color="neutral"
              variant="soft"
              size="sm"
              class="font-mono"
              data-testid="shelved"
            />
          </li>
        </ul>
        <span
          v-else
          class="text-muted"
        >nothing on the shelf now</span>
      </dd>
      <dt class="font-mono text-muted">
        gathered
      </dt>
      <dd data-testid="gathered">
        <time
          v-if="source.gathered_at !== null"
          :datetime="instant(source.gathered_at)"
        >{{ when(source.gathered_at) }}</time>
        <span
          v-else
          class="text-muted"
        >never put anything on the shelf</span>
      </dd>
    </dl>

    <div
      class="flex flex-wrap gap-2"
      role="group"
      :aria-label="`What to do with ${source.name}`"
    >
      <UButton
        label="Gather again"
        icon="i-lucide-refresh-cw"
        size="sm"
        color="neutral"
        variant="outline"
        :disabled="busy"
        data-testid="again"
        @click="again"
      />
      <UButton
        v-if="source.kind === 'marketplace'"
        label="List its plugins"
        icon="i-lucide-list-checks"
        size="sm"
        color="neutral"
        variant="outline"
        :disabled="busy"
        data-testid="list-plugins"
        @click="list"
      />
      <UButton
        label="Rename"
        icon="i-lucide-text-cursor-input"
        size="sm"
        color="neutral"
        variant="outline"
        :aria-expanded="renaming"
        :disabled="busy"
        data-testid="rename-open"
        @click="renaming = !renaming"
      />
      <UButton
        label="See what forgetting it does"
        icon="i-lucide-file-search"
        size="sm"
        color="neutral"
        variant="outline"
        :disabled="busy"
        data-testid="forget-dry"
        @click="look"
      />
    </div>

    <form
      v-if="renaming"
      class="flex flex-wrap items-end gap-2"
      :aria-label="`Rename ${source.name}`"
      data-testid="rename"
      @submit.prevent="rename"
    >
      <UFormField
        label="New name"
        description="What gather sources lists and --from takes from now on: letters, digits, ., _ and -."
        name="to"
        class="w-full sm:w-auto"
      >
        <UInput
          v-model="to"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono sm:w-72"
          :disabled="busy"
          data-testid="rename-to"
        />
      </UFormField>
      <UButton
        type="submit"
        label="Rename"
        icon="i-lucide-text-cursor-input"
        color="neutral"
        variant="outline"
        size="sm"
        :disabled="busy || to.trim() === '' || to.trim() === source.name"
        data-testid="rename-run"
      />
    </form>

    <PluginPicker
      v-if="plugins"
      :listed="plugins"
      :source="source.url"
      :reference="source.reference"
      @ran="pluginsRan"
    />

    <template v-if="dry && dry.of === source.name">
      <pre
        class="w-full overflow-x-auto rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
        aria-label="What forgetting it would do"
        data-testid="forget-said"
      >{{ dry.said }}</pre>
      <UButton
        label="Forget it"
        icon="i-lucide-trash"
        color="error"
        size="sm"
        class="self-start"
        :disabled="busy"
        data-testid="forget-yes"
        @click="forget"
      />
    </template>

    <!-- What it holds of its own, a marketplace's plugins, each a source drawn as this one is. -->
    <slot />
  </li>
</template>
