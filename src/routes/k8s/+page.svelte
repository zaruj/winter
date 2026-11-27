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
	import ListPage from '#lib/components/ListPage.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import ShipWheel from '@lucide/svelte/icons/ship-wheel';
	import { onMount, tick } from 'svelte';

	let contexts = $state<ContextInfo[] | null>(null);
	let error = $state<string | null>(null);
	let query = $state('');
	let cursor = $state(0);
	let search: HTMLInputElement | undefined = $state();
	let list: HTMLElement | undefined = $state();

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
		search?.focus();
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
			search?.focus();
		}
	});
</script>

<ListPage
	title="Kubernetes clusters"
	bind:query
	bind:search
	onQuery={() => (cursor = 0)}
	placeholder="Search clusters, then Enter"
>
	{#snippet actions()}
		<Button variant="outline" size="icon" onclick={load} aria-label="Reload kubeconfig" title="Reload kubeconfig">
			<RefreshCw />
		</Button>
	{/snippet}
	{#snippet notice()}
		{#if error}
			<Alert.Root variant="destructive">
				<Alert.Title>Could not read kubeconfig</Alert.Title>
				<Alert.Description>{error}</Alert.Description>
			</Alert.Root>
		{/if}
	{/snippet}
	<ul class="grid gap-0.5" bind:this={list}>
		{#if contexts === null}
			{#each { length: 4 } as _, i (i)}<li><Skeleton class="h-10 w-full rounded-lg" /></li>{/each}
		{:else if contexts.length === 0 && !error}
			<li class="text-muted-foreground py-12 text-center text-sm">
				No contexts in kubeconfig (<code>$KUBECONFIG</code> or <code>~/.kube/config</code>).
			</li>
		{:else if shown.length === 0}
			<li class="text-muted-foreground py-12 text-center text-sm">No cluster matches “{query}”.</li>
		{/if}
		{#each shown as c, i (c.name)}
			<li>
				<ListRow
					href={href(c)}
					selected={i === clamp(cursor, shown.length)}
					onhover={() => (cursor = i)}
					icon={ShipWheel}
					name={c.name}
					details={[c.server ?? 'unknown server', c.user, c.namespace && `ns ${c.namespace}`].filter(Boolean).join(' · ')}
				>
					{#snippet badges()}
						{#if c.current}<Badge variant="secondary">current</Badge>{/if}
					{/snippet}
				</ListRow>
			</li>
		{/each}
	</ul>
	{#snippet hints()}
		<span>type to search</span>
		<span><Kbd>↑</Kbd> <Kbd>↓</Kbd> move</span>
		<span><Kbd>Enter</Kbd> open</span>
		<span><Kbd>Esc</Kbd> clear / back</span>
		<span class="ml-auto">{contexts?.length ?? 0} contexts · <Kbd>?</Kbd> help</span>
	{/snippet}
</ListPage>
