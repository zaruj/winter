<script lang="ts">
	import { goto } from '$app/navigation';
	import logo from '#lib/assets/favicon.svg';
	import Kbd from '#lib/components/Kbd.svelte';
	import { matches } from '#lib/k8s/format.js';
	import { isTyping, plain, useKeys } from '#lib/keys.svelte.js';
	import Search from '@lucide/svelte/icons/search';
	import Cloud from '@lucide/svelte/icons/cloud';
	import CloudCog from '@lucide/svelte/icons/cloud-cog';
	import Cloudy from '@lucide/svelte/icons/cloudy';
	import GitBranch from '@lucide/svelte/icons/git-branch';
	import GitMerge from '@lucide/svelte/icons/git-merge';
	import Globe from '@lucide/svelte/icons/globe';
	import InfinityIcon from '@lucide/svelte/icons/infinity';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import Network from '@lucide/svelte/icons/network';
	import Rocket from '@lucide/svelte/icons/rocket';
	import ShipWheel from '@lucide/svelte/icons/ship-wheel';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import type { Component } from 'svelte';

	interface Target {
		name: string;
		description: string;
		icon: Component;
		/** Set when the target is available. */
		href?: string;
		key?: string;
	}

	const sections: { title: string; targets: Target[] }[] = [
		{
			title: 'Service',
			targets: [
				{ name: 'Kubernetes', description: 'Clusters from your kubeconfig', icon: ShipWheel, href: '/k8s', key: 'k' },
				{ name: 'Terminal', description: 'Local shell', icon: SquareTerminal, href: '/terminal', key: 't' },
				{ name: 'SSH', description: 'Remote hosts', icon: KeyRound }
			]
		},
		{
			title: 'Cloud Provider',
			targets: [
				{ name: 'AWS', description: 'Profiles from ~/.aws', icon: Cloud, href: '/aws', key: 'a' },
				{ name: 'Azure', description: 'Microsoft Azure', icon: CloudCog },
				{ name: 'Google Cloud', description: 'Google Cloud Platform', icon: Cloudy },
				{ name: 'Cloudflare', description: 'Edge and DNS', icon: Globe },
				{ name: 'Akamai', description: 'Linode and CDN', icon: Network }
			]
		},
		{
			title: 'CI/CD',
			targets: [
				{ name: 'GitHub', description: 'Actions pipelines', icon: GitBranch, href: '/github', key: 'g' },
				{ name: 'GitLab', description: 'GitLab CI pipelines', icon: GitMerge },
				{ name: 'Jenkins', description: 'Jobs and builds', icon: InfinityIcon },
				{ name: 'Argo CD', description: 'GitOps deployments', icon: Rocket }
			]
		}
	];

	const byKey = new Map(
		sections.flatMap((s) => s.targets).filter((t) => t.key && t.href).map((t) => [t.key!, t.href!])
	);

	let grid: HTMLElement;
	let search: HTMLInputElement | undefined = $state();
	let query = $state('');

	/** Sections with only the targets matching the search (by name or description). */
	const shown = $derived(
		sections
			.map((s) => ({
				...s,
				targets: s.targets.filter((t) => matches(query, t.name, t.description, s.title))
			}))
			.filter((s) => s.targets.length)
	);
	/** Enter in the search box opens the first available match. */
	const firstMatch = $derived(shown.flatMap((s) => s.targets).find((t) => t.href));

	const ARROWS: Record<string, [number, number]> = {
		ArrowLeft: [-1, 0],
		ArrowRight: [1, 0],
		ArrowUp: [0, -1],
		ArrowDown: [0, 1]
	};

	/** Focuses the nearest available card in the arrow's direction, across sections. */
	function moveFocus([dx, dy]: [number, number]) {
		const cards = [...grid.querySelectorAll<HTMLElement>('a[data-card]')];
		const current = document.activeElement as HTMLElement;
		if (!cards.includes(current)) return cards[0]?.focus();
		const from = current.getBoundingClientRect();
		let best: HTMLElement | undefined;
		let bestScore = Infinity;
		for (const card of cards) {
			if (card === current) continue;
			const to = card.getBoundingClientRect();
			const along = (to.x - from.x) * dx + (to.y - from.y) * dy;
			if (along <= 0) continue;
			const across = Math.abs(dx ? to.y - from.y : to.x - from.x);
			const score = along + across * 2;
			if (score < bestScore) [best, bestScore] = [card, score];
		}
		best?.focus();
	}

	useKeys((event) => {
		if (event.target === search) {
			if (event.key === 'Escape') {
				if (query) query = '';
				else search?.blur();
				return true;
			}
			if (event.key === 'Enter') {
				if (firstMatch?.href) goto(firstMatch.href);
				return true;
			}
			if (event.key === 'ArrowDown') {
				grid.querySelector<HTMLElement>('a[data-card]')?.focus();
				return true;
			}
			return;
		}
		if (isTyping(event) || !plain(event)) return;
		if (event.key === '/') {
			search?.focus();
			return true;
		}
		const href = byKey.get(event.key);
		if (href) {
			goto(href);
			return true;
		}
		const arrow = ARROWS[event.key];
		if (arrow) {
			moveFocus(arrow);
			return true;
		}
	});
</script>

<main class="mx-auto flex h-screen max-w-5xl flex-col overflow-y-auto px-4 py-5 sm:px-8 sm:py-6" bind:this={grid}>
	<div class="flex items-center gap-4">
		<h1 class="flex items-center gap-2 text-2xl font-semibold tracking-tight">
			<img src={logo} alt="" class="-m-1 size-9" />winter
		</h1>
		<div class="relative ml-auto w-full max-w-72">
			<Search class="text-muted-foreground absolute top-1/2 left-2.5 size-4 -translate-y-1/2" />
			<input
				bind:this={search}
				bind:value={query}
				placeholder="Search services"
				aria-label="Search services"
				spellcheck="false"
				autocomplete="off"
				class="border-input focus-visible:border-ring focus-visible:ring-ring/50 h-8 w-full rounded-md border bg-transparent pr-8 pl-8 text-sm outline-none focus-visible:ring-3"
			/>
			{#if !query}<span class="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2"><Kbd>/</Kbd></span>{/if}
		</div>
		<span class="text-muted-foreground hidden text-xs whitespace-nowrap sm:inline"><Kbd>?</Kbd> shortcuts</span>
	</div>

	{#if query && !shown.length}
		<p class="text-muted-foreground mt-8 text-center text-sm">Nothing matches “{query}”.</p>
	{/if}
	{#each shown as section (section.title)}
		<section class="mt-5">
			<h2 class="text-muted-foreground mb-2 text-xs font-medium tracking-wide uppercase">
				{section.title}
			</h2>
			<!-- As many columns as fit; compact cards so every section shows without scrolling. -->
			<div class="grid grid-cols-[repeat(auto-fill,minmax(12.5rem,1fr))] gap-2">
				{#each section.targets as t (t.name)}
					{@const Icon = t.icon}
					{#if t.href}
						<a
							href={t.href}
							data-card
							class="bg-card hover:border-foreground/30 focus-visible:border-foreground/40 focus-visible:ring-ring/50 flex items-center gap-3 rounded-lg border px-3 py-2.5 transition-colors outline-none focus-visible:ring-3"
						>
							<Icon class="size-5 shrink-0" />
							<div class="min-w-0 flex-1">
								<div class="truncate text-sm font-medium">{t.name}</div>
								<div class="text-muted-foreground truncate text-xs">{t.description}</div>
							</div>
							{#if query && t === firstMatch}<Kbd>Enter</Kbd>{:else if t.key}<Kbd>{t.key}</Kbd>{/if}
						</a>
					{:else}
						<div class="flex items-center gap-3 rounded-lg border border-dashed px-3 py-2.5 opacity-60" aria-disabled="true">
							<Icon class="text-muted-foreground size-5 shrink-0" />
							<div class="min-w-0 flex-1">
								<div class="truncate text-sm font-medium">{t.name}</div>
								<div class="text-muted-foreground truncate text-xs">{t.description}</div>
							</div>
							<span class="text-muted-foreground text-[10px] uppercase">Soon</span>
						</div>
					{/if}
				{/each}
			</div>
		</section>
	{/each}
</main>
