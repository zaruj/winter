import type { ResourceKind } from './api.js';
import type { Kind } from './view.svelte.js';

// k9s-style `:` resource search: match what the user typed against the names kubectl accepts.

const core = (kind: string, plural: string, shortNames: string[], namespaced = true): ResourceKind => ({
	group: '',
	version: 'v1',
	kind,
	plural,
	singular: kind.toLowerCase(),
	shortNames,
	namespaced
});

/** Kinds with a dedicated tab. Also the suggestions shown before discovery finishes. */
export const BUILTIN: { tab: Exclude<Kind, 'custom'>; kind: ResourceKind }[] = [
	{ tab: 'namespaces', kind: core('Namespace', 'namespaces', ['ns'], false) },
	{ tab: 'deployments', kind: { ...core('Deployment', 'deployments', ['deploy']), group: 'apps' } },
	{ tab: 'pods', kind: core('Pod', 'pods', ['po']) },
	{ tab: 'services', kind: core('Service', 'services', ['svc']) },
	{ tab: 'ingresses', kind: { ...core('Ingress', 'ingresses', ['ing']), group: 'networking.k8s.io' } },
	{ tab: 'pvcs', kind: core('PersistentVolumeClaim', 'persistentvolumeclaims', ['pvc']) },
	{ tab: 'configmaps', kind: core('ConfigMap', 'configmaps', ['cm']) },
	{ tab: 'secrets', kind: core('Secret', 'secrets', []) },
	{ tab: 'nodes', kind: core('Node', 'nodes', ['no'], false) }
];

const id = (k: ResourceKind) => `${k.plural}.${k.group}`;

/** The dedicated tab for a kind, if it has one. */
export function builtinTab(kind: ResourceKind): Exclude<Kind, 'custom'> | undefined {
	return BUILTIN.find((b) => id(b.kind) === id(kind))?.tab;
}

/** Names that select a kind, as in kubectl: plural, singular, short names, kind, plural.group. */
export function namesOf(k: ResourceKind): string[] {
	const names = [k.plural, k.singular, ...k.shortNames, k.kind.toLowerCase()];
	if (k.group) names.push(`${k.plural}.${k.group}`);
	return names;
}

/** Lower is better; null when the kind does not match. */
function score(token: string, k: ResourceKind, order: number): number | null {
	const names = namesOf(k);
	// Built-in kinds win ties, as kubectl prefers core and apps over look-alike CRDs.
	const bias = order / 10_000;
	if (names.includes(token)) return 0 + bias;
	const prefix = names.filter((n) => n.startsWith(token));
	if (prefix.length) return 1 + Math.min(...prefix.map((n) => n.length)) / 1000 + bias;
	if (names.some((n) => n.includes(token))) return 2 + bias;
	return null;
}

export interface Parsed {
	resource: string;
	/** Second word: a namespace, or `all` / `-A` for every namespace. */
	namespace: string | null;
	/** True while the user is typing the second word. */
	onNamespace: boolean;
}

export function parse(input: string): Parsed {
	const words = input.trimStart().split(/\s+/);
	return {
		resource: (words[0] ?? '').toLowerCase(),
		namespace: words.length > 1 && words[1] ? words[1] : null,
		onNamespace: words.length > 1
	};
}

/** Best matching kinds for `token`, best first. Duplicates (same plural and group) are dropped. */
export function suggestKinds(token: string, kinds: ResourceKind[], limit = 8): ResourceKind[] {
	if (!token) return [];
	const seen = new Set<string>();
	return kinds
		.map((k, i) => ({ k, s: score(token, k, i) }))
		.filter((x): x is { k: ResourceKind; s: number } => x.s !== null)
		.sort((a, b) => a.s - b.s)
		.map((x) => x.k)
		.filter((k) => !seen.has(id(k)) && !!seen.add(id(k)))
		.slice(0, limit);
}

/** Namespaces starting with (then containing) `token`, plus `all`. */
export function suggestNamespaces(token: string, namespaces: string[], limit = 8): string[] {
	const t = token.toLowerCase();
	const options = ['all', ...namespaces];
	const starts = options.filter((n) => n.startsWith(t));
	const contains = options.filter((n) => !n.startsWith(t) && n.includes(t));
	return [...starts, ...contains].slice(0, limit);
}

/** Builtins first, then everything discovered (builtins again are deduplicated by callers). */
export function withBuiltins(discovered: ResourceKind[]): ResourceKind[] {
	return [...BUILTIN.map((b) => b.kind), ...discovered];
}
