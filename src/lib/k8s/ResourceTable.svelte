<script lang="ts" module>
	import type { Snippet } from 'svelte';

	/**
	 * Hide a column when the table is narrower than this (its own width, not the window's: the
	 * sidebar, dock or a detail pane take part of the window). Least useful columns first.
	 */
	export type Hide = 'sm' | 'md' | 'lg' | 'xl';

	export interface Column<T> {
		header: string;
		class?: string;
		hide?: Hide;
		/** Plain text cell. */
		text?: (row: T) => string;
		/** Custom cell; used instead of `text`. */
		cell?: Snippet<[T]>;
		/**
		 * Longest the cell may grow (a `max-w-*` class), cutting the text with an ellipsis. Browsers
		 * ignore `max-width` on table cells themselves, so it is applied to a block inside.
		 */
		max?: string;
	}
</script>

<script lang="ts" generics="T">
	import * as Table from '#lib/components/ui/table/index.js';
	import ListState from './ListState.svelte';
	import { Paging } from './paging.svelte.js';

	let {
		list,
		rows: filtered,
		columns,
		key,
		cursor = $bindable(0),
		onActivate,
		fixed = false,
		width = $bindable(0)
	}: {
		/** Loading state of the underlying list. */
		list: { loading: boolean; error: string | null; items: readonly unknown[] };
		/** Rows after filtering. */
		rows: T[];
		columns: Column<T>[];
		key: (row: T) => string;
		cursor?: number;
		/** Double-click on a row. */
		onActivate?: (row: T) => void;
		/** Fixed layout: never wider than its container; set widths in column classes. */
		fixed?: boolean;
		/** The table's width, measured; for callers that adapt cells to it. */
		width?: number;
	} = $props();

	const paging = new Paging(() => cursor);

	/**
	 * Narrowest table that shows a column of each kind, in px. Columns that do not fit are left
	 * out of the DOM rather than hidden with CSS: WebKit still reserves the width of hidden cells
	 * in a fixed-layout table, which made it scroll sideways.
	 */
	const MIN_WIDTH: Record<Hide, number> = { sm: 480, md: 680, lg: 860, xl: 1100 };
	const visible = $derived(columns.filter((c) => !c.hide || width >= MIN_WIDTH[c.hide]));
	const classOf = (column: Column<T>) => column.class;
	// Compact rows, as in k9s.
	const cellClass = (column: Column<T>) => ['py-1', classOf(column)].filter(Boolean).join(' ');
	const rows = $derived(filtered.slice(0, paging.limit));
</script>

<div class="w-full" bind:clientWidth={width}>
<Table.Root class={fixed ? 'table-fixed' : undefined}>
	<Table.Header>
		<Table.Row>
			{#each visible as column (column.header)}
				<Table.Head class={['h-8', classOf(column)].filter(Boolean).join(' ')}>{column.header}</Table.Head>
			{/each}
		</Table.Row>
	</Table.Header>
	<Table.Body>
		{#each rows as row, i (key(row))}
			<Table.Row
				class="data-selected:bg-muted data-selected:shadow-[inset_2px_0_0_var(--foreground)]"
				data-selected={i === cursor || undefined}
				onclick={() => (cursor = i)}
				ondblclick={() => onActivate?.(row)}
			>
				{#each visible as column (column.header)}
					<Table.Cell class={cellClass(column)}>
						{#if column.max}
							<div class="truncate {column.max}" title={column.text?.(row)}>
								{#if column.cell}{@render column.cell(row)}{:else}{column.text?.(row)}{/if}
							</div>
						{:else if column.cell}{@render column.cell(row)}{:else}{column.text?.(row)}{/if}
					</Table.Cell>
				{/each}
			</Table.Row>
		{/each}
		<ListState
			columns={visible.length}
			loading={list.loading}
			error={list.error}
			total={list.items.length}
			shown={rows.length}
			filtered={filtered.length}
			onShowMore={() => paging.more()}
		/>
	</Table.Body>
</Table.Root>
</div>
