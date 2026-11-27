<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import { moveCursor } from '#lib/keys.svelte.js';
		import { suggest, suggestRegions, type Suggestion, type View } from './services.js';

	// k9s-style `:` for AWS: a service by name (`ec2`, `eks`, `alb`), optionally followed by a
	// region (`ec2 us-east-1`); or just a region to switch to.
	let {
		regions,
		current,
		onRun,
		onClose
	}: {
		regions: string[];
		/** The region shown now. */
		current: string;
		/** `kind` null: only switch region. `region` null: stay in this region. */
		onRun: (kind: View | null, region: string | null) => void;
		onClose: () => void;
	} = $props();

	let input = $state('');
	let cursor = $state(0);
	let field: HTMLInputElement;

	const words = $derived(input.trimStart().split(/\s+/));
	const first = $derived((words[0] ?? '').toLowerCase());
	const onRegion = $derived(words.length > 1);
	const matches = $derived(suggest(first, regions));
	const regionMatches = $derived(onRegion ? suggestRegions(words[1] ?? '', regions) : []);
	const count = $derived(onRegion ? regionMatches.length : matches.length);

	$effect(() => field.focus());

	$effect(() => {
		input;
		cursor = 0;
	});

	const nameOf = (s: Suggestion) => (s.type === 'service' ? s.service.names[0] : s.region);

	function complete() {
		if (onRegion) {
			const region = regionMatches[cursor];
			if (region) input = `${first} ${region}`;
		} else {
			const s = matches[cursor];
			if (s) input = s.type === 'service' ? `${nameOf(s)} ` : nameOf(s);
		}
	}

	function run() {
		if (onRegion) {
			const s = suggest(first, regions).find((x) => x.type === 'service');
			if (!s || s.type !== 'service') return;
			const region = regionMatches[cursor] ?? (regions.includes(words[1]) ? words[1] : null);
			onRun(s.service.kind, region);
			return;
		}
		const s = matches[cursor];
		if (!s) return;
		if (s.type === 'service') onRun(s.service.kind, null);
		else onRun(null, s.region);
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
</script>

<div class="bg-popover text-popover-foreground absolute inset-x-4 top-2 z-20 rounded-xl border shadow-lg">
	<div class="flex items-center gap-2 border-b px-3">
		<span class="text-muted-foreground font-mono">:</span>
		<input
			bind:this={field}
			bind:value={input}
			{onkeydown}
			onblur={onClose}
			placeholder="ec2, rds, s3, sg, cost, alb us-east-1, ap-southeast-1, …"
			aria-label="Service"
			spellcheck="false"
			autocomplete="off"
			class="h-10 flex-1 bg-transparent font-mono text-sm outline-none"
		/>
	</div>
	<!-- Keyboard selection is handled by the input above. -->
	<ul class="max-h-80 overflow-auto p-1 text-sm" role="listbox" aria-label="Suggestions">
		{#if onRegion}
			{#each regionMatches as region, i (region)}
				<li
					class="data-selected:bg-muted flex cursor-pointer items-center rounded-md px-2 py-1.5"
					role="option"
					aria-selected={i === cursor}
					tabindex="-1"
					data-selected={i === cursor || undefined}
					onmousedown={(e) => e.preventDefault()}
					onclick={() => ((cursor = i), run())}
					{onkeydown}
				>
					<span class="font-mono">{region}</span>
					{#if region === current}<span class="text-muted-foreground ml-2 text-xs">current</span>{/if}
				</li>
			{:else}
				<li class="text-muted-foreground px-2 py-1.5">No region matches “{words[1]}”.</li>
			{/each}
		{:else}
			{#each matches as s, i (s.type + nameOf(s))}
				<li
					class="data-selected:bg-muted flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5"
					role="option"
					aria-selected={i === cursor}
					tabindex="-1"
					data-selected={i === cursor || undefined}
					onmousedown={(e) => e.preventDefault()}
					onclick={() => ((cursor = i), run())}
					{onkeydown}
				>
					{#if s.type === 'service'}
						<s.service.icon class="text-muted-foreground size-4 shrink-0" />
						<span class="font-medium">{s.service.label}</span>
						<span class="text-muted-foreground font-mono text-xs">{s.service.names.slice(0, 4).join(', ')}</span>
						<span class="text-muted-foreground ml-auto text-xs">{s.service.description}</span>
					{:else}
						<span class="text-muted-foreground size-4 shrink-0 text-center">⌖</span>
						<span class="font-mono">{s.region}</span>
						<span class="text-muted-foreground ml-auto text-xs">
							{s.region === current ? 'current region' : 'switch region'}
						</span>
					{/if}
				</li>
			{:else}
				<li class="text-muted-foreground px-2 py-1.5">No service or region matches “{first}”.</li>
			{/each}
		{/if}
	</ul>
	<div class="text-muted-foreground flex flex-wrap gap-x-4 border-t px-3 py-1.5 text-xs">
		<span><Kbd>Enter</Kbd> open</span>
		<span><Kbd>Tab</Kbd> complete</span>
		<span><Kbd>↑</Kbd> <Kbd>↓</Kbd> choose</span>
		<span>add a region: <span class="font-mono">ec2 us-east-1</span></span>
		<span class="ml-auto"><Kbd>Esc</Kbd> close</span>
	</div>
</div>
