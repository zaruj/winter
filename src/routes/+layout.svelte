<script lang="ts">
	import './layout.css';
	import { afterNavigate } from '$app/navigation';
	import favicon from '#lib/assets/favicon.svg';
	import ShortcutHelp from '#lib/components/ShortcutHelp.svelte';
	import { back, dispatchKey, inTerminal, isTyping, plain, trackNavigation } from '#lib/keys.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
	let help = $state(false);

	afterNavigate((nav) => trackNavigation(nav.type, 'delta' in nav ? nav.delta : undefined));

	function onkeydown(event: KeyboardEvent) {
		// Already handled, e.g. Esc closing a dropdown or dialog.
		if (event.defaultPrevented || help) return;
		if (dispatchKey(event)) return;

		if (event.metaKey && event.key === '[') {
			event.preventDefault();
			back();
		} else if (event.key === 'Escape' && plain(event) && !inTerminal(event)) {
			event.preventDefault();
			back();
		} else if (event.key === '?' && !isTyping(event)) {
			event.preventDefault();
			help = true;
		}
	}
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<svelte:window {onkeydown} />

{@render children()}

<ShortcutHelp bind:open={help} />
