<script lang="ts">
	import { goto } from '$app/navigation';
	import Kbd from '#lib/components/Kbd.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { FALLBACK_REGION, listProfiles, type ProfileInfo } from '#lib/aws/api.js';
	import { matches } from '#lib/k8s/format.js';
	import { clamp, moveCursor, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import ListPage from '#lib/components/ListPage.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import { age } from '#lib/k8s/format.js';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import Cloud from '@lucide/svelte/icons/cloud';
	import { onMount, tick } from 'svelte';

	let contexts = $state<ProfileInfo[] | null>(null);
	let error = $state<string | null>(null);
	let query = $state('');
	let cursor = $state(0);
	let search: HTMLInputElement | undefined = $state();
	let list: HTMLElement | undefined = $state();
	let now = $state(Date.now());

	const shown = $derived(
		(contexts ?? []).filter((c) => matches(query, c.name, c.region, c.auth))
	);

	/** Opens in the region last used for the profile, else its configured one. */
	const href = (c: ProfileInfo) =>
		`/aws/account?${new URLSearchParams({ profile: c.name, region: c.lastRegion ?? c.region ?? FALLBACK_REGION })}`;

	const AUTH: Record<string, string> = {
		keys: 'access keys',
		sso: 'SSO',
		role: 'assumed role',
		process: 'credential process',
		unknown: 'environment or instance role'
	};

	async function load() {
		error = null;
		try {
			contexts = await listProfiles();
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
		const timer = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(timer);
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
	title="AWS profiles"
	bind:query
	bind:search
	onQuery={() => (cursor = 0)}
	placeholder="Search profiles, then Enter"
>
	{#snippet actions()}
		<Button variant="outline" size="icon" onclick={load} aria-label="Reload AWS config" title="Reload ~/.aws">
			<RefreshCw />
		</Button>
	{/snippet}
	{#snippet notice()}
		{#if error}
			<Alert.Root variant="destructive">
				<Alert.Title>Could not read AWS config</Alert.Title>
				<Alert.Description>{error}</Alert.Description>
			</Alert.Root>
		{/if}
	{/snippet}
	<ul class="grid gap-0.5" bind:this={list}>
		{#if contexts === null}
			{#each { length: 4 } as _, i (i)}<li><Skeleton class="h-10 w-full rounded-lg" /></li>{/each}
		{:else if contexts.length === 0 && !error}
			<li class="text-muted-foreground py-12 text-center text-sm">
				No profiles in <code>~/.aws/config</code> or <code>~/.aws/credentials</code>. Run <code>aws configure</code> to add one.
			</li>
		{:else if shown.length === 0}
			<li class="text-muted-foreground py-12 text-center text-sm">No profile matches “{query}”.</li>
		{/if}
		{#each shown as c, i (c.name)}
			<li>
				<ListRow
					href={href(c)}
					selected={i === clamp(cursor, shown.length)}
					onhover={() => (cursor = i)}
					icon={Cloud}
					name={c.name}
					details={[
						c.lastRegion ?? c.region ?? `no region (uses ${FALLBACK_REGION})`,
						AUTH[c.auth] ?? c.auth
					].join(' · ')}
				>
					{#snippet badges()}
						{#if c.current}<Badge variant="secondary">current</Badge>{/if}
						{#if c.lastUsed}
							<Badge variant="outline" class="text-muted-foreground">{age(new Date(c.lastUsed).toISOString(), now)} ago</Badge>
						{/if}
					{/snippet}
				</ListRow>
			</li>
		{/each}
	</ul>
	{#snippet hints()}
		<span>type to search</span>
		<span><Kbd>↑</Kbd> <Kbd>↓</Kbd> move</span>
		<span><Kbd>Enter</Kbd> open</span>
		<span class="hidden sm:inline">recently used first</span>
		<span class="ml-auto">{contexts?.length ?? 0} profiles · <Kbd>?</Kbd> help</span>
	{/snippet}
</ListPage>
