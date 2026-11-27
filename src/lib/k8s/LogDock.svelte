<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { appearance } from '#lib/terminal/appearance.svelte.js';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import LoaderCircle from '@lucide/svelte/icons/loader-circle';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import X from '@lucide/svelte/icons/x';
	import type { LogDock, ShellTab } from './log-dock.svelte.js';
	import LogView, { type Status } from './LogView.svelte';

	// Pod logs and shells in tabs at the bottom of the cluster page, like an IDE's panel.
	let { context, dock }: { context: string; dock: LogDock } = $props();

	let root: HTMLElement | undefined = $state();
	const views: Record<number, LogView | undefined> = $state({});
	const states: Record<number, { status: Status; error: string | null }> = $state({});

	function patch(id: number, change: Partial<{ status: Status; error: string | null }>) {
		states[id] = Object.assign({ status: 'starting' as Status, error: null }, states[id], change);
	}

	const active = $derived(dock.tabs.find((t) => t.id === dock.active));

	const MIN_HEIGHT = 120;

	/** Drag the top edge to resize, keeping some of the table in view. */
	function startResize(event: PointerEvent) {
		const startY = event.clientY;
		const startHeight = root?.getBoundingClientRect().height ?? dock.height;
		// The CSS caps the dock at half the window, so the list always keeps the other half.
		const max = () => window.innerHeight / 2;
		const move = (e: PointerEvent) => {
			dock.height = Math.max(MIN_HEIGHT, Math.min(max(), startHeight + startY - e.clientY));
		};
		const up = () => {
			removeEventListener('pointermove', move);
			removeEventListener('pointerup', up);
		};
		addEventListener('pointermove', move);
		addEventListener('pointerup', up);
		event.preventDefault();
	}

	function openShell() {
		if (!active) return;
		const { namespace, pod, container } = active;
		dock.openShell(context, { namespace, pod, container });
	}

	/** Shows a shell's terminal in `node` for as long as the tab is open. */
	function shellHost(node: HTMLElement, tab: ShellTab) {
		// Focus a shell the user just opened; not one that comes back with the page.
		const focus = dock.focusNext === tab.id;
		if (focus) dock.focusNext = null;
		tab.terminal.attach(node, focus);
		const observer = new ResizeObserver(() => requestAnimationFrame(() => tab.terminal.fit()));
		observer.observe(node);
		return { destroy: () => observer.disconnect() };
	}

	/** Puts keyboard focus in the dock: the shell, or the log so j/k scroll it. */
	export function focus() {
		if (active?.kind === 'shell') active.terminal.focus();
		else root?.focus();
	}

	/** True when typing goes to a shell in the dock (so the page must leave keys alone). */
	export function shellFocused() {
		return active?.kind === 'shell' && hasFocus();
	}

	/** True when keyboard focus is in the dock. */
	export function hasFocus() {
		return !!root?.contains(document.activeElement);
	}

	/** Keys while the dock has focus: scrolling, `s` shell, `x` close tab, `[` `]` switch tab. */
	export function handleKey(event: KeyboardEvent): boolean {
		if (!active || active.kind === 'shell') return false;
		if (views[active.id]?.handleKey(event)) return true;
		if (event.metaKey || event.ctrlKey || event.altKey) return false;
		switch (event.key) {
			case 's':
				openShell();
				return true;
			case 'x':
				dock.close(active.id);
				if (!dock.visible) (document.activeElement as HTMLElement | null)?.blur();
				return true;
			case '[':
				dock.step(-1);
				return true;
			case ']':
				dock.step(1);
				return true;
		}
		return false;
	}
</script>

<section
	bind:this={root}
	tabindex="-1"
	data-log-dock
	class="relative flex shrink-0 flex-col border-t outline-none"
	style:height="min({dock.height}px, 50vh)"
	style:background-color={appearance.background}
	style:color={appearance.foreground}
>
	<!-- Resize handle -->
	<div
		class="hover:bg-primary/40 absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize"
		role="separator"
		aria-orientation="horizontal"
		aria-label="Resize logs"
		onpointerdown={startResize}
	></div>

	<header class="flex min-w-0 items-center gap-1 border-b border-white/10 px-1 py-1 text-sm">
		<div class="flex min-w-0 flex-1 flex-wrap gap-1" role="tablist" aria-label="Logs and shells">
			{#each dock.tabs as tab (tab.id)}
				{@const state = states[tab.id]}
				<div
					class="flex max-w-64 min-w-0 items-center rounded-md {tab.id === dock.active ? 'bg-white/15' : 'opacity-70 hover:bg-white/10'}"
				>
					<button
						role="tab"
						aria-selected={tab.id === dock.active}
						class="flex min-w-0 items-center gap-1.5 py-0.5 pl-2 text-left"
						title="{tab.namespace}/{tab.pod}"
						onclick={() => (dock.active = tab.id)}
					>
						{#if tab.kind === 'shell'}
							<SquareTerminal class="size-3 shrink-0 text-emerald-400" />
						{:else if state?.status === 'starting'}
							<LoaderCircle class="size-3 shrink-0 animate-spin" />
						{:else}
							<span
								class="size-1.5 shrink-0 rounded-full {state?.status === 'following'
									? 'bg-emerald-500'
									: state?.status === 'failed'
										? 'bg-red-500'
										: 'bg-white/40'}"
							></span>
						{/if}
						<span class="truncate">{tab.pod}</span>
					</button>
					<button
						class="rounded p-1 opacity-60 hover:opacity-100"
						aria-label="Close {tab.pod} {tab.kind}"
						title="Close (⌘W)"
						onclick={() => dock.close(tab.id)}
					>
						<X class="size-3" />
					</button>
				</div>
			{/each}
		</div>
		{#if active}
			{#if active.kind === 'log' && active.containers.length > 1}
				<Select.Root type="single" value={active.container ?? ''} onValueChange={(v) => (active.container = v)}>
					<Select.Trigger size="sm" class="h-7 w-40 border-white/20 bg-transparent" aria-label="Container">
						<span class="truncate">{active.container}</span>
					</Select.Trigger>
					<Select.Content>
						{#each active.containers as name (name)}
							<Select.Item value={name} label={name} />
						{/each}
					</Select.Content>
				</Select.Root>
			{/if}
			<Button variant="ghost" size="icon-sm" title="New shell in this pod (s)" aria-label="New shell" class="hover:bg-white/10" onclick={openShell}>
				<SquareTerminal />
			</Button>
		{/if}
		<Button variant="ghost" size="icon-sm" title="Hide (L, or ⌘J from a shell)" aria-label="Hide logs and shells" class="hover:bg-white/10" onclick={() => (dock.visible = false)}>
			<ChevronDown />
		</Button>
	</header>

	{#if active && states[active.id]?.error}
		<div class="border-b border-red-500/30 bg-red-500/10 px-3 py-1 text-xs text-red-400">{states[active.id].error}</div>
	{/if}

	<div class="relative min-h-0 flex-1">
		{#each dock.tabs as tab (tab.id)}
			<div class="absolute inset-0 {tab.id === dock.active ? '' : 'invisible'}">
				{#if tab.kind === 'shell'}
					<!-- data-shell: Esc belongs to the shell, not "back". -->
					<div class="h-full px-2 pt-1" data-shell use:shellHost={tab}></div>
				{:else}
				<LogView
					bind:this={views[tab.id]}
					{context}
					namespace={tab.namespace}
					pod={tab.pod}
					container={tab.container}
					bind:status={() => states[tab.id]?.status ?? 'starting', (status) => patch(tab.id, { status })}
					bind:error={() => states[tab.id]?.error ?? null, (error) => patch(tab.id, { error })}
				/>
				{/if}
			</div>
		{/each}
	</div>

	<footer class="hidden gap-x-3 border-t border-white/10 px-3 py-0.5 text-[11px] opacity-60 sm:flex">
		{#if active?.kind === 'shell'}
			<span>keys go to the shell</span>
			<span><Kbd>⌘</Kbd><Kbd>J</Kbd> back to the list</span>
			<span><Kbd>⌘</Kbd><Kbd>W</Kbd> close</span>
			<span class="ml-auto">exit closes the tab</span>
		{:else}
			<span>click or <Kbd>L</Kbd> to focus, then <Kbd>j</Kbd> <Kbd>k</Kbd> <Kbd>u</Kbd> <Kbd>d</Kbd> <Kbd>g</Kbd> <Kbd>G</Kbd> scroll</span>
			<span><Kbd>[</Kbd> <Kbd>]</Kbd> tabs</span>
			<span><Kbd>s</Kbd> shell</span>
			<span><Kbd>x</Kbd> or <Kbd>⌘</Kbd><Kbd>W</Kbd> close</span>
			<span><Kbd>Esc</Kbd> back to the list</span>
			<span class="ml-auto">last 1000 lines, then live</span>
		{/if}
	</footer>
</section>
