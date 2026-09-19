<script lang="ts">
  import ConstructionIcon from '@lucide/svelte/icons/construction';
  import AppLayout from '../layout/AppLayout.svelte';
  import { EmptyState } from '$lib/components/ui/empty-state';

  let {
    title,
    subtitle = '',
    task,
    description
  }: { title: string; subtitle?: string; task: string; description: string } = $props();
</script>

<AppLayout {title} {subtitle}>
  <div class="p-6">
    <!-- The shell lists this page before its contents exist, so it says which
         task fills it instead of looking broken. -->
    <EmptyState icon={ConstructionIcon} title={`${title} lands in ${task}`} {description} />
  </div>
</AppLayout>
