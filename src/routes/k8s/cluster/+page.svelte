<script lang="ts">
	import { page } from '$app/state';
	import Kbd from '#lib/components/Kbd.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import {
		connect,
		listResourceKinds,
		nodeUsage,
		streamKind,
		streamNamespaces,
		streamNodes,
		streamTable,
		type ResourceKind,
		type TableColumn,
		type TableRow,
		type ClusterInfo,
		type ConfigMapSummary,
		type DeploymentSummary,
		type IngressSummary,
		type NamespacedKind,
		type NamespacedKinds,
		type NamespaceSummary,
		type NodeSummary,
		type NodeUsage,
		type PodSummary,
		type PvcSummary,
		type SecretSummary,
		type ServiceSummary
	} from '#lib/k8s/api.js';
	import { age, bytes, cpu, healthDot, matches, nodeHealth } from '#lib/k8s/format.js';
	import { itemKey, ResourceList } from '#lib/k8s/resource-list.svelte.js';
	import ResourceTable, { type Column } from '#lib/k8s/ResourceTable.svelte';
	import PodTable from '#lib/k8s/PodTable.svelte';
	import UsageBar from '#lib/k8s/UsageBar.svelte';
	import NodeDetails from '#lib/k8s/NodeDetails.svelte';
	import LogDock from '#lib/k8s/LogDock.svelte';
	import ObjectPanel from '#lib/k8s/ObjectPanel.svelte';
	import { openLogs, openShell } from '#lib/k8s/pod-actions.js';
	import { clusterView, GROUPS, KINDS, NAMESPACED, TABS, type Kind } from '#lib/k8s/view.svelte.js';
	import CommandBar from '#lib/k8s/CommandBar.svelte';
	import { builtinTab, withBuiltins, BUILTIN } from '#lib/k8s/command.js';
	import { clamp, isTyping, moveCursor, plain, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import Search from '@lucide/svelte/icons/search';
	import X from '@lucide/svelte/icons/x';
	import Boxes from '@lucide/svelte/icons/boxes';
	import Box from '@lucide/svelte/icons/box';
	import FileCog from '@lucide/svelte/icons/file-cog';
	import Globe from '@lucide/svelte/icons/globe';
	import HardDrive from '@lucide/svelte/icons/hard-drive';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import Layers from '@lucide/svelte/icons/layers';
	import Network from '@lucide/svelte/icons/network';
	import Puzzle from '@lucide/svelte/icons/puzzle';
	import Server from '@lucide/svelte/icons/server';
	import type { Component } from 'svelte';
	import { onDestroy, tick, untrack } from 'svelte';

	const context = $derived(page.url.searchParams.get('context') ?? '');
	const view = $derived(clusterView(context));
	const selected = $derived(view.namespaces);

	type Connection =
		| { state: 'connecting' }
		| { state: 'connected'; info: ClusterInfo }
		| { state: 'failed'; error: string };

	let connection = $state<Connection>({ state: 'connecting' });
	let query = $state('');
	let now = $state(Date.now());
	let filter: HTMLInputElement | null = $state(null);
	let scroller: HTMLElement | undefined = $state();
	let logDock: LogDock | undefined = $state();
	const dock = $derived(view.logs);
	let details: NodeDetails | undefined = $state();
	/** Node details are open beside the Nodes tab, and take the keys. */
	const detailsOpen = $derived(view.tab === 'nodes' && view.node !== null);
	let commandOpen = $state(false);
	let panel: ObjectPanel | undefined = $state();
	/** The list and the panel share the width; too narrow for both, the panel takes it all. */
	let bodyWidth = $state(0);
	const panelAlone = $derived(view.panel !== null && bodyWidth < 760);
	let discovered = $state<ResourceKind[]>([]);
	let discovering = $state(false);
	const kinds = $derived(withBuiltins(discovered));

	const lists = {
		namespaces: new ResourceList<NamespaceSummary>(),
		deployments: new ResourceList<DeploymentSummary>(),
		pods: new ResourceList<PodSummary>(),
		services: new ResourceList<ServiceSummary>(),
		ingresses: new ResourceList<IngressSummary>(),
		pvcs: new ResourceList<PvcSummary>(),
		configmaps: new ResourceList<ConfigMapSummary>(),
		secrets: new ResourceList<SecretSummary>(),
		nodes: new ResourceList<NodeSummary>(),
		custom: new ResourceList<TableRow>()
	} satisfies {
		namespaces: ResourceList<NamespaceSummary>;
		nodes: ResourceList<NodeSummary>;
		custom: ResourceList<TableRow>;
	} & {
		[K in NamespacedKind]: ResourceList<NamespacedKinds[K]>;
	};

	/** Columns of the `custom` tab, as the API server prints them. */
	let customColumns = $state<TableColumn[]>([]);
	/** Node usage from metrics-server, by node name; null without metrics-server. */
	let usage = $state<Map<string, NodeUsage> | null>(null);

	const busy = $derived(Object.values(lists).some((l) => l.loading));
	/** Show the namespace column unless exactly one namespace is in view. */
	const showNamespace = $derived(selected.size !== 1);

	// Search fields per kind.
	const rows = $derived({
		namespaces: lists.namespaces.items.filter((n) => matches(query, n.name, n.status)),
		deployments: lists.deployments.items.filter((d) => matches(query, d.name, d.namespace, ...d.images)),
		pods: lists.pods.items.filter((p) => matches(query, p.name, p.namespace, p.status, p.node)),
		services: lists.services.items.filter((s) =>
			matches(query, s.name, s.namespace, s.type, s.clusterIp, ...s.external)
		),
		ingresses: lists.ingresses.items.filter((i) => matches(query, i.name, i.namespace, ...i.hosts)),
		pvcs: lists.pvcs.items.filter((p) => matches(query, p.name, p.namespace, p.status, p.volume)),
		configmaps: lists.configmaps.items.filter((c) => matches(query, c.name, c.namespace)),
		secrets: lists.secrets.items.filter((s) => matches(query, s.name, s.namespace, s.type)),
		nodes: lists.nodes.items.filter((n) =>
			matches(query, n.name, n.status, n.internalIp, n.version, ...n.roles)
		),
		custom: lists.custom.items.filter((r) => matches(query, r.name, r.namespace, ...r.cells))
	});

	// --- loading ---------------------------------------------------------------------------
	// Every list stays live: after loading, the cluster's changes are applied as they happen.

	let attempt = 0;
	async function connectTo(ctx: string, fresh: boolean) {
		const mine = ++attempt;
		connection = { state: 'connecting' };
		try {
			const info = await connect(ctx, fresh);
			if (mine === attempt) connection = { state: 'connected', info };
		} catch (e) {
			if (mine === attempt) connection = { state: 'failed', error: String(e) };
		}
	}

	const scopeKey = $derived([...selected].sort().join('\n'));
	let loadedScope: string | null = null;

	/** Loads every namespaced kind for the ticked namespaces (all when none). */
	function loadScoped() {
		loadedScope = scopeKey;
		const namespaces = selected.size ? [...selected] : null;
		const inScope = new Set(namespaces);
		const keep = namespaces ? (item: { namespace: string }) => inScope.has(item.namespace) : undefined;
		for (const kind of NAMESPACED) {
			// Every namespaced summary has `namespace`; TS cannot see that through the union.
			const list = lists[kind] as unknown as ResourceList<NamespacedKinds[typeof kind]>;
			list.load((sink) => streamKind(kind, context, namespaces, sink), keep);
		}
		if (view.custom?.namespaced) loadCustom();
	}

	/** Loads the kind in the `custom` tab, scoped like the other tabs when it is namespaced. */
	function loadCustom() {
		const kind = view.custom;
		if (!kind) return;
		const namespaces = kind.namespaced && selected.size ? [...selected] : null;
		const inScope = new Set(namespaces);
		const keep = namespaces ? (row: TableRow) => inScope.has(row.namespace ?? '') : undefined;
		lists.custom.load(
			(sink) =>
				streamTable(
					context,
					kind,
					namespaces,
					(columns) => {
						if (view.custom === kind) customColumns = columns;
					},
					sink
				),
			keep
		);
	}

	/** Runs a `:` command: show a kind, optionally switching namespace (`all` for every one). */
	function runCommand(kind: ResourceKind, namespace: string | null) {
		commandOpen = false;
		if (namespace === 'all' || namespace === '-A') selected.clear();
		else if (namespace) {
			selected.clear();
			selected.add(namespace);
		}
		const tab = builtinTab(kind);
		if (tab) {
			view.tab = tab;
			return;
		}
		if (view.custom?.plural !== kind.plural || view.custom?.group !== kind.group) {
			view.custom = kind;
			view.cursors.custom = 0;
			customColumns = [];
			lists.custom.reset();
			loadCustom();
		}
		view.tab = 'custom';
	}

	/** Connect and list everything at once; the lists share the one connection. */
	function loadAll(fresh = false) {
		connectTo(context, fresh);
		lists.namespaces.load((sink) => streamNamespaces(context, sink));
		lists.nodes.load((sink) => streamNodes(context, sink));
		loadScoped();
		if (view.custom && !view.custom.namespaced) loadCustom();
		refreshUsage();
	}

	async function discover(ctx: string) {
		discovered = [];
		discovering = true;
		try {
			const found = await listResourceKinds(ctx);
			if (ctx === context) discovered = found;
		} catch {
			// Suggestions fall back to the built-in kinds.
		} finally {
			if (ctx === context) discovering = false;
		}
	}

	async function refreshUsage() {
		const ctx = context;
		try {
			const found = await nodeUsage(ctx);
			if (ctx === context) usage = found && new Map(found.map((u) => [u.name, u]));
		} catch {
			if (ctx === context) usage = null;
		}
	}

	$effect(() => {
		context;
		untrack(() => {
			for (const list of Object.values(lists)) list.reset();
			customColumns = [];
			usage = null;
			loadAll();
			discover(context);
		});
	});

	// Leaving the page (for logs, a shell or a node) stops the watches; coming back reloads.
	onDestroy(() => {
		for (const list of Object.values(lists)) list.stop();
	});

	// Reload when the ticked namespaces change, batching quick successive ticks.
	$effect(() => {
		const key = scopeKey;
		if (loadedScope === null || key === loadedScope) return;
		const timer = setTimeout(() => untrack(loadScoped), 200);
		return () => clearTimeout(timer);
	});

	// metrics-server has no watch; poll it while node usage is on screen.
	$effect(() => {
		if (view.tab !== 'nodes') return;
		untrack(refreshUsage);
		const timer = setInterval(refreshUsage, 15_000);
		return () => clearInterval(timer);
	});

	// --- cursor and selection --------------------------------------------------------------

	// Keep each cursor on a row once its list has loaded (not while it is still empty and loading,
	// which would lose the cursor remembered from the last visit).
	$effect(() => {
		for (const kind of TABS) {
			if (lists[kind].loading) continue;
			const fixed = clamp(view.cursors[kind], rows[kind].length);
			if (fixed !== view.cursors[kind]) view.cursors[kind] = fixed;
		}
	});

	// A new filter starts at the top (but opening the page keeps the remembered cursors).
	let lastQuery = '';
	$effect(() => {
		if (query === lastQuery) return;
		lastQuery = query;
		untrack(() => {
			for (const kind of TABS) view.cursors[kind] = 0;
		});
	});

	async function moveTo(index: number) {
		view.cursors[view.tab] = index;
		await tick();
		scrollToSelected(scroller?.querySelector<HTMLElement>(`[data-tab=${view.tab}]`) ?? undefined);
	}

	function toggle(namespace: string) {
		if (selected.has(namespace)) selected.delete(namespace);
		else selected.add(namespace);
	}

	/** Ticks the namespace and shows its deployments. */
	function openNamespace(namespace: string) {
		selected.add(namespace);
		view.tab = 'deployments';
	}

	/** Shows the node's details beside the list. */
	function openNode(node: NodeSummary) {
		if (view.node !== node.name) view.nodePod = 0;
		view.node = node.name;
	}

	const ICONS: Record<Kind, Component> = {
		namespaces: Boxes,
		nodes: Server,
		deployments: Layers,
		pods: Box,
		services: Network,
		ingresses: Globe,
		pvcs: HardDrive,
		configmaps: FileCog,
		secrets: KeyRound,
		custom: Puzzle
	};

	/** Sidebar order, for `[` and `]`; the `:` kind only once something has been opened with `:`. */
	const tabOrder = $derived<Kind[]>([...GROUPS.flatMap((g) => g.kinds), ...(view.custom ? (['custom'] as Kind[]) : [])]);

	function stepTab(by: number) {
		const at = tabOrder.indexOf(view.tab);
		view.tab = tabOrder[(at + by + tabOrder.length) % tabOrder.length];
	}

	function current<K extends Kind>(kind: K) {
		return view.tab === kind ? rows[kind][view.cursors[kind]] : undefined;
	}

	/** The kind shown in the current tab, as the API names it. */
	const tabKind = $derived(
		view.tab === 'custom' ? view.custom : (BUILTIN.find((b) => b.tab === view.tab)?.kind ?? null)
	);
	/** The selected row of the current tab: what the details / YAML panel shows. */
	const selectedItem = $derived.by(() => {
		const row = rows[view.tab][view.cursors[view.tab]] as { name: string; namespace?: string | null } | undefined;
		return row ? { name: row.name, namespace: row.namespace ?? null } : null;
	});

	/** `d` / `y`: open that view, switch to it, or close it if it is already shown. */
	function togglePanel(mode: 'detail' | 'yaml') {
		view.panel = view.panel === mode ? null : mode;
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
		}
		// ⌘J moves between the list and the dock (and is the way out of a shell there).
		if (event.metaKey && !event.ctrlKey && !event.altKey && event.key.toLowerCase() === 'j') {
			if (logDock?.hasFocus()) (document.activeElement as HTMLElement | null)?.blur();
			else if (dock.tabs.length) {
				dock.visible = true;
				tick().then(() => logDock?.focus());
			}
			return true;
		}
		// ⌘W closes the dock's current log or shell, as it closes a tab elsewhere.
		if (event.metaKey && !event.ctrlKey && !event.altKey && event.key.toLowerCase() === 'w') {
			if (!dock.visible || dock.active === null) return true;
			const inDock = logDock?.hasFocus();
			dock.close(dock.active);
			// Keep typing in the dock if it still has a tab, else go back to the list.
			// (next frame: the closed terminal gives up focus as it is disposed)
			if (inDock && dock.visible) requestAnimationFrame(() => logDock?.focus());
			else (document.activeElement as HTMLElement | null)?.blur();
			return true;
		}
		// Everything typed in a shell in the dock belongs to the shell.
		if (logDock?.shellFocused()) return;
		// Keys in the log dock scroll the log; Esc goes back to the list.
		if (!inFilter && logDock?.hasFocus()) {
			if (event.key === 'Escape') {
				(document.activeElement as HTMLElement | null)?.blur();
				return true;
			}
			if (event.key === 'L' && plain(event)) {
				(document.activeElement as HTMLElement | null)?.blur();
				dock.visible = false;
				return true;
			}
			if (logDock.handleKey(event)) return true;
			if (event.key.length === 1 || event.key === 'Enter') return true;
		} else if (!inFilter && isTyping(event)) {
			return;
		}
		if (!inFilter && detailsOpen && details?.handleKey(event)) return true;
		if (!inFilter && view.panel && plain(event)) {
			if (event.key === 'Escape') {
				view.panel = null;
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
			if (event.key === 'x' && tabKind?.kind === 'Secret' && !tabKind.group) {
				view.reveal = !view.reveal;
				return true;
			}
		}
		if (!inFilter && plain(event) && event.key === 'L' && dock.tabs.length) {
			// Show the logs and focus them (L again, from the logs, hides them).
			dock.visible = true;
			tick().then(() => logDock?.focus());
			return true;
		}
		if (!inFilter && event.key === 'Escape' && dock.visible) {
			dock.visible = false;
			return true;
		}

		const tab = view.tab;
		const next = moveCursor(event, view.cursors[tab], rows[tab].length, !inFilter);
		if (next !== null) {
			moveTo(next);
			return true;
		}
		if (inFilter || !plain(event)) return;

		const index = Number(event.key) - 1;
		if (Number.isInteger(index) && index >= 0 && index < KINDS.length) {
			view.tab = KINDS[index].kind;
			return true;
		}
		const ns = current('namespaces');
		const pod = current('pods');
		const node = current('nodes');
		switch (event.key) {
			case ':':
				commandOpen = true;
				return true;
			case '[':
				stepTab(-1);
				return true;
			case ']':
				stepTab(1);
				return true;
			case '0':
				selected.clear();
				return true;
			case ' ':
				if (ns) toggle(ns.name);
				return !!ns;
			case 'Enter':
				if (ns) openNamespace(ns.name);
				else if (node) openNode(node);
				else if (pod) openLogs(context, pod);
				return !!(ns || node || pod);
			case 'd':
				if (!tabKind) return;
				togglePanel('detail');
				return true;
			case 'y':
				if (!tabKind) return;
				togglePanel('yaml');
				return true;
			case 'l':
				if (pod) openLogs(context, pod);
				return !!pod;
			case 's':
				if (pod) openShell(context, pod);
				return !!pod;
			case '/':
				filter?.focus();
				return true;
			case 'r':
				loadAll(connection.state === 'failed');
				panel?.load();
				return true;
		}
	});


	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 10_000);
		return () => clearInterval(timer);
	});

	// --- columns ---------------------------------------------------------------------------
	// `hide` drops the least useful columns first as the window narrows.

	const ageColumn = <T extends { created: string | null }>(): Column<T> => ({
		header: 'Age',
		class: 'text-muted-foreground text-right',
		hide: 'sm',
		text: (r) => age(r.created, now)
	});
	const nsColumn = <T extends { namespace: string }>(): Column<T>[] =>
		showNamespace
			? [{ header: 'Namespace', class: 'text-muted-foreground', hide: 'md', text: (r) => r.namespace }]
			: [];
	const list = (values: string[]) => values.join(', ') || '-';

	const columns = $derived({
		namespaces: [
			{ header: '', class: 'w-8', cell: nsCheck },
			{ header: 'Name', class: 'font-medium', text: (n) => n.name },
			{ header: 'Status', cell: nsStatus },
			ageColumn()
		] as Column<NamespaceSummary>[],
		deployments: [
			...nsColumn<DeploymentSummary>(),
			{ header: 'Name', cell: deploymentName },
			{ header: 'Ready', cell: deploymentReady },
			{ header: 'Up-to-date', hide: 'lg', text: (d) => String(d.upToDate) },
			{ header: 'Available', hide: 'md', text: (d) => String(d.available) },
			ageColumn()
		] as Column<DeploymentSummary>[],
		services: [
			...nsColumn<ServiceSummary>(),
			{ header: 'Name', class: 'font-medium', text: (s) => s.name },
			{ header: 'Type', hide: 'sm', text: (s) => s.type },
			{ header: 'Cluster IP', hide: 'lg', text: (s) => s.clusterIp ?? '-' },
			{ header: 'External', hide: 'md', text: (s) => list(s.external) },
			{ header: 'Ports', text: (s) => list(s.ports) },
			ageColumn()
		] as Column<ServiceSummary>[],
		ingresses: [
			...nsColumn<IngressSummary>(),
			{ header: 'Name', class: 'font-medium', text: (i) => i.name },
			{ header: 'Class', hide: 'lg', text: (i) => i.class ?? '-' },
			{ header: 'Hosts', text: (i) => list(i.hosts) },
			{ header: 'Address', hide: 'md', text: (i) => list(i.addresses) },
			{ header: 'TLS', hide: 'lg', text: (i) => (i.tls ? 'yes' : '-') },
			ageColumn()
		] as Column<IngressSummary>[],
		pvcs: [
			...nsColumn<PvcSummary>(),
			{ header: 'Name', class: 'font-medium', text: (p) => p.name },
			{ header: 'Status', text: (p) => p.status },
			{ header: 'Volume', class: 'text-muted-foreground', hide: 'xl', text: (p) => p.volume ?? '-' },
			{ header: 'Capacity', hide: 'sm', text: (p) => p.capacity ?? '-' },
			{ header: 'Access', hide: 'md', text: (p) => list(p.accessModes) },
			{ header: 'Storage class', hide: 'lg', text: (p) => p.storageClass ?? '-' },
			ageColumn()
		] as Column<PvcSummary>[],
		configmaps: [
			...nsColumn<ConfigMapSummary>(),
			{ header: 'Name', class: 'font-medium', text: (c) => c.name },
			{ header: 'Data', text: (c) => String(c.keys) },
			ageColumn()
		] as Column<ConfigMapSummary>[],
		secrets: [
			...nsColumn<SecretSummary>(),
			{ header: 'Name', class: 'font-medium', text: (s) => s.name },
			{ header: 'Type', class: 'text-muted-foreground', hide: 'md', text: (s) => s.type },
			{ header: 'Data', text: (s) => String(s.keys) },
			ageColumn()
		] as Column<SecretSummary>[],
		nodes: [
			{ header: 'Name', cell: nodeName },
			{ header: 'Status', cell: nodeStatus },
			{ header: 'Roles', hide: 'md', text: (n) => list(n.roles) },
			{ header: 'Version', class: 'text-muted-foreground', hide: 'lg', text: (n) => n.version },
			{ header: 'CPU', hide: 'sm', cell: nodeCpu },
			{ header: 'Memory', hide: 'sm', cell: nodeMemory },
			ageColumn()
		] as Column<NodeSummary>[],
		custom: [
			...(view.custom?.namespaced && showNamespace
				? [{ header: 'Namespace', class: 'text-muted-foreground', hide: 'md', text: (r: TableRow) => r.namespace ?? '-' }]
				: []),
			// The server lists the most useful columns first; drop later ones as space runs out.
			...customColumns.map((c, i) => ({
				header: c.name,
				class: i === 0 ? 'font-medium' : undefined,
				hide: i >= 5 ? 'lg' : i >= 3 ? 'md' : i >= 2 ? 'sm' : undefined,
				text: (r: TableRow) => r.cells[i] || '-'
			}))
		] as Column<TableRow>[]
	});
</script>

{#snippet count(kind: Kind)}
	<span class="text-muted-foreground tabular-nums">
		{lists[kind].loading && lists[kind].items.length === 0 ? '…' : lists[kind].items.length}
	</span>
{/snippet}

{#snippet sideItem(kind: Kind, label: string, shortcut: string)}
	{@const Icon = ICONS[kind]}
	<Tabs.Trigger
		value={kind}
		title="{label}{shortcut ? ` (${shortcut})` : ''}"
		data-tab-trigger={kind}
		class="data-active:bg-muted h-7 w-full flex-none justify-center gap-2 px-2 md:justify-start"
	>
		<Icon class="size-4 shrink-0" />
		<span class="hidden min-w-0 flex-1 truncate text-left md:inline">{label}</span>
		<span class="hidden text-xs md:inline">{@render count(kind)}</span>
	</Tabs.Trigger>
{/snippet}

{#snippet nsCheck(n: NamespaceSummary)}
	<Checkbox
		checked={selected.has(n.name)}
		onCheckedChange={() => toggle(n.name)}
		aria-label="Select {n.name}"
	/>
{/snippet}

{#snippet nsStatus(n: NamespaceSummary)}
	<span class="inline-flex items-center gap-2">
		<span class="size-2 rounded-full {n.status === 'Active' ? 'bg-emerald-500' : 'bg-amber-500'}"></span>
		{n.status}
	</span>
{/snippet}

{#snippet deploymentName(d: DeploymentSummary)}
	<div class="font-medium">{d.name}</div>
	<div class="text-muted-foreground max-w-[50vw] truncate text-xs md:max-w-md" title={d.images.join('\n')}>
		{d.images.join(', ')}
	</div>
{/snippet}

{#snippet deploymentReady(d: DeploymentSummary)}
	<span class={d.ready < d.desired ? 'text-amber-600 dark:text-amber-400' : ''}>{d.ready}/{d.desired}</span>
{/snippet}

{#snippet nodeName(n: NodeSummary)}
	<div class="font-medium">{n.name}</div>
	{#if n.internalIp}<div class="text-muted-foreground text-xs">{n.internalIp}</div>{/if}
{/snippet}

{#snippet nodeStatus(n: NodeSummary)}
	<span class="inline-flex items-center gap-2">
		<span class="size-2 shrink-0 rounded-full {healthDot[nodeHealth(n.status)]}"></span>
		{n.status}
	</span>
{/snippet}

{#snippet nodeCpu(n: NodeSummary)}
	<UsageBar used={usage?.get(n.name)?.cpuMillis} total={n.cpuMillis} format={cpu} />
{/snippet}

{#snippet nodeMemory(n: NodeSummary)}
	<UsageBar used={usage?.get(n.name)?.memoryBytes} total={n.memoryBytes} format={bytes} />
{/snippet}

<main class="flex h-screen flex-col">
	<header class="flex items-center gap-2 border-b px-2 py-2 sm:gap-3 sm:px-4 sm:py-3">
		<Button variant="ghost" size="icon" href="/k8s" aria-label="Back to clusters">
			<ArrowLeft />
		</Button>
		<div class="min-w-0 flex-1">
			<div class="flex items-center gap-2">
				<h1 class="truncate font-semibold">{context}</h1>
				{#if connection.state === 'connecting'}
					<Badge variant="secondary"><LoaderCircle class="animate-spin" /><span class="hidden sm:inline">Connecting</span></Badge>
				{:else if connection.state === 'connected'}
					<Badge variant="outline" class="hidden sm:inline-flex">
						<span class="size-1.5 rounded-full bg-emerald-500"></span>
						{connection.info.version}
					</Badge>
				{:else}
					<Badge variant="destructive">Disconnected</Badge>
				{/if}
			</div>
			{#if connection.state === 'connected'}
				<div class="text-muted-foreground hidden truncate text-xs sm:block">{connection.info.server}</div>
			{/if}
		</div>

		<div class="flex min-w-0 items-center gap-1" data-scope>
			<Button
				variant="outline"
				class="max-w-32 justify-start sm:max-w-72"
				onclick={() => (view.tab = 'namespaces')}
				title="Choose namespaces (1)"
			>
				<span class="truncate">
					{#if selected.size === 0}All namespaces{:else}{[...selected].sort().join(', ')}{/if}
				</span>
			</Button>
			{#if selected.size > 0}
				<Button variant="ghost" size="icon" aria-label="All namespaces" title="All namespaces (0)" onclick={() => selected.clear()}>
					<X />
				</Button>
			{/if}
		</div>

		<Button
			variant="outline"
			size="icon"
			onclick={() => loadAll(connection.state === 'failed')}
			aria-label="Refresh"
			title="Refresh (r)"
		>
			<RefreshCw class={busy ? 'animate-spin' : ''} />
		</Button>
	</header>

	{#if connection.state === 'failed'}
		<div class="p-4">
			<Alert.Root variant="destructive">
				<Alert.Title>Could not connect to {context}</Alert.Title>
				<Alert.Description class="break-all">{connection.error}</Alert.Description>
			</Alert.Root>
			<div class="mt-3 flex flex-wrap gap-2">
				<Button onclick={() => loadAll(true)}>Try again</Button>
				<Button variant="outline" href="/k8s">Choose another cluster</Button>
			</div>
		</div>
	{:else}
		<Tabs.Root
			bind:value={() => view.tab, (v) => (view.tab = v as Kind)}
			orientation="vertical"
			class="flex min-h-0 flex-1 flex-row gap-0"
		>
			<!-- Sidebar: kinds in groups; icons only on narrow windows. -->
			<nav class="flex w-12 shrink-0 flex-col overflow-y-auto border-r px-1.5 py-2 md:w-52 md:px-2" aria-label="Resources">
				<Tabs.List class="h-auto w-full flex-col items-stretch gap-0 bg-transparent p-0">
					{#each GROUPS as group (group.label)}
						<div class="text-muted-foreground mt-2 mb-0.5 hidden px-2 text-[11px] font-medium tracking-wide uppercase first:mt-0 md:block">
							{group.label}
						</div>
						<div class="bg-border mx-1 my-1.5 h-px first:hidden md:hidden"></div>
						{#each group.kinds as kind (kind)}
							{@render sideItem(kind, KINDS.find((k) => k.kind === kind)!.label, String(KINDS.findIndex((k) => k.kind === kind) + 1))}
						{/each}
					{/each}
					{#if view.custom}
						<div class="text-muted-foreground mt-2 mb-0.5 hidden px-2 text-[11px] font-medium tracking-wide uppercase md:block">
							Opened with :
						</div>
						<div class="bg-border mx-1 my-1.5 h-px md:hidden"></div>
						{@render sideItem('custom', view.custom.kind, '')}
					{/if}
				</Tabs.List>
			</nav>
			<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			<div class="flex items-center gap-2 px-2 pt-2 sm:px-4 sm:pt-3">
				<h2 class="min-w-0 flex-1 truncate text-sm font-medium">
					{view.tab === 'custom' ? view.custom?.kind : KINDS.find((k) => k.kind === view.tab)?.label}
					{#if view.tab === 'nodes' && view.node}<span class="text-muted-foreground"> / {view.node}</span>{/if}
				</h2>
				<div class="relative w-40 shrink-0 sm:w-56">
					<Search class="text-muted-foreground absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
					<Input class="h-8 pl-8" placeholder="Filter  /" bind:value={query} bind:ref={filter} />
				</div>
			</div>
			<div class="flex min-h-0 flex-1" bind:clientWidth={bodyWidth}>
			<div
				class="relative min-h-0 min-w-0 flex-1 px-2 sm:px-4 {detailsOpen ? 'overflow-hidden pb-2' : 'overflow-auto'} {panelAlone ? 'hidden' : ''}"
				bind:this={scroller}
			>
				{#if commandOpen}
					<CommandBar
						{kinds}
						namespaces={lists.namespaces.items.map((n) => n.name).sort()}
						{discovering}
						onRun={runCommand}
						onClose={() => (commandOpen = false)}
					/>
				{/if}
				<Tabs.Content value="namespaces" data-tab="namespaces">
					<ResourceTable
						list={lists.namespaces}
						rows={rows.namespaces}
						columns={columns.namespaces}
						key={itemKey}
						bind:cursor={view.cursors.namespaces}
						onActivate={(n) => openNamespace(n.name)}
					/>
				</Tabs.Content>
				<Tabs.Content value="deployments" data-tab="deployments">
					<ResourceTable list={lists.deployments} rows={rows.deployments} columns={columns.deployments} key={itemKey} bind:cursor={view.cursors.deployments} />
				</Tabs.Content>
				<Tabs.Content value="pods" data-tab="pods">
					<PodTable {context} list={lists.pods} rows={rows.pods} {now} {showNamespace} bind:cursor={view.cursors.pods} />
				</Tabs.Content>
				<Tabs.Content value="services" data-tab="services">
					<ResourceTable list={lists.services} rows={rows.services} columns={columns.services} key={itemKey} bind:cursor={view.cursors.services} />
				</Tabs.Content>
				<Tabs.Content value="ingresses" data-tab="ingresses">
					<ResourceTable list={lists.ingresses} rows={rows.ingresses} columns={columns.ingresses} key={itemKey} bind:cursor={view.cursors.ingresses} />
				</Tabs.Content>
				<Tabs.Content value="pvcs" data-tab="pvcs">
					<ResourceTable list={lists.pvcs} rows={rows.pvcs} columns={columns.pvcs} key={itemKey} bind:cursor={view.cursors.pvcs} />
				</Tabs.Content>
				<Tabs.Content value="configmaps" data-tab="configmaps">
					<ResourceTable list={lists.configmaps} rows={rows.configmaps} columns={columns.configmaps} key={itemKey} bind:cursor={view.cursors.configmaps} />
				</Tabs.Content>
				<Tabs.Content value="secrets" data-tab="secrets">
					<ResourceTable list={lists.secrets} rows={rows.secrets} columns={columns.secrets} key={itemKey} bind:cursor={view.cursors.secrets} />
				</Tabs.Content>
				<!-- Node details replace the list and fill the area exactly: nothing scrolls. -->
				<Tabs.Content value="nodes" data-tab="nodes" class={view.node ? 'h-full' : undefined}>
					{#if view.node}
						<div class="bg-card h-full min-h-0 rounded-lg border" data-node-details>
							{#key view.node}
								<NodeDetails
									bind:this={details}
									{context}
									name={view.node}
									bind:cursor={view.nodePod}
									onClose={() => (view.node = null)}
								/>
							{/key}
						</div>
					{:else}
						<ResourceTable
							list={lists.nodes}
							rows={rows.nodes}
							columns={columns.nodes}
							key={itemKey}
							bind:cursor={view.cursors.nodes}
							onActivate={openNode}
						/>
					{/if}
				</Tabs.Content>
				<Tabs.Content value="custom" data-tab="custom">
					{#if view.custom}
						<ResourceTable
							list={lists.custom}
							rows={rows.custom}
							columns={columns.custom.length ? columns.custom : [{ header: 'Name', text: (r) => r.name }]}
							key={itemKey}
							bind:cursor={view.cursors.custom}
						/>
					{/if}
				</Tabs.Content>
			</div>
			{#if view.panel && tabKind}
				<div class="bg-card m-2 ml-0 min-h-0 min-w-0 rounded-lg border {panelAlone ? 'ml-2 flex-1' : 'w-[45%] max-w-[52rem] min-w-80'}">
					<ObjectPanel
						bind:this={panel}
						{context}
						kind={tabKind}
						namespace={tabKind.namespaced ? (selectedItem?.namespace ?? null) : null}
						name={selectedItem?.name ?? null}
						bind:mode={() => view.panel ?? 'detail', (m) => (view.panel = m)}
						bind:reveal={() => view.reveal, (r) => (view.reveal = r)}
						{now}
						onClose={() => (view.panel = null)}
					/>
				</div>
			{/if}
			</div>
			<!-- Under the list only, so the sidebar keeps its full height. -->
			{#if dock.tabs.length}
				<div class={dock.visible ? 'contents' : 'hidden'}>
					<LogDock bind:this={logDock} {context} {dock} />
				</div>
			{/if}
			</div>
		</Tabs.Root>
		<footer class="text-muted-foreground flex flex-wrap gap-x-4 gap-y-1 border-t px-2 py-2 text-xs sm:px-4">
			{#if !detailsOpen}<span class="hidden sm:inline"><Kbd>j</Kbd> <Kbd>k</Kbd> move</span>{/if}
			<span><Kbd>:</Kbd> resources</span>
			<span class="hidden md:inline"><Kbd>1</Kbd>–<Kbd>9</Kbd> <Kbd>[</Kbd> <Kbd>]</Kbd> tabs</span>
			{#if view.tab === 'namespaces'}
				<span><Kbd>Space</Kbd> tick</span>
				<span><Kbd>Enter</Kbd> open</span>
			{/if}
			<span><Kbd>d</Kbd> details</span>
			<span><Kbd>y</Kbd> YAML</span>
			{#if view.tab === 'pods'}
				<span><Kbd>l</Kbd> logs</span>
				<span><Kbd>s</Kbd> shell</span>
			{/if}
			{#if detailsOpen}
				<span><Kbd>j</Kbd> <Kbd>k</Kbd> pods</span>
				<span><Kbd>l</Kbd> logs</span>
				<span><Kbd>s</Kbd> shell</span>
				<span><Kbd>Esc</Kbd> close details</span>
			{:else if view.tab === 'nodes'}
				<span><Kbd>Enter</Kbd> details</span>
			{/if}
			<span class="hidden lg:inline"><Kbd>0</Kbd> all namespaces</span>
			<span class="hidden md:inline"><Kbd>/</Kbd> filter</span>
			<span class="hidden lg:inline"><Kbd>r</Kbd> refresh</span>
			{#if !detailsOpen}<span class="hidden lg:inline"><Kbd>Esc</Kbd> back</span>{/if}
			{#if dock.tabs.length}
				<button class="hover:text-foreground" onclick={() => dock.toggle()}>
					<Kbd>L</Kbd> logs & shells ({dock.tabs.length})
				</button>
			{/if}
			<span class="ml-auto"><Kbd>?</Kbd> help</span>
		</footer>
	{/if}
</main>
