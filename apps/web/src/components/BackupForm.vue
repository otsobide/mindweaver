<script setup lang="ts">
import { ref } from 'vue'
import type { Kind } from '@/api/generated/Kind'
import { useRunner } from '@/api/commands'

/*
 * The library backed up to a git repository (`mindweaver backup`): an
 * export into a fresh check-out, committed and pushed. Each field is a
 * value of the line, sent as typed and judged by the server: the
 * repository as a gather's source is, the branch by the backup. A token is
 * never typed here, nor sent: the form names the variable holding one in
 * the environment `mindweaver ui` was started with, which the server reads
 * for the one run, and only one named for a token (D-16). Followed while it
 * runs, and stopped until it pushes, by the runner, as a gather is.
 */

const props = defineProps<{
  /** The kinds the workspace keeps, in the snapshot's order. */
  kinds: Kind[]
}>()

const runner = useRunner()
const { busy } = runner

const url = ref('')
const branch = ref('')
const message = ref('')
const tokenEnv = ref('')

/** The kinds ticked: none is every kind. */
const ticked = ref<Kind[]>([])

function tick(kind: Kind, on: boolean | 'indeterminate') {
  ticked.value = on === true ? [...ticked.value, kind] : ticked.value.filter(each => each !== kind)
}

/** A field left empty, which the line leaves out, as the command's own default. */
function given(text: string): string | null {
  return text.trim() === '' ? null : text
}

async function send() {
  const kinds = props.kinds.filter(kind => ticked.value.includes(kind))
  await runner.ask({
    type: 'backup',
    url: url.value,
    branch: given(branch.value),
    kinds,
    message: given(message.value),
    token_env: given(tokenEnv.value)
  })
}
</script>

<template>
  <form
    class="flex flex-col gap-3"
    aria-labelledby="backup-heading"
    data-testid="backup-form"
    @submit.prevent="send"
  >
    <div>
      <h3
        id="backup-heading"
        class="text-sm font-medium text-highlighted"
      >
        Back up to a git repository
      </h3>
      <p class="text-sm text-muted">
        The same export, committed and pushed with the <code class="font-mono">git</code> the server runs, logged in as it logs in. It shows each step, and can be stopped until it pushes.
      </p>
    </div>

    <UFormField
      label="Repository"
      description="An https or ssh URL, or the absolute path of one on this machine."
      name="url"
    >
      <UInput
        v-model="url"
        icon="i-lucide-cloud-upload"
        placeholder="git@github.com:you/library-backup.git"
        autocomplete="off"
        spellcheck="false"
        size="sm"
        class="w-full font-mono"
        :disabled="busy"
        data-testid="backup-url"
      />
    </UFormField>

    <div class="grid gap-3 sm:grid-cols-2">
      <UFormField
        label="Branch"
        description="Empty for the repository's own."
        name="branch"
      >
        <UInput
          v-model="branch"
          icon="i-lucide-git-branch"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="backup-branch"
        />
      </UFormField>
      <UFormField
        label="Token variable"
        description="The name of a variable holding a token, ending in TOKEN; never the token. Empty logs in as git does."
        name="token_env"
      >
        <UInput
          v-model="tokenEnv"
          icon="i-lucide-key-round"
          placeholder="GITHUB_TOKEN"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono"
          :disabled="busy"
          data-testid="backup-token-env"
        />
      </UFormField>
    </div>

    <UFormField
      label="Message"
      description="What the commit says; empty for the command's own."
      name="message"
    >
      <UInput
        v-model="message"
        autocomplete="off"
        size="sm"
        class="w-full"
        :disabled="busy"
        data-testid="backup-message"
      />
    </UFormField>

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
          :data-testid="`backup-kind-${kind}`"
          @update:model-value="tick(kind, $event)"
        />
      </div>
    </fieldset>

    <UButton
      type="submit"
      label="Back up"
      icon="i-lucide-cloud-upload"
      size="sm"
      class="self-start"
      :disabled="busy || url === ''"
      data-testid="backup"
    />
  </form>
</template>
