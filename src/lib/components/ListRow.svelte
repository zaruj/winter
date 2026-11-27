<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import type { Component, Snippet } from 'svelte';

	// One row of a ListPage: icon, name with badges, and details. On wide windows the details
	// sit on the same line as the name, so more rows fit; on narrow ones they go underneath.

	let {
		href,
		selected,
		onhover,
		icon: Icon,
		name,
		badges,
		details
	}: {
		href: string;
		selected: boolean;
		onhover: () => void;
		icon: Component;
		name: string;
		badges?: Snippet;
		/** Plain details text, joined with ` · ` by the caller. */
		details?: string;
	} = $props();
</script>

<a
	{href}
	data-selected={selected || undefined}
	onmouseenter={onhover}
	class="hover:border-foreground/30 data-selected:border-foreground/40 data-selected:bg-muted/60 flex min-h-10 items-center gap-3 rounded-lg border border-transparent px-3 py-1.5 transition-colors outline-none"
>
	<Icon class="text-muted-foreground size-4 shrink-0" />
	<div class="flex min-w-0 flex-1 flex-col md:flex-row md:items-center md:gap-3">
		<div class="flex min-w-0 shrink-0 items-center gap-2 md:max-w-[55%]">
			<span class="truncate text-sm font-medium">{name}</span>
			{@render badges?.()}
		</div>
		{#if details}
			<span class="text-muted-foreground min-w-0 truncate text-xs md:ml-auto md:text-right" title={details}>{details}</span>
		{/if}
	</div>
	<span class="flex w-12 shrink-0 justify-end">
		{#if selected}<Kbd>Enter</Kbd>{:else}<ChevronRight class="text-muted-foreground size-4" />{/if}
	</span>
</a>
