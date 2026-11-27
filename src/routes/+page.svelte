<script lang="ts">
	import { goto } from '$app/navigation';
	import Kbd from '#lib/components/Kbd.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { isTyping, plain, useKeys } from '#lib/keys.svelte.js';
	import Cloud from '@lucide/svelte/icons/cloud';
	import CloudCog from '@lucide/svelte/icons/cloud-cog';
	import Cloudy from '@lucide/svelte/icons/cloudy';
	import Globe from '@lucide/svelte/icons/globe';
	import KeyRound from '@lucide/svelte/icons/key-round';
	import Network from '@lucide/svelte/icons/network';
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
		}
	];

	const byKey = new Map(
		sections.flatMap((s) => s.targets).filter((t) => t.key && t.href).map((t) => [t.key!, t.href!])
	);

	let grid: HTMLElement;

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
		if (isTyping(event) || !plain(event)) return;
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

<main class="mx-auto max-w-4xl p-8" bind:this={grid}>
	<div class="flex items-baseline justify-between">
		<h1 class="text-2xl font-semibold tracking-tight">winter</h1>
		<span class="text-muted-foreground text-xs">Press <Kbd>?</Kbd> for shortcuts</span>
	</div>

	{#each sections as section (section.title)}
		<section class="mt-8">
			<h2 class="text-muted-foreground mb-3 text-xs font-medium tracking-wide uppercase">
				{section.title}
			</h2>
			<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
				{#each section.targets as t (t.name)}
					{@const Icon = t.icon}
					{#if t.href}
						<a
							href={t.href}
							data-card
							class="bg-card hover:border-foreground/30 focus-visible:border-foreground/40 focus-visible:ring-ring/50 flex flex-col gap-3 rounded-xl border p-4 transition-colors outline-none focus-visible:ring-3"
						>
							<div class="flex items-start justify-between">
								<Icon class="size-6" />
								{#if t.key}<Kbd>{t.key}</Kbd>{/if}
							</div>
							<div>
								<div class="font-medium">{t.name}</div>
								<div class="text-muted-foreground text-xs">{t.description}</div>
							</div>
						</a>
					{:else}
						<div class="bg-card flex flex-col gap-3 rounded-xl border p-4 opacity-50" aria-disabled="true">
							<div class="flex items-start justify-between">
								<Icon class="size-6" />
								<Badge variant="secondary">Soon</Badge>
							</div>
							<div>
								<div class="font-medium">{t.name}</div>
								<div class="text-muted-foreground text-xs">{t.description}</div>
							</div>
						</div>
					{/if}
				{/each}
			</div>
		</section>
	{/each}
</main>
