import { goto } from '$app/navigation';

/** Returns true when it handled the key; the event is then not passed on. */
export type KeyHandler = (event: KeyboardEvent) => boolean | void;

const handlers: KeyHandler[] = [];

/**
 * Registers a page's key handler for as long as the calling component is mounted.
 * Page handlers run before the global ones (Esc, Cmd+[, ?) in `+layout.svelte`.
 * Call during component initialisation.
 */
export function useKeys(handler: KeyHandler) {
	$effect(() => {
		handlers.push(handler);
		return () => {
			handlers.splice(handlers.indexOf(handler), 1);
		};
	});
}

/** Runs page handlers, newest first. Used by the layout. */
export function dispatchKey(event: KeyboardEvent): boolean {
	for (let i = handlers.length - 1; i >= 0; i--) {
		if (handlers[i](event)) {
			event.preventDefault();
			return true;
		}
	}
	return false;
}

/** True when the key is going into a text field or the terminal, so letter shortcuts must not fire. */
export function isTyping(event: KeyboardEvent): boolean {
	const target = event.target as HTMLElement | null;
	return !!target?.closest?.('input, textarea, select, [contenteditable=""], [contenteditable="true"], .xterm');
}

/** True inside an interactive shell (marked `data-shell`), where Esc belongs to the shell. */
export function inTerminal(event: KeyboardEvent): boolean {
	return !!(event.target as HTMLElement | null)?.closest?.('[data-shell]');
}

/** A key without Cmd, Ctrl or Alt (Shift allowed, e.g. for `G` or `?`). */
export function plain(event: KeyboardEvent): boolean {
	return !event.metaKey && !event.ctrlKey && !event.altKey;
}

/**
 * New cursor position for list navigation keys, or null if the key is not one.
 * Always: ↑ ↓ Ctrl+K Ctrl+J. With `letters` (not while typing): j k g G, as in k9s.
 */
export function moveCursor(
	event: KeyboardEvent,
	index: number,
	count: number,
	letters: boolean
): number | null {
	if (count === 0) return null;
	const last = count - 1;
	const ctrl = event.ctrlKey && !event.metaKey && !event.altKey;
	const key = event.key;
	if ((plain(event) && key === 'ArrowDown') || (ctrl && key === 'j') || (letters && plain(event) && key === 'j'))
		return Math.min(index + 1, last);
	if ((plain(event) && key === 'ArrowUp') || (ctrl && key === 'k') || (letters && plain(event) && key === 'k'))
		return Math.max(index - 1, 0);
	if (letters && plain(event) && key === 'g') return 0;
	if (letters && plain(event) && key === 'G') return last;
	return null;
}

/** Moves `index` back into range after a list shrinks. */
export function clamp(index: number, count: number): number {
	return Math.max(0, Math.min(index, count - 1));
}

// In-app history depth, so "back" never leaves the app or does nothing on the first page.
let depth = 0;

export function trackNavigation(type: string, delta: number | undefined) {
	if (type === 'popstate') depth = Math.max(0, depth + (delta ?? 0));
	else if (type !== 'enter') depth++;
}

/** Previous page, or the parent route when there is no in-app history. */
export function back() {
	if (depth > 0) {
		history.back();
		return;
	}
	const path = location.pathname.replace(/\/+$/, '');
	if (path === '') return;
	goto(path.slice(0, path.lastIndexOf('/')) || '/');
}

/** Keeps the selected row of a list in view. */
export function scrollToSelected(container: HTMLElement | undefined) {
	container?.querySelector('[data-selected]')?.scrollIntoView({ block: 'nearest' });
}
