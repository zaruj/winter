<script lang="ts">
	import { goto } from '$app/navigation';
	import Kbd from '#lib/components/Kbd.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import {
		connect,
		openUrl,
		signIn,
		signInWithGit,
		signOut,
		streamRepos,
		type Account,
		type Repo,
		type TokenSource
	} from '#lib/github/api.js';
	import { age, matches } from '#lib/k8s/format.js';
	import { clamp, isTyping, moveCursor, scrollToSelected, useKeys } from '#lib/keys.svelte.js';
	import ListPage from '#lib/components/ListPage.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import ExternalLink from '@lucide/svelte/icons/external-link';
	import GitBranch from '@lucide/svelte/icons/git-branch';
	import Lock from '@lucide/svelte/icons/lock';
	import LogOut from '@lucide/svelte/icons/log-out';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import { onMount, tick } from 'svelte';

	// GitHub: sign in, then pick a repository to see its Actions pipelines. Repositories opened
	// recently come first (remembered in ~/.config/winter/state.json), then by last push.

	/** undefined while connecting; null when no token was found. */
	let account = $state<Account | null | undefined>(undefined);
	let connectError = $state<string | null>(null);
	let repos = $state.raw<Repo[]>([]);
	let reposLoading = $state(false);
	let reposError = $state<string | null>(null);
	let query = $state('');
	let cursor = $state(0);
	let now = $state(Date.now());
	let search: HTMLInputElement | undefined = $state();
	let list: HTMLElement | undefined = $state();

	let tokenInput = $state('');
	let signingIn = $state(false);
	let signInError = $state<string | null>(null);
	let tokenField: HTMLInputElement | undefined = $state();

	const SOURCE: Record<TokenSource, string> = {
		keychain: 'token saved in winter',
		environment: 'GH_TOKEN / GITHUB_TOKEN',
		ghCli: 'GitHub CLI'
	};

	/** Opened in winter (newest first), then the rest in GitHub's order (last push first). */
	const sorted = $derived(
		[...repos].sort((a, b) => (b.lastUsed ?? 0) - (a.lastUsed ?? 0))
	);

	/** `owner/name` typed in full: offer to open it even if it is not in the list (any public repo). */
	const typed = $derived(/^[\w.-]+\/[\w.-]+$/.test(query.trim()) ? query.trim() : null);

	type Entry = { type: 'repo'; repo: Repo } | { type: 'open'; name: string };
	const shown = $derived.by((): Entry[] => {
		const found = sorted
			.filter((r) => matches(query, r.name, r.description, r.language))
			.map((repo): Entry => ({ type: 'repo', repo }));
		if (typed && !repos.some((r) => r.name.toLowerCase() === typed.toLowerCase())) {
			found.push({ type: 'open', name: typed });
		}
		return found;
	});

	const nameOf = (e: Entry) => (e.type === 'repo' ? e.repo.name : e.name);
	const href = (e: Entry) => `/github/repo?${new URLSearchParams({ name: nameOf(e) })}`;

	/** Connecting has taken a while: most likely macOS is asking about the Keychain. */
	let slow = $state(false);

	async function start(fresh = false) {
		account = undefined;
		connectError = null;
		slow = false;
		const timer = setTimeout(() => (slow = account === undefined), 2000);
		try {
			account = await connect(fresh);
		} catch (e) {
			connectError = String(e);
			account = null;
		} finally {
			clearTimeout(timer);
			slow = false;
		}
		if (account) loadRepos();
		else {
			await tick();
			tokenField?.focus();
		}
	}

	let generation = 0;
	async function loadRepos() {
		const mine = ++generation;
		reposLoading = true;
		reposError = null;
		let fresh: Repo[] = [];
		const streaming = repos.length === 0;
		try {
			await streamRepos((page) => {
				if (mine !== generation) return;
				fresh = [...fresh, ...page];
				if (streaming) repos = fresh;
			});
			if (mine === generation) repos = fresh;
		} catch (e) {
			if (mine === generation) reposError = String(e);
		} finally {
			if (mine === generation) reposLoading = false;
		}
		await tick();
		search?.focus();
	}

	async function submitToken(event?: Event) {
		event?.preventDefault();
		await finishSignIn(() => signIn(tokenInput));
	}

	async function finishSignIn(how: () => Promise<Account>) {
		signingIn = true;
		signInError = null;
		try {
			account = await how();
			tokenInput = '';
			connectError = null;
			loadRepos();
		} catch (e) {
			signInError = String(e);
		} finally {
			signingIn = false;
		}
	}

	async function leave() {
		try {
			await signOut();
		} catch (e) {
			connectError = String(e);
			return;
		}
		repos = [];
		generation++;
		start();
	}

	function setQuery(value: string) {
		query = value;
		cursor = 0;
	}

	async function moveTo(index: number) {
		cursor = index;
		await tick();
		scrollToSelected(list);
	}

	onMount(() => {
		start();
		const timer = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(timer);
	});

	useKeys((event) => {
		if (!account) return;
		const next = moveCursor(event, clamp(cursor, shown.length), shown.length, false);
		if (next !== null) {
			moveTo(next);
			return true;
		}
		if (event.key === 'Enter' && shown.length) {
			goto(href(shown[clamp(cursor, shown.length)]));
			return true;
		}
		if (event.key === 'Escape' && query) {
			setQuery('');
			return true;
		}
		if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !isTyping(event)) {
			search?.focus();
		}
	});

	const TOKEN_URL =
		'https://github.com/settings/personal-access-tokens/new?description=winter&actions=read&metadata=read';
</script>

<ListPage
	title="GitHub Actions"
	bind:query
	bind:search
	onQuery={() => (cursor = 0)}
	placeholder="Search repositories, or type owner/name, then Enter"
	showSearch={!!account}
>
	{#snippet actions()}
		{#if account}
			<div class="flex min-w-0 items-center gap-2 text-sm">
				{#if account.avatarUrl}<img src={account.avatarUrl} alt="" class="size-6 shrink-0 rounded-full" />{/if}
				<span class="hidden truncate font-medium sm:inline">{account.login}</span>
				<Badge variant="outline" class="hidden md:inline-flex">{SOURCE[account.source]}</Badge>
			</div>
			<Button variant="outline" size="icon" onclick={loadRepos} aria-label="Reload repositories" title="Reload repositories">
				<RefreshCw class={reposLoading ? 'animate-spin' : ''} />
			</Button>
			{#if account.source === 'keychain'}
				<Button variant="outline" size="icon" onclick={leave} aria-label="Sign out" title="Sign out (removes the token from the Keychain)">
					<LogOut />
				</Button>
			{/if}
		{/if}
	{/snippet}
	{#snippet notice()}
		{#if connectError}
			<Alert.Root variant="destructive">
				<Alert.Title>Could not sign in to GitHub</Alert.Title>
				<Alert.Description class="whitespace-pre-wrap">{connectError}</Alert.Description>
			</Alert.Root>
		{/if}
		{#if reposError}
			<Alert.Root variant="destructive">
				<Alert.Title>Could not list repositories</Alert.Title>
				<Alert.Description class="whitespace-pre-wrap">{reposError}</Alert.Description>
			</Alert.Root>
		{/if}
	{/snippet}

	{#if account === undefined}
		{#if slow}
			<p class="bg-muted/50 mb-2 rounded-lg border px-3 py-2 text-sm">
				Waiting for the macOS Keychain. If a dialog asks whether winter may use “dev.winter.github”, choose
				<strong>Always Allow</strong> (a new build of winter asks once).
			</p>
		{/if}
		<div class="grid gap-1">{#each { length: 4 } as _, i (i)}<Skeleton class="h-10 w-full rounded-lg" />{/each}</div>
	{:else if account === null}
		<section class="bg-card mx-auto max-w-2xl rounded-xl border p-5">
			<h2 class="font-medium">Connect to GitHub</h2>
			<p class="text-muted-foreground mt-1 text-sm">
				winter reads workflows, runs, jobs and logs (read-only). The token is kept in the macOS Keychain, never in a file.
			</p>
			<form class="mt-4 flex gap-2" onsubmit={submitToken}>
				<input
					bind:this={tokenField}
					bind:value={tokenInput}
					type="password"
					placeholder="Personal access token (github_pat_… or ghp_…)"
					aria-label="Personal access token"
					autocomplete="off"
					spellcheck="false"
					class="border-input focus-visible:border-ring focus-visible:ring-ring/50 h-9 min-w-0 flex-1 rounded-md border bg-transparent px-3 font-mono text-sm outline-none focus-visible:ring-3"
				/>
				<Button type="submit" disabled={signingIn || !tokenInput.trim()}>Connect</Button>
			</form>
			{#if signInError}<p class="text-destructive mt-2 text-sm whitespace-pre-wrap">{signInError}</p>{/if}
			<div class="mt-4 grid gap-2 text-sm sm:grid-cols-3">
				<button
					class="hover:bg-muted/60 rounded-lg border p-3 text-left"
					onclick={() => openUrl(TOKEN_URL)}
				>
					<div class="flex items-center gap-1.5 font-medium">Create a token <ExternalLink class="size-3" /></div>
					<div class="text-muted-foreground mt-0.5 text-xs">Fine-grained: Actions and Metadata, read-only</div>
				</button>
				<button
					class="hover:bg-muted/60 rounded-lg border p-3 text-left disabled:opacity-50"
					disabled={signingIn}
					onclick={() => finishSignIn(signInWithGit)}
				>
					<div class="font-medium">Use git's credential</div>
					<div class="text-muted-foreground mt-0.5 text-xs">The one git has for github.com, copied to the Keychain</div>
				</button>
				<button class="hover:bg-muted/60 rounded-lg border p-3 text-left" onclick={() => start(true)}>
					<div class="font-medium">Check again</div>
					<div class="text-muted-foreground mt-0.5 text-xs">After <code>gh auth login</code> or setting <code>GH_TOKEN</code></div>
				</button>
			</div>
		</section>
	{:else}
		<ul class="grid gap-0.5" bind:this={list}>
			{#if reposLoading && repos.length === 0}
				{#each { length: 4 } as _, i (i)}<li><Skeleton class="h-10 w-full rounded-lg" /></li>{/each}
			{:else if shown.length === 0}
				<li class="text-muted-foreground py-12 text-center text-sm">
					{query ? `No repository matches “${query}”.` : 'The token sees no repositories.'}
				</li>
			{/if}
			{#each shown as entry, i (nameOf(entry))}
				{@const selected = i === clamp(cursor, shown.length)}
				<li>
					{#if entry.type === 'repo'}
						{@const r = entry.repo}
						<ListRow
							href={href(entry)}
							{selected}
							onhover={() => (cursor = i)}
							icon={GitBranch}
							name={r.name}
							details={[r.language, r.pushedAt && `pushed ${age(r.pushedAt, now)} ago`, r.description].filter(Boolean).join(' · ')}
						>
							{#snippet badges()}
								{#if r.private}<Lock class="text-muted-foreground size-3 shrink-0" aria-label="private" />{/if}
								{#if r.archived}<Badge variant="secondary">archived</Badge>{/if}
								{#if r.lastUsed}
									<Badge variant="outline" class="text-muted-foreground">opened {age(new Date(r.lastUsed).toISOString(), now)} ago</Badge>
								{/if}
							{/snippet}
						</ListRow>
					{:else}
						<ListRow
							href={href(entry)}
							{selected}
							onhover={() => (cursor = i)}
							icon={GitBranch}
							name="Open {entry.name}"
							details="Any repository the token can see, public ones included"
						/>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	{#snippet hints()}
		{#if account}
			<span>type to search, or <span class="font-mono">owner/name</span></span>
			<span><Kbd>↑</Kbd> <Kbd>↓</Kbd> move</span>
			<span><Kbd>Enter</Kbd> open</span>
			<span class="hidden sm:inline">recently opened first</span>
			<span class="ml-auto">{repos.length} repositories · <Kbd>?</Kbd> help</span>
		{:else}
			<span><Kbd>Esc</Kbd> back</span>
			<span class="ml-auto"><Kbd>?</Kbd> help</span>
		{/if}
	{/snippet}
</ListPage>
