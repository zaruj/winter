import type { ITerminalOptions, ITheme } from '@xterm/xterm';
import { terminalProfile, type TerminalProfile } from './api.js';

const ANSI: (keyof ITheme)[] = [
	'black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white',
	'brightBlack', 'brightRed', 'brightGreen', 'brightYellow',
	'brightBlue', 'brightMagenta', 'brightCyan', 'brightWhite'
];

const FALLBACK = { background: '#0a0a0a', foreground: '#e5e5e5', selection: '#404040' };
const MONO = 'ui-monospace, Menlo, monospace';

/** CSS font-family for a Terminal.app PostScript font name. */
function fontFamily(postscript: string | null): string {
	if (!postscript) return MONO;
	// Terminal's bundled SF Mono is not visible to web views; ui-monospace is SF Mono.
	if (postscript.startsWith('SFMono')) return `ui-monospace, "SF Mono", Menlo, monospace`;
	const family = postscript.replace(/-[A-Za-z]+$/, '');
	return `"${postscript}", "${family}", ${MONO}`;
}

function toOptions(p: TerminalProfile | null): Partial<ITerminalOptions> {
	const theme: ITheme = {
		background: p?.background ?? FALLBACK.background,
		foreground: p?.foreground ?? FALLBACK.foreground,
		cursor: p?.cursor ?? p?.foreground ?? FALLBACK.foreground,
		selectionBackground: p?.selection ?? FALLBACK.selection
	};
	p?.ansi.forEach((color, i) => {
		if (color) (theme as Record<string, string>)[ANSI[i]] = color;
	});
	return { theme, fontFamily: fontFamily(p?.font ?? null), fontSize: p?.fontSize ?? 13 };
}

/** Terminal look, from Terminal.app's default profile when there is one. */
export const appearance = $state({
	profile: null as string | null,
	background: FALLBACK.background,
	foreground: FALLBACK.foreground
});

let loaded: Promise<Partial<ITerminalOptions>> | null = null;

/** xterm options matching the user's terminal. Loaded once. */
export function terminalOptions(): Promise<Partial<ITerminalOptions>> {
	loaded ??= terminalProfile()
		.catch(() => null)
		.then((p) => {
			const options = toOptions(p);
			appearance.profile = p?.name ?? null;
			appearance.background = options.theme!.background!;
			appearance.foreground = options.theme!.foreground!;
			return options;
		});
	return loaded;
}
