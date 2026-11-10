<script setup lang="ts">
import { ref } from 'vue'
import type { Kind } from '@/api/generated/Kind'
import { useRunner } from '@/api/commands'
import FolderPicker from '@/components/FolderPicker.vue'

/*
 * The library exported into a folder (`mindweaver export`), laid out as a
 * workspace of its own. The folder goes to the server as typed, or as the
 * explorer spelled it, held there to the path policy (absolute, normalized,
 * outside every `.mindweaver`); the kinds are those ticked, none for every
 * kind, as the line then names none; and `replace` is `--yes`, which lets
 * it change or remove what an earlier export wrote there, and which the
 * preview has somebody confirm. Nothing is judged here: the command says
 * what it would do, and does it.
 */

const props = defineProps<{
  /** The kinds the workspace keeps, in the snapshot's order. */
  kinds: Kind[]
}>()

const runner = useRunner()
const { busy } = runner

/** The folder to export into, as typed or chosen. */
const folder = ref('')

/** The kinds ticked: none is every kind. */
const ticked = ref<Kind[]>([])

/** Whether what an earlier export wrote there may be changed or removed. */
const replace = ref(false)

/** Whether the explorer is open, to choose the folder rather than type it. */
const browsing = ref(false)

function tick(kind: Kind, on: boolean | 'indeterminate') {
  ticked.value = on === true ? [...ticked.value, kind] : ticked.value.filter(each => each !== kind)
}

async function send() {
  // In the snapshot's order whatever order they were ticked in, so the
  // line names them as the library lists them.
  const kinds = props.kinds.filter(kind => ticked.value.includes(kind))
  await runner.ask({ type: 'export', folder: folder.value, kinds, yes: replace.value })
}
</script>

<template>
  <form
    class="flex flex-col gap-3"
    aria-labelledby="export-heading"
    data-testid="export-form"
    @submit.prevent="send"
  >
    <div>
      <h3
        id="export-heading"
        class="text-sm font-medium text-highlighted"
      >
        Export into a folder
      </h3>
      <p class="text-sm text-muted">
        A workspace of its own, which <code class="font-mono">mindweaver load</code> offers to any workspace's projects.
      </p>
    </div>

    <div class="flex flex-wrap items-end gap-2">
      <UFormField
        label="Folder"
        description="Its absolute path, made if it is not there."
        name="folder"
        class="w-full sm:w-auto"
      >
        <UInput
          v-model="folder"
          icon="i-lucide-folder-output"
          placeholder="/home/you/library-backup"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono sm:w-80"
          :disabled="busy"
          data-testid="export-folder"
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
        data-testid="export-browse"
        @click="browsing = true"
      />
    </div>

    <fieldset class="flex flex-col gap-1.5">
      <legend class="mb-1.5 text-xs text-muted">
        Only these kinds; none ticked is every kind
      </legend>
      <div class="flex flex-wrap gap-4">
        <UCheckbox
          v-for="kind in kinds"
          :key="kind"
          :model-value="ticked.includes(kind)"
          :label="kind"
          :disabled="busy"
          :ui="{ label: 'font-mono text-sm' }"
          :data-testid="`export-kind-${kind}`"
          @update:model-value="tick(kind, $event)"
        />
      </div>
    </fieldset>

    <UCheckbox
      v-model="replace"
      label="Change or remove what an earlier export wrote there"
      description="Without it, an export there adds and says what it would change. With it, you confirm before it runs."
      :disabled="busy"
      data-testid="export-replace"
    />

    <UButton
      type="submit"
      label="Export"
      icon="i-lucide-hard-drive-download"
      size="sm"
      class="self-start"
      :disabled="busy || folder === ''"
      data-testid="export"
    />

    <FolderPicker
      v-model:open="browsing"
      :start="folder"
      title="Choose a folder to export into"
      verb="Choose"
      @chosen="folder = $event"
    />
  </form>
</template>
