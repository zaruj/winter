<script lang="ts">
	import '@xterm/xterm/css/xterm.css';
	import { stopStream, streamLogs } from './api.js';
	import { terminalOptions } from '#lib/terminal/appearance.svelte.js';
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';

	// Follows one container's log in a read-only xterm (ANSI colours, fast scrollback).
	// Mounted for as long as its dock tab is open; hidden tabs keep following.

	const TAIL_LINES = 1000;

	export type Status = 'starting' | 'following' | 'ended' | 'failed';

	let {
		context,
		namespace,
		pod,
		container,
		status = $bindable('starting'),
		error = $bindable(null)
	}: {
		context: string;
		namespace: string;
		pod: string;
		container: string | null;
		status?: Status;
		error?: string | null;
	} = $props();

	let host: HTMLElement | undefined = $state();
	let term: Terminal | undefined;

	$effect(() => {
		if (!host) return;
		const t = new Terminal({
			disableStdin: true,
			convertEol: true,
			scrollback: 100_000,
			cursorInactiveStyle: 'none',
			cursorStyle: 'bar',
			cursorWidth: 1
		});
		const fit = new FitAddon();
		t.loadAddon(fit);
		let disposed = false;
		// A hidden tab has no size; fit once it is shown again.
		const refit = () => !disposed && host && host.clientWidth > 0 && fit.fit();
		// Next frame, so resizing the terminal does not feed back into this observer.
		const observer = new ResizeObserver(() => requestAnimationFrame(refit));
		terminalOptions().then((options) => {
			if (disposed || !host) return;
			Object.assign(t.options, options);
			t.open(host);
			refit();
			term = t;
			observer.observe(host);
			follow(t);
		});
		return () => {
			disposed = true;
			observer.disconnect();
			term = undefined;
			t.dispose();
		};
	});

	let streamId: number | null = null;
	let generation = 0;

	/** (Re)starts following the container's log into `t`. */
	async function follow(t: Terminal) {
		const mine = ++generation;
		if (streamId !== null) stopStream(streamId);
		streamId = null;
		t.reset();
		status = 'starting';
		error = null;
		try {
			const id = await streamLogs(
				{ context, namespace, pod, container },
				TAIL_LINES,
				(bytes) => mine === generation && t.write(new Uint8Array(bytes)),
				(message) => {
					if (mine !== generation) return;
					status = message ? 'failed' : 'ended';
					error = message;
				}
			);
			if (mine !== generation) return stopStream(id);
			streamId = id;
			if (status === 'starting') status = 'following';
		} catch (e) {
			if (mine !== generation) return;
			status = 'failed';
			error = String(e);
		}
	}

	// Restart when another container is picked; the first start comes from the xterm effect.
	let shown = '';
	$effect(() => {
		const key = `${context}/${namespace}/${pod}/${container}`;
		if (shown && key !== shown && term) follow(term);
		shown = key;
	});

	$effect(() => () => {
		generation++;
		if (streamId !== null) stopStream(streamId);
	});

	/** Scroll keys, as in `less`: j k, u d (pages), g G (top, bottom). */
	export function handleKey(event: KeyboardEvent): boolean {
		if (!term || event.metaKey || event.ctrlKey || event.altKey) return false;
		switch (event.key) {
			case 'j':
			case 'ArrowDown':
				term.scrollLines(1);
				return true;
			case 'k':
			case 'ArrowUp':
				term.scrollLines(-1);
				return true;
			case 'd':
			case 'PageDown':
				term.scrollPages(1);
				return true;
			case 'u':
			case 'PageUp':
				term.scrollPages(-1);
				return true;
			case 'g':
				term.scrollToTop();
				return true;
			case 'G':
				term.scrollToBottom();
				return true;
		}
		return false;
	}
</script>

<div class="h-full min-h-0 px-2 pt-1" data-logs bind:this={host}></div>
