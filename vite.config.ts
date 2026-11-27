import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Tauri has no Node server, so ship a single-page app. Tauri loads index.html.
			adapter: adapter({ fallback: 'index.html' })
		})
	],

	// Keep Rust errors visible in `tauri dev`.
	clearScreen: false,
	server: {
		// Must match build.devUrl in src-tauri/tauri.conf.json.
		host: '127.0.0.1',
		port: 1420,
		strictPort: true,
		watch: {
			ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**']
		}
	}
});
