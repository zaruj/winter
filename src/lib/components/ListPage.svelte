<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import Search from '@lucide/svelte/icons/search';
	import type { Snippet } from 'svelte';

	// The layout every "pick one" page shares (clusters, AWS profiles, GitHub repositories): a
	// header bar like the inner pages, a search box, the list, and key hints. The window never
	// scrolls; only the list does, and only when it is longer than the window.

	let {
		title,
		back = '/',
		query = $bindable(''),
		search = $bindable(),
		placeholder,
		onQuery,
		actions,
		notice,
		children,
		hints,
		showSearch = true
	}: {
		title: string;
		back?: string;
		query?: string;
		/** The search box, to focus it. */
		search?: HTMLInputElement;
		placeholder: string;
		/** Called as the user types (e.g. to reset a cursor). */
		onQuery?: (value: string) => void;
		/** Buttons at the right of the header. */
		actions?: Snippet;
		/** Alerts between the search box and the list. */
		notice?: Snippet;
		/** The list. */
		children: Snippet;
		hints?: Snippet;
		showSearch?: boolean;
	} = $props();
</script>

<main class="flex h-screen flex-col">
	<header class="flex shrink-0 items-center gap-2 border-b px-2 py-2 sm:gap-3 sm:px-4 sm:py-3">
		<Button variant="ghost" size="icon" href={back} aria-label="Back"><ArrowLeft /></Button>
		<h1 class="min-w-0 flex-1 truncate font-semibold">{title}</h1>
		{@render actions?.()}
	</header>

	<div class="mx-auto flex min-h-0 w-full max-w-4xl flex-1 flex-col px-3 pt-3 sm:px-6 sm:pt-4">
		{#if showSearch}
			<div class="relative shrink-0">
				<Search class="text-muted-foreground absolute top-1/2 left-3 size-4 -translate-y-1/2" />
				<input
					bind:this={search}
					value={query}
					oninput={(e) => {
						query = e.currentTarget.value;
						onQuery?.(query);
					}}
					{placeholder}
					aria-label={placeholder}
					spellcheck="false"
					autocomplete="off"
					class="border-input focus-visible:border-ring focus-visible:ring-ring/50 h-9 w-full rounded-lg border bg-transparent pr-3 pl-9 text-sm outline-none focus-visible:ring-3"
				/>
			</div>
		{/if}
		{#if notice}<div class="mt-3 shrink-0">{@render notice()}</div>{/if}
		<div class="-mx-1 mt-3 min-h-0 flex-1 overflow-y-auto px-1 pb-3" data-list-scroller>
			{@render children()}
		</div>
	</div>

	{#if hints}
		<footer class="text-muted-foreground flex shrink-0 flex-wrap gap-x-4 gap-y-1 border-t px-2 py-2 text-xs sm:px-4">
			{@render hints()}
		</footer>
	{/if}
</main>
