<script lang="ts">
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import {
		nodeUsage,
		streamKind,
		streamNode,
		type NodeDetail,
		type NodeUsage,
		type PodSummary
	} from '#lib/k8s/api.js';
	import { age, bytes, cpu, healthDot, nodeHealth } from '#lib/k8s/format.js';
	import { openLogs, openShell } from '#lib/k8s/pod-actions.js';
	import PodTable from '#lib/k8s/PodTable.svelte';
	import { ResourceList } from '#lib/k8s/resource-list.svelte.js';
	import UsageBar from '#lib/k8s/UsageBar.svelte';
	import { clamp, moveCursor, plain } from '#lib/keys.svelte.js';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import X from '@lucide/svelte/icons/x';
	import { onDestroy, untrack } from 'svelte';

	// One node, like `kubectl describe node`, with the pods running on it. Both stay live.
	// Fills the Nodes tab without scrolling: info on the left, pods on the right in pages sized
	// to the space. The cluster page passes its keys in through `handleKey`.

	let {
		context,
		name,
		cursor = $bindable(0),
		onClose
	}: {
		context: string;
		name: string;
		/** Selected pod in the node's pod list. */
		cursor?: number;
		onClose: () => void;
	} = $props();

	const node = new ResourceList<NodeDetail>();
	const pods = new ResourceList<PodSummary>();
	let usage = $state<NodeUsage | null>(null);
	let now = $state(Date.now());
	let podsArea: HTMLElement | undefined = $state();
	/** Pods that fit in the pod area, so the list pages instead of scrolling. */
	let perPage = $state(10);

	const detail = $derived(node.items[0]);
	const sortedPods = $derived(
		[...pods.items].sort((a, b) => a.namespace.localeCompare(b.namespace) || a.name.localeCompare(b.name))
	);
	const start = $derived(Math.floor(cursor / perPage) * perPage);
	const pagePods = $derived(sortedPods.slice(start, start + perPage));
	const pages = $derived(Math.max(1, Math.ceil(sortedPods.length / perPage)));

	// Rows that fit = (area height - header row) / row height, measured from the table itself.
	$effect(() => {
		const area = podsArea;
		if (!area) return;
		const measure = () => {
			const head = area.querySelector('thead tr') as HTMLElement | null;
			const row = area.querySelector('tbody tr[data-selected], tbody tr') as HTMLElement | null;
			const rowHeight = row?.offsetHeight || 33;
			const fit = Math.floor((area.clientHeight - (head?.offsetHeight ?? 33)) / rowHeight);
			perPage = Math.max(1, fit);
		};
		const observer = new ResizeObserver(() => requestAnimationFrame(measure));
		observer.observe(area);
		return () => observer.disconnect();
	});

	/** Capacity worth showing: CPU, memory, pods, storage, and anything else not zero. */
	const capacityKeys = $derived(
		detail
			? Object.keys(detail.capacity).filter(
					(k) => ['cpu', 'memory', 'pods', 'ephemeral-storage'].includes(k) || detail.capacity[k] !== '0'
				)
			: []
	);
	const MAX_LABELS = 10;
	const labels = $derived(detail ? Object.entries(detail.labels) : []);

	const running = $derived(pods.items.filter((p) => p.status !== 'Completed' && p.status !== 'Error').length);

	export function load() {
		const ctx = context;
		const node_ = name;
		node.load((sink) => streamNode(ctx, node_, sink));
		pods.load((sink) => streamKind('pods', ctx, null, sink, `spec.nodeName=${node_}`));
		refreshUsage();
	}

	async function refreshUsage() {
		const ctx = context;
		const node_ = name;
		try {
			const all = await nodeUsage(ctx);
			if (ctx === context && node_ === name) usage = all?.find((u) => u.name === node_) ?? null;
		} catch {
			usage = null;
		}
	}

	$effect(() => {
		context;
		name;
		untrack(() => {
			node.reset();
			pods.reset();
			usage = null;
			load();
		});
	});

	onDestroy(() => {
		node.stop();
		pods.stop();
	});

	// metrics-server has no watch; poll it.
	$effect(() => {
		const usageTimer = setInterval(refreshUsage, 15_000);
		const clock = setInterval(() => (now = Date.now()), 10_000);
		return () => {
			clearInterval(usageTimer);
			clearInterval(clock);
		};
	});

	$effect(() => {
		if (pods.loading) return;
		const fixed = clamp(cursor, sortedPods.length);
		if (fixed !== cursor) cursor = fixed;
	});

	/** Keys for the panel: move in pods, logs, shell, refresh, Esc to close. */
	export function handleKey(event: KeyboardEvent): boolean | void {
		const next = moveCursor(event, cursor, sortedPods.length, true);
		if (next !== null) {
			cursor = next;
			return true;
		}
		if (!plain(event)) return;
		const pod = sortedPods[cursor];
		switch (event.key) {
			case 'Enter':
			case 'l':
				if (pod) openLogs(context, pod);
				return !!pod;
			case 's':
				if (pod) openShell(context, pod);
				return !!pod;
			case 'r':
				load();
				return true;
			case 'Escape':
				onClose();
				return true;
		}
	}

	const conditionBad = (type: string, status: string) =>
		type === 'Ready' ? status !== 'True' : status === 'True';

	const quantity = (value: string | undefined) => value ?? '-';
</script>

{#snippet field(label: string, value: string | null | undefined)}
	<dt class="text-muted-foreground truncate">{label}</dt>
	<dd class="min-w-0 truncate" title={value ?? ''}>{value || '-'}</dd>
{/snippet}

{#snippet heading(title: string, count?: number)}
	<h2 class="text-muted-foreground mb-1.5 text-[11px] font-medium tracking-wide uppercase">
		{title}{#if count !== undefined}<span class="ml-1 tabular-nums">{count}</span>{/if}
	</h2>
{/snippet}

<!-- Compact, like `kubectl describe node`, laid out to fit the area exactly. -->
<div class="@container flex h-full min-h-0 flex-col text-xs">
	<header class="flex items-center gap-1.5 border-b px-3 py-1.5">
		<div class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
			<h1 class="truncate text-sm font-semibold">{name}</h1>
			{#if detail}
				<span class="inline-flex items-center gap-1">
					<span class="size-1.5 rounded-full {healthDot[nodeHealth(detail.status)]}"></span>
					{detail.status}
				</span>
				{#each detail.roles as role (role)}<Badge variant="secondary" class="h-4 px-1.5 text-[10px]">{role}</Badge>{/each}
			{/if}
		</div>
		<Button variant="ghost" size="icon-xs" onclick={load} aria-label="Refresh node" title="Refresh (r)">
			<RefreshCw class={node.loading || pods.loading ? 'animate-spin' : ''} />
		</Button>
		<Button variant="ghost" size="icon-xs" onclick={onClose} aria-label="Close details" title="Back to nodes (Esc)">
			<X />
		</Button>
	</header>

	<!-- Info and pods side by side, or stacked on narrow windows. Nothing scrolls: pods page, and taints and labels clip to the room left. -->
	<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-3 @xl:grid @xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
		<div class="flex min-h-0 flex-col overflow-hidden">
			{#if node.error}
				<p class="text-destructive">{node.error}</p>
			{:else if !detail && node.loading}
				<Skeleton class="mb-3 h-10" />
				<Skeleton class="h-24" />
			{:else if !detail}
				<p class="text-muted-foreground py-6 text-center">Node {name} is gone.</p>
			{/if}

			{#if detail}
				<div class="flex min-h-0 flex-1 flex-col gap-3">
					<section class="grid shrink-0 grid-cols-3 gap-3" data-resources>
						<div>
							<div class="text-muted-foreground mb-1">CPU</div>
							<UsageBar used={usage?.cpuMillis} total={detail.cpuMillis} format={cpu} />
						</div>
						<div>
							<div class="text-muted-foreground mb-1">Memory</div>
							<UsageBar used={usage?.memoryBytes} total={detail.memoryBytes} format={bytes} />
						</div>
						<div>
							<div class="text-muted-foreground mb-1">Pods</div>
							<UsageBar
								used={pods.loading && !pods.items.length ? null : running}
								total={detail.pods}
								format={(v) => (v == null ? '-' : String(v))}
							/>
						</div>
						{#if !usage}
							<p class="text-muted-foreground col-span-3 -mt-2 text-[11px]">CPU and memory use need metrics-server.</p>
						{/if}
					</section>

					<section class="shrink-0">
						{@render heading('System')}
						<dl class="grid grid-cols-[max-content_minmax(0,1fr)_max-content_minmax(0,1fr)] gap-x-3 gap-y-0.5">
							{#each detail.addresses as a (a.type + a.address)}
								{@render field(a.type, a.address)}
							{/each}
							{@render field('OS', `${detail.osImage} (${detail.arch})`)}
							{@render field('Kernel', detail.kernel)}
							{@render field('Runtime', detail.runtime)}
							{@render field('Kubelet', detail.version)}
							{@render field('Pod CIDRs', detail.podCidrs.join(', '))}
							{@render field('Provider', detail.providerId)}
							{@render field('Age', age(detail.created, now))}
							{@render field('Images', String(detail.images))}
						</dl>
					</section>

					<section class="shrink-0">
						{@render heading('Capacity / allocatable')}
						<dl class="grid grid-cols-[max-content_minmax(0,1fr)_max-content_minmax(0,1fr)] gap-x-3 gap-y-0.5">
							{#each capacityKeys as key (key)}
								{@const cap = quantity(detail.capacity[key])}
								{@const alloc = quantity(detail.allocatable[key])}
								{@render field(key, cap === alloc ? cap : `${cap} / ${alloc}`)}
							{/each}
						</dl>
					</section>

					<section class="shrink-0">
						{@render heading('Conditions')}
						<ul class="grid gap-0.5">
							{#each detail.conditions as c (c.type)}
								<li class="flex min-w-0 items-center gap-2" title={c.message ?? ''}>
									<span class="size-1.5 shrink-0 rounded-full {conditionBad(c.type, c.status) ? 'bg-red-500' : 'bg-emerald-500'}"></span>
									<span class="w-28 shrink-0 truncate font-medium">{c.type}</span>
									<span class="w-10 shrink-0">{c.status}</span>
									<span class="text-muted-foreground min-w-0 flex-1 truncate">{c.reason ?? ''}</span>
									<span class="text-muted-foreground shrink-0 tabular-nums">{age(c.lastTransition, now)}</span>
								</li>
							{/each}
						</ul>
					</section>

					<section class="min-h-0 shrink overflow-hidden">
						{@render heading('Taints', detail.taints.length)}
						{#if detail.taints.length}
							<div class="flex flex-wrap gap-1">
								{#each detail.taints as t (t.key + t.effect)}
									<Badge variant="outline" class="h-4 max-w-full truncate px-1.5 text-[10px] font-normal">
										{t.key}{t.value ? `=${t.value}` : ''}:{t.effect}
									</Badge>
								{/each}
							</div>
						{:else}
							<p class="text-muted-foreground">None</p>
						{/if}
					</section>

					<section class="min-h-0 flex-1 overflow-hidden">
						{@render heading('Labels', labels.length)}
						<div class="flex flex-wrap gap-1">
							{#each labels.slice(0, MAX_LABELS) as [key, value] (key)}
								<Badge variant="secondary" class="h-4 max-w-full truncate px-1.5 text-[10px] font-normal" title="{key}={value}">
									{key}{value ? `=${value}` : ''}
								</Badge>
							{/each}
							{#if labels.length > MAX_LABELS}
								<Badge
									variant="outline"
									class="h-4 px-1.5 text-[10px] font-normal"
									title={labels.slice(MAX_LABELS).map(([k, v]) => (v ? `${k}=${v}` : k)).join('\n')}
								>
									+{labels.length - MAX_LABELS} more
								</Badge>
							{/if}
						</div>
					</section>
				</div>
			{/if}
		</div>

		<section class="flex min-h-48 min-w-0 flex-1 flex-col @xl:min-h-0" data-node-pods>
			<div class="flex items-center gap-2">
				<div class="flex-1">{@render heading('Pods', pods.items.length)}</div>
				{#if pages > 1}
					<span class="text-muted-foreground -mt-1.5 tabular-nums">
						{start + 1}–{Math.min(start + perPage, sortedPods.length)} of {sortedPods.length}
					</span>
					<Button variant="ghost" size="icon-xs" class="-mt-1.5" aria-label="Previous pods" disabled={start === 0} onclick={() => (cursor = Math.max(0, start - perPage))}>
						<ChevronLeft />
					</Button>
					<Button variant="ghost" size="icon-xs" class="-mt-1.5" aria-label="Next pods" disabled={start + perPage >= sortedPods.length} onclick={() => (cursor = Math.min(sortedPods.length - 1, start + perPage))}>
						<ChevronRight />
					</Button>
				{/if}
			</div>
			<div class="min-h-0 flex-1 overflow-hidden" bind:this={podsArea}>
				<PodTable
					{context}
					list={pods}
					rows={pagePods}
					{now}
					compact
					bind:cursor={() => cursor - start, (i) => (cursor = start + i)}
				/>
			</div>
		</section>
	</div>
</div>
