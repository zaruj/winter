<script lang="ts" module>
	export type PanelMode = 'detail' | 'yaml';
</script>

<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import FileCode from '@lucide/svelte/icons/file-code';
	import FileText from '@lucide/svelte/icons/file-text';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import X from '@lucide/svelte/icons/x';
	import { getObject, type ObjectView, type ResourceKind } from './api.js';
	import Sections from '#lib/components/Sections.svelte';
	import { describe } from './describe.js';
	import { age } from './format.js';

	// Details (`d`, as kubectl describe) or YAML (`y`, as kubectl get -o yaml) of the selected row,
	// beside the list. It follows the cursor: moving in the list shows the next object.

	let {
		context,
		kind,
		namespace,
		name,
		mode = $bindable(),
		reveal = $bindable(false),
		now,
		onClose
	}: {
		context: string;
		kind: ResourceKind;
		namespace: string | null;
		/** Null when the list is empty. */
		name: string | null;
		mode: PanelMode;
		/** Show Secret values. */
		reveal?: boolean;
		now: number;
		onClose: () => void;
	} = $props();

	/** While moving quickly through the list, fetch only where the cursor rests. */
	const SETTLE_MS = 150;

	let view = $state<ObjectView | null>(null);
	/** What `view` shows, to tell a stale one from the current one. */
	let shown = $state('');
	let error = $state<string | null>(null);
	let loading = $state(false);
	let body: HTMLElement | undefined = $state();

	const target = $derived(name ? `${kind.plural}.${kind.group}/${namespace ?? ''}/${name}` : '');
	const isSecret = $derived(kind.kind === 'Secret' && !kind.group);

	let generation = 0;
	export async function load() {
		const mine = ++generation;
		const [t, ns, n, r] = [target, namespace, name, reveal];
		if (!n) {
			view = null;
			shown = '';
			return;
		}
		loading = true;
		try {
			const found = await getObject(context, kind, ns, n, r);
			if (mine !== generation) return;
			if (shown !== t) body?.scrollTo({ top: 0 });
			view = found;
			shown = t;
			error = null;
		} catch (e) {
			if (mine === generation) {
				error = String(e);
				shown = t;
				view = null;
			}
		} finally {
			if (mine === generation) loading = false;
		}
	}

	$effect(() => {
		target;
		reveal;
		const timer = setTimeout(load, view ? SETTLE_MS : 0);
		return () => clearTimeout(timer);
	});

	/** Scrolls the panel by about a page (`J` / `K`, PageDown / PageUp). */
	export function scrollBy(pages: number) {
		body?.scrollBy({ top: pages * (body.clientHeight - 40) });
	}

	const sections = $derived(view && mode === 'detail' ? describe(view.object, now) : []);
	const lines = $derived(view && mode === 'yaml' ? view.yaml.replace(/\n$/, '').split('\n') : []);
	/** Splits a YAML line into indent, key and the rest, for colouring keys. */
	const KEY = /^(\s*(?:- )?)([^\s:#'"][^:#]*?:)(\s.*|$)/;
</script>

{#snippet modeButton(m: PanelMode, label: string, key: string)}
	<button
		class="flex items-center gap-1.5 rounded-md px-2 py-0.5 {mode === m ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground'}"
		onclick={() => (mode = m)}
		title="{label} ({key})"
	>
		{#if m === 'detail'}<FileText class="size-3.5" />{:else}<FileCode class="size-3.5" />{/if}
		{label}
	</button>
{/snippet}

<aside class="flex h-full min-h-0 min-w-0 flex-col text-xs" data-object-panel={mode} aria-label="{kind.kind} {mode}">
	<header class="flex items-center gap-1 border-b px-2 py-1">
		<div class="min-w-0 flex-1">
			<div class="truncate text-sm font-semibold" title={name ?? ''}>{name ?? 'Nothing selected'}</div>
			<div class="text-muted-foreground truncate">
				{kind.kind}{namespace ? ` · ${namespace}` : ''}
				{#if view?.object?.metadata?.creationTimestamp}· {age(view.object.metadata.creationTimestamp, now)}{/if}
			</div>
		</div>
		<div class="flex shrink-0 items-center text-xs">
			{@render modeButton('detail', 'Details', 'd')}
			{@render modeButton('yaml', 'YAML', 'y')}
		</div>
		<Button variant="ghost" size="icon-xs" onclick={load} aria-label="Refresh" title="Refresh (r)">
			<RefreshCw class={loading ? 'animate-spin' : ''} />
		</Button>
		<Button variant="ghost" size="icon-xs" onclick={onClose} aria-label="Close panel" title="Close (Esc)">
			<X />
		</Button>
	</header>

	<div class="min-h-0 flex-1 overflow-auto {loading && shown !== target ? 'opacity-60' : ''}" bind:this={body}>
		{#if error}
			<p class="text-destructive p-3 whitespace-pre-wrap">{error}</p>
		{:else if !view}
			<div class="grid gap-2 p-3">
				{#if name}<Skeleton class="h-6" /><Skeleton class="h-24" /><Skeleton class="h-16" />{:else}
					<p class="text-muted-foreground">The list is empty.</p>
				{/if}
			</div>
		{:else if mode === 'yaml'}
			<pre class="p-2 font-mono text-[11px] leading-4 select-text"><code
					>{#each lines as line, i (i)}<span class="flex"
							><span class="text-muted-foreground/60 w-8 shrink-0 pr-2 text-right select-none">{i + 1}</span
							>{#if KEY.exec(line)}{@const m = KEY.exec(line)!}<span class="whitespace-pre-wrap"
									>{m[1]}<span class="text-sky-700 dark:text-sky-300">{m[2]}</span>{m[3]}</span
								>{:else}<span class="whitespace-pre-wrap">{line}</span>{/if}</span
						>{/each}</code
				></pre>
		{:else}
			<div class="grid gap-3 p-3">
				<Sections {sections} />

				<section>
					<h3 class="text-muted-foreground mb-1 text-[11px] font-medium tracking-wide uppercase">
						Events <span class="tabular-nums">{view.events.length}</span>
					</h3>
					{#if view.eventsError}
						<p class="text-muted-foreground">Cannot read events: {view.eventsError}</p>
					{:else if view.events.length === 0}
						<p class="text-muted-foreground">None recently (events last about an hour).</p>
					{:else}
						<ul class="grid gap-1">
							{#each view.events as e, i (i)}
								<li class="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-x-2">
									<span class="font-medium {e.type === 'Warning' ? 'text-amber-600 dark:text-amber-400' : ''}">{e.reason}</span>
									<span class="min-w-0 break-words select-text">{e.message}</span>
									<span class="text-muted-foreground text-right whitespace-nowrap tabular-nums">
										{e.count > 1 ? `×${e.count} · ` : ''}{e.last ? age(e.last, now) : ''}
									</span>
								</li>
							{/each}
						</ul>
					{/if}
				</section>
			</div>
		{/if}
	</div>

	<footer class="text-muted-foreground flex flex-wrap gap-x-3 border-t px-2 py-1">
		<span><Kbd>J</Kbd> <Kbd>K</Kbd> scroll</span>
		<span><Kbd>{mode === 'yaml' ? 'd' : 'y'}</Kbd> {mode === 'yaml' ? 'details' : 'YAML'}</span>
		{#if isSecret}<span><Kbd>x</Kbd> {reveal ? 'hide' : 'show'} values</span>{/if}
		<span class="ml-auto"><Kbd>Esc</Kbd> close</span>
	</footer>
</aside>
