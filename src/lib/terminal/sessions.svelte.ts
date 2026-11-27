import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import {
	openPodShell,
	resizePodShell,
	stopStream,
	writePodShell,
	type Target
} from '#lib/k8s/api.js';
import { closeSession, openSession, resizeSession, writeSession } from './api.js';
import { terminalOptions } from './appearance.svelte.js';

/** What a tab talks to: the local shell, or a shell inside a pod. */
export interface ShellBackend {
	title: string;
	open(
		cols: number,
		rows: number,
		onOutput: (bytes: ArrayBuffer) => void,
		onExit: (code: number | null) => void
	): Promise<number>;
	write(id: number, data: string): Promise<void>;
	resize(id: number, cols: number, rows: number): Promise<void>;
	close(id: number): Promise<void>;
}

export const localShell = (): ShellBackend => ({
	title: 'shell',
	open: openSession,
	write: writeSession,
	resize: resizeSession,
	close: closeSession
});

export const podShell = (target: Target): ShellBackend => ({
	title: `${target.pod}${target.container ? ` (${target.container})` : ''}`,
	open: (cols, rows, onOutput, onExit) => openPodShell(target, cols, rows, onOutput, onExit),
	write: writePodShell,
	resize: resizePodShell,
	close: stopStream
});

/** Keys the app uses itself; xterm must not send them to the shell. */
export function isAppShortcut(event: KeyboardEvent): boolean {
	return event.metaKey && /^([tw[]|[1-9])$/i.test(event.key);
}

/**
 * One shell in a tab. The xterm instance renders into its own element, which the terminal page
 * attaches while visible. Output keeps flowing into the buffer while the page is closed, so the
 * tab is intact when the user comes back.
 */
export class TerminalTab {
	readonly key: number;
	readonly element = document.createElement('div');
	readonly term: Terminal;
	readonly backend: ShellBackend;
	title = $state('shell');
	error = $state<string | null>(null);
	#fit = new FitAddon();
	#id: number | null = null;
	#opened = false;
	#exited = false;
	#onExit: (tab: TerminalTab) => void;

	constructor(key: number, backend: ShellBackend, onExit: (tab: TerminalTab) => void) {
		this.key = key;
		this.backend = backend;
		this.title = backend.title;
		this.#onExit = onExit;
		this.element.className = 'h-full w-full';
		this.term = new Terminal({ cursorBlink: true, scrollback: 10_000 });
		this.term.loadAddon(this.#fit);
		this.term.attachCustomKeyEventHandler((event) => !isAppShortcut(event));
		this.term.onData((data) => this.#id !== null && backend.write(this.#id, data));
		this.term.onResize(({ cols, rows }) => this.#id !== null && backend.resize(this.#id, cols, rows));
		this.term.onTitleChange((title) => (this.title = title || backend.title));
	}

	/** Shows the terminal inside `host`; starts the shell the first time, sized to fit. */
	async attach(host: HTMLElement, focus = true) {
		host.replaceChildren(this.element);
		if (!this.#opened) {
			this.#opened = true;
			Object.assign(this.term.options, await terminalOptions());
			this.term.open(this.element);
		}
		if (!this.element.isConnected) return;
		this.fit();
		if (focus) this.term.focus();
		// The renderer measures cells after layout; fit again once it has.
		requestAnimationFrame(() => {
			this.fit();
			if (focus) this.term.focus();
		});
		if (this.#id === null && !this.error) await this.#start();
	}

	focus() {
		this.term.focus();
	}

	fit() {
		if (this.element.isConnected && this.#opened) this.#fit.fit();
	}

	async #start() {
		try {
			this.#id = await this.backend.open(
				this.term.cols,
				this.term.rows,
				(bytes) => this.term.write(new Uint8Array(bytes)),
				() => this.#exit()
			);
			// Resizes made while the shell was starting were not sent.
			await this.backend.resize(this.#id, this.term.cols, this.term.rows);
		} catch (e) {
			this.error = String(e);
			this.term.write(`\r\n\x1b[31mCould not start shell: ${this.error}\x1b[0m\r\n`);
		}
	}

	/**
	 * Ends the shell and removes the tab. Stopping a pod shell cancels its stream, so no exit
	 * comes back from it; the tab is removed once closing is done either way.
	 */
	close() {
		if (this.#id === null) this.#exit();
		else this.backend.close(this.#id).finally(() => this.#exit());
	}

	/** Reports the exit once, whether the shell ended by itself or was closed. */
	#exit() {
		if (this.#exited) return;
		this.#exited = true;
		this.#onExit(this);
	}

	dispose() {
		this.term.dispose();
		this.element.remove();
	}
}

class Sessions {
	tabs = $state<TerminalTab[]>([]);
	active = $state(0);
	#nextKey = 0;

	get current(): TerminalTab | undefined {
		return this.tabs[this.active];
	}

	create(backend: ShellBackend = localShell()): TerminalTab {
		const tab = new TerminalTab(++this.#nextKey, backend, (t) => this.#remove(t));
		this.tabs.push(tab);
		this.active = this.tabs.length - 1;
		return tab;
	}

	select(index: number) {
		if (index >= 0 && index < this.tabs.length) this.active = index;
	}

	#remove(tab: TerminalTab) {
		const index = this.tabs.indexOf(tab);
		if (index < 0) return;
		this.tabs.splice(index, 1);
		tab.dispose();
		if (this.active >= this.tabs.length) this.active = Math.max(0, this.tabs.length - 1);
		else if (index < this.active) this.active--;
	}
}

/** Lives for the whole app session, so shells survive leaving the terminal page. */
export const sessions = new Sessions();
