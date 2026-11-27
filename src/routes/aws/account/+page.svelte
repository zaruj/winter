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
	import {
		connect,
		listRegions,
		streamAws,
		type AwsKind,
		type AwsKinds,
		type Ec2Instance,
		type EksCluster,
		type Identity,
		type LoadBalancer
	} from '#lib/aws/api.js';
	import { age, matches } from '#lib/k8s/format.js';
	import { ResourceList } from '#lib/k8s/resource-list.svelte.js';
	import ResourceTable, { type Column } from '#lib/k8s/ResourceTable.svelte';
	import { clamp, isTyping, moveCursor, plain, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import AwsCommandBar from '#lib/aws/CommandBar.svelte';
	import Ec2Details from '#lib/aws/Ec2Details.svelte';
	import { GROUPS, ORDER, serviceOf } from '#lib/aws/services.js';
	import { balancerTone, clusterTone, instanceTone, TONE_DOT, type Tone } from '#lib/aws/tone.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import Search from '@lucide/svelte/icons/search';
	import { tick, untrack } from 'svelte';

	// One AWS profile in one region: EC2 instances, EKS clusters and load balancers.
	// AWS has no watch, so lists load when the page opens or the region changes, and on `r`.

	const profile = $derived(page.url.searchParams.get('profile') ?? 'default');
	const region = $derived(page.url.searchParams.get('region') ?? 'us-east-1');

	const labelOf = (kind: AwsKind) => serviceOf(kind).label;

	type Connection =
		| { state: 'connecting' }
		| { state: 'connected'; identity: Identity }
		| { state: 'failed'; error: string };

	let connection = $state<Connection>({ state: 'connecting' });
	let regions = $state<string[]>([]);
	let tab = $state<AwsKind>('ec2');
	let cursors = $state<Record<AwsKind, number>>({ ec2: 0, eks: 0, elb: 0 });
	let query = $state('');
	let now = $state(Date.now());
	let filter: HTMLInputElement | null = $state(null);
	let scroller: HTMLElement | undefined = $state();
	let commandOpen = $state(false);
	/** Instance shown in detail instead of the EC2 list. */
	let instance = $state<string | null>(null);
	let details: Ec2Details | undefined = $state();
	const detailsOpen = $derived(tab === 'ec2' && instance !== null);

	const lists = {
		ec2: new ResourceList<Ec2Instance>(),
		eks: new ResourceList<EksCluster>(),
		elb: new ResourceList<LoadBalancer>()
	} satisfies { [K in AwsKind]: ResourceList<AwsKinds[K]> };

	const busy = $derived(Object.values(lists).some((l) => l.loading));

	const rows = $derived({
		ec2: lists.ec2.items.filter((i) =>
			matches(query, i.id, i.name, i.state, i.type, i.privateIp, i.publicIp, i.zone, i.vpc)
		),
		eks: lists.eks.items.filter((c) => matches(query, c.name, c.status, c.version, c.vpc)),
		elb: lists.elb.items.filter((l) => matches(query, l.name, l.type, l.scheme, l.state, l.dnsName, l.vpc))
	});

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
		for (const kind of ORDER) {
			const list = lists[kind] as unknown as ResourceList<AwsKinds[typeof kind]>;
			list.load(async (sink) => {
				await streamAws(kind, p, r, sink.page);
			});
		}
	}

	$effect(() => {
		profile;
		region;
		untrack(() => {
			for (const list of Object.values(lists)) list.reset();
			for (const kind of ORDER) cursors[kind] = 0;
			instance = null;
			loadAll();
		});
	});

	$effect(() => {
		const p = profile;
		untrack(() => loadRegions(p, region));
	});

	/** Runs a `:` command: show a service, switching region first if one was given. */
	function runCommand(kind: AwsKind | null, next: string | null) {
		commandOpen = false;
		if (kind) {
			tab = kind;
			if (kind === 'ec2') instance = null;
		}
		if (next) pickRegion(next);
	}

	function openInstance(i: Ec2Instance) {
		instance = i.id;
	}

	function pickRegion(next: string) {
		if (!next || next === region) return;
		goto(`?${new URLSearchParams({ profile, region: next })}`, { replace: true });
	}

	// --- cursor ----------------------------------------------------------------------------

	$effect(() => {
		for (const kind of ORDER) {
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
			for (const kind of ORDER) cursors[kind] = 0;
		});
	});

	async function moveTo(index: number) {
		cursors[tab] = index;
		await tick();
		scrollToSelected(scroller?.querySelector<HTMLElement>(`[data-tab=${tab}]`) ?? undefined);
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
		if (!inFilter && detailsOpen && details?.handleKey(event)) return true;
		if (detailsOpen && moveCursor(event, 0, 1, true) !== null) return true;

		const next = moveCursor(event, cursors[tab], rows[tab].length, !inFilter);
		if (next !== null) {
			moveTo(next);
			return true;
		}
		if (inFilter || !plain(event)) return;

		const index = Number(event.key) - 1;
		if (Number.isInteger(index) && index >= 0 && index < ORDER.length) {
			tab = ORDER[index];
			return true;
		}
		switch (event.key) {
			case 'Enter': {
				const row = tab === 'ec2' ? rows.ec2[cursors.ec2] : undefined;
				if (row) openInstance(row);
				return !!row;
			}
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
				loadAll(connection.state === 'failed');
				return true;
		}
	});

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 10_000);
		return () => clearInterval(timer);
	});

	// --- columns ---------------------------------------------------------------------------

	const dash = (v: string | null | undefined) => v || '-';
	const ageColumn = <T extends { name: string }>(at: (r: T) => string | null): Column<T> => ({
		header: 'Age',
		class: 'text-muted-foreground text-right',
		hide: 'sm',
		text: (r) => age(at(r), now)
	});

	const columns = $derived({
		ec2: [
			{ header: 'Name', cell: instanceName },
			{ header: 'State', cell: instanceState },
			{ header: 'Type', text: (i) => i.type },
			{ header: 'Private IP', class: 'tabular-nums', hide: 'sm', text: (i) => dash(i.privateIp) },
			{ header: 'Public IP', class: 'tabular-nums', hide: 'md', text: (i) => dash(i.publicIp) },
			{ header: 'Zone', class: 'text-muted-foreground', hide: 'lg', text: (i) => dash(i.zone) },
			{ header: 'Platform', class: 'text-muted-foreground', hide: 'xl', text: (i) => dash(i.platform) },
			ageColumn<Ec2Instance>((i) => i.launched)
		] as Column<Ec2Instance>[],
		eks: [
			{ header: 'Name', class: 'font-medium', text: (c) => c.name },
			{ header: 'Status', cell: clusterStatus },
			{ header: 'Version', text: (c) => dash(c.version) },
			{ header: 'Platform', class: 'text-muted-foreground', hide: 'lg', text: (c) => dash(c.platformVersion) },
			{ header: 'Endpoint', class: 'text-muted-foreground max-w-80 truncate', hide: 'md', text: (c) => dash(c.endpoint) },
			{ header: 'VPC', class: 'text-muted-foreground', hide: 'xl', text: (c) => dash(c.vpc) },
			ageColumn<EksCluster>((c) => c.created)
		] as Column<EksCluster>[],
		elb: [
			{ header: 'Name', class: 'font-medium', text: (l) => l.name },
			{ header: 'Type', text: (l) => l.type },
			{ header: 'State', cell: balancerState },
			{ header: 'Scheme', hide: 'sm', text: (l) => dash(l.scheme) },
			{ header: 'DNS name', class: 'text-muted-foreground max-w-96 truncate', hide: 'md', text: (l) => dash(l.dnsName) },
			{ header: 'Zones', class: 'text-muted-foreground', hide: 'xl', text: (l) => l.zones.join(', ') || '-' },
			ageColumn<LoadBalancer>((l) => l.created)
		] as Column<LoadBalancer>[]
	});

	const keys = {
		ec2: (i: Ec2Instance) => i.id,
		eks: (c: EksCluster) => c.name,
		elb: (l: LoadBalancer) => `${l.type}/${l.name}`
	};
</script>

{#snippet dot(tone: Tone, label: string)}
	<span class="inline-flex items-center gap-2">
		<span class="size-2 shrink-0 rounded-full {TONE_DOT[tone]}"></span>
		{label}
	</span>
{/snippet}

{#snippet instanceName(i: Ec2Instance)}
	<span class="font-medium">{i.name || i.id}</span>
	{#if i.name}<span class="text-muted-foreground ml-2 text-xs">{i.id}</span>{/if}
{/snippet}

{#snippet instanceState(i: Ec2Instance)}
	{@render dot(instanceTone(i.state), i.state)}
{/snippet}

{#snippet clusterStatus(c: EksCluster)}
	{@render dot(clusterTone(c.status), c.status)}
{/snippet}

{#snippet balancerState(l: LoadBalancer)}
	{@render dot(balancerTone(l.state), l.state ?? '-')}
{/snippet}

{#snippet count(kind: AwsKind)}
	<span class="text-muted-foreground tabular-nums">
		{lists[kind].loading && lists[kind].items.length === 0 ? '…' : lists[kind].items.length}
	</span>
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
			bind:value={() => tab, (v) => (tab = v as AwsKind)}
			orientation="vertical"
			class="flex min-h-0 flex-1 flex-row gap-0"
		>
			<nav class="flex w-12 shrink-0 flex-col overflow-y-auto border-r px-1.5 py-2 md:w-52 md:px-2" aria-label="Services">
				<Tabs.List class="h-auto w-full flex-col items-stretch gap-0 bg-transparent p-0">
					{#each GROUPS as group (group.label)}
						<div class="text-muted-foreground mt-2 mb-0.5 hidden px-2 text-[11px] font-medium tracking-wide uppercase first:mt-0 md:block">
							{group.label}
						</div>
						<div class="bg-border mx-1 my-1.5 h-px first:hidden md:hidden"></div>
						{#each group.kinds as kind (kind)}
							{@const info = serviceOf(kind)}
							<Tabs.Trigger
								value={kind}
								title="{info.label} ({ORDER.indexOf(kind) + 1})"
								data-tab-trigger={kind}
								class="data-active:bg-muted h-7 w-full flex-none justify-center gap-2 px-2 md:justify-start"
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
						{#if detailsOpen}<span class="text-muted-foreground"> / {lists.ec2.items.find((i) => i.id === instance)?.name || instance}</span>{/if}
						<span class="text-muted-foreground font-normal">· {region}</span>
					</h2>
					<div class="relative w-40 shrink-0 sm:w-56">
						<Search class="text-muted-foreground absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
						<Input class="h-8 pl-8" placeholder="Filter  /" bind:value={query} bind:ref={filter} />
					</div>
				</div>
				<div
					class="relative min-h-0 flex-1 px-2 sm:px-4 {detailsOpen ? 'overflow-hidden pb-2' : 'overflow-auto pb-4'}"
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
					<!-- Instance details replace the list and fill the area exactly: nothing scrolls. -->
					<Tabs.Content value="ec2" data-tab="ec2" class={detailsOpen ? 'h-full' : undefined}>
						{#if instance}
							<div class="bg-card h-full min-h-0 rounded-lg border">
								{#key instance}
									<Ec2Details bind:this={details} {profile} {region} id={instance} {now} onClose={() => (instance = null)} />
								{/key}
							</div>
						{:else}
							<ResourceTable
								list={lists.ec2}
								rows={rows.ec2}
								columns={columns.ec2}
								key={keys.ec2}
								bind:cursor={cursors.ec2}
								onActivate={openInstance}
							/>
						{/if}
					</Tabs.Content>
					<Tabs.Content value="eks" data-tab="eks">
						<ResourceTable list={lists.eks} rows={rows.eks} columns={columns.eks} key={keys.eks} bind:cursor={cursors.eks} />
					</Tabs.Content>
					<Tabs.Content value="elb" data-tab="elb">
						<ResourceTable list={lists.elb} rows={rows.elb} columns={columns.elb} key={keys.elb} bind:cursor={cursors.elb} />
					</Tabs.Content>
				</div>
			</div>
		</Tabs.Root>
		<footer class="text-muted-foreground flex flex-wrap gap-x-4 gap-y-1 border-t px-2 py-2 text-xs sm:px-4">
			<span class="hidden sm:inline"><Kbd>j</Kbd> <Kbd>k</Kbd> move</span>
			<span class="hidden md:inline"><Kbd>1</Kbd>–<Kbd>3</Kbd> <Kbd>[</Kbd> <Kbd>]</Kbd> services</span>
			<span><Kbd>:</Kbd> services</span>
			{#if detailsOpen}
				<span><Kbd>Esc</Kbd> back to instances</span>
			{:else if tab === 'ec2'}
				<span><Kbd>Enter</Kbd> details</span>
			{/if}
			<span><Kbd>/</Kbd> filter</span>
			<span><Kbd>r</Kbd> refresh</span>
			{#if !detailsOpen}<span class="hidden lg:inline"><Kbd>Esc</Kbd> back</span>{/if}
			<span class="ml-auto"><Kbd>?</Kbd> help</span>
		</footer>
	{/if}
</main>
