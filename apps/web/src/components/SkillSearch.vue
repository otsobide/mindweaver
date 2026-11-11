<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Hit } from '@/api/generated/Hit'
import type { Said } from '@/api/generated/Said'
import type { SkillsFound } from '@/api/generated/SkillsFound'
import { searched, trimmed, type Searched } from '@/api/catalogs'
import { useRunner } from '@/api/commands'

const runner = useRunner()
const { busy } = runner

/** What to look for, as typed. */
const typed = ref('')
const query = computed(() => trimmed(typed.value))

/**
 * What the last search found, read back from what its line printed, as
 * core listed it; and why its line did not run, in its words and by its
 * code, when it did not, which is all a refused search has to say.
 */
const found = ref<Searched<SkillsFound>>()
const refused = ref<Said>()

/** The results of the listing shown that were taken, each by its place in it. */
const taken = ref<number[]>([])

/**
 * `mindweaver gather search --json <query>`: a task like any other, shown
 * before it runs, followed while each directory is asked and stopped when
 * asked to. No token goes with it, nor any address: the server asks the
 * directories the workspace enables, where it was started to.
 */
async function search() {
  const asked = query.value
  if (asked === '') {
    return
  }
  const ran = await runner.ask({ type: 'search_skills', query: asked }, { drawn: true })
  if (!ran) {
    return
  }
  found.value = searched<SkillsFound>(asked, ran)
  refused.value = ran.outcome.runs[0]?.error ?? undefined
  taken.value = []
}

/**
 * Take `hit`, the result listed at `at`: sent back whole, as the search
 * printed it, for the backend to gather by the line `gather search --take`
 * amounts to, shown before it runs. Which folder and branch it is gathered
 * from is the line's to say, never the page's.
 */
async function take(hit: Hit, at: number) {
  const ran = await runner.ask({ type: 'take_skill', hit })
  if (ran?.outcome.ok) {
    taken.value = [...taken.value, at]
  }
}
</script>

<template>
  <section
    aria-labelledby="skills-heading"
    class="flex flex-col gap-3 rounded-lg border border-default p-4"
    data-testid="skills"
  >
    <h2
      id="skills-heading"
      class="text-base font-semibold text-highlighted"
    >
      Skill directories
    </h2>
    <p class="text-sm text-muted">
      Look for skills in the directories your workspace's <code class="font-mono text-xs">weaver.toml</code> lists in
      <code class="font-mono text-xs">finders</code>, as <code class="font-mono text-xs">mindweaver gather search</code> does, and take one onto the shelf
      from its repository, as <code class="font-mono text-xs">gather git</code> would.
      GitHub's code search needs a token, which the panel never hands over: search it from a terminal with <code class="font-mono text-xs">--gh</code>.
      skills.sh and claude-plugins.dev are unstable and undocumented: list one only once you have read its terms.
    </p>
    <form
      class="flex flex-wrap items-end gap-2"
      aria-label="Search the skill directories"
      data-testid="skills-form"
      @submit.prevent="search"
    >
      <UFormField
        label="Skills about"
        description="Words a skill's name or description holds."
        name="skills-query"
        class="w-full sm:w-auto"
      >
        <UInput
          v-model="typed"
          icon="i-lucide-search"
          placeholder="pdf"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full sm:w-80"
          :disabled="busy"
          data-testid="skills-query"
        />
      </UFormField>
      <UButton
        type="submit"
        label="Search"
        icon="i-lucide-telescope"
        size="sm"
        :disabled="busy || query === ''"
        data-testid="skills-search"
      />
    </form>

    <p
      v-if="refused"
      role="status"
      class="text-sm text-error"
      data-testid="skills-refused"
    >
      Nothing was searched: {{ refused.message }} <code class="font-mono text-xs">{{ refused.code }}</code>
    </p>

    <template v-else-if="found">
      <h3 class="text-sm font-medium text-highlighted">
        What the directories found for <code class="font-mono">{{ found.query }}</code>
      </h3>
      <p
        v-if="found.found.hits.length === 0"
        class="text-sm text-muted"
        data-testid="skills-none"
      >
        No directory asked found a skill for it.
      </p>
      <ul
        v-else
        class="flex flex-col gap-3"
        aria-label="Each skill found, in the order the directories ranked them"
        data-testid="hits"
      >
        <li
          v-for="(hit, at) in found.found.hits"
          :key="at"
          class="flex flex-col gap-2 rounded-md border border-default p-3"
          data-testid="hit"
        >
          <div class="flex flex-wrap items-center gap-2">
            <span
              class="font-medium text-highlighted"
              data-testid="hit-name"
            >{{ hit.name }}</span>
            <UBadge
              v-for="finder in hit.finders"
              :key="finder"
              :label="finder"
              color="neutral"
              variant="outline"
              size="sm"
              class="font-mono"
              data-testid="hit-finder"
            />
          </div>
          <dl class="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1 text-xs">
            <dt class="font-mono text-muted">
              repository
            </dt>
            <dd>
              <code
                class="break-all font-mono"
                data-testid="hit-repository"
              >{{ hit.repository }}</code>
            </dd>
            <dt class="font-mono text-muted">
              branch
            </dt>
            <dd data-testid="hit-reference">
              <code
                v-if="hit.reference !== null"
                class="font-mono"
              >{{ hit.reference }}</code>
              <span
                v-else
                class="text-muted"
              >the repository's own</span>
            </dd>
            <dt class="font-mono text-muted">
              folder
            </dt>
            <dd data-testid="hit-path">
              <code
                v-if="hit.path !== null"
                class="break-all font-mono"
              >{{ hit.path }}</code>
              <span
                v-else
                class="text-muted"
              >none named: the whole repository is gathered</span>
            </dd>
            <template v-if="hit.installs !== null">
              <dt class="font-mono text-muted">
                installs
              </dt>
              <dd data-testid="hit-installs">
                {{ hit.installs }}
              </dd>
            </template>
            <template v-if="hit.stars !== null">
              <dt class="font-mono text-muted">
                stars
              </dt>
              <dd data-testid="hit-stars">
                {{ hit.stars }}
              </dd>
            </template>
          </dl>
          <div class="flex flex-wrap items-center gap-2">
            <UButton
              label="Take"
              icon="i-lucide-package-plus"
              size="sm"
              variant="outline"
              :aria-label="`Take ${hit.name} onto the shelf`"
              :disabled="busy"
              data-testid="take"
              @click="take(hit, at)"
            />
            <span
              v-if="taken.includes(at)"
              class="flex items-center gap-1 text-sm text-success"
              data-testid="taken"
            >
              <UIcon
                name="i-lucide-circle-check"
                class="size-4"
                aria-hidden="true"
              />
              Gathered: <RouterLink
                to="/shelf"
                class="underline"
              >see it on the shelf</RouterLink>
            </span>
          </div>
        </li>
      </ul>
      <ul
        v-if="found.said.length > 0"
        class="flex flex-col gap-1 text-sm"
        aria-label="What the search said beside what it found"
        data-testid="skills-said"
      >
        <li
          v-for="(line, index) in found.said"
          :key="index"
          class="flex items-start gap-2"
          data-testid="said-line"
        >
          <UIcon
            :name="line.code === null ? 'i-lucide-info' : 'i-lucide-triangle-alert'"
            :class="['mt-0.5 size-4 shrink-0', line.code === null ? 'text-info' : 'text-warning']"
            aria-hidden="true"
          />
          <span class="min-w-0">
            {{ line.text }}
            <code
              v-if="line.code !== null"
              class="font-mono text-xs"
            >{{ line.code }}</code>
          </span>
        </li>
      </ul>
    </template>
  </section>
</template>
