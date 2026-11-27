<script lang="ts">
	import { goto } from '$app/navigation';
	import Kbd from '#lib/components/Kbd.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { listContexts, type ContextInfo } from '#lib/k8s/api.js';
	import { matches } from '#lib/k8s/format.js';
	import { clamp, moveCursor, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import Search from '@lucide/svelte/icons/search';
	import ShipWheel from '@lucide/svelte/icons/ship-wheel';
	import { onMount, tick } from 'svelte';

	let contexts = $state<ContextInfo[] | null>(null);
	let error = $state<string | null>(null);
	let query = $state('');
	let cursor = $state(0);
	let search: HTMLInputElement;
	let list: HTMLElement;

	const shown = $derived(
		(contexts ?? []).filter((c) => matches(query, c.name, c.cluster, c.server, c.user))
	);

	const href = (c: ContextInfo) => `/k8s/cluster?context=${encodeURIComponent(c.name)}`;

	async function load() {
		error = null;
		try {
			contexts = await listContexts();
		} catch (e) {
			error = String(e);
			contexts ??= [];
		}
	}

	function setQuery(value: string) {
		query = value;
		cursor = 0;
	}

	async function moveTo(index: number) {
		cursor = index;
		await tick();
		scrollToSelected(list);
	}

	onMount(() => {
		search.focus();
		load();
	});

	// Typing goes straight to the search box; arrows / Ctrl+J/K move, Enter opens.
	useKeys((event) => {
		const next = moveCursor(event, clamp(cursor, shown.length), shown.length, false);
		if (next !== null) {
			moveTo(next);
			return true;
		}
		if (event.key === 'Enter' && shown.length) {
			goto(href(shown[clamp(cursor, shown.length)]));
			return true;
		}
		if (event.key === 'Escape' && query) {
			setQuery('');
			return true;
		}
		// Any printable key typed elsewhere on the page goes to the search box.
		if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && document.activeElement !== search) {
			search.focus();
		}
	});
</script>

<main class="mx-auto max-w-3xl p-8">
	<header class="flex items-center gap-3">
		<Button variant="ghost" size="icon" href="/" aria-label="Back"><ArrowLeft /></Button>
		<h1 class="flex-1 text-xl font-semibold tracking-tight">Kubernetes clusters</h1>
		<Button variant="outline" size="icon" onclick={load} aria-label="Reload kubeconfig">
			<RefreshCw />
		</Button>
	</header>

	<div class="relative mt-6">
		<Search class="text-muted-foreground absolute top-1/2 left-3 size-4 -translate-y-1/2" />
		<input
			bind:this={search}
			value={query}
			oninput={(e) => setQuery(e.currentTarget.value)}
			placeholder="Search clusters, then Enter"
			aria-label="Search clusters"
			spellcheck="false"
			autocomplete="off"
			class="border-input focus-visible:border-ring focus-visible:ring-ring/50 h-10 w-full rounded-lg border bg-transparent pr-3 pl-9 text-sm outline-none focus-visible:ring-3"
		/>
	</div>

	{#if error}
		<Alert.Root variant="destructive" class="mt-6">
			<Alert.Title>Could not read kubeconfig</Alert.Title>
			<Alert.Description>{error}</Alert.Description>
		</Alert.Root>
	{/if}

	<ul class="mt-4 grid gap-2" bind:this={list}>
		{#if contexts === null}
			{#each { length: 3 } as _, i (i)}
				<li><Skeleton class="h-16 w-full rounded-xl" /></li>
			{/each}
		{:else if contexts.length === 0 && !error}
			<li class="text-muted-foreground py-12 text-center text-sm">
				No contexts in kubeconfig (<code>$KUBECONFIG</code> or <code>~/.kube/config</code>).
			</li>
		{:else if shown.length === 0}
			<li class="text-muted-foreground py-12 text-center text-sm">No cluster matches “{query}”.</li>
		{/if}
		{#each shown as c, i (c.name)}
			{@const selected = i === clamp(cursor, shown.length)}
			<li>
				<a
					href={href(c)}
					data-selected={selected || undefined}
					onmouseenter={() => (cursor = i)}
					class="bg-card hover:border-foreground/30 data-selected:border-foreground/40 data-selected:bg-muted/50 flex items-center gap-4 rounded-xl border p-4 transition-colors outline-none"
				>
					<ShipWheel class="text-muted-foreground size-5 shrink-0" />
					<div class="min-w-0 flex-1">
						<div class="flex items-center gap-2">
							<span class="truncate font-medium">{c.name}</span>
							{#if c.current}<Badge variant="secondary">current</Badge>{/if}
						</div>
						<div class="text-muted-foreground truncate text-xs">
							{c.server ?? 'unknown server'}
							{#if c.user}· {c.user}{/if}
							{#if c.namespace}· ns {c.namespace}{/if}
						</div>
					</div>
					{#if selected}<Kbd>Enter</Kbd>{:else}<ChevronRight class="text-muted-foreground size-4 shrink-0" />{/if}
				</a>
			</li>
		{/each}
	</ul>
</main>
