<script lang="ts" module>
	import type { Column } from '#lib/k8s/ResourceTable.svelte';
	import { age } from '#lib/k8s/format.js';
	import type {
		AwsKind,
		AwsKinds,
		Bucket,
		DbInstance,
		EbsVolume,
		Ec2Instance,
		EksCluster,
		HostedZone,
		LoadBalancer,
		MskCluster,
		SecurityGroup,
		Vpc
	} from './api.js';
	import { balancerTone, clusterTone, dbTone, instanceTone, TONE_DOT, volumeTone, vpcTone, type Tone } from './tone.js';

	// How each AWS list is shown: its columns, row key and the fields the filter searches.

	export interface ListSpec<T> {
		/** `now` is the current time, for ages. */
		columns: (now: number) => Column<T>[];
		key: (row: T) => string;
		search: (row: T) => (string | null | undefined)[];
		/** What `aws_detail` needs to find the row's resource. */
		detailId: (row: T) => string;
		/** Region to ask in, when not the page's (S3 buckets). */
		detailRegion?: (row: T) => string | null;
		/** Name shown on the details panel. */
		title: (row: T) => string;
	}

	const dash = (v: string | number | null | undefined) => (v === null || v === undefined || v === '' ? '-' : String(v));
	const yes = (v: boolean) => (v ? 'yes' : '-');
	const muted = 'text-muted-foreground';

	function ageColumn<T>(at: (r: T) => string | null, now: number): Column<T> {
		return { header: 'Age', class: `${muted} text-right`, hide: 'sm', text: (r) => age(at(r), now) };
	}

	const specs: { [K in AwsKind]: ListSpec<AwsKinds[K]> } = {
		ec2: {
			columns: (now) => [
				{ header: 'Name', cell: named, max: 'max-w-72 xl:max-w-[28rem]' },
				{ header: 'State', cell: instanceState },
				{ header: 'Type', text: (i) => i.type },
				{ header: 'Private IP', class: 'tabular-nums', hide: 'sm', text: (i) => dash(i.privateIp) },
				{ header: 'Public IP', class: 'tabular-nums', hide: 'md', text: (i) => dash(i.publicIp) },
				{ header: 'Zone', class: muted, hide: 'lg', text: (i) => dash(i.zone) },
				{ header: 'Platform', class: muted, hide: 'xl', text: (i) => dash(i.platform) },
				ageColumn<Ec2Instance>((i) => i.launched, now)
			],
			key: (i) => i.id,
			detailId: (i) => i.id,
			title: (i) => i.name || i.id,
			search: (i) => [i.id, i.name, i.state, i.type, i.privateIp, i.publicIp, i.zone, i.vpc]
		},
		eks: {
			columns: (now) => [
				{ header: 'Name', class: 'font-medium', text: (c) => c.name },
				{ header: 'Status', cell: clusterStatus },
				{ header: 'Version', text: (c) => dash(c.version) },
				{ header: 'Platform', class: muted, hide: 'lg', text: (c) => dash(c.platformVersion) },
				{ header: 'Endpoint', class: muted, max: 'max-w-56 xl:max-w-80', hide: 'lg', text: (c) => dash(c.endpoint) },
				{ header: 'VPC', class: muted, hide: 'xl', text: (c) => dash(c.vpc) },
				ageColumn<EksCluster>((c) => c.created, now)
			],
			key: (c) => c.name,
			detailId: (c) => c.name,
			title: (c) => c.name,
			search: (c) => [c.name, c.status, c.version, c.vpc]
		},
		s3: {
			columns: (now) => [
				{ header: 'Name', class: 'font-medium', max: 'max-w-md xl:max-w-xl', text: (b) => b.name },
				{ header: 'Region', class: muted, text: (b) => dash(b.region) },
				ageColumn<Bucket>((b) => b.created, now)
			],
			key: (b) => b.name,
			detailId: (b) => b.name,
			detailRegion: (b) => b.region,
			title: (b) => b.name,
			search: (b) => [b.name, b.region]
		},
		ebs: {
			columns: (now) => [
				{ header: 'Name', cell: named, max: 'max-w-72 xl:max-w-[28rem]' },
				{ header: 'State', cell: volumeState },
				{ header: 'Size', class: 'text-right tabular-nums', text: (v) => (v.sizeGib === null ? '-' : `${v.sizeGib} GiB`) },
				{ header: 'Type', text: (v) => dash(v.type) },
				{ header: 'IOPS', class: 'text-right tabular-nums', hide: 'sm', text: (v) => dash(v.iops?.toLocaleString()) },
				{ header: 'Attached to', class: muted, hide: 'md', text: (v) => (v.instance ? `${v.instance} ${v.device ?? ''}` : '-') },
				{ header: 'Zone', class: muted, hide: 'lg', text: (v) => dash(v.zone) },
				{ header: 'Encrypted', class: muted, hide: 'xl', text: (v) => yes(v.encrypted) },
				ageColumn<EbsVolume>((v) => v.created, now)
			],
			key: (v) => v.id,
			detailId: (v) => v.id,
			title: (v) => v.name || v.id,
			search: (v) => [v.id, v.name, v.state, v.type, v.instance, v.zone]
		},
		rds: {
			columns: (now) => [
				{ header: 'Name', class: 'font-medium', max: 'max-w-56 xl:max-w-80', text: (d) => d.name },
				{ header: 'Status', cell: dbStatus },
				{ header: 'Engine', max: 'max-w-44', text: (d) => [d.engine, d.version].filter(Boolean).join(' ') || '-' },
				{ header: 'Class', text: (d) => dash(d.class) },
				{ header: 'Endpoint', class: muted, max: 'max-w-56 xl:max-w-96', hide: 'lg', text: (d) => dash(d.endpoint) },
				{ header: 'Multi-AZ', class: muted, hide: 'lg', text: (d) => yes(d.multiAz) },
				{ header: 'Storage', class: `${muted} tabular-nums`, hide: 'xl', text: (d) => (d.storageGib === null ? '-' : `${d.storageGib} GiB ${d.storageType ?? ''}`) },
				ageColumn<DbInstance>((d) => d.created, now)
			],
			key: (d) => d.name,
			detailId: (d) => d.name,
			title: (d) => d.name,
			search: (d) => [d.name, d.engine, d.version, d.class, d.status, d.endpoint, d.cluster, d.zone]
		},
		msk: {
			columns: (now) => [
				{ header: 'Name', class: 'font-medium', text: (c) => c.name },
				{ header: 'State', cell: mskState },
				{ header: 'Type', text: (c) => c.type.toLowerCase() },
				{ header: 'Kafka', text: (c) => dash(c.kafkaVersion) },
				{ header: 'Brokers', class: 'text-right tabular-nums', hide: 'sm', text: (c) => dash(c.brokers) },
				{ header: 'Broker type', class: muted, hide: 'md', text: (c) => dash(c.instanceType) },
				ageColumn<MskCluster>((c) => c.created, now)
			],
			key: (c) => c.arn ?? c.name,
			detailId: (c) => c.arn ?? c.name,
			title: (c) => c.name,
			search: (c) => [c.name, c.state, c.type, c.kafkaVersion, c.instanceType]
		},
		vpc: {
			columns: () => [
				{ header: 'Name', cell: named, max: 'max-w-72 xl:max-w-[28rem]' },
				{ header: 'State', cell: vpcState },
				{ header: 'IPv4 CIDR', class: 'tabular-nums', text: (v) => v.cidrs.join(', ') || '-' },
				{ header: 'Default', class: muted, text: (v) => yes(v.isDefault) },
				{ header: 'Tenancy', class: muted, hide: 'lg', text: (v) => dash(v.tenancy) },
				{ header: 'IPv6 CIDR', class: muted, hide: 'xl', text: (v) => v.ipv6Cidrs.join(', ') || '-' }
			],
			key: (v) => v.id,
			detailId: (v) => v.id,
			title: (v) => v.name || v.id,
			search: (v) => [v.id, v.name, v.state, ...v.cidrs, ...v.ipv6Cidrs]
		},
		sg: {
			columns: () => [
				{ header: 'Name', cell: groupName, max: 'max-w-72 xl:max-w-[28rem]' },
				{ header: 'VPC', class: muted, hide: 'sm', text: (g) => dash(g.vpc) },
				{ header: 'Inbound', class: 'text-right tabular-nums', text: (g) => String(g.inbound) },
				{ header: 'Outbound', class: 'text-right tabular-nums', text: (g) => String(g.outbound) },
				{ header: 'Description', class: muted, max: 'max-w-56 xl:max-w-96', hide: 'lg', text: (g) => dash(g.description) }
			],
			key: (g) => g.id,
			detailId: (g) => g.id,
			title: (g) => g.name,
			search: (g) => [g.id, g.name, g.vpc, g.description]
		},
		elb: {
			columns: (now) => [
				{ header: 'Name', class: 'font-medium', text: (l) => l.name },
				{ header: 'Type', text: (l) => l.type },
				{ header: 'State', cell: balancerState },
				{ header: 'Scheme', hide: 'sm', text: (l) => dash(l.scheme) },
				{ header: 'DNS name', class: muted, max: 'max-w-56 xl:max-w-96', hide: 'lg', text: (l) => dash(l.dnsName) },
				{ header: 'Zones', class: muted, hide: 'xl', text: (l) => l.zones.join(', ') || '-' },
				ageColumn<LoadBalancer>((l) => l.created, now)
			],
			key: (l) => `${l.type}/${l.name}`,
			detailId: (l) => l.arn ?? `classic/${l.name}`,
			title: (l) => l.name,
			search: (l) => [l.name, l.type, l.scheme, l.state, l.dnsName, l.vpc]
		},
		route53: {
			columns: () => [
				{ header: 'Domain', class: 'font-medium', text: (z) => z.name },
				{ header: 'Type', text: (z) => (z.private ? 'private' : 'public') },
				{ header: 'Records', class: 'text-right tabular-nums', text: (z) => dash(z.records?.toLocaleString()) },
				{ header: 'Zone ID', class: muted, hide: 'sm', text: (z) => z.id },
				{ header: 'Comment', class: muted, max: 'max-w-56 xl:max-w-96', hide: 'lg', text: (z) => dash(z.comment) }
			],
			key: (z) => z.id,
			detailId: (z) => z.id,
			title: (z) => z.name,
			search: (z) => [z.name, z.id, z.comment, z.private ? 'private' : 'public']
		}
	};

	/** The spec for a list; loosely typed, for code that handles every kind alike. */
	export function specOf(kind: AwsKind): ListSpec<AwsKinds[AwsKind]> {
		return specs[kind] as unknown as ListSpec<AwsKinds[AwsKind]>;
	}
</script>

{#snippet dot(tone: Tone, label: string)}
	<span class="inline-flex items-center gap-2">
		<span class="size-2 shrink-0 rounded-full {TONE_DOT[tone]}"></span>
		{label}
	</span>
{/snippet}

<!-- The `Name` tag with the ID beside it, or the ID alone when untagged. -->
{#snippet named(r: { id: string; name: string })}
	<span class="font-medium">{r.name || r.id}</span>
	{#if r.name}<span class="text-muted-foreground ml-2 text-xs">{r.id}</span>{/if}
{/snippet}

{#snippet groupName(g: SecurityGroup)}
	<span class="font-medium">{g.name}</span>
	<span class="text-muted-foreground ml-2 text-xs">{g.id}</span>
{/snippet}

{#snippet instanceState(i: Ec2Instance)}
	{@render dot(instanceTone(i.state), i.state)}
{/snippet}

{#snippet clusterStatus(c: EksCluster)}
	{@render dot(clusterTone(c.status), c.status)}
{/snippet}

{#snippet balancerState(l: LoadBalancer)}
	{@render dot(balancerTone(l.state), l.state ?? '-')}
{/snippet}

{#snippet volumeState(v: EbsVolume)}
	{@render dot(volumeTone(v.state), v.state)}
{/snippet}

{#snippet dbStatus(d: DbInstance)}
	{@render dot(dbTone(d.status), d.status)}
{/snippet}

{#snippet mskState(c: MskCluster)}
	{@render dot(clusterTone(c.state), c.state)}
{/snippet}

{#snippet vpcState(v: Vpc)}
	{@render dot(vpcTone(v.state), v.state)}
{/snippet}
