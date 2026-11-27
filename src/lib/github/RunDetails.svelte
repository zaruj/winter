<script lang="ts">
	import Kbd from '#lib/components/Kbd.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { age } from '#lib/k8s/format.js';
	import { clamp, moveCursor, plain, scrollToSelected } from '#lib/keys.svelte.js';
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import X from '@lucide/svelte/icons/x';
	import { tick, untrack } from 'svelte';
	import { getJobLog, getRun, isActive, listJobs, openUrl, type Job, type Run } from './api.js';
	import { duration, statusLabel, statusTone, TONE_DOT, type Tone } from './status.js';

	// One workflow run: its jobs on the left, the selected job's steps (or its log) on the right.
	// While the run is going it is fetched again every few seconds; unchanged answers are free.

	let {
		repo,
		run: initial,
		now,
		onClose
	}: {
		repo: string;
		/** The run as listed; refreshed here. */
		run: Run;
		now: number;
		onClose: () => void;
	} = $props();

	const POLL_MS = 5_000;

	// The parent re-creates this component for each run, so the first value is the one to keep.
	let run = $state<Run>(untrack(() => initial));
	let jobs = $state<Job[] | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);
	let cursor = $state(0);
	/** The job whose log is shown, instead of its steps. */
	let logJob = $state<number | null>(null);
	let log = $state<string | null>(null);
	let logError = $state<string | null>(null);
	let logBox: HTMLElement | undefined = $state();
	let jobList: HTMLElement | undefined = $state();

	const job = $derived(jobs?.[clamp(cursor, jobs.length)]);

	let generation = 0;
	export async function load() {
		const mine = ++generation;
		loading = true;
		try {
			const [r, j] = await Promise.all([getRun(repo, initial.id), listJobs(repo, initial.id)]);
			if (mine !== generation) return;
			run = r;
			jobs = j;
			error = null;
		} catch (e) {
			if (mine === generation) error = String(e);
		} finally {
			if (mine === generation) loading = false;
		}
	}

	$effect(() => {
		load();
		const timer = setInterval(() => {
			if (isActive(run.status) && !document.hidden) load();
		}, POLL_MS);
		return () => clearInterval(timer);
	});

	async function openLog(j: Job) {
		logJob = j.id;
		log = null;
		logError = null;
		try {
			const text = await getJobLog(repo, j.id);
			if (logJob !== j.id) return;
			log = text;
			await tick();
			logBox?.scrollTo({ top: logBox.scrollHeight });
		} catch (e) {
			if (logJob === j.id) logError = String(e);
		}
	}

	async function moveTo(index: number) {
		cursor = index;
		await tick();
		scrollToSelected(jobList);
	}

	/** j/k jobs, Enter or `l` log, `o` open on GitHub, `r` refresh, Esc back. */
	export function handleKey(event: KeyboardEvent): boolean | void {
		if (!plain(event)) return;
		if (event.key === 'Escape') {
			if (logJob !== null) logJob = null;
			else onClose();
			return true;
		}
		if (event.key === 'r') {
			load();
			if (logJob !== null && job) openLog(job);
			return true;
		}
		if (event.key === 'o') {
			openUrl(logJob !== null && job?.htmlUrl ? job.htmlUrl : run.htmlUrl);
			return true;
		}
		if (!jobs?.length) return;
		const next = moveCursor(event, clamp(cursor, jobs.length), jobs.length, true);
		if (next !== null) {
			moveTo(next);
			if (logJob !== null) openLog(jobs[next]);
			return true;
		}
		if ((event.key === 'Enter' || event.key === 'l') && job) {
			openLog(job);
			return true;
		}
	}

	/** GitHub log lines start with an ISO timestamp; shown dimmed and short. */
	function splitTime(line: string): [string, string] {
		const m = /^(\d{4}-\d\d-\d\dT(\d\d:\d\d:\d\d))\.\d+Z (.*)$/.exec(line);
		return m ? [m[2], m[3]] : ['', line];
	}
	const logLines = $derived(log === null ? [] : log.replace(/\r/g, '').split('\n'));
</script>

{#snippet dot(tone: Tone, label: string)}
	<span class="inline-flex min-w-0 items-center gap-1.5">
		<span class="size-2 shrink-0 rounded-full {TONE_DOT[tone]} {tone === 'pending' ? 'animate-pulse' : ''}"></span>
		<span class="truncate">{label}</span>
	</span>
{/snippet}

<div class="flex h-full min-h-0 flex-col text-xs" data-run-details>
	<header class="flex items-center gap-2 border-b px-3 py-1.5">
		<div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-0.5">
			<h1 class="truncate text-sm font-semibold" title={run.title}>{run.title || run.name}</h1>
			<span class="text-muted-foreground">{run.name} #{run.number}{run.attempt > 1 ? ` (attempt ${run.attempt})` : ''}</span>
			{@render dot(statusTone(run.status, run.conclusion), statusLabel(run.status, run.conclusion))}
			{#if run.branch}<Badge variant="secondary" class="h-4 px-1.5 text-[10px]">{run.branch}</Badge>{/if}
			<span class="text-muted-foreground font-mono">{run.sha}</span>
			<span class="text-muted-foreground">
				{run.event}{run.actor ? ` by ${run.actor}` : ''} · {age(run.createdAt, now)} ago ·
				{duration(run.startedAt, isActive(run.status) ? null : run.updatedAt, now)}
			</span>
		</div>
		<Button variant="ghost" size="icon-xs" onclick={() => openUrl(run.htmlUrl)} aria-label="Open on GitHub" title="Open on GitHub (o)">
			<ExternalLink />
		</Button>
		<Button variant="ghost" size="icon-xs" onclick={load} aria-label="Refresh run" title="Refresh (r)">
			<RefreshCw class={loading ? 'animate-spin' : ''} />
		</Button>
		<Button variant="ghost" size="icon-xs" onclick={onClose} aria-label="Close run" title="Back to runs (Esc)">
			<X />
		</Button>
	</header>

	{#if error && !jobs}
		<p class="text-destructive p-3 whitespace-pre-wrap">{error}</p>
	{:else if !jobs}
		<div class="grid gap-2 p-3"><Skeleton class="h-8" /><Skeleton class="h-24" /></div>
	{:else}
		<div class="flex min-h-0 flex-1">
			<!-- Jobs -->
			<ul class="w-56 shrink-0 overflow-y-auto border-r p-1 md:w-64" bind:this={jobList} aria-label="Jobs">
				{#each jobs as j, i (j.id)}
					<li>
						<button
							class="data-selected:bg-muted flex w-full items-center gap-2 rounded-md px-2 py-1 text-left"
							data-selected={i === clamp(cursor, jobs.length) || undefined}
							onclick={() => ((cursor = i), logJob !== null && openLog(j))}
							ondblclick={() => openLog(j)}
						>
							<span class="min-w-0 flex-1">{@render dot(statusTone(j.status, j.conclusion), j.name)}</span>
							<span class="text-muted-foreground shrink-0 tabular-nums">
								{duration(j.startedAt, j.completedAt, now)}
							</span>
						</button>
					</li>
				{:else}
					<li class="text-muted-foreground p-2">No jobs yet.</li>
				{/each}
			</ul>

			<!-- Steps or log of the selected job -->
			<div class="flex min-h-0 min-w-0 flex-1 flex-col">
				{#if job}
					<div class="flex items-center gap-2 border-b px-3 py-1">
						<span class="truncate font-medium">{job.name}</span>
						{#if job.runner}<span class="text-muted-foreground truncate">on {job.runner}</span>{/if}
						{#if job.labels.length}<span class="text-muted-foreground hidden truncate md:inline">{job.labels.join(', ')}</span>{/if}
						<span class="text-muted-foreground ml-auto shrink-0">
							{#if logJob !== null}<Kbd>Esc</Kbd> steps{:else}<Kbd>Enter</Kbd> log{/if}
						</span>
					</div>
					{#if logJob !== null}
						<div class="min-h-0 flex-1 overflow-auto bg-black/[0.03] p-2 font-mono text-[11px] leading-4 dark:bg-white/[0.03]" bind:this={logBox}>
							{#if logError}
								<p class="text-muted-foreground font-sans whitespace-pre-wrap">{logError}</p>
							{:else if log === null}
								<p class="text-muted-foreground font-sans">Loading log…</p>
							{:else}
								{#each logLines as line, i (i)}
									{@const [time, text] = splitTime(line)}
									<div class="flex gap-2 whitespace-pre-wrap select-text {text.startsWith('##[error]') ? 'text-red-600 dark:text-red-400' : text.startsWith('##[group]') ? 'font-semibold' : ''}">
										<span class="text-muted-foreground shrink-0 select-none">{time}</span>
										<span class="min-w-0 break-all">{text.replace(/^##\[(group|endgroup|error|warning|notice)\]/, '')}</span>
									</div>
								{/each}
							{/if}
						</div>
					{:else}
						<ol class="min-h-0 flex-1 overflow-y-auto p-1" aria-label="Steps">
							{#each job.steps as s (s.number)}
								<li class="flex items-center gap-2 px-2 py-0.5">
									<span class="text-muted-foreground w-5 shrink-0 text-right tabular-nums">{s.number}</span>
									<span class="min-w-0 flex-1">{@render dot(statusTone(s.status, s.conclusion), s.name)}</span>
									<span class="text-muted-foreground shrink-0">{statusLabel(s.status, s.conclusion)}</span>
									<span class="text-muted-foreground w-14 shrink-0 text-right tabular-nums">
										{s.status === 'queued' ? '' : duration(s.startedAt, s.completedAt, now)}
									</span>
								</li>
							{:else}
								<li class="text-muted-foreground p-2">No steps yet.</li>
							{/each}
						</ol>
					{/if}
				{/if}
			</div>
		</div>
		{#if error}<p class="text-destructive border-t px-3 py-1 truncate" title={error}>{error}</p>{/if}
	{/if}
</div>
