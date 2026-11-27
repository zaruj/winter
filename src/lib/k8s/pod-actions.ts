import type { PodSummary } from './api.js';
import { clusterView } from './view.svelte.js';

// Logs and shells open as tabs in the cluster page's dock, below the list.

/** Opens the pod's log (its default container). */
export function openLogs(context: string, pod: PodSummary) {
	clusterView(context).logs.open(pod);
}

/** Opens a shell in the pod's default container. */
export function openShell(context: string, pod: PodSummary) {
	clusterView(context).logs.openShell(context, {
		namespace: pod.namespace,
		pod: pod.name,
		container: pod.defaultContainer
	});
}
