import { age } from './format.js';

// What `kubectl describe` shows first, worked out from the object itself: a few facts per kind,
// tables for containers, ports, rules and conditions, and the labels. Kinds this does not know
// (custom resources) still get their scalar spec and status fields.

/* eslint-disable @typescript-eslint/no-explicit-any */
type Obj = Record<string, any>;

export type { Section } from '#lib/components/Sections.svelte';
import type { Section } from '#lib/components/Sections.svelte';

export interface Table {
	columns: string[];
	rows: string[][];
}

const str = (v: unknown): string =>
	v === null || v === undefined ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v);
const list = (v: unknown): any[] => (Array.isArray(v) ? v : []);
const entries = (v: unknown): [string, string][] =>
	v && typeof v === 'object' && !Array.isArray(v) ? Object.entries(v as Obj).map(([k, x]) => [k, str(x)]) : [];
const facts = (pairs: [string, unknown][]): [string, string][] =>
	pairs.map(([k, v]): [string, string] => [k, str(v)]).filter(([, v]) => v !== '');

/** Ready/started state of a container from its status, in one phrase. */
function containerState(s: Obj | undefined): string {
	const state = s?.state ?? {};
	if (state.running) return 'running';
	if (state.waiting) return state.waiting.reason ?? 'waiting';
	if (state.terminated) {
		const t = state.terminated;
		return `${t.reason ?? 'terminated'}${t.exitCode !== undefined ? ` (exit ${t.exitCode})` : ''}`;
	}
	return '-';
}

function resources(r: Obj | undefined): string {
	const part = (name: string, m: Obj | undefined) =>
		m ? `${name} ${Object.entries(m).map(([k, v]) => `${k} ${v}`).join(', ')}` : '';
	return [part('req', r?.requests), part('lim', r?.limits)].filter(Boolean).join(' · ');
}

function containers(spec: Obj, statuses: Obj[] = [], init = false): Section | null {
	const items = list(init ? spec.initContainers : spec.containers);
	if (!items.length) return null;
	const status = (name: string) => statuses.find((s) => s.name === name);
	return {
		title: init ? 'Init containers' : 'Containers',
		table: {
			columns: ['Name', 'Image', 'State', 'Ready', 'Restarts', 'Ports', 'Resources'],
			rows: items.map((c) => {
				const s = status(c.name);
				return [
					c.name,
					c.image ?? '',
					s ? containerState(s) : '',
					s ? (s.ready ? 'yes' : 'no') : '',
					s ? str(s.restartCount ?? 0) : '',
					list(c.ports).map((p) => `${p.containerPort}/${p.protocol ?? 'TCP'}`).join(', '),
					resources(c.resources)
				];
			})
		}
	};
}

function podTemplate(template: Obj | undefined): Section[] {
	const spec = template?.spec;
	if (!spec) return [];
	return [containers(spec, [], true), containers(spec)].filter((s): s is Section => s !== null);
}

function replicas(o: Obj): [string, unknown][] {
	const s = o.status ?? {};
	return [
		['Replicas', o.spec?.replicas !== undefined ? `${s.readyReplicas ?? 0}/${o.spec.replicas} ready` : ''],
		['Updated', s.updatedReplicas],
		['Available', s.availableReplicas],
		['Unavailable', s.unavailableReplicas]
	];
}

const selector = (sel: Obj | undefined) =>
	entries(sel?.matchLabels ?? sel)
		.map(([k, v]) => `${k}=${v}`)
		.join(', ');

/** Facts and tables for kinds kubectl knows; null for the rest. */
function byKind(kind: string, o: Obj): Section[] | null {
	const spec = o.spec ?? {};
	const status = o.status ?? {};
	switch (kind) {
		case 'Pod':
			return [
				{
					title: 'Pod',
					facts: facts([
						['Status', status.phase],
						['Node', spec.nodeName],
						['Pod IP', list(status.podIPs).map((p) => p.ip).join(', ') || status.podIP],
						['Host IP', status.hostIP],
						['QoS', status.qosClass],
						['Service account', spec.serviceAccountName],
						['Restart policy', spec.restartPolicy],
						['Priority class', spec.priorityClassName],
						['Reason', status.reason],
						['Message', status.message]
					])
				},
				...[
					containers(spec, list(status.initContainerStatuses), true),
					containers(spec, list(status.containerStatuses))
				].filter((s): s is Section => s !== null),
				...(list(spec.volumes).length
					? [
							{
								title: 'Volumes',
								table: {
									columns: ['Name', 'Type', 'Source'],
									rows: list(spec.volumes).map((v) => {
										const [type, src] = Object.entries(v).find(([k]) => k !== 'name') ?? ['', {}];
										const source = src as Obj;
										return [
											v.name,
											type,
											str(source.claimName ?? source.name ?? source.secretName ?? source.path ?? source.medium ?? '')
										];
									})
								}
							}
						]
					: [])
			];
		case 'Deployment':
		case 'StatefulSet':
		case 'ReplicaSet':
		case 'DaemonSet':
			return [
				{
					title: kind,
					facts: facts([
						...(kind === 'DaemonSet'
							? ([
									['Desired', status.desiredNumberScheduled],
									['Ready', status.numberReady],
									['Available', status.numberAvailable]
								] as [string, unknown][])
							: replicas(o)),
						['Strategy', spec.strategy?.type ?? spec.updateStrategy?.type],
						['Selector', selector(spec.selector)],
						['Service', spec.serviceName]
					])
				},
				...podTemplate(spec.template)
			];
		case 'Job':
		case 'CronJob': {
			const job = kind === 'CronJob' ? spec.jobTemplate?.spec ?? {} : spec;
			return [
				{
					title: kind,
					facts: facts([
						['Schedule', spec.schedule],
						['Suspended', spec.suspend],
						['Last schedule', status.lastScheduleTime],
						['Completions', job.completions],
						['Parallelism', job.parallelism],
						['Succeeded', status.succeeded],
						['Failed', status.failed],
						['Active', Array.isArray(status.active) ? status.active.length : status.active]
					])
				},
				...podTemplate(job.template)
			];
		}
		case 'Service':
			return [
				{
					title: 'Service',
					facts: facts([
						['Type', spec.type],
						['Cluster IP', list(spec.clusterIPs).join(', ') || spec.clusterIP],
						['External', [...list(spec.externalIPs), ...list(status.loadBalancer?.ingress).map((i) => i.hostname ?? i.ip)].join(', ')],
						['External name', spec.externalName],
						['Selector', selector(spec.selector)],
						['Session affinity', spec.sessionAffinity]
					])
				},
				{
					title: 'Ports',
					table: {
						columns: ['Name', 'Port', 'Target', 'Node port', 'Protocol'],
						rows: list(spec.ports).map((p) => [p.name ?? '', str(p.port), str(p.targetPort), str(p.nodePort), p.protocol ?? 'TCP'])
					}
				}
			];
		case 'Ingress':
			return [
				{
					title: 'Ingress',
					facts: facts([
						['Class', spec.ingressClassName],
						['Address', list(status.loadBalancer?.ingress).map((i) => i.hostname ?? i.ip).join(', ')],
						['TLS', list(spec.tls).flatMap((t) => list(t.hosts)).join(', ')]
					])
				},
				{
					title: 'Rules',
					table: {
						columns: ['Host', 'Path', 'Backend'],
						rows: list(spec.rules).flatMap((r) =>
							list(r.http?.paths).map((p) => [
								r.host ?? '*',
								p.path ?? '/',
								p.backend?.service
									? `${p.backend.service.name}:${p.backend.service.port?.number ?? p.backend.service.port?.name ?? ''}`
									: str(p.backend?.resource?.name)
							])
						)
					}
				}
			];
		case 'PersistentVolumeClaim':
			return [
				{
					title: 'Claim',
					facts: facts([
						['Status', status.phase],
						['Volume', spec.volumeName],
						['Capacity', status.capacity?.storage],
						['Requested', spec.resources?.requests?.storage],
						['Access modes', list(spec.accessModes).join(', ')],
						['Storage class', spec.storageClassName],
						['Volume mode', spec.volumeMode]
					])
				}
			];
		case 'ConfigMap':
		case 'Secret': {
			const data = { ...(o.data ?? {}), ...(o.binaryData ?? {}), ...(o.stringData ?? {}) } as Obj;
			return [
				{
					title: kind === 'Secret' ? 'Secret' : 'Data',
					facts: facts([['Type', o.type]]),
					table: {
						columns: ['Key', kind === 'Secret' ? 'Value' : 'Size'],
						rows: Object.entries(data).map(([k, v]) => [
							k,
							kind === 'Secret' ? str(v) : `${str(v).length} bytes`
						])
					}
				}
			];
		}
		case 'Namespace':
			return [{ title: 'Namespace', facts: facts([['Status', status.phase]]) }];
		case 'Node': {
			const info = status.nodeInfo ?? {};
			return [
				{
					title: 'Node',
					facts: facts([
						['Addresses', list(status.addresses).map((a) => `${a.type} ${a.address}`).join(', ')],
						['Kubelet', info.kubeletVersion],
						['OS', info.osImage],
						['Kernel', info.kernelVersion],
						['Runtime', info.containerRuntimeVersion],
						['Pod CIDR', spec.podCIDR],
						['Unschedulable', spec.unschedulable],
						['Taints', list(spec.taints).map((t) => `${t.key}${t.value ? `=${t.value}` : ''}:${t.effect}`).join(', ')]
					])
				},
				{
					title: 'Capacity',
					table: {
						columns: ['Resource', 'Capacity', 'Allocatable'],
						rows: Object.keys(status.capacity ?? {}).map((k) => [k, str(status.capacity[k]), str(status.allocatable?.[k])])
					}
				}
			];
		}
		default:
			return null;
	}
}

/** Scalar fields of an object (one level), for kinds without their own summary. */
function scalars(title: string, v: unknown): Section | null {
	const pairs = entries(v).filter(([, x]) => x !== '' && !x.startsWith('{') && !x.startsWith('['));
	return pairs.length ? { title, facts: pairs } : null;
}

export function describe(o: Obj, now: number): Section[] {
	const meta = o.metadata ?? {};
	const sections: Section[] = [
		{
			title: 'Metadata',
			facts: facts([
				['Namespace', meta.namespace],
				['Created', meta.creationTimestamp ? `${age(meta.creationTimestamp, now)} ago (${meta.creationTimestamp})` : ''],
				['Owner', list(meta.ownerReferences).map((r) => `${r.kind}/${r.name}`).join(', ')],
				['Deleting', meta.deletionTimestamp ? `since ${meta.deletionTimestamp}` : ''],
				['Finalizers', list(meta.finalizers).join(', ')],
				['API version', o.apiVersion]
			])
		}
	];
	const known = byKind(o.kind ?? '', o);
	if (known) sections.push(...known);
	else sections.push(...[scalars('Spec', o.spec), scalars('Status', o.status)].filter((s): s is Section => s !== null));

	const conditions = list(o.status?.conditions);
	if (conditions.length) {
		sections.push({
			title: 'Conditions',
			table: {
				columns: ['Type', 'Status', 'Reason', 'Age', 'Message'],
				rows: conditions.map((c) => [
					c.type,
					c.status,
					c.reason ?? '',
					c.lastTransitionTime ? age(c.lastTransitionTime, now) : '',
					c.message ?? ''
				])
			}
		});
	}
	const labels = entries(meta.labels);
	if (labels.length) sections.push({ title: 'Labels', chips: labels });
	const annotations = entries(meta.annotations).map(([k, v]): [string, string] =>
		// Often a whole copy of the object; the YAML view has it.
		k === 'kubectl.kubernetes.io/last-applied-configuration' ? [k, `(${v.length} bytes, see YAML)`] : [k, v]
	);
	if (annotations.length) sections.push({ title: 'Annotations', chips: annotations });
	return sections;
}
