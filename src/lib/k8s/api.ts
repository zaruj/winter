import { Channel, invoke } from '@tauri-apps/api/core';

// Typed wrappers for the Kubernetes commands in src-tauri/src/commands.rs.
// Commands reject with an error message string.

export interface ContextInfo {
	name: string;
	cluster: string | null;
	user: string | null;
	namespace: string | null;
	server: string | null;
	current: boolean;
}

export interface ClusterInfo {
	context: string;
	server: string;
	version: string;
	platform: string;
}

export interface NamespaceSummary {
	name: string;
	status: string;
	created: string | null;
}

export interface DeploymentSummary {
	namespace: string;
	name: string;
	desired: number;
	ready: number;
	upToDate: number;
	available: number;
	images: string[];
	created: string | null;
}

export interface PodSummary {
	namespace: string;
	name: string;
	ready: number;
	containers: number;
	status: string;
	restarts: number;
	node: string | null;
	ip: string | null;
	created: string | null;
	containerNames: string[];
	defaultContainer: string | null;
}

export interface ServiceSummary {
	namespace: string;
	name: string;
	type: string;
	clusterIp: string | null;
	external: string[];
	ports: string[];
	created: string | null;
}

export interface IngressSummary {
	namespace: string;
	name: string;
	class: string | null;
	hosts: string[];
	addresses: string[];
	tls: boolean;
	created: string | null;
}

export interface PvcSummary {
	namespace: string;
	name: string;
	status: string;
	volume: string | null;
	capacity: string | null;
	accessModes: string[];
	storageClass: string | null;
	created: string | null;
}

export interface ConfigMapSummary {
	namespace: string;
	name: string;
	keys: number;
	created: string | null;
}

/** Secret values are never sent to the UI. */
export interface SecretSummary {
	namespace: string;
	name: string;
	type: string;
	keys: number;
	created: string | null;
}

/** Namespaced kinds and their summaries, keyed by the command suffix. */
export interface NamespacedKinds {
	deployments: DeploymentSummary;
	pods: PodSummary;
	services: ServiceSummary;
	ingresses: IngressSummary;
	pvcs: PvcSummary;
	configmaps: ConfigMapSummary;
	secrets: SecretSummary;
}
export type NamespacedKind = keyof NamespacedKinds;

export interface NodeSummary {
	name: string;
	/** `Ready`, `NotReady` or `Unknown`, plus `,SchedulingDisabled` when cordoned. */
	status: string;
	roles: string[];
	version: string;
	internalIp: string | null;
	externalIp: string | null;
	osImage: string;
	kernel: string;
	arch: string;
	runtime: string;
	/** Allocatable CPU in millicores. */
	cpuMillis: number | null;
	/** Allocatable memory in bytes. */
	memoryBytes: number | null;
	pods: number | null;
	taints: number;
	created: string | null;
}

export interface NodeDetail extends Omit<NodeSummary, 'taints'> {
	addresses: { type: string; address: string }[];
	conditions: {
		type: string;
		status: string;
		reason: string | null;
		message: string | null;
		lastTransition: string | null;
	}[];
	taints: { key: string; value: string | null; effect: string }[];
	labels: Record<string, string>;
	capacity: Record<string, string>;
	allocatable: Record<string, string>;
	podCidrs: string[];
	providerId: string | null;
	kubeProxy: string;
	images: number;
}

/** Usage now, from metrics-server. */
export interface NodeUsage {
	name: string;
	cpuMillis: number;
	memoryBytes: number;
}

/** A change to a listed item, sent after the list has loaded. */
export type Change<T> =
	| { type: 'applied'; item: T }
	| { type: 'deleted'; item: T }
	/** The watch could not resume and stopped: list again. */
	| { type: 'reset' };

/**
 * Where a list command delivers results: each page as it arrives, then every later change, and
 * the id of each watch started (to stop with `stopStream`).
 */
export interface Sink<T> {
	page: (items: T[]) => void;
	change: (change: Change<T>) => void;
	watching: (id: number) => void;
}

interface Listing {
	total: number;
	watch: number | null;
}

export function listContexts(): Promise<ContextInfo[]> {
	return invoke('k8s_contexts');
}

/** `fresh` drops any cached connection first, e.g. to pick up new credentials. */
export function connect(context: string, fresh = false): Promise<ClusterInfo> {
	return invoke('k8s_connect', { context, fresh });
}

/** Resolves once every page has been delivered; changes keep coming until the watch stops. */
export function streamNamespaces(context: string, sink: Sink<NamespaceSummary>) {
	return stream('k8s_namespaces', { context }, sink);
}

export function streamNodes(context: string, sink: Sink<NodeSummary>) {
	return stream('k8s_nodes', { context }, sink);
}

/** One node, as a list of one that is kept up to date. */
export function streamNode(context: string, name: string, sink: Sink<NodeDetail>) {
	return stream('k8s_node', { context, name }, sink);
}

/** Null when the cluster has no metrics-server. */
export function nodeUsage(context: string): Promise<NodeUsage[] | null> {
	return invoke('k8s_node_usage', { context });
}

/**
 * Streams a namespaced kind. `namespaces: null` lists across all namespaces; otherwise each
 * listed namespace is fetched in parallel into the same sink. `fields` is a field selector,
 * e.g. `spec.nodeName=node-1`.
 */
export async function streamKind<K extends NamespacedKind>(
	kind: K,
	context: string,
	namespaces: string[] | null,
	sink: Sink<NamespacedKinds[K]>,
	fields: string | null = null
): Promise<void> {
	const command = `k8s_${kind}`;
	await Promise.all(
		(namespaces ?? [null]).map((namespace) => stream(command, { context, namespace, fields }, sink))
	);
}

export interface Target {
	context: string;
	namespace: string;
	pod: string;
	container: string | null;
}

/** Follows a container log from the last `tailLines` lines. Returns an id for `stopStream`. */
export function streamLogs(
	target: Target,
	tailLines: number,
	onChunk: (bytes: ArrayBuffer) => void,
	onEnd: (error: string | null) => void
): Promise<number> {
	return invoke('k8s_logs', {
		...target,
		tailLines,
		onChunk: new Channel<ArrayBuffer>(onChunk),
		onEnd: new Channel<string | null>(onEnd)
	});
}

export function openPodShell(
	target: Target,
	cols: number,
	rows: number,
	onOutput: (bytes: ArrayBuffer) => void,
	onExit: (code: number | null) => void
): Promise<number> {
	return invoke('k8s_shell_open', {
		...target,
		cols,
		rows,
		onOutput: new Channel<ArrayBuffer>(onOutput),
		onExit: new Channel<number | null>(onExit)
	});
}

export function writePodShell(id: number, data: string): Promise<void> {
	return invoke('k8s_shell_write', { id, data });
}

export function resizePodShell(id: number, cols: number, rows: number): Promise<void> {
	return invoke('k8s_shell_resize', { id, cols, rows });
}

/** Stops a log stream or closes a pod shell. */
export function stopStream(id: number): Promise<void> {
	return invoke('k8s_stream_stop', { id });
}

async function stream<T>(command: string, args: Record<string, unknown>, sink: Sink<T>) {
	const listing = await invoke<Listing>(command, {
		...args,
		onPage: new Channel<T[]>(sink.page),
		onChange: new Channel<Change<T>>(sink.change)
	});
	if (listing.watch !== null) sink.watching(listing.watch);
}

/** A listable resource kind, as discovered from the API server. */
export interface ResourceKind {
	/** Empty for the core group. */
	group: string;
	version: string;
	kind: string;
	plural: string;
	singular: string;
	shortNames: string[];
	namespaced: boolean;
}

export interface TableColumn {
	name: string;
	type: string;
	format: string;
}

export interface TableRow {
	namespace: string | null;
	name: string;
	cells: string[];
}

export interface TablePage {
	columns: TableColumn[];
	rows: TableRow[];
}

/** Every kind the cluster can list, including custom resources. */
export function listResourceKinds(context: string): Promise<ResourceKind[]> {
	return invoke('k8s_resource_kinds', { context });
}

/**
 * Streams any kind as `kubectl get` prints it, then its changes. `namespaces: null` (or a
 * cluster-scoped kind) lists everything; otherwise each namespace is fetched in parallel.
 */
export async function streamTable(
	context: string,
	kind: ResourceKind,
	namespaces: string[] | null,
	onColumns: (columns: TableColumn[]) => void,
	sink: Sink<TableRow>
): Promise<void> {
	const list = async (namespace: string | null) => {
		const listing = await invoke<Listing>('k8s_table', {
			context,
			kind,
			namespace,
			onPage: new Channel<TablePage>((page) => {
				onColumns(page.columns);
				sink.page(page.rows);
			}),
			onChange: new Channel<Change<TableRow>>(sink.change)
		});
		if (listing.watch !== null) sink.watching(listing.watch);
	};
	await Promise.all(!namespaces || !kind.namespaced ? [list(null)] : namespaces.map(list));
}
