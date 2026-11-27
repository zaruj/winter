<script lang="ts" module>
	/** A titled part of a details view: facts, a table and/or key=value chips. */
	export interface Section {
		title: string;
		facts?: [string, string][];
		table?: { columns: string[]; rows: string[][] } | null;
		chips?: [string, string][];
	}
</script>

<script lang="ts">
	// Renders details the same way everywhere: Kubernetes objects and AWS resources.
	let { sections }: { sections: Section[] } = $props();
</script>

{#each sections as section (section.title)}
	<section>
		<h3 class="text-muted-foreground mb-1 text-[11px] font-medium tracking-wide uppercase">{section.title}</h3>
		{#if section.facts?.length}
			<dl class="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-3 gap-y-0.5">
				{#each section.facts as [label, value] (label)}
					<dt class="text-muted-foreground">{label}</dt>
					<dd class="min-w-0 break-words select-text">{value}</dd>
				{/each}
			</dl>
		{/if}
		{#if section.table}
			{#if section.table.rows.length}
				<div class="overflow-x-auto">
					<table class="w-full">
						<thead class="text-muted-foreground">
							<tr>
								{#each section.table.columns as c (c)}<th class="py-0.5 pr-3 text-left font-medium">{c}</th>{/each}
							</tr>
						</thead>
						<tbody>
							{#each section.table.rows as row, r (r)}
								<tr class="border-t align-top">
									{#each row as cell, c (c)}
										<td class="py-0.5 pr-3 break-words select-text {c === 0 ? 'font-medium' : ''}">{cell}</td>
									{/each}
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="text-muted-foreground">None</p>
			{/if}
		{/if}
		{#if section.chips?.length}
			<div class="flex flex-wrap gap-1">
				{#each section.chips as [k, v] (k)}
					<span class="bg-muted max-w-full truncate rounded px-1.5 py-px font-mono text-[10px]" title="{k}={v}">
						{k}{v ? `=${v}` : ''}
					</span>
				{/each}
			</div>
		{/if}
	</section>
{/each}
