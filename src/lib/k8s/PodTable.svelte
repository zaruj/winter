<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import ScrollText from '@lucide/svelte/icons/scroll-text';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import type { PodSummary } from './api.js';
	import { age, healthDot, podHealth } from './format.js';
	import { openLogs, openShell } from './pod-actions.js';
	import { itemKey } from './resource-list.svelte.js';
	import ResourceTable, { type Column } from './ResourceTable.svelte';

	// Pods as `kubectl get pods` shows them, with Logs and Shell buttons on each row.
	let {
		context,
		list,
		rows,
		cursor = $bindable(0),
		now,
		showNamespace = true,
		showNode = true,
		compact = false
	}: {
		context: string;
		list: { loading: boolean; error: string | null; items: readonly unknown[] };
		rows: PodSummary[];
		cursor?: number;
		/** Current time for the Age column, ticking in the parent. */
		now: number;
		showNamespace?: boolean;
		showNode?: boolean;
		/** For a narrow panel: namespace under the name, fewer columns. */
		compact?: boolean;
	} = $props();

	const columns = $derived<Column<PodSummary>[]>(compact ? [
		{ header: 'Name', class: 'truncate', cell: podNameWithNamespace },
		{ header: 'Ready', class: 'w-11', text: (p) => `${p.ready}/${p.containers}` },
		{ header: 'Status', class: 'w-28 truncate', cell: podStatus },
		{ header: ' ', class: 'w-16 text-right', cell: podActions }
	] : [
		...(showNamespace
			? [{ header: 'Namespace', class: 'text-muted-foreground', hide: 'md', text: (p) => p.namespace } as Column<PodSummary>]
			: []),
		{ header: 'Name', cell: podName },
		{ header: 'Ready', text: (p) => `${p.ready}/${p.containers}` },
		{ header: 'Status', cell: podStatus },
		{ header: 'Restarts', hide: 'sm', cell: podRestarts },
		{ header: 'IP', class: 'text-muted-foreground tabular-nums', hide: 'md', text: (p) => p.ip ?? '-' },
		...(showNode
			? [{ header: 'Node', class: 'text-muted-foreground', hide: 'lg', text: (p) => p.node ?? '-' } as Column<PodSummary>]
			: []),
		{ header: 'Age', class: 'text-muted-foreground text-right', hide: 'sm', text: (p) => age(p.created, now) },
		{ header: ' ', class: 'w-16 text-right', cell: podActions }
	]);
</script>

{#snippet podName(p: PodSummary)}
	<div class="truncate font-medium {compact ? '' : 'max-w-[50vw] md:max-w-md'}" title={p.name}>{p.name}</div>
{/snippet}

{#snippet podNameWithNamespace(p: PodSummary)}
	<div class="truncate" title="{p.namespace}/{p.name}">
		<span class="text-muted-foreground">{p.namespace}/</span><span class="font-medium">{p.name}</span>
	</div>
{/snippet}

{#snippet podStatus(p: PodSummary)}
	<span class="inline-flex items-center gap-2">
		<span class="size-2 shrink-0 rounded-full {healthDot[podHealth(p.status)]}"></span>
		{p.status}
	</span>
{/snippet}

{#snippet podRestarts(p: PodSummary)}
	<span class={p.restarts > 0 ? 'text-amber-600 dark:text-amber-400' : ''}>{p.restarts}</span>
{/snippet}

{#snippet podActions(p: PodSummary)}
	<span class="inline-flex gap-0.5">
		<Button variant="ghost" size="icon-xs" title="Logs (l)" aria-label="Logs" onclick={() => openLogs(context, p)}>
			<ScrollText />
		</Button>
		<Button variant="ghost" size="icon-xs" title="Shell (s)" aria-label="Shell" onclick={() => openShell(context, p)}>
			<SquareTerminal />
		</Button>
	</span>
{/snippet}

<ResourceTable
	{list}
	{rows}
	{columns}
	key={itemKey}
	bind:cursor
	onActivate={(p) => openLogs(context, p)}
	fixed={compact}
/>
