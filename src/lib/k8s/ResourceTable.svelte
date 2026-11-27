<script lang="ts" module>
	import type { Snippet } from 'svelte';

	/** Hide a column on windows narrower than this breakpoint, least useful columns first. */
	export type Hide = 'sm' | 'md' | 'lg' | 'xl';

	export interface Column<T> {
		header: string;
		class?: string;
		hide?: Hide;
		/** Plain text cell. */
		text?: (row: T) => string;
		/** Custom cell; used instead of `text`. */
		cell?: Snippet<[T]>;
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
		fixed = false
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
	} = $props();

	const paging = new Paging(() => cursor);

	// Literal class names, so Tailwind generates them.
	const HIDE: Record<Hide, string> = {
		sm: 'hidden sm:table-cell',
		md: 'hidden md:table-cell',
		lg: 'hidden lg:table-cell',
		xl: 'hidden xl:table-cell'
	};
	const classOf = (column: Column<T>) =>
		[column.class, column.hide && HIDE[column.hide]].filter(Boolean).join(' ') || undefined;
	// Compact rows, as in k9s.
	const cellClass = (column: Column<T>) => ['py-1', classOf(column)].filter(Boolean).join(' ');
	const rows = $derived(filtered.slice(0, paging.limit));
</script>

<Table.Root class={fixed ? 'table-fixed' : undefined}>
	<Table.Header>
		<Table.Row>
			{#each columns as column (column.header)}
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
				{#each columns as column (column.header)}
					<Table.Cell class={cellClass(column)}>
						{#if column.cell}{@render column.cell(row)}{:else}{column.text?.(row)}{/if}
					</Table.Cell>
				{/each}
			</Table.Row>
		{/each}
		<ListState
			columns={columns.length}
			loading={list.loading}
			error={list.error}
			total={list.items.length}
			shown={rows.length}
			filtered={filtered.length}
			onShowMore={() => paging.more()}
		/>
	</Table.Body>
</Table.Root>
