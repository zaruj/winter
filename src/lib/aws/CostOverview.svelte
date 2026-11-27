<script lang="ts" module>
	import type { CostOverview } from './api.js';

	/** Fetched overviews by profile, kept while the app runs: every fetch is billed by AWS. */
	const cache = new Map<string, { data: CostOverview; at: number }>();
</script>

<script lang="ts">
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { age } from '#lib/k8s/format.js';
	import { getCostOverview } from './api.js';

	// Cost Explorer overview for one account: three stat tiles, daily spend for last month and this
	// month, and the services that cost the most. Laid out to fill the area without scrolling; the
	// service list shows as many rows as fit and folds the rest into "Other".

	let { profile, now }: { profile: string; now: number } = $props();

	let data = $state<CostOverview | null>(null);
	let fetchedAt = $state(0);
	let error = $state<string | null>(null);
	let loading = $state(false);

	let generation = 0;
	/** Fetches again, even when cached (three billed requests). */
	export async function reload() {
		const mine = ++generation;
		const p = profile;
		loading = true;
		error = null;
		try {
			const found = await getCostOverview(p);
			cache.set(p, { data: found, at: Date.now() });
			if (mine === generation) {
				data = found;
				fetchedAt = Date.now();
			}
		} catch (e) {
			if (mine === generation) error = String(e);
		} finally {
			if (mine === generation) loading = false;
		}
	}

	$effect(() => {
		const cached = cache.get(profile);
		if (cached) {
			data = cached.data;
			fetchedAt = cached.at;
		} else {
			data = null;
			reload();
		}
	});

	// --- numbers ---------------------------------------------------------------------------

	const money = $derived(
		new Intl.NumberFormat('en-US', { style: 'currency', currency: data?.currency ?? 'USD', maximumFractionDigits: 2 })
	);
	const fmt = (n: number) => money.format(n);
	const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
	const dayLabel = (date: string) => `${MONTHS[Number(date.slice(5, 7)) - 1]} ${Number(date.slice(8, 10))}`;
	const monthName = (date: string) => MONTHS[Number(date.slice(5, 7)) - 1];

	/** Change against a base, as a signed percentage; null when there is no base. */
	function change(value: number, base: number): number | null {
		if (Math.abs(base) < 0.005) return null;
		return ((value - base) / Math.abs(base)) * 100;
	}
	const signed = (pct: number) => `${pct > 0 ? '+' : ''}${pct.toFixed(Math.abs(pct) < 10 ? 1 : 0)}%`;

	// --- chart -----------------------------------------------------------------------------

	const days = $derived(data?.daily ?? []);
	/** A clean top for the axis: 1, 2 or 5 times a power of ten. */
	const axisMax = $derived.by(() => {
		const max = Math.max(0, ...days.map((d) => d.amount));
		if (max <= 0) return 1;
		const step = 10 ** Math.floor(Math.log10(max));
		return [1, 2, 5, 10].map((m) => m * step).find((v) => v >= max) ?? max;
	});
	const ticks = $derived([axisMax, axisMax / 2, 0]);
	const compact = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 });

	let hovered = $state<number | null>(null);
	const hoverDay = $derived(hovered === null ? null : days[hovered]);

	// --- services --------------------------------------------------------------------------

	let listHeight = $state(0);
	const ROW = 24;
	/** Rows that fit under the header; the last one becomes "Other" when services are left over. */
	const fit = $derived(Math.max(1, Math.floor(listHeight / ROW) - 1));
	const services = $derived.by(() => {
		const all = data?.services ?? [];
		if (all.length <= fit) return all;
		const shown = all.slice(0, fit - 1);
		const rest = all.slice(fit - 1);
		return [
			...shown,
			{
				name: `Other ${rest.length} services`,
				thisMonth: rest.reduce((s, x) => s + x.thisMonth, 0),
				lastMonth: rest.reduce((s, x) => s + x.lastMonth, 0),
				other: true
			}
		];
	});
	const topShare = $derived(Math.max(0.01, ...(data?.services ?? []).map((s) => s.thisMonth)));
</script>

{#snippet tile(label: string, value: string, note: string | null, delta: number | null)}
	<div class="bg-card min-w-0 flex-1 rounded-lg border px-3 py-2">
		<div class="text-muted-foreground truncate text-xs">{label}</div>
		<div class="truncate text-xl font-semibold">{value}</div>
		<div class="text-muted-foreground flex min-w-0 items-center gap-1.5 truncate text-xs">
			{#if delta !== null}
				<!-- For spend, up is the bad direction. -->
				<span class={delta > 0 ? 'text-red-600 dark:text-red-400' : 'text-emerald-600 dark:text-emerald-400'}>
					{delta > 0 ? '▲' : '▼'} {signed(delta)}
				</span>
			{/if}
			{#if note}<span class="truncate">{note}</span>{/if}
		</div>
	</div>
{/snippet}

<div class="flex h-full min-h-0 flex-col gap-3 text-sm" data-cost-overview>
	{#if error}
		<div class="text-destructive rounded-lg border p-3 whitespace-pre-wrap">
			{error}
			<p class="text-muted-foreground mt-2">
				The profile needs <code>ce:GetCostAndUsage</code> and <code>ce:GetCostForecast</code>, and Cost Explorer
				must be enabled for the account (Billing console). Press <kbd>r</kbd> to try again.
			</p>
		</div>
	{:else if !data}
		<div class="flex gap-3">
			{#each { length: 3 } as _, i (i)}<Skeleton class="h-20 flex-1" />{/each}
		</div>
		<Skeleton class="h-40" />
		<Skeleton class="min-h-0 flex-1" />
	{:else}
		{@const d = data}
		{@const lastName = monthName(d.daily[0]?.date ?? d.monthStart)}
		<div class="flex shrink-0 gap-3">
			{@render tile(
				`${monthName(d.monthStart)} so far`,
				fmt(d.monthToDate),
				`vs ${fmt(d.lastMonthToDate)} same days of ${lastName}`,
				change(d.monthToDate, d.lastMonthToDate)
			)}
			{@render tile(
				'Forecast for month end',
				d.forecast === null ? '-' : fmt(d.forecast),
				d.forecast === null ? 'not enough history to forecast' : `vs ${fmt(d.lastMonth)} in ${lastName}`,
				d.forecast === null ? null : change(d.forecast, d.lastMonth)
			)}
			{@render tile(`${lastName} total`, fmt(d.lastMonth), 'unblended cost', null)}
		</div>

		<!-- Daily spend: one column per day, last month muted, this month in the accent. -->
		<section class="shrink-0" aria-label="Daily spend">
			<div class="mb-1 flex items-center gap-3 text-xs">
				<h2 class="text-muted-foreground font-medium tracking-wide uppercase">Daily spend</h2>
				<span class="text-muted-foreground inline-flex items-center gap-1.5">
					<span class="bg-muted-foreground/35 size-2 rounded-sm"></span>{lastName}
				</span>
				<span class="text-muted-foreground inline-flex items-center gap-1.5">
					<span class="size-2 rounded-sm bg-sky-500 dark:bg-sky-400"></span>{monthName(d.monthStart)}
				</span>
				<span class="text-muted-foreground ml-auto truncate">
					{#if hoverDay}
						<span class="text-foreground font-medium">{dayLabel(hoverDay.date)}</span>
						{fmt(hoverDay.amount)}{hoverDay.estimated ? ' (so far)' : ''}
					{:else}
						Updated {age(new Date(fetchedAt).toISOString(), now)} ago · <kbd>r</kbd> refresh (3 billed requests)
					{/if}
				</span>
			</div>
			<div class="flex h-36 gap-2">
				<div class="text-muted-foreground flex w-10 shrink-0 flex-col justify-between pb-4 text-right text-[10px] tabular-nums">
					{#each ticks as t (t)}<span class="-my-1.5">{t === 0 ? '0' : compact.format(t)}</span>{/each}
				</div>
				<div class="relative flex min-w-0 flex-1 flex-col">
					<div class="relative min-h-0 flex-1">
						{#each ticks as t (t)}
							<div class="border-border absolute inset-x-0 border-t" style="top: {(1 - t / axisMax) * 100}%"></div>
						{/each}
						<div class="absolute inset-0 flex items-end gap-0.5" role="list" onpointerleave={() => (hovered = null)}>
							{#each days as day, i (day.date)}
								{@const current = day.date >= d.monthStart}
								<div
									class="group flex h-full min-w-0 flex-1 cursor-default items-end justify-center"
									role="listitem"
									aria-label="{dayLabel(day.date)}: {fmt(day.amount)}"
									onpointerenter={() => (hovered = i)}
								>
									<div
										class="w-full max-w-6 rounded-t-[4px] transition-opacity {current
											? 'bg-sky-500 dark:bg-sky-400'
											: 'bg-muted-foreground/35'} {day.estimated ? 'opacity-60' : ''} {hovered === i
											? 'ring-foreground/60 ring-2'
											: ''}"
										style="height: {Math.max(0, day.amount / axisMax) * 100}%"
									></div>
								</div>
							{/each}
						</div>
					</div>
					<div class="text-muted-foreground relative h-4 text-[10px]">
						{#each days as day, i (day.date)}
							{#if day.date.endsWith('-01') || i === 0}
								<span class="absolute top-0.5" style="left: {(i / days.length) * 100}%">{dayLabel(day.date)}</span>
							{/if}
						{/each}
					</div>
				</div>
			</div>
		</section>

		<!-- Top services this month, with last month beside them. -->
		<section class="flex min-h-0 flex-1 flex-col" aria-label="Cost by service">
			<div class="min-h-0 flex-1 overflow-hidden" bind:clientHeight={listHeight}>
				<table class="w-full table-fixed text-xs">
					<thead class="text-muted-foreground">
						<tr class="h-6 text-left">
							<th class="font-medium tracking-wide uppercase">Service</th>
							<th class="hidden w-40 font-medium md:table-cell"></th>
							<th class="w-24 text-right font-medium">{monthName(d.monthStart)}</th>
							<th class="w-28 text-right font-medium">{lastName} (full)</th>
						</tr>
					</thead>
					<tbody>
						{#each services as s (s.name)}
							<tr class="h-6 border-t">
								<td class="truncate {'other' in s ? 'text-muted-foreground' : ''}" title={s.name}>{s.name}</td>
								<td class="hidden pr-3 md:table-cell">
									{#if !('other' in s)}
										<div class="bg-muted h-1 rounded-full">
											<div
												class="h-1 rounded-full bg-sky-500 dark:bg-sky-400"
												style="width: {Math.max(0, s.thisMonth / topShare) * 100}%"
											></div>
										</div>
									{/if}
								</td>
								<td class="text-right tabular-nums">{fmt(s.thisMonth)}</td>
								<td class="text-muted-foreground text-right tabular-nums">{fmt(s.lastMonth)}</td>
							</tr>
						{:else}
							<tr class="h-6 border-t"><td colspan="4" class="text-muted-foreground">No spend this month or last.</td></tr>
						{/each}
					</tbody>
				</table>
			</div>
		</section>
		{#if loading}<div class="text-muted-foreground absolute right-6 bottom-14 text-xs">Refreshing…</div>{/if}
	{/if}
</div>
