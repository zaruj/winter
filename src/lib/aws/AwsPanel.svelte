<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import Sections from '#lib/components/Sections.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import X from '@lucide/svelte/icons/x';
	import { getDetail, type AwsKind, type DetailSection } from './api.js';

	// Details of the selected AWS resource, beside the list, as the Kubernetes panel does. It
	// follows the cursor; each service fetches what its console page shows first.

	let {
		profile,
		region,
		kind,
		label,
		id,
		title,
		onClose
	}: {
		profile: string;
		/** Region to ask in (the bucket's own for S3). */
		region: string;
		kind: AwsKind;
		/** The service's name, e.g. "Security groups". */
		label: string;
		/** Null when the list is empty. */
		id: string | null;
		title: string;
		onClose: () => void;
	} = $props();

	/** While moving quickly through the list, fetch only where the cursor rests. */
	const SETTLE_MS = 200;

	let sections = $state<DetailSection[] | null>(null);
	let shown = $state('');
	let error = $state<string | null>(null);
	let loading = $state(false);
	let body: HTMLElement | undefined = $state();

	const target = $derived(id ? `${kind}/${region}/${id}` : '');

	let generation = 0;
	export async function load() {
		const mine = ++generation;
		const [t, i, r, k] = [target, id, region, kind];
		if (!i) {
			sections = null;
			shown = '';
			return;
		}
		loading = true;
		try {
			const found = await getDetail(profile, r, k, i);
			if (mine !== generation) return;
			if (shown !== t) body?.scrollTo({ top: 0 });
			sections = found;
			shown = t;
			error = null;
		} catch (e) {
			if (mine === generation) {
				error = String(e);
				sections = null;
				shown = t;
			}
		} finally {
			if (mine === generation) loading = false;
		}
	}

	$effect(() => {
		target;
		const timer = setTimeout(load, sections ? SETTLE_MS : 0);
		return () => clearTimeout(timer);
	});

	/** Scrolls by about a page (`J` / `K`, PageDown / PageUp). */
	export function scrollBy(pages: number) {
		body?.scrollBy({ top: pages * (body.clientHeight - 40) });
	}
</script>

<aside class="flex h-full min-h-0 min-w-0 flex-col text-xs" data-aws-panel={kind} aria-label="{label} details">
	<header class="flex items-center gap-1 border-b px-2 py-1">
		<div class="min-w-0 flex-1">
			<div class="truncate text-sm font-semibold" title={title}>{id ? title : 'Nothing selected'}</div>
			<div class="text-muted-foreground truncate">
				{label}{id && id !== title ? ` · ${id}` : ''}
			</div>
		</div>
		<Button variant="ghost" size="icon-xs" onclick={load} aria-label="Refresh" title="Refresh (r)">
			<RefreshCw class={loading ? 'animate-spin' : ''} />
		</Button>
		<Button variant="ghost" size="icon-xs" onclick={onClose} aria-label="Close details" title="Close (Esc)">
			<X />
		</Button>
	</header>

	<div class="min-h-0 flex-1 overflow-auto {loading && shown !== target ? 'opacity-60' : ''}" bind:this={body}>
		{#if error}
			<p class="text-destructive p-3 whitespace-pre-wrap">{error}</p>
		{:else if !sections}
			<div class="grid gap-2 p-3">
				{#if id}<Skeleton class="h-6" /><Skeleton class="h-24" /><Skeleton class="h-16" />{:else}
					<p class="text-muted-foreground">The list is empty.</p>
				{/if}
			</div>
		{:else}
			<div class="grid gap-3 p-3"><Sections {sections} /></div>
		{/if}
	</div>

	<footer class="text-muted-foreground flex flex-wrap gap-x-3 border-t px-2 py-1">
		<span><Kbd>j</Kbd> <Kbd>k</Kbd> next item</span>
		<span><Kbd>J</Kbd> <Kbd>K</Kbd> scroll</span>
		<span class="ml-auto"><Kbd>Esc</Kbd> close</span>
	</footer>
</aside>
