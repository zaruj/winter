import { Channel, invoke } from '@tauri-apps/api/core';

// Typed wrappers for the terminal commands in src-tauri/src/commands.rs.

/** Starts the login shell. `onOutput` gets raw bytes; `onExit` the exit code (null if unknown). */
export function openSession(
	cols: number,
	rows: number,
	onOutput: (bytes: ArrayBuffer) => void,
	onExit: (code: number | null) => void
): Promise<number> {
	const output = new Channel<ArrayBuffer>(onOutput);
	const exit = new Channel<number | null>(onExit);
	return invoke('terminal_open', { cols, rows, onOutput: output, onExit: exit });
}

export function writeSession(id: number, data: string): Promise<void> {
	return invoke('terminal_write', { id, data });
}

export function resizeSession(id: number, cols: number, rows: number): Promise<void> {
	return invoke('terminal_resize', { id, cols, rows });
}

export function closeSession(id: number): Promise<void> {
	return invoke('terminal_close', { id });
}

export interface TerminalProfile {
	name: string;
	font: string | null;
	fontSize: number | null;
	background: string | null;
	foreground: string | null;
	bold: string | null;
	cursor: string | null;
	selection: string | null;
	/** 16 ANSI colours: normal then bright. */
	ansi: (string | null)[];
}

/** Font and colours of Terminal.app's default profile (macOS), if available. */
export function terminalProfile(): Promise<TerminalProfile | null> {
	return invoke('terminal_profile');
}
