import { SvelteSet } from 'svelte/reactivity';
import type { NamespacedKind, ResourceKind } from './api.js';
import { LogDock } from './log-dock.svelte.js';

/** A tab: one of the built-in kinds, or `custom` for any other kind picked with `:`. */
export type Kind = 'namespaces' | NamespacedKind | 'nodes' | 'custom';

export const KINDS: { kind: Kind; label: string }[] = [
	{ kind: 'namespaces', label: 'Namespaces' },
	{ kind: 'deployments', label: 'Deployments' },
	{ kind: 'pods', label: 'Pods' },
	{ kind: 'services', label: 'Services' },
	{ kind: 'ingresses', label: 'Ingresses' },
	{ kind: 'pvcs', label: 'PVCs' },
	{ kind: 'configmaps', label: 'ConfigMaps' },
	{ kind: 'secrets', label: 'Secrets' },
	{ kind: 'nodes', label: 'Nodes' }
];

/** The sidebar: kinds grouped as in most Kubernetes UIs. Number keys still follow `KINDS`. */
export const GROUPS: { label: string; kinds: Exclude<Kind, 'custom'>[] }[] = [
	{ label: 'Cluster', kinds: ['namespaces', 'nodes'] },
	{ label: 'Workloads', kinds: ['deployments', 'pods'] },
	{ label: 'Network', kinds: ['services', 'ingresses'] },
	{ label: 'Storage', kinds: ['pvcs'] },
	{ label: 'Config', kinds: ['configmaps', 'secrets'] }
];

export const NAMESPACED = KINDS.map((k) => k.kind).filter(
	(k): k is NamespacedKind => k !== 'namespaces' && k !== 'nodes' && k !== 'custom'
);

export const TABS: Kind[] = [...KINDS.map((k) => k.kind), 'custom'];

/**
 * What the user was looking at in a cluster: tab, ticked namespaces (empty = all) and the cursor
 * in each tab. Kept for the whole app session, so it survives opening logs or a shell and coming
 * back.
 */
export class ClusterView {
	tab = $state<Kind>('namespaces');
	readonly namespaces = new SvelteSet<string>();
	/** Kind shown in the `custom` tab. */
	custom = $state<ResourceKind | null>(null);
	/** Pod logs docked at the bottom. */
	readonly logs = new LogDock();
	/** Node shown in the details panel beside the Nodes tab. */
	node = $state<string | null>(null);
	/** Selected pod in that panel. */
	nodePod = $state(0);
	cursors = $state(Object.fromEntries(TABS.map((k) => [k, 0])) as Record<Kind, number>);
}

const views = new Map<string, ClusterView>();

export function clusterView(context: string): ClusterView {
	let view = views.get(context);
	if (!view) {
		view = new ClusterView();
		views.set(context, view);
	}
	return view;
}
