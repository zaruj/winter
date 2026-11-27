<script lang="ts">
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import Kbd from './Kbd.svelte';

	let { open = $bindable(false) }: { open?: boolean } = $props();

	const groups: { title: string; keys: [string[], string][] }[] = [
		{
			title: 'Everywhere',
			keys: [
				[['Esc'], 'Back (outside the terminal)'],
				[['⌘', '['], 'Back'],
				[['?'], 'This help']
			]
		},
		{
			title: 'Home',
			keys: [
				[['k'], 'Kubernetes'],
				[['t'], 'Terminal'],
				[['a'], 'AWS'],
				[['g'], 'GitHub Actions'],
				[['/'], 'Search services, Enter opens the first'],
				[['←', '↑', '→', '↓'], 'Move, then Enter']
			]
		},
		{
			title: 'Clusters',
			keys: [
				[['type'], 'Search'],
				[['↑', '↓'], 'Move (or Ctrl+K / Ctrl+J)'],
				[['Enter'], 'Open cluster'],
				[['Esc'], 'Clear search, then back']
			]
		},
		{
			title: 'Cluster',
			keys: [
				[[':'], 'Any resource (e.g. :sts, :no, :po argocd)'],
				[['j', 'k'], 'Move down / up'],
				[['g', 'G'], 'Top / bottom'],
				[['1–9'], 'Namespaces … Secrets, Nodes (numbers in the sidebar)'],
				[['[', ']'], 'Previous / next in the sidebar'],
				[['Space'], 'Tick namespace'],
				[['Enter'], 'Open namespace, node details, pod logs'],
				[['d'], 'Details of the selected item, beside the list (as kubectl describe)'],
				[['y'], 'YAML of the selected item, beside the list'],
				[['J', 'K'], 'Scroll the details / YAML (Esc closes)'],
				[['x'], 'Show / hide Secret values in that panel'],
				[['0'], 'All namespaces'],
				[['l'], 'Pod logs (opens below)'],
				[['L'], 'Show and focus / hide logs and shells'],
				[['⌘', 'J'], 'Switch between the list and the dock'],
				[['s'], 'Shell in pod (opens below)'],
				[['/'], 'Filter'],
				[['r'], 'Refresh']
			]
		},
		{
			title: 'Node details (Enter on a node)',
			keys: [
				[['j', 'k'], 'Move in the node’s pods (pages turn by themselves)'],
				[['l', 'Enter'], 'Pod logs'],
				[['s'], 'Shell in pod'],
				[['r'], 'Refresh'],
				[['Esc'], 'Back to the nodes']
			]
		},
		{
			title: 'Logs (bottom; click or L to focus)',
			keys: [
				[['j', 'k'], 'Scroll'],
				[['u', 'd'], 'Page up / down'],
				[['g', 'G'], 'Top / bottom'],
				[['[', ']'], 'Previous / next log'],
				[['x'], 'Close log (or ⌘W)'],
				[['s'], 'Shell in pod'],
				[['Esc'], 'Back to the list']
			]
		},
		{
			title: 'Shell (bottom)',
			keys: [
				[['type'], 'Keys go to the shell, Esc too'],
				[['⌘', 'J'], 'Back to the list'],
				[['⌘', 'W'], 'Close the shell'],
				[['exit'], 'Closes the tab too']
			]
		},
		{
			title: 'AWS',
			keys: [
				[['type'], 'Search profiles, then Enter'],
				[[':'], 'Service or region (e.g. :eks, :alb us-east-1, :ap-southeast-1)'],
				[['1–9'], 'Services in sidebar order (Route 53 and Cost via : or [ ])'],
				[['d'], 'Details of the selected resource, beside the list (Enter opens too)'],
				[['J', 'K'], 'Scroll the details (Esc closes)'],
				[['[', ']'], 'Previous / next service'],
				[['j', 'k'], 'Move'],
				[['/'], 'Filter'],
				[['r'], 'Refresh (on Cost overview: only the cost, which AWS bills)']
			]
		},
		{
			title: 'GitHub',
			keys: [
				[['type'], 'Search repositories, or owner/name, then Enter'],
				[['[', ']'], 'Previous / next workflow'],
				[['Enter'], 'Run: jobs and steps; on a job: its log'],
				[['j', 'k'], 'Move'],
				[['o'], 'Open on GitHub'],
				[['/'], 'Filter'],
				[['r'], 'Refresh']
			]
		},
		{
			title: 'Terminal',
			keys: [
				[['⌘', 'T'], 'New tab'],
				[['⌘', 'W'], 'Close tab'],
				[['⌘', '1–9'], 'Switch tab'],
				[['⌘', '['], 'Leave (Esc goes to the shell)']
			]
		}
	];
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-h-[85vh] overflow-auto sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>Keyboard shortcuts</Dialog.Title>
		</Dialog.Header>
		<div class="grid gap-5">
			{#each groups as group (group.title)}
				<section>
					<h3 class="text-muted-foreground mb-2 text-xs font-medium uppercase">{group.title}</h3>
					<dl class="grid gap-1.5 text-sm">
						{#each group.keys as [keys, label] (label)}
							<div class="flex items-center justify-between gap-4">
								<dt>{label}</dt>
								<dd class="flex gap-1">
									{#each keys as key (key)}<Kbd>{key}</Kbd>{/each}
								</dd>
							</div>
						{/each}
					</dl>
				</section>
			{/each}
		</div>
	</Dialog.Content>
</Dialog.Root>
