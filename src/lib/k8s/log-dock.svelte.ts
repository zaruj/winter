import { podShell, TerminalTab } from '#lib/terminal/sessions.svelte.js';
import type { PodSummary } from './api.js';

/** A pod's log or a shell in a pod, open as a tab in the dock. */
export type DockTab = LogTab | ShellTab;

export interface LogTab {
	kind: 'log';
	id: number;
	namespace: string;
	pod: string;
	container: string | null;
	/** Every container of the pod, for the picker. */
	containers: string[];
}

export interface ShellTab {
	kind: 'shell';
	id: number;
	namespace: string;
	pod: string;
	container: string | null;
	/** Keeps running while the page is closed, like the Terminal page's tabs. */
	terminal: TerminalTab;
}

let nextId = 1;

/**
 * Pod logs and shells in tabs docked at the bottom of a cluster page. Kept per cluster for the
 * whole app session: shells keep running when the page is left, and logs restart from their
 * tail when it is shown again.
 */
export class LogDock {
	tabs = $state<DockTab[]>([]);
	active = $state<number | null>(null);
	/** Shown, or hidden with its tabs kept. */
	visible = $state(false);
	/** Height in CSS pixels, set by dragging the dock's top edge. */
	height = $state(300);
	/** A shell just opened, to be given keyboard focus once shown. */
	focusNext = $state<number | null>(null);

	/** Opens the pod's log, or switches to it if it is already open. */
	open(pod: PodSummary) {
		const found = this.tabs.find(
			(t) => t.kind === 'log' && t.namespace === pod.namespace && t.pod === pod.name
		);
		if (found) this.#show(found.id);
		else
			this.#add({
				kind: 'log',
				id: nextId++,
				namespace: pod.namespace,
				pod: pod.name,
				container: pod.defaultContainer,
				containers: pod.containerNames
			});
	}

	/** Opens a new shell in the pod (`kubectl exec -it`). It closes when the shell exits. */
	openShell(context: string, target: { namespace: string; pod: string; container: string | null }) {
		const id = nextId++;
		const terminal = new TerminalTab(id, podShell({ context, ...target }), () => this.#remove(id));
		this.#add({ kind: 'shell', id, ...target, terminal });
		this.focusNext = id;
	}

	/** Closes a tab right away; a shell is ended in the background. */
	close(id: number) {
		const tab = this.tabs.find((t) => t.id === id);
		if (tab?.kind === 'shell') tab.terminal.close();
		this.#remove(id);
	}

	/** Shows or hides the dock; nothing to show without tabs. */
	toggle() {
		this.visible = !this.visible && this.tabs.length > 0;
	}

	/** Moves to the next (1) or previous (-1) tab. */
	step(by: number) {
		const at = this.tabs.findIndex((t) => t.id === this.active);
		if (at === -1 || !this.tabs.length) return;
		this.active = this.tabs[(at + by + this.tabs.length) % this.tabs.length].id;
	}

	#add(tab: DockTab) {
		this.tabs = [...this.tabs, tab];
		this.#show(tab.id);
	}

	#show(id: number) {
		this.active = id;
		this.visible = true;
	}

	#remove(id: number) {
		const at = this.tabs.findIndex((t) => t.id === id);
		if (at === -1) return;
		const [tab] = this.tabs.splice(at, 1);
		this.tabs = [...this.tabs];
		if (tab.kind === 'shell') tab.terminal.dispose();
		if (this.active === id) this.active = (this.tabs[at] ?? this.tabs[at - 1])?.id ?? null;
		if (!this.tabs.length) this.visible = false;
	}
}
