<script setup lang="ts">
import AppHeader from '@/components/AppHeader.vue'
import ServerSearch from '@/components/ServerSearch.vue'
import SkillSearch from '@/components/SkillSearch.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'

/*
 * The catalogs a workspace finds what to take in: the skill directories it
 * enables, whose results are taken as git sources onto the shelf, and the
 * MCP Registry, whose servers are added to the library as definitions.
 * Every search, take and add is a task, its lines shown before they run,
 * as the TUI's shelf runs them (`f` and `r`): what was found is read back
 * from what a search's line printed, and nothing is fetched by the browser,
 * which never sees a catalog's address. Searching writes nothing, so its
 * results are this page's to hold for as long as it is open.
 */
</script>

<template>
  <UDashboardPanel
    id="catalogs"
    role="main"
    data-testid="catalogs"
  >
    <template #header>
      <AppHeader
        title="Catalogs"
        icon="i-lucide-telescope"
      />
    </template>

    <template #body>
      <SnapshotGate>
        <SkillSearch />
        <ServerSearch />
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
