<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Said } from '@/api/generated/Said'
import type { ServerListing } from '@/api/generated/ServerListing'
import type { ServerSummary } from '@/api/generated/ServerSummary'
import type { Warning } from '@/api/generated/Warning'
import { searched, trimmed, type Searched } from '@/api/catalogs'
import { useRunner } from '@/api/commands'
import type { Task } from '@/api/transport'
import { serversLink } from '@/routes'

const runner = useRunner()
const { busy } = runner

/** What a server's name holds, as typed. */
const typed = ref('')
const query = computed(() => trimmed(typed.value))

/**
 * What the last search listed, read back from what its line printed, as
 * core listed it; and why its line did not run, in its words and by its
 * code, when it did not.
 */
const found = ref<Searched<ServerListing>>()
const refused = ref<Said>()

/**
 * What an add's dry run said, once it ran: of which server, by its name and
 * the version listed, what a definition of it would be and where it would
 * be written, and the notes beside it, to read before saying yes. Let go
 * once another search lists something else, or the add ran.
 */
const dry = ref<{ name: string, version: string, said: string, notes: Warning[] }>()

/** The servers written into the library from the listing shown, by name. */
const added = ref<string[]>([])

/**
 * `mindweaver gather mcp search --json <query>`: a task like any other,
 * shown before it runs. It names no registry: the server asks the one it
 * was started to, the official one unless it was told another.
 */
async function search() {
  const asked = query.value
  if (asked === '') {
    return
  }
  const ran = await runner.ask({ type: 'search_servers', query: asked }, { drawn: true })
  if (!ran) {
    return
  }
  found.value = searched<ServerListing>(asked, ran)
  refused.value = ran.outcome.runs[0]?.error ?? undefined
  dry.value = undefined
  added.value = []
}

/** `gather mcp add` of `server`, the version listed, told yes or not. */
function adding(server: ServerSummary, yes: boolean): Task {
  return { type: 'import_server', server: server.name, version: server.version, yes }
}

/**
 * The dry run: what `server` would be written as, the program an agent
 * would run or the address it would reach, the variables it reads and
 * where it goes, said by the line and written nowhere. What it said is
 * shown, notes and all, whenever it ran.
 */
async function look(server: ServerSummary) {
  const ran = await runner.ask(adding(server, false))
  if (ran && ran.outcome.runs.every(run => run.error === null)) {
    dry.value = {
      name: server.name,
      version: server.version,
      said: ran.outcome.runs.map(run => run.stdout).join(''),
      notes: ran.outcome.runs.flatMap(run => run.stderr)
    }
  }
}

/**
 * The add itself, the same task told yes, offered only once its dry run has
 * been read: the line is shown again before it writes the definition into
 * the library. Nothing is started.
 */
async function add(server: ServerSummary) {
  const ran = await runner.ask(adding(server, true))
  if (ran?.outcome.ok) {
    dry.value = undefined
    added.value = [...added.value, server.name]
  }
}

/** Whether the dry run shown is of `server`, as listed. */
function lookedAt(server: ServerSummary): boolean {
  return dry.value?.name === server.name && dry.value.version === server.version
}
</script>

<template>
  <section
    aria-labelledby="servers-heading"
    class="flex flex-col gap-3 rounded-lg border border-default p-4"
    data-testid="registry"
  >
    <h2
      id="servers-heading"
      class="text-base font-semibold text-highlighted"
    >
      MCP Registry
    </h2>
    <p class="text-sm text-muted">
      Look for MCP servers in the MCP Registry, as <code class="font-mono text-xs">mindweaver gather mcp search</code> does, and add one to the library as a
      definition. Adding one says first what an agent would run or reach, what it reads and where it would be written, and writes nothing until you add it.
      Nothing is started either way: a definition reaches an agent only once it is installed into a project.
    </p>
    <form
      class="flex flex-wrap items-end gap-2"
      aria-label="Search the MCP Registry"
      data-testid="servers-form"
      @submit.prevent="search"
    >
      <UFormField
        label="Servers named"
        description="Three characters or more of a server's name."
        name="servers-query"
        class="w-full sm:w-auto"
      >
        <UInput
          v-model="typed"
          icon="i-lucide-search"
          placeholder="github"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full sm:w-80"
          :disabled="busy"
          data-testid="servers-query"
        />
      </UFormField>
      <UButton
        type="submit"
        label="Search"
        icon="i-lucide-telescope"
        size="sm"
        :disabled="busy || query === ''"
        data-testid="servers-search"
      />
    </form>

    <p
      v-if="refused"
      role="status"
      class="text-sm text-error"
      data-testid="servers-refused"
    >
      Nothing was listed: {{ refused.message }} <code class="font-mono text-xs">{{ refused.code }}</code>
    </p>

    <template v-else-if="found">
      <h3 class="text-sm font-medium text-highlighted">
        What the registry lists for <code class="font-mono">{{ found.query }}</code>
      </h3>
      <p
        v-if="found.found.servers.length === 0"
        class="text-sm text-muted"
        data-testid="servers-none"
      >
        The registry lists no server whose name holds it.
      </p>
      <ul
        v-else
        class="flex flex-col gap-3"
        aria-label="Each server listed, by the name the registry gives it"
        data-testid="listed"
      >
        <li
          v-for="server in found.found.servers"
          :key="server.name"
          class="flex flex-col gap-2 rounded-md border border-default p-3"
          data-testid="listed-server"
        >
          <div class="flex flex-wrap items-center gap-2">
            <code
              class="break-all font-mono text-sm font-medium text-highlighted"
              data-testid="server-name"
            >{{ server.name }}</code>
            <span
              v-if="server.title !== null"
              class="text-sm"
              data-testid="server-title"
            >{{ server.title }}</span>
            <UBadge
              :label="server.version"
              color="neutral"
              variant="soft"
              size="sm"
              class="font-mono"
              data-testid="server-version"
            />
            <UBadge
              :label="server.status"
              color="neutral"
              variant="outline"
              size="sm"
              data-testid="server-status"
            />
          </div>
          <p
            class="text-sm"
            data-testid="server-description"
          >
            {{ server.description }}
          </p>
          <p
            v-if="server.status_message !== null"
            class="text-sm text-muted"
            data-testid="server-status-message"
          >
            {{ server.status_message }}
          </p>
          <div class="flex flex-wrap items-center gap-1 text-xs">
            <span class="text-muted">Started or reached by</span>
            <UBadge
              v-for="(way, at) in server.ways"
              :key="at"
              :label="way"
              color="neutral"
              variant="outline"
              size="sm"
              class="font-mono"
              data-testid="server-way"
            />
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <UButton
              label="See what adding it writes"
              icon="i-lucide-file-search"
              size="sm"
              color="neutral"
              variant="outline"
              :aria-label="`See what adding ${server.name} writes`"
              :disabled="busy"
              data-testid="add-dry"
              @click="look(server)"
            />
            <span
              v-if="added.includes(server.name)"
              class="flex items-center gap-1 text-sm text-success"
              data-testid="added"
            >
              <UIcon
                name="i-lucide-circle-check"
                class="size-4"
                aria-hidden="true"
              />
              Added to the library: <RouterLink
                :to="serversLink()"
                class="underline"
              >see it among the MCP servers</RouterLink>
            </span>
          </div>
          <template v-if="dry && lookedAt(server)">
            <pre
              class="w-full overflow-x-auto rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
              :aria-label="`What adding ${server.name} would write`"
              data-testid="add-said"
            >{{ dry.said }}</pre>
            <ul
              v-if="dry.notes.length > 0"
              class="flex flex-col gap-1 text-sm"
              aria-label="What the import says of it"
              data-testid="add-notes"
            >
              <li
                v-for="(note, index) in dry.notes"
                :key="index"
                class="flex items-start gap-2"
                data-testid="said-line"
              >
                <UIcon
                  name="i-lucide-info"
                  class="mt-0.5 size-4 shrink-0 text-info"
                  aria-hidden="true"
                />
                <span class="min-w-0">
                  {{ note.text }}
                  <code
                    v-if="note.code !== null"
                    class="font-mono text-xs"
                  >{{ note.code }}</code>
                </span>
              </li>
            </ul>
            <UButton
              label="Add to the library"
              icon="i-lucide-plug"
              size="sm"
              class="self-start"
              :aria-label="`Add ${server.name} to the library`"
              :disabled="busy"
              data-testid="add-yes"
              @click="add(server)"
            />
          </template>
        </li>
      </ul>
    </template>
  </section>
</template>
