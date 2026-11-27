<script lang="ts">
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Table from '#lib/components/ui/table/index.js';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';

	// Rows shown in a table body around the real data: skeletons, empty and error states,
	// a "loading more" line while pages are still arriving, and a "show more" button.
	let {
		columns,
		loading,
		error,
		total,
		shown,
		filtered,
		onShowMore
	}: {
		columns: number;
		loading: boolean;
		error: string | null;
		/** Rows loaded so far. */
		total: number;
		/** Rows rendered. */
		shown: number;
		/** Rows matching the search. */
		filtered: number;
		onShowMore: () => void;
	} = $props();
</script>

{#if error}
	<Table.Row>
		<Table.Cell colspan={columns} class="text-destructive whitespace-normal">{error}</Table.Cell>
	</Table.Row>
{/if}

{#if loading && total === 0}
	{#each { length: 6 } as _, i (i)}
		<Table.Row>
			{#each { length: columns } as _, j (j)}
				<Table.Cell><Skeleton class="h-4 w-full max-w-40" /></Table.Cell>
			{/each}
		</Table.Row>
	{/each}
{:else if !loading && !error && filtered === 0}
	<Table.Row>
		<Table.Cell colspan={columns} class="text-muted-foreground py-8 text-center">
			{total === 0 ? 'Nothing here' : 'No matches'}
		</Table.Cell>
	</Table.Row>
{/if}

{#if shown < filtered}
	<Table.Row>
		<Table.Cell colspan={columns} class="text-center">
			<button class="text-muted-foreground hover:text-foreground text-sm" onclick={onShowMore}>
				Showing {shown} of {filtered}. Show more
			</button>
		</Table.Cell>
	</Table.Row>
{/if}

{#if loading && total > 0}
	<Table.Row>
		<Table.Cell colspan={columns} class="text-muted-foreground text-sm">
			<span class="inline-flex items-center gap-2">
				<LoaderCircle class="size-3.5 animate-spin" /> Loading…
			</span>
		</Table.Cell>
	</Table.Row>
{/if}
