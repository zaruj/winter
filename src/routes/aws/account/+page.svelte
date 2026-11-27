<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Kbd from '#lib/components/Kbd.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import { connect, listRegions, streamAws, type AwsKind, type AwsKinds, type Identity } from '#lib/aws/api.js';
	import { specOf } from '#lib/aws/Columns.svelte';
	import CostOverview from '#lib/aws/CostOverview.svelte';
	import { matches } from '#lib/k8s/format.js';
	import { ResourceList } from '#lib/k8s/resource-list.svelte.js';
	import ResourceTable from '#lib/k8s/ResourceTable.svelte';
	import { clamp, isTyping, moveCursor, plain, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import AwsCommandBar from '#lib/aws/CommandBar.svelte';
	import AwsPanel from '#lib/aws/AwsPanel.svelte';
	import { GROUPS, LISTS, ORDER, serviceOf, type View } from '#lib/aws/services.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import Search from '@lucide/svelte/icons/search';
	import { tick, untrack } from 'svelte';

	// One AWS profile in one region: resource lists by service, and the account's cost overview.
	// AWS has no watch, so lists load when the page opens or the region changes, and on `r`.
	// The cost overview is billed per request, so it loads only when opened (and on `r` there).

	const profile = $derived(page.url.searchParams.get('profile') ?? 'default');
	const region = $derived(page.url.searchParams.get('region') ?? 'us-east-1');

	const labelOf = (kind: View) => serviceOf(kind).label;

	type Connection =
		| { state: 'connecting' }
		| { state: 'connected'; identity: Identity }
		| { state: 'failed'; error: string };

	let connection = $state<Connection>({ state: 'connecting' });
	let regions = $state<string[]>([]);
	let tab = $state<View>('ec2');
	let cursors = $state(Object.fromEntries(LISTS.map((k) => [k, 0])) as Record<AwsKind, number>);
	let query = $state('');
	let now = $state(Date.now());
	let filter: HTMLInputElement | null = $state(null);
	let scroller: HTMLElement | undefined = $state();
	let commandOpen = $state(false);
	/** Instance shown in detail instead of the EC2 list. */
	/** Details of the selected row beside the list (`d` or Enter); follows the cursor. */
	let panelOpen = $state(false);
	let panel: AwsPanel | undefined = $state();
	/** The list and the panel share the width; too narrow for both, the panel takes it all. */
	let bodyWidth = $state(0);
	let cost: CostOverview | undefined = $state();
	const detailsOpen = $derived(panelOpen && tab !== 'cost');
	const panelAlone = $derived(detailsOpen && bodyWidth < 760);
	/** A view that fills the area instead of a scrolling list. */
	const filling = $derived(tab === 'cost');
	const isList = (v: View): v is AwsKind => v !== 'cost';

	type Item = AwsKinds[AwsKind];
	const lists = Object.fromEntries(LISTS.map((k) => [k, new ResourceList<Item>()])) as Record<
		AwsKind,
		ResourceList<Item>
	>;

	const busy = $derived(Object.values(lists).some((l) => l.loading));

	const rows = $derived(
		Object.fromEntries(
			LISTS.map((k) => [k, lists[k].items.filter((r) => matches(query, ...specOf(k).search(r)))])
		) as Record<AwsKind, Item[]>
	);

	// --- loading ---------------------------------------------------------------------------

	let attempt = 0;
	async function connectTo(p: string, r: string, fresh: boolean) {
		const mine = ++attempt;
		connection = { state: 'connecting' };
		try {
			const identity = await connect(p, r, fresh);
			if (mine === attempt) connection = { state: 'connected', identity };
		} catch (e) {
			if (mine === attempt) connection = { state: 'failed', error: String(e) };
		}
	}

	async function loadRegions(p: string, r: string) {
		try {
			const found = await listRegions(p, r);
			if (p === profile) regions = found.includes(r) ? found : [...found, r].sort();
		} catch {
			if (p === profile) regions = [r];
		}
	}

	/** Connects, then lists everything; the lists share the one configuration. */
	async function loadAll(fresh = false) {
		const p = profile;
		const r = region;
		await connectTo(p, r, fresh);
		if (connection.state !== 'connected' || p !== profile || r !== region) return;
		for (const kind of LISTS) {
			lists[kind].load(async (sink) => {
				await streamAws(kind, p, r, sink.page);
			});
		}
	}

	$effect(() => {
		profile;
		region;
		untrack(() => {
			for (const list of Object.values(lists)) list.reset();
			for (const kind of LISTS) cursors[kind] = 0;
			panelOpen = false;
			loadAll();
		});
	});

	$effect(() => {
		const p = profile;
		untrack(() => loadRegions(p, region));
	});

	/** Runs a `:` command: show a service, switching region first if one was given. */
	function runCommand(kind: View | null, next: string | null) {
		commandOpen = false;
		if (kind) {
			tab = kind;
		}
		if (next) pickRegion(next);
	}

	/** The selected row of the current list, and how to ask for its details. */
	const selection = $derived.by(() => {
		if (!isList(tab)) return null;
		const row = rows[tab][cursors[tab]];
		const spec = specOf(tab);
		return row
			? { id: spec.detailId(row), title: spec.title(row), region: spec.detailRegion?.(row) ?? region }
			: null;
	});

	function pickRegion(next: string) {
		if (!next || next === region) return;
		goto(`?${new URLSearchParams({ profile, region: next })}`, { replace: true });
	}

	// --- cursor ----------------------------------------------------------------------------

	$effect(() => {
		for (const kind of LISTS) {
			if (lists[kind].loading) continue;
			const fixed = clamp(cursors[kind], rows[kind].length);
			if (fixed !== cursors[kind]) cursors[kind] = fixed;
		}
	});

	let lastQuery = '';
	$effect(() => {
		if (query === lastQuery) return;
		lastQuery = query;
		untrack(() => {
			for (const kind of LISTS) cursors[kind] = 0;
		});
	});

	async function moveTo(kind: AwsKind, index: number) {
		cursors[kind] = index;
		await tick();
		scrollToSelected(scroller?.querySelector<HTMLElement>(`[data-tab=${kind}]`) ?? undefined);
	}

	function stepTab(by: number) {
		const at = ORDER.indexOf(tab);
		tab = ORDER[(at + by + ORDER.length) % ORDER.length];
	}

	// --- keys ------------------------------------------------------------------------------

	useKeys((event) => {
		const inFilter = event.target === filter;
		if (inFilter) {
			if (event.key === 'Escape') {
				query = '';
				filter?.blur();
				return true;
			}
			if (event.key === 'Enter') {
				filter?.blur();
				return true;
			}
		} else if (isTyping(event)) {
			return;
		}
		if (!inFilter && plain(event) && event.key === ':') {
			commandOpen = true;
			return true;
		}
		if (!inFilter && detailsOpen && plain(event)) {
			if (event.key === 'Escape') {
				panelOpen = false;
				return true;
			}
			if (event.key === 'J' || event.key === 'PageDown') {
				panel?.scrollBy(1);
				return true;
			}
			if (event.key === 'K' || event.key === 'PageUp') {
				panel?.scrollBy(-1);
				return true;
			}
		}
		if (filling && moveCursor(event, 0, 1, true) !== null) return true;

		if (isList(tab)) {
			const next = moveCursor(event, cursors[tab], rows[tab].length, !inFilter);
			if (next !== null) {
				moveTo(tab, next);
				return true;
			}
		}
		if (inFilter || !plain(event)) return;

		const index = Number(event.key) - 1;
		if (Number.isInteger(index) && index >= 0 && index < Math.min(9, ORDER.length)) {
			tab = ORDER[index];
			return true;
		}
		switch (event.key) {
			case 'Enter':
				if (!isList(tab)) return;
				panelOpen = true;
				return true;
			case 'd':
				if (!isList(tab)) return;
				panelOpen = !panelOpen;
				return true;
			case '[':
				stepTab(-1);
				return true;
			case ']':
				stepTab(1);
				return true;
			case '/':
				filter?.focus();
				return true;
			case 'r':
				// On the cost overview, refresh only that: it is the one AWS bills for.
				if (tab === 'cost' && connection.state === 'connected') cost?.reload();
				else {
					loadAll(connection.state === 'failed');
					panel?.load();
				}
				return true;
		}
	});

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 10_000);
		return () => clearInterval(timer);
	});
</script>

{#snippet count(kind: View)}
	{#if isList(kind)}
		<span class="text-muted-foreground tabular-nums">
			{lists[kind].loading && lists[kind].items.length === 0 ? '…' : lists[kind].items.length}
		</span>
	{/if}
{/snippet}

<main class="flex h-screen flex-col">
	<header class="flex items-center gap-2 border-b px-2 py-2 sm:gap-3 sm:px-4 sm:py-3">
		<Button variant="ghost" size="icon" href="/aws" aria-label="Back to profiles">
			<ArrowLeft />
		</Button>
		<div class="min-w-0 flex-1">
			<div class="flex items-center gap-2">
				<h1 class="truncate font-semibold">{profile}</h1>
				{#if connection.state === 'connecting'}
					<Badge variant="secondary"><LoaderCircle class="animate-spin" /><span class="hidden sm:inline">Connecting</span></Badge>
				{:else if connection.state === 'connected'}
					<Badge variant="outline" class="hidden sm:inline-flex" title={connection.identity.arn}>
						<span class="size-1.5 rounded-full bg-emerald-500"></span>
						{connection.identity.account}
					</Badge>
				{:else}
					<Badge variant="destructive">Not signed in</Badge>
				{/if}
			</div>
			{#if connection.state === 'connected'}
				<div class="text-muted-foreground hidden truncate text-xs sm:block">{connection.identity.arn}</div>
			{/if}
		</div>

		<Select.Root type="single" value={region} onValueChange={pickRegion}>
			<Select.Trigger class="w-36 sm:w-44" aria-label="Region" data-region>
				<span class="truncate">{region}</span>
			</Select.Trigger>
			<Select.Content class="max-h-80">
				{#each regions.length ? regions : [region] as name (name)}
					<Select.Item value={name} label={name} />
				{/each}
			</Select.Content>
		</Select.Root>

		<Button
			variant="outline"
			size="icon"
			onclick={() => loadAll(connection.state === 'failed')}
			aria-label="Refresh"
			title="Refresh (r)"
		>
			<RefreshCw class={busy || connection.state === 'connecting' ? 'animate-spin' : ''} />
		</Button>
	</header>

	{#if connection.state === 'failed'}
		<div class="p-4">
			<Alert.Root variant="destructive">
				<Alert.Title>Could not sign in with profile {profile} in {region}</Alert.Title>
				<Alert.Description class="break-words whitespace-pre-wrap">{connection.error}</Alert.Description>
			</Alert.Root>
			<p class="text-muted-foreground mt-3 text-sm">
				For an SSO profile, run <code>aws sso login --profile {profile}</code>, then try again.
			</p>
			<div class="mt-3 flex flex-wrap gap-2">
				<Button onclick={() => loadAll(true)}>Try again</Button>
				<Button variant="outline" href="/aws">Choose another profile</Button>
			</div>
		</div>
	{:else}
		<Tabs.Root
			bind:value={() => tab, (v) => (tab = v as View)}
			orientation="vertical"
			class="flex min-h-0 flex-1 flex-row gap-0"
		>
			<nav class="flex w-12 shrink-0 flex-col overflow-y-auto border-r px-1.5 py-1.5 md:w-52 md:px-2 md:py-2" aria-label="Services">
				<Tabs.List class="h-auto w-full flex-col items-stretch gap-0 bg-transparent p-0">
					{#each GROUPS as group (group.label)}
						<div class="text-muted-foreground mt-2 mb-0.5 hidden px-2 text-[11px] font-medium tracking-wide uppercase first:mt-0 md:block">
							{group.label}
						</div>
						<div class="bg-border mx-1 my-1 h-px first:hidden md:hidden"></div>
						{#each group.kinds as kind (kind)}
							{@const info = serviceOf(kind)}
							<Tabs.Trigger
								value={kind}
								title={ORDER.indexOf(kind) < 9 ? `${info.label} (${ORDER.indexOf(kind) + 1})` : info.label}
								data-tab-trigger={kind}
								class="data-active:bg-muted h-6 w-full flex-none justify-center gap-2 px-2 md:h-7 md:justify-start"
							>
								<info.icon class="size-4 shrink-0" />
								<span class="hidden min-w-0 flex-1 truncate text-left md:inline">{info.label}</span>
								<span class="hidden text-xs md:inline">{@render count(kind)}</span>
							</Tabs.Trigger>
						{/each}
					{/each}
				</Tabs.List>
			</nav>
			<div class="flex min-h-0 min-w-0 flex-1 flex-col">
				<div class="flex items-center gap-2 px-2 pt-2 sm:px-4 sm:pt-3">
					<h2 class="min-w-0 flex-1 truncate text-sm font-medium">
						{labelOf(tab)}
						<span class="text-muted-foreground font-normal">· {serviceOf(tab).global ? 'all regions' : region}</span>
					</h2>
					<div class="relative w-40 shrink-0 sm:w-56 {tab === 'cost' ? 'invisible' : ''}">
						<Search class="text-muted-foreground absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
						<Input class="h-8 pl-8" placeholder="Filter  /" bind:value={query} bind:ref={filter} />
					</div>
				</div>
				<div class="flex min-h-0 flex-1" bind:clientWidth={bodyWidth}>
				<div
					class="relative min-h-0 min-w-0 flex-1 px-2 sm:px-4 {filling ? 'overflow-hidden pb-2' : 'overflow-auto'} {panelAlone ? 'hidden' : ''}"
					bind:this={scroller}
				>
					{#if commandOpen}
						<AwsCommandBar
							regions={regions.length ? regions : [region]}
							current={region}
							onRun={runCommand}
							onClose={() => (commandOpen = false)}
						/>
					{/if}
					{#each LISTS as kind (kind)}
						<Tabs.Content value={kind} data-tab={kind}>
							<ResourceTable
								list={lists[kind]}
								rows={rows[kind]}
								columns={specOf(kind).columns(now)}
								key={specOf(kind).key}
								bind:cursor={cursors[kind]}
								onActivate={() => (panelOpen = true)}
							/>
						</Tabs.Content>
					{/each}
					<Tabs.Content value="cost" data-tab="cost" class="h-full">
						{#if connection.state === 'connected'}
							<CostOverview bind:this={cost} {profile} {now} />
						{/if}
					</Tabs.Content>
				</div>
				{#if detailsOpen && isList(tab)}
					<div class="bg-card m-2 ml-0 min-h-0 min-w-0 rounded-lg border {panelAlone ? 'ml-2 flex-1' : 'w-[45%] max-w-[52rem] min-w-80'}">
						<AwsPanel
							bind:this={panel}
							{profile}
							region={selection?.region ?? region}
							kind={tab}
							label={labelOf(tab)}
							id={selection?.id ?? null}
							title={selection?.title ?? ''}
							onClose={() => (panelOpen = false)}
						/>
					</div>
				{/if}
				</div>
			</div>
		</Tabs.Root>
		<footer class="text-muted-foreground flex flex-wrap gap-x-4 gap-y-1 border-t px-2 py-2 text-xs sm:px-4">
			<span class="hidden sm:inline"><Kbd>j</Kbd> <Kbd>k</Kbd> move</span>
			<span class="hidden md:inline"><Kbd>1</Kbd>–<Kbd>9</Kbd> <Kbd>[</Kbd> <Kbd>]</Kbd> services</span>
			<span><Kbd>:</Kbd> services</span>
			{#if tab !== 'cost'}
				<span><Kbd>d</Kbd> {detailsOpen ? 'close details' : 'details'}</span>
			{/if}
			{#if tab !== 'cost'}<span><Kbd>/</Kbd> filter</span>{/if}
			<span><Kbd>r</Kbd> refresh</span>
			{#if !detailsOpen}<span class="hidden lg:inline"><Kbd>Esc</Kbd> back</span>{/if}
			<span class="ml-auto"><Kbd>?</Kbd> help</span>
		</footer>
	{/if}
</main>
