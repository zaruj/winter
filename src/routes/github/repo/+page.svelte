<script lang="ts">
	import { page } from '$app/state';
	import Kbd from '#lib/components/Kbd.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import {
		connect,
		getRepo,
		isActive,
		listRuns,
		listWorkflows,
		openUrl,
		type Repo,
		type Run,
		type Workflow
	} from '#lib/github/api.js';
	import RunDetails from '#lib/github/RunDetails.svelte';
	import { duration, statusLabel, statusTone, TONE_DOT } from '#lib/github/status.js';
	import { age, matches } from '#lib/k8s/format.js';
	import ResourceTable, { type Column } from '#lib/k8s/ResourceTable.svelte';
	import { clamp, isTyping, moveCursor, plain, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import Layers from '@lucide/svelte/icons/layers';
	import Lock from '@lucide/svelte/icons/lock';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import Search from '@lucide/svelte/icons/search';
	import Workflow_ from '@lucide/svelte/icons/workflow';
	import { tick, untrack } from 'svelte';

	// One repository's GitHub Actions: workflows on the side, their latest runs in the middle.
	// Runs are fetched again every 10 seconds while any is going (every minute otherwise);
	// unchanged answers come back as 304 and cost no rate limit.

	const name = $derived(page.url.searchParams.get('name') ?? '');

	const ACTIVE_POLL_MS = 10_000;
	const IDLE_POLL_MS = 60_000;

	let repo = $state<Repo | null>(null);
	let fatal = $state<string | null>(null);
	let workflows = $state<Workflow[]>([]);
	/** null: all workflows. */
	let workflow = $state<number | null>(null);
	let runs = $state({ loading: false, error: null as string | null, items: [] as Run[], total: 0 });
	let cursor = $state(0);
	let query = $state('');
	let now = $state(Date.now());
	let filter: HTMLInputElement | null = $state(null);
	let scroller: HTMLElement | undefined = $state();
	/** Run shown in detail instead of the list. */
	let open = $state<Run | null>(null);
	let details: RunDetails | undefined = $state();

	const rows = $derived(
		runs.items.filter((r) => matches(query, r.title, r.name, r.branch, r.event, r.actor, r.sha, statusLabel(r.status, r.conclusion)))
	);
	const active = $derived(runs.items.some((r) => isActive(r.status)));
	/** All workflows first, then active ones, then disabled ones. */
	const sidebar = $derived([
		null,
		...workflows.filter((w) => w.state === 'active'),
		...workflows.filter((w) => w.state !== 'active')
	]);

	// --- loading ---------------------------------------------------------------------------

	async function loadRepo(n: string) {
		fatal = null;
		repo = null;
		workflows = [];
		try {
			// A fresh app start lands here signed out: sign in with the saved token first.
			await connect();
			const [r, w] = await Promise.all([getRepo(n), listWorkflows(n)]);
			if (n !== name) return;
			repo = r;
			workflows = w;
		} catch (e) {
			if (n === name) fatal = String(e);
		}
	}

	let generation = 0;
	async function loadRuns() {
		const mine = ++generation;
		const [n, w] = [name, workflow];
		runs.loading = true;
		try {
			const found = await listRuns(n, w);
			if (mine !== generation) return;
			runs.items = found.runs;
			runs.total = found.total;
			runs.error = null;
		} catch (e) {
			if (mine === generation) runs.error = String(e);
		} finally {
			if (mine === generation) runs.loading = false;
		}
	}

	$effect(() => {
		const n = name;
		untrack(() => {
			open = null;
			workflow = null;
			loadRepo(n);
		});
	});

	// Runs: when the repository is ready or the workflow changes, then on a timer.
	$effect(() => {
		if (!repo) return;
		workflow;
		untrack(() => {
			runs.items = [];
			cursor = 0;
			loadRuns();
		});
		let timer: ReturnType<typeof setTimeout>;
		const poll = () => {
			timer = setTimeout(
				() => {
					if (!document.hidden && !open) loadRuns();
					poll();
				},
				untrack(() => active) ? ACTIVE_POLL_MS : IDLE_POLL_MS
			);
		};
		poll();
		return () => clearTimeout(timer);
	});

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1_000);
		return () => clearInterval(timer);
	});

	// --- cursor and keys -------------------------------------------------------------------

	$effect(() => {
		const fixed = clamp(cursor, rows.length);
		if (fixed !== cursor && !runs.loading) cursor = fixed;
	});

	async function moveTo(index: number) {
		cursor = index;
		await tick();
		scrollToSelected(scroller);
	}

	function stepWorkflow(by: number) {
		const at = sidebar.indexOf(sidebar.find((w) => (w?.id ?? null) === workflow) ?? null);
		const next = sidebar[(at + by + sidebar.length) % sidebar.length];
		pick(next?.id ?? null);
	}

	function pick(id: number | null) {
		open = null;
		workflow = id;
	}

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
		if (!inFilter && open && details?.handleKey(event)) return true;
		if (open) return moveCursor(event, 0, 1, true) !== null || undefined;

		const next = moveCursor(event, cursor, rows.length, !inFilter);
		if (next !== null) {
			moveTo(next);
			return true;
		}
		if (inFilter || !plain(event)) return;
		switch (event.key) {
			case 'Enter': {
				const run = rows[cursor];
				if (run) open = run;
				return !!run;
			}
			case '[':
				stepWorkflow(-1);
				return true;
			case ']':
				stepWorkflow(1);
				return true;
			case '/':
				filter?.focus();
				return true;
			case 'r':
				loadRuns();
				return true;
			case 'o': {
				const run = rows[cursor];
				const url = run?.htmlUrl ?? repo?.htmlUrl;
				if (url) openUrl(`${url}${run ? '' : '/actions'}`);
				return true;
			}
		}
	});

	// --- columns ---------------------------------------------------------------------------

	const columns = $derived<Column<Run>[]>([
		// Fixed widths; the run title takes the rest and is cut short, never scrolling sideways.
		{ header: 'Status', class: 'w-28 truncate', cell: status },
		{ header: 'Run', class: 'truncate', cell: title },
		{ header: 'Branch', class: 'w-32 truncate', hide: 'sm', text: (r) => r.branch ?? '-' },
		{ header: 'Event', class: 'w-28 truncate text-muted-foreground', hide: 'md', text: (r) => r.event },
		{ header: 'Actor', class: 'w-32 truncate text-muted-foreground', hide: 'lg', text: (r) => r.actor ?? '-' },
		{
			header: 'Duration',
			class: 'w-20 text-right tabular-nums',
			hide: 'sm',
			text: (r) => duration(r.startedAt, isActive(r.status) ? null : r.updatedAt, now)
		},
		{ header: 'Age', class: 'w-14 text-muted-foreground text-right', text: (r) => age(r.createdAt, now) }
	]);

	const workflowName = (id: number | null) =>
		id === null ? 'All workflows' : (workflows.find((w) => w.id === id)?.name ?? `Workflow ${id}`);
</script>

{#snippet status(r: Run)}
	{@const tone = statusTone(r.status, r.conclusion)}
	<span class="inline-flex items-center gap-2">
		<span class="size-2 shrink-0 rounded-full {TONE_DOT[tone]} {tone === 'pending' ? 'animate-pulse' : ''}"></span>
		{statusLabel(r.status, r.conclusion)}
	</span>
{/snippet}

{#snippet title(r: Run)}
	<div class="truncate" title={r.title}>
		<span class="font-medium" title={r.title}>{r.title || r.name}</span>
		<span class="text-muted-foreground ml-2 text-xs">
			{workflow === null ? `${r.name} ` : ''}#{r.number}
		</span>
	</div>
{/snippet}

<main class="flex h-screen flex-col">
	<header class="flex items-center gap-2 border-b px-2 py-2 sm:gap-3 sm:px-4 sm:py-3">
		<Button variant="ghost" size="icon" href="/github" aria-label="Back to repositories">
			<ArrowLeft />
		</Button>
		<div class="min-w-0 flex-1">
			<div class="flex items-center gap-2">
				<h1 class="truncate font-semibold">{name}</h1>
				{#if repo?.private}<Lock class="text-muted-foreground size-3.5 shrink-0" aria-label="private" />{/if}
				{#if repo?.defaultBranch}<Badge variant="secondary" class="hidden sm:inline-flex">{repo.defaultBranch}</Badge>{/if}
			</div>
			{#if repo?.description}<div class="text-muted-foreground hidden truncate text-xs sm:block">{repo.description}</div>{/if}
		</div>
		{#if repo}
			<Button variant="outline" size="icon" onclick={() => openUrl(`${repo!.htmlUrl}/actions`)} aria-label="Open on GitHub" title="Open on GitHub">
				<ExternalLink />
			</Button>
		{/if}
		<Button variant="outline" size="icon" onclick={loadRuns} aria-label="Refresh" title="Refresh (r)">
			<RefreshCw class={runs.loading ? 'animate-spin' : ''} />
		</Button>
	</header>

	{#if fatal}
		<div class="p-4">
			<Alert.Root variant="destructive">
				<Alert.Title>Could not open {name}</Alert.Title>
				<Alert.Description class="whitespace-pre-wrap">{fatal}</Alert.Description>
			</Alert.Root>
			<div class="mt-3 flex gap-2">
				<Button onclick={() => loadRepo(name)}>Try again</Button>
				<Button variant="outline" href="/github">Choose another repository</Button>
			</div>
		</div>
	{:else}
		<div class="flex min-h-0 flex-1">
			<nav class="flex w-12 shrink-0 flex-col overflow-y-auto border-r px-1.5 py-2 md:w-56 md:px-2" aria-label="Workflows">
				<div class="text-muted-foreground mb-0.5 hidden px-2 text-[11px] font-medium tracking-wide uppercase md:block">
					Workflows
				</div>
				{#each sidebar as w (w?.id ?? 'all')}
					{@const id = w?.id ?? null}
					<button
						class="data-active:bg-muted hover:bg-muted/50 flex h-7 w-full items-center justify-center gap-2 rounded-md px-2 text-sm md:justify-start"
						data-active={workflow === id || undefined}
						data-workflow={id ?? 'all'}
						title={w ? `${w.name} (${w.path})` : 'All workflows'}
						onclick={() => pick(id)}
					>
						{#if w}
							<Workflow_ class="size-4 shrink-0" />
						{:else}
							<Layers class="size-4 shrink-0" />
						{/if}
						<span class="hidden min-w-0 flex-1 truncate text-left md:inline {w && w.state !== 'active' ? 'text-muted-foreground' : ''}">
							{w ? w.name : 'All workflows'}
						</span>
						{#if w && w.state !== 'active'}<span class="text-muted-foreground hidden text-[10px] md:inline">off</span>{/if}
					</button>
				{/each}
			</nav>

			<div class="flex min-h-0 min-w-0 flex-1 flex-col">
				<div class="flex items-center gap-2 px-2 pt-2 sm:px-4 sm:pt-3">
					<h2 class="min-w-0 flex-1 truncate text-sm font-medium">
						{workflowName(workflow)}
						{#if open}<span class="text-muted-foreground"> / #{open.number}</span>{/if}
						<span class="text-muted-foreground font-normal">
							· {runs.total > runs.items.length ? `latest ${runs.items.length} of ${runs.total}` : `${runs.items.length} runs`}
							{#if active}· live{/if}
						</span>
					</h2>
					<div class="relative w-40 shrink-0 sm:w-56 {open ? 'invisible' : ''}">
						<Search class="text-muted-foreground absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
						<Input class="h-8 pl-8" placeholder="Filter  /" bind:value={query} bind:ref={filter} />
					</div>
				</div>
				<div
					class="relative min-h-0 flex-1 px-2 sm:px-4 {open ? 'overflow-hidden pb-2' : 'overflow-auto'}"
					bind:this={scroller}
				>
					{#if open}
						<div class="bg-card h-full min-h-0 rounded-lg border">
							{#key open.id}
								<RunDetails bind:this={details} repo={name} run={open} {now} onClose={() => (open = null)} />
							{/key}
						</div>
					{:else if repo && workflows.length === 0 && !runs.loading && runs.items.length === 0}
						<p class="text-muted-foreground py-12 text-center text-sm">
							{name} has no GitHub Actions workflows.
						</p>
					{:else}
						<ResourceTable
							list={runs}
							{rows}
							{columns}
							key={(r) => String(r.id)}
							fixed
							bind:cursor
							onActivate={(r) => (open = r)}
						/>
					{/if}
				</div>
			</div>
		</div>
		<footer class="text-muted-foreground flex flex-wrap gap-x-4 gap-y-1 border-t px-2 py-2 text-xs sm:px-4">
			<span class="hidden sm:inline"><Kbd>j</Kbd> <Kbd>k</Kbd> move</span>
			{#if open}
				<span><Kbd>Enter</Kbd> job log</span>
				<span><Kbd>Esc</Kbd> back</span>
			{:else}
				<span><Kbd>Enter</Kbd> jobs and steps</span>
				<span><Kbd>[</Kbd> <Kbd>]</Kbd> workflows</span>
				<span><Kbd>/</Kbd> filter</span>
			{/if}
			<span><Kbd>o</Kbd> open on GitHub</span>
			<span><Kbd>r</Kbd> refresh</span>
			<span class="ml-auto"><Kbd>?</Kbd> help</span>
		</footer>
	{/if}
</main>
