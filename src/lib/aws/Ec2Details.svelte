<script lang="ts">
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { age } from '#lib/k8s/format.js';
	import RefreshCw from '@lucide/svelte/icons/refresh-cw';
	import X from '@lucide/svelte/icons/x';
	import { getEc2Instance, type Ec2InstanceDetail } from './api.js';
	import { instanceTone, TONE_DOT } from './tone.js';

	// One EC2 instance, like the console's instance page, laid out to fit the area: nothing
	// scrolls; tags clip to the room left.

	let {
		profile,
		region,
		id,
		now,
		onClose
	}: {
		profile: string;
		region: string;
		id: string;
		/** Current time for ages, ticking in the parent. */
		now: number;
		onClose: () => void;
	} = $props();

	let detail = $state<Ec2InstanceDetail | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);

	let generation = 0;
	export async function load() {
		const mine = ++generation;
		loading = true;
		error = null;
		try {
			const found = await getEc2Instance(profile, region, id);
			if (mine === generation) detail = found;
		} catch (e) {
			if (mine === generation) error = String(e);
		} finally {
			if (mine === generation) loading = false;
		}
	}

	$effect(() => {
		profile;
		region;
		id;
		detail = null;
		load();
	});

	/** `r` refreshes, `Esc` goes back to the list. */
	export function handleKey(event: KeyboardEvent): boolean | void {
		if (event.metaKey || event.ctrlKey || event.altKey) return;
		if (event.key === 'r') {
			load();
			return true;
		}
		if (event.key === 'Escape') {
			onClose();
			return true;
		}
	}

	const yesNo = (v: boolean | null | undefined) => (v == null ? null : v ? 'yes' : 'no');
	const checkTone = (s: string | null) =>
		s === 'ok' ? 'ok' : s === 'impaired' ? 'bad' : s === null || s === 'not-applicable' ? 'off' : 'pending';
	const MAX_TAGS = 16;
</script>

{#snippet field(label: string, value: string | null | undefined)}
	<dt class="text-muted-foreground truncate">{label}</dt>
	<dd class="min-w-0 truncate select-text" title={value ?? ''}>{value || '-'}</dd>
{/snippet}

{#snippet heading(title: string, count?: number)}
	<h2 class="text-muted-foreground mb-1.5 text-[11px] font-medium tracking-wide uppercase">
		{title}{#if count !== undefined}<span class="ml-1 tabular-nums">{count}</span>{/if}
	</h2>
{/snippet}

{#snippet dot(tone: keyof typeof TONE_DOT, label: string)}
	<span class="inline-flex items-center gap-1.5">
		<span class="size-1.5 shrink-0 rounded-full {TONE_DOT[tone]}"></span>
		{label}
	</span>
{/snippet}

<div class="@container flex h-full min-h-0 flex-col text-xs" data-ec2-details>
	<header class="flex items-center gap-1.5 border-b px-3 py-1.5">
		<div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-0.5">
			<h1 class="truncate text-sm font-semibold">{detail?.name || id}</h1>
			{#if detail?.name}<span class="text-muted-foreground">{id}</span>{/if}
			{#if detail}
				{@render dot(instanceTone(detail.state), detail.state)}
				<Badge variant="secondary" class="h-4 px-1.5 text-[10px]">{detail.type}</Badge>
			{/if}
		</div>
		<Button variant="ghost" size="icon-xs" onclick={load} aria-label="Refresh instance" title="Refresh (r)">
			<RefreshCw class={loading ? 'animate-spin' : ''} />
		</Button>
		<Button variant="ghost" size="icon-xs" onclick={onClose} aria-label="Close details" title="Back to instances (Esc)">
			<X />
		</Button>
	</header>

	{#if error}
		<p class="text-destructive p-3 whitespace-pre-wrap">{error}</p>
	{:else if !detail}
		<div class="grid gap-3 p-3">
			<Skeleton class="h-10" />
			<Skeleton class="h-32" />
		</div>
	{:else}
		{@const d = detail}
		<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-3 @xl:grid @xl:grid-cols-2">
			<!-- Left: what it is -->
			<div class="flex min-h-0 flex-col gap-3 overflow-hidden">
				<section class="shrink-0">
					{@render heading('Status')}
					<div class="flex flex-wrap gap-x-4 gap-y-1">
						<span>System check {@render dot(checkTone(d.systemStatus), d.systemStatus ?? '-')}</span>
						<span>Instance check {@render dot(checkTone(d.instanceStatus), d.instanceStatus ?? '-')}</span>
						<span>Launched {age(d.launched, now)} ago</span>
					</div>
					{#if d.stateReason}<p class="text-muted-foreground mt-1 truncate" title={d.stateReason}>{d.stateReason}</p>{/if}
				</section>

				<section class="shrink-0">
					{@render heading('Instance')}
					<dl class="grid grid-cols-[max-content_minmax(0,1fr)_max-content_minmax(0,1fr)] gap-x-3 gap-y-0.5">
						{@render field('ID', d.id)}
						{@render field('Type', d.type)}
						{@render field('AMI', d.ami)}
						{@render field('AMI name', d.amiName)}
						{@render field('Platform', d.platform)}
						{@render field('Arch', d.arch)}
						{@render field('vCPU', d.cpuCores != null ? `${d.cpuCores} × ${d.threadsPerCore ?? 1} threads` : null)}
						{@render field('Virtualization', d.virtualization)}
						{@render field('Key pair', d.keyName)}
						{@render field('IAM profile', d.iamProfile)}
						{@render field('Zone', d.zone)}
						{@render field('Tenancy', d.tenancy)}
						{@render field('Launched', d.launched?.replace('T', ' ').replace('Z', ' UTC'))}
						{@render field('Monitoring', d.monitoring === 'enabled' ? 'detailed' : d.monitoring ? 'basic' : null)}
						{@render field('IMDS', d.imdsTokens === 'required' ? 'v2 only' : d.imdsTokens ? 'v1 and v2' : null)}
						{@render field('Termination protection', yesNo(d.terminationProtection))}
						{@render field('EBS optimized', yesNo(d.ebsOptimized))}
					</dl>
				</section>

				<section class="min-h-0 flex-1 overflow-hidden">
					{@render heading('Tags', d.tags.length)}
					<div class="flex flex-wrap gap-1">
						{#each d.tags.slice(0, MAX_TAGS) as [key, value] (key)}
							<Badge variant="secondary" class="h-4 max-w-full truncate px-1.5 text-[10px] font-normal" title="{key}={value}">
								{key}{value ? `=${value}` : ''}
							</Badge>
						{:else}
							<p class="text-muted-foreground">None</p>
						{/each}
						{#if d.tags.length > MAX_TAGS}
							<Badge
								variant="outline"
								class="h-4 px-1.5 text-[10px] font-normal"
								title={d.tags.slice(MAX_TAGS).map(([k, v]) => `${k}=${v}`).join('\n')}
							>
								+{d.tags.length - MAX_TAGS} more
							</Badge>
						{/if}
					</div>
				</section>
			</div>

			<!-- Right: how to reach it, and what it stores -->
			<div class="flex min-h-0 flex-col gap-3 overflow-hidden">
				<section class="shrink-0">
					{@render heading('Network')}
					<dl class="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-3 gap-y-0.5">
						{@render field('Public IP', d.publicIp)}
						{@render field('Public DNS', d.publicDns)}
						{@render field('Private IP', d.privateIp)}
						{@render field('Private DNS', d.privateDns)}
						{@render field('VPC', d.vpc)}
						{@render field('Subnet', d.subnet)}
					</dl>
				</section>

				<section class="shrink-0">
					{@render heading('Security groups', d.securityGroups.length)}
					<div class="flex flex-wrap gap-1">
						{#each d.securityGroups as g (g.id)}
							<Badge variant="outline" class="h-4 max-w-full truncate px-1.5 text-[10px] font-normal" title={g.id}>
								{g.name} <span class="text-muted-foreground ml-1">{g.id}</span>
							</Badge>
						{:else}
							<p class="text-muted-foreground">None</p>
						{/each}
					</div>
				</section>

				<section class="shrink-0">
					{@render heading('Storage', d.volumes.length)}
					<ul class="grid gap-0.5">
						{#each d.volumes as v (v.id)}
							<li class="flex min-w-0 items-center gap-2" title={v.id}>
								<span class="w-20 shrink-0 truncate font-medium">{v.device}</span>
								<span class="w-14 shrink-0 tabular-nums">{v.sizeGib != null ? `${v.sizeGib} GiB` : '-'}</span>
								<span class="w-10 shrink-0">{v.type ?? '-'}</span>
								<span class="text-muted-foreground w-20 shrink-0 tabular-nums">{v.iops != null ? `${v.iops} IOPS` : ''}</span>
								<span class="text-muted-foreground min-w-0 flex-1 truncate">
									{[v.encrypted ? 'encrypted' : v.encrypted === false ? 'not encrypted' : null, v.id === d.volumes[0]?.id && d.rootDevice === v.device ? 'root' : null, v.deleteOnTermination === false ? 'kept on terminate' : null].filter(Boolean).join(' · ')}
								</span>
							</li>
						{:else}
							<li class="text-muted-foreground">{d.rootDeviceType === 'instance-store' ? 'Instance store only' : 'None'}</li>
						{/each}
					</ul>
				</section>

				<section class="min-h-0 shrink overflow-hidden">
					{@render heading('Network interfaces', d.interfaces.length)}
					<ul class="grid gap-0.5">
						{#each d.interfaces as n (n.id)}
							<li class="flex min-w-0 items-center gap-2">
								<span class="w-40 shrink-0 truncate font-medium">{n.id}</span>
								<span class="w-24 shrink-0 tabular-nums">{n.privateIp ?? '-'}</span>
								<span class="w-24 shrink-0 tabular-nums">{n.publicIp ?? ''}</span>
								<span class="text-muted-foreground min-w-0 flex-1 truncate">{n.mac ?? ''}</span>
							</li>
						{/each}
					</ul>
				</section>
			</div>
		</div>
	{/if}
</div>
