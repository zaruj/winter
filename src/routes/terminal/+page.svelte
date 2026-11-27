<script lang="ts">
	import '@xterm/xterm/css/xterm.css';
	import Kbd from '#lib/components/Kbd.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { back, useKeys } from '#lib/keys.svelte.js';
	import { appearance } from '#lib/terminal/appearance.svelte.js';
	import { isAppShortcut, sessions } from '#lib/terminal/sessions.svelte.js';
	import ArrowLeft from '@lucide/svelte/icons/arrow-left';
	import Plus from '@lucide/svelte/icons/plus';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import X from '@lucide/svelte/icons/x';
	import { afterNavigate } from '$app/navigation';
	import { onMount } from 'svelte';

	let host: HTMLElement | undefined = $state();

	onMount(() => {
		if (sessions.tabs.length === 0) sessions.create();
	});

	// SvelteKit moves focus to <body> when a navigation ends; give it back to the shell.
	afterNavigate(() => sessions.current?.focus());

	// Show the active tab; re-runs when it changes or a tab closes.
	$effect(() => {
		const tab = sessions.current;
		if (host && tab) tab.attach(host);
		else host?.replaceChildren();
	});

	$effect(() => {
		if (!host) return;
		const observer = new ResizeObserver(() => sessions.current?.fit());
		observer.observe(host);
		return () => observer.disconnect();
	});

	useKeys((event) => {
		if (!isAppShortcut(event) || event.key === '[') return;
		const key = event.key.toLowerCase();
		if (key === 't') sessions.create();
		else if (key === 'w') sessions.current?.close();
		else sessions.select(Number(key) - 1);
		return true;
	});
</script>

<main
	class="flex h-screen flex-col text-neutral-200"
	style:background-color={appearance.background}
	style:color={appearance.foreground}
>
	<header class="flex items-center gap-1 border-b border-neutral-800 px-2 py-1.5">
		<Button
			variant="ghost"
			size="icon-sm"
			onclick={back}
			aria-label="Back"
			title="Back (⌘[)"
			class="text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
		>
			<ArrowLeft />
		</Button>
		<div class="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto" role="tablist">
			{#each sessions.tabs as tab, i (tab.key)}
				<div
					class="group flex max-w-56 min-w-0 items-center rounded-md text-xs {i === sessions.active
						? 'bg-neutral-800 text-neutral-100'
						: 'text-neutral-400 hover:bg-neutral-900'}"
				>
					<button
						role="tab"
						aria-selected={i === sessions.active}
						class="flex min-w-0 items-center gap-1.5 py-1 pl-2"
						onclick={() => sessions.select(i)}
					>
						<span class="text-neutral-500">{i + 1}</span>
						<span class="truncate">{tab.title}</span>
					</button>
					<button
						class="mx-1 rounded p-0.5 opacity-60 hover:bg-neutral-700 hover:opacity-100"
						aria-label="Close tab"
						onclick={() => tab.close()}
					>
						<X class="size-3" />
					</button>
				</div>
			{/each}
			<Button
				variant="ghost"
				size="icon-xs"
				aria-label="New tab"
				title="New tab (⌘T)"
				class="text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
				onclick={() => sessions.create()}
			>
				<Plus />
			</Button>
		</div>
	</header>

	{#if sessions.tabs.length === 0}
		<div class="flex flex-1 flex-col items-center justify-center gap-3 text-sm text-neutral-400">
			<SquareTerminal class="size-8" />
			<p>No open terminals</p>
			<Button variant="secondary" onclick={() => sessions.create()}>
				New terminal <Kbd class="ml-1">⌘T</Kbd>
			</Button>
		</div>
	{/if}
	<div
		class="min-h-0 flex-1 p-2 {sessions.tabs.length === 0 ? 'hidden' : ''}"
		data-shell
		bind:this={host}
	></div>
</main>
