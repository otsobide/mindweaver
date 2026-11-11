<script lang="ts">
/** A view of the panel: a page under src/pages and a line of the sidebar. */
export interface View {
  label: string
  icon: string
  to: string
  /** The pages reached from this one, which light its line too. */
  lights?: string[]
}

/**
 * Every view, in the sidebar's order, which the command palette lists them
 * in too: spelled once, so neither offers a page the other leaves out.
 * Here rather than in a module of its own, since Nuxt UI bundles the icons
 * a `.vue` file names, and only those.
 *
 * A project's page is reached from the projects, so it lights their line,
 * and one of the library's own, its editor and the page adding one light
 * the library's: `/project` is no child of `/projects` to the router, nor
 * `/artifact` of `/library` (src/routes.ts says why).
 */
export const VIEWS: readonly View[] = [
  { label: 'Overview', icon: 'i-lucide-layout-dashboard', to: '/' },
  { label: 'Projects', icon: 'i-lucide-folder-git-2', to: '/projects', lights: ['/project'] },
  { label: 'Discover', icon: 'i-lucide-radar', to: '/discover' },
  { label: 'Assign', icon: 'i-lucide-grid-3x3', to: '/assign' },
  { label: 'Updates', icon: 'i-lucide-circle-arrow-up', to: '/updates' },
  { label: 'Library', icon: 'i-lucide-library-big', to: '/library', lights: ['/artifact', '/edit', '/new'] },
  { label: 'MCP servers', icon: 'i-lucide-plug', to: '/mcp' },
  { label: 'Loads', icon: 'i-lucide-layers', to: '/loads' },
  { label: 'Shelf', icon: 'i-lucide-package', to: '/shelf' },
  { label: 'Sources', icon: 'i-lucide-git-branch', to: '/sources' },
  { label: 'Catalogs', icon: 'i-lucide-telescope', to: '/catalogs' }
]
</script>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { NavigationMenuItem } from '@nuxt/ui'
import { useKbd } from '@nuxt/ui/composables'
import { storeToRefs } from 'pinia'
import { useShell } from '@/stores/shell'
import AppLogo from './AppLogo.vue'

const shell = useShell()
const { sidebarOpen, sidebarCollapsed } = storeToRefs(shell)
const route = useRoute()

const links = computed<NavigationMenuItem[]>(() => VIEWS.map(({ label, icon, to, lights }) => ({
  label,
  icon,
  to,
  active: lights?.includes(route.path) || undefined
})))

// The keys that open the command palette, as assistive technology is told
// them: Nuxt UI reads its `meta` as Ctrl off a Mac, so Meta+K (Super) opens
// nothing there, and only a Mac takes both.
const { macOS } = useKbd()
const paletteKeys = computed(() => (macOS.value ? 'Meta+K Control+K' : 'Control+K'))

// Which server this panel came from. Each `mindweaver ui` takes a port of its
// own, so two open panels can belong to two different launches.
const address = window.location.host
</script>

<template>
  <!--
    An id of its own, or Nuxt UI keys the stored size and collapse on
    useId(), whose value moves whenever something that renders before the
    sidebar asks for an id too, and the stored state with it. A landmark of
    its own, so the logo and the server's address are in one: nothing on a
    page is outside a landmark.
  -->
  <UDashboardSidebar
    id="default"
    v-model:open="sidebarOpen"
    v-model:collapsed="sidebarCollapsed"
    role="complementary"
    aria-label="Sidebar"
    collapsible
    resizable
    data-testid="sidebar"
    class="bg-elevated/25"
    :min-size="13"
    :default-size="15"
    :max-size="20"
    :ui="{
      header: 'h-(--ui-header-height) border-b border-default',
      footer: 'border-t border-default py-2.5'
    }"
  >
    <template #header="{ collapsed }">
      <AppLogo :collapsed="collapsed" />
    </template>

    <template #default="{ collapsed }">
      <!--
        What opens the command palette, for a pointer or a touch: Ctrl+K,
        or Cmd+K on a Mac, from the keyboard, which the keys beside it say.
      -->
      <UButton
        :label="collapsed ? undefined : 'Search'"
        icon="i-lucide-search"
        color="neutral"
        variant="outline"
        :square="collapsed"
        :block="!collapsed"
        aria-label="Search: the command palette"
        :aria-keyshortcuts="paletteKeys"
        class="justify-start"
        data-testid="open-palette"
        @click="shell.paletteOpen = true"
      >
        <template
          v-if="!collapsed"
          #trailing
        >
          <span
            class="ms-auto flex items-center gap-0.5"
            aria-hidden="true"
          >
            <UKbd
              value="meta"
              variant="subtle"
            />
            <UKbd
              value="k"
              variant="subtle"
            />
          </span>
        </template>
      </UButton>
      <UNavigationMenu
        :items="links"
        :collapsed="collapsed"
        orientation="vertical"
        tooltip
        aria-label="Panel"
      />
    </template>

    <template #footer="{ collapsed }">
      <UTooltip
        :text="`Served by ${address}`"
        :disabled="!collapsed"
      >
        <div
          class="flex min-w-0 items-center gap-2 px-1.5 font-mono text-xs text-dimmed"
          data-testid="address"
        >
          <UIcon
            name="i-lucide-server"
            class="size-3.5 shrink-0"
          />
          <span
            v-if="!collapsed"
            class="truncate"
          >{{ address }}</span>
        </div>
      </UTooltip>
    </template>
  </UDashboardSidebar>
</template>
