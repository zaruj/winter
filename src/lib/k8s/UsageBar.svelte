<script lang="ts">
	// How much of a node resource is in use: a thin bar with the percentage, or just the total
	// when usage is unknown (no metrics-server).
	let {
		used,
		total,
		format
	}: {
		used: number | null | undefined;
		total: number | null | undefined;
		format: (value: number | null | undefined) => string;
	} = $props();

	const percent = $derived(used != null && total ? Math.round((used / total) * 100) : null);
	const tone = $derived(
		percent === null ? '' : percent >= 90 ? 'bg-red-500' : percent >= 70 ? 'bg-amber-500' : 'bg-emerald-500'
	);
</script>

{#if percent === null}
	<span class="text-muted-foreground">{format(total)}</span>
{:else}
	<span class="inline-flex min-w-28 flex-col gap-1" title="{format(used)} of {format(total)}">
		<span class="flex justify-between gap-2 text-xs tabular-nums">
			<span>{percent}%</span>
			<span class="text-muted-foreground">{format(used)} / {format(total)}</span>
		</span>
		<span class="bg-muted h-1 overflow-hidden rounded-full">
			<span class="block h-full rounded-full {tone}" style="width: {Math.min(percent, 100)}%"></span>
		</span>
	</span>
{/if}
