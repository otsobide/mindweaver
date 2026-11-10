<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Gatherable } from '@/api/generated/Gatherable'
import type { Kind } from '@/api/generated/Kind'
import { useRunner } from '@/api/commands'
import type { Ran, Task } from '@/api/transport'
import FolderPicker from '@/components/FolderPicker.vue'

/*
 * A source added (`mindweaver gather git`): what a gather would take said
 * first, by the same line with `--dry-run`, which writes nothing, and the
 * gather offered once that is read, of exactly what it was read of, as a
 * removal everywhere is offered once its dry run is. Every field is a value
 * of the line, sent without the white space around it, which the TUI's
 * form takes off too, and judged by the server: the repository as a
 * gather's source is, an address or a folder on this machine by its
 * absolute path, and the rest by the gather. Which kinds a gather takes,
 * and the folder each is looked for in when none is typed, are core's
 * (`gatherable`). A token is never typed here, nor sent (D-16): a private
 * repository logs in as git's credential helpers do on the server's machine.
 */

const props = defineProps<{
  /** The kinds a gather takes, in the snapshot's order, each with its folder unsaid. */
  gatherable: Gatherable[]
}>()

const emit = defineEmits<{
  /** A gather ran, however it ended: what it said, for the page to show. */
  ran: [what: string, ran: Ran]
}>()

const runner = useRunner()
const { busy } = runner

const url = ref('')
const kind = ref<Kind | undefined>(props.gatherable[0]?.kind)
const folder = ref('')
const reference = ref('')
const name = ref('')
const pin = ref('')

/** Whether the explorer is open, to choose a repository on this machine rather than type its path. */
const browsing = ref(false)

/** The folder a gather of the kind chosen looks in when none is typed, or none, which leaves one to name. */
const unsaid = computed(() => props.gatherable.find(each => each.kind === kind.value)?.folder ?? null)

/** A field as the line takes it: what it holds without the white space around it, or none. */
function given(text: string): string | null {
  const trimmed = text.trim()
  return trimmed === '' ? null : trimmed
}

/**
 * The gather the fields say, dry or not: none while there is no repository,
 * or no folder for a kind a gather looks in none of unsaid. Whatever else
 * would make the line fail is the line's to say, by its code.
 */
function gather(dryRun: boolean): Task | undefined {
  const source = given(url.value)
  const looked = given(folder.value) ?? unsaid.value
  if (source === null || looked === null || kind.value === undefined) {
    return undefined
  }
  return {
    type: 'gather',
    url: source,
    folder: looked,
    reference: given(reference.value),
    kind: kind.value,
    name: given(name.value),
    pin: given(pin.value),
    dry_run: dryRun
  }
}

/** The gather the fields say now, as text, which tells one dry run from another's. */
const asked = computed(() => JSON.stringify(gather(false) ?? null))

/**
 * What the dry run said, once it ran: what a gather would add, update,
 * leave and take off the shelf, to read before gathering. Of the gather it
 * was asked of, and shown only while the fields still say that one.
 */
const dry = ref<{ of: string, said: string }>()

/**
 * The dry run: a task like any other, shown before it runs and followed
 * while it fetches, which writes nothing and says what a gather would
 * take. What it said is kept whenever its line ran, a warning beside it
 * included: a skill that would not be taken is said there, and the rest is
 * no less worth reading for it.
 */
async function look() {
  const task = gather(true)
  if (task === undefined) {
    return
  }
  const of = asked.value
  const ran = await runner.ask(task)
  if (ran && ran.outcome.runs.every(run => run.error === null)) {
    dry.value = { of, said: ran.outcome.runs.map(run => run.stdout).join('') }
  }
}

/**
 * The gather itself, offered only once its dry run has been read: the same
 * line without `--dry-run`. What it said, a stop among it, is the page's to
 * show once it is over.
 */
async function take() {
  const task = gather(false)
  if (task === undefined || dry.value?.of !== asked.value) {
    return
  }
  const ran = await runner.ask(task)
  if (ran) {
    dry.value = undefined
    emit('ran', 'the gather', ran)
  }
}
</script>

<template>
  <form
    class="flex flex-col gap-3 rounded-lg border border-default p-4"
    aria-labelledby="add-heading"
    data-testid="add-source"
    @submit.prevent="look"
  >
    <div>
      <h2
        id="add-heading"
        class="text-base font-semibold text-highlighted"
      >
        Add a source
      </h2>
      <p class="text-sm text-muted">
        A git repository, gathered onto the shelf as <code class="font-mono text-xs">mindweaver gather git</code> gathers it.
        See what it would take first: nothing is gathered until you gather it, and either can be stopped while it fetches.
      </p>
    </div>

    <div class="flex flex-wrap items-end gap-2">
      <UFormField
        label="Repository"
        description="Its address, owner/repo on GitHub, owner/repo@ref, the address of a folder on GitHub or GitLab, or the absolute path of one on this machine."
        name="url"
        required
        class="w-full sm:flex-1"
      >
        <UInput
          v-model="url"
          icon="i-lucide-git-branch-plus"
          placeholder="acme/skills"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="source-url"
        />
      </UFormField>
      <UButton
        type="button"
        label="Browse"
        icon="i-lucide-folder-search"
        size="sm"
        color="neutral"
        variant="outline"
        :disabled="busy"
        data-testid="source-browse"
        @click="browsing = true"
      />
    </div>

    <URadioGroup
      v-model="kind"
      legend="What to take"
      orientation="horizontal"
      :items="gatherable.map(each => ({ value: each.kind, label: each.kind }))"
      :disabled="busy"
      :ui="{ legend: 'mb-1.5 text-sm font-medium text-default', label: 'font-mono text-sm' }"
      data-testid="source-kind"
    />

    <div class="grid gap-3 sm:grid-cols-2">
      <UFormField
        label="Folder in it"
        name="folder"
        :required="unsaid === null"
      >
        <template #description>
          <template v-if="unsaid === null">
            Needed: a gather of this kind looks in no folder unless one is named.
          </template>
          <template v-else>
            Empty looks in <code class="font-mono">{{ unsaid }}</code>.
          </template>
          <code class="font-mono">.</code> is the whole repository.
        </template>
        <UInput
          v-model="folder"
          icon="i-lucide-folder"
          :placeholder="unsaid ?? 'rules'"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="source-folder"
        />
      </UFormField>
      <UFormField
        label="Branch or tag"
        description="Empty for whatever the repository's default is."
        name="reference"
      >
        <UInput
          v-model="reference"
          icon="i-lucide-git-branch"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="source-reference"
        />
      </UFormField>
      <UFormField
        label="Name"
        description="What to call it, instead of its owner and repository: what --from takes."
        name="name"
      >
        <UInput
          v-model="name"
          icon="i-lucide-tag"
          placeholder="acme-skills"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="source-name"
        />
      </UFormField>
      <UFormField
        label="Pin to a commit"
        description="A commit in full to take, instead of whatever the branch is at now; empty to follow it."
        name="pin"
      >
        <UInput
          v-model="pin"
          icon="i-lucide-pin"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="source-pin"
        />
      </UFormField>
    </div>

    <UButton
      type="submit"
      label="See what it would take"
      icon="i-lucide-file-search"
      size="sm"
      color="neutral"
      variant="outline"
      class="self-start"
      :disabled="busy || gather(true) === undefined"
      data-testid="source-dry"
    />

    <template v-if="dry && dry.of === asked">
      <pre
        class="w-full overflow-x-auto rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
        aria-label="What a gather would take"
        data-testid="source-said"
      >{{ dry.said }}</pre>
      <UButton
        type="button"
        label="Gather"
        icon="i-lucide-download"
        size="sm"
        class="self-start"
        :disabled="busy"
        data-testid="source-gather"
        @click="take"
      />
    </template>

    <FolderPicker
      v-model:open="browsing"
      title="Choose a repository on this machine"
      verb="Choose"
      @chosen="url = $event"
    />
  </form>
</template>
