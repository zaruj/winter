<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import { moveCursor } from '#lib/keys.svelte.js';
	import type { ResourceKind } from './api.js';
	import { builtinTab, parse, suggestKinds, suggestNamespaces } from './command.js';

	let {
		kinds,
		namespaces,
		discovering,
		onRun,
		onClose
	}: {
		kinds: ResourceKind[];
		namespaces: string[];
		/** True while the cluster's kinds are still being discovered. */
		discovering: boolean;
		onRun: (kind: ResourceKind, namespace: string | null) => void;
		onClose: () => void;
	} = $props();

	let input = $state('');
	let cursor = $state(0);
	let field: HTMLInputElement;

	const parsed = $derived(parse(input));
	const kindMatches = $derived(suggestKinds(parsed.resource, kinds));
	const nsMatches = $derived(
		parsed.onNamespace ? suggestNamespaces(parsed.namespace ?? '', namespaces) : []
	);
	const count = $derived(parsed.onNamespace ? nsMatches.length : kindMatches.length);

	$effect(() => field.focus());

	// New input: back to the best match.
	$effect(() => {
		input;
		cursor = 0;
	});

	/** Fills in the highlighted suggestion. */
	function complete() {
		if (parsed.onNamespace) {
			const ns = nsMatches[cursor];
			if (ns) input = `${parsed.resource} ${ns}`;
		} else {
			const kind = kindMatches[cursor];
			if (kind) input = `${kind.shortNames[0] ?? kind.plural} `;
		}
	}

	function run() {
		// The resource word picks the best kind (or the highlighted one when choosing a kind).
		const kind = parsed.onNamespace ? suggestKinds(parsed.resource, kinds)[0] : kindMatches[cursor];
		if (!kind) return;
		const ns = parsed.onNamespace ? (nsMatches[cursor] ?? parsed.namespace) : null;
		onRun(kind, ns);
	}

	function onkeydown(event: KeyboardEvent) {
		const next = moveCursor(event, cursor, count, false);
		if (next !== null) cursor = next;
		else if (event.key === 'Tab') complete();
		else if (event.key === 'Enter') run();
		else if (event.key === 'Escape') onClose();
		else return;
		event.preventDefault();
		event.stopPropagation();
	}

	const label = (k: ResourceKind) => (k.group ? `${k.plural}.${k.group}` : k.plural);
</script>

<div class="bg-popover text-popover-foreground absolute inset-x-4 top-2 z-20 rounded-xl border shadow-lg">
	<div class="flex items-center gap-2 border-b px-3">
		<span class="text-muted-foreground font-mono">:</span>
		<input
			bind:this={field}
			bind:value={input}
			{onkeydown}
			onblur={onClose}
			placeholder="pods, svc, deploy argocd, crd, …"
			aria-label="Resource"
			spellcheck="false"
			autocomplete="off"
			class="h-10 flex-1 bg-transparent font-mono text-sm outline-none"
		/>
		{#if discovering}<span class="text-muted-foreground text-xs">loading kinds…</span>{/if}
	</div>
	<!-- Keyboard selection is handled by the input above. -->
	<ul class="max-h-80 overflow-auto p-1 text-sm" role="listbox" aria-label="Suggestions">
		{#if parsed.onNamespace}
			{#each nsMatches as ns, i (ns)}
				<li
					class="flex cursor-pointer items-center rounded-md px-2 py-1.5 data-selected:bg-muted"
					role="option"
					aria-selected={i === cursor}
					tabindex="-1"
					data-selected={i === cursor || undefined}
					onmousedown={(e) => e.preventDefault()}
					onclick={() => ((cursor = i), run())}
					onkeydown={onkeydown}
				>
					<span class="font-mono">{ns}</span>
					{#if ns === 'all'}<span class="text-muted-foreground ml-2 text-xs">every namespace</span>{/if}
				</li>
			{/each}
		{:else}
			{#each kindMatches as k, i (label(k))}
				<li
					class="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 data-selected:bg-muted"
					role="option"
					aria-selected={i === cursor}
					tabindex="-1"
					data-selected={i === cursor || undefined}
					onmousedown={(e) => e.preventDefault()}
					onclick={() => ((cursor = i), run())}
					onkeydown={onkeydown}
				>
					<span class="font-medium">{k.kind}</span>
					<span class="text-muted-foreground font-mono text-xs">{label(k)}</span>
					{#if k.shortNames.length}
						<span class="text-muted-foreground font-mono text-xs">{k.shortNames.join(', ')}</span>
					{/if}
					<span class="text-muted-foreground ml-auto text-xs">
						{builtinTab(k) ? 'tab' : k.namespaced ? 'namespaced' : 'cluster'}
					</span>
				</li>
			{:else}
				{#if parsed.resource}
					<li class="text-muted-foreground px-2 py-1.5">No resource kind matches “{parsed.resource}”.</li>
				{/if}
			{/each}
		{/if}
	</ul>
	<div class="text-muted-foreground flex gap-4 border-t px-3 py-1.5 text-xs">
		<span><Kbd>Enter</Kbd> open</span>
		<span><Kbd>Tab</Kbd> complete</span>
		<span><Kbd>↑</Kbd> <Kbd>↓</Kbd> choose</span>
		<span>add a namespace: <span class="font-mono">po argocd</span>, <span class="font-mono">po all</span></span>
		<span class="ml-auto"><Kbd>Esc</Kbd> close</span>
	</div>
</div>
