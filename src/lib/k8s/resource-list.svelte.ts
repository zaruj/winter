import { stopStream, type Change, type Sink } from './api.js';

export type Fetcher<T> = (sink: Sink<T>) => Promise<void>;

/** Identity of an item: namespace (if any) and name. */
export const itemKey = (item: { name: string; namespace?: string | null }) =>
	`${item.namespace ?? ''}/${item.name}`;

/** Changes arriving within this window are applied together, so a busy cluster re-renders less. */
const BATCH_MS = 100;

/**
 * Reactive list filled from a paged command, then kept up to date by its watch.
 *
 * - Empty list: rows are appended as each page arrives, so the first rows show immediately.
 * - Rows already shown: they stay visible (narrowed by `keep`, e.g. to a newly picked namespace)
 *   and are swapped for the fresh list once it is complete, so the table never flickers empty.
 * - After that, added, changed and deleted items are applied as the cluster reports them. If the
 *   watch cannot resume, the list is fetched again quietly.
 *
 * Starting a new load stops the previous load's watches and ignores its results.
 */
export class ResourceList<T extends { name: string; namespace?: string | null }> {
	items = $state.raw<T[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);
	#generation = 0;
	#watches: number[] = [];

	async load(fetch: Fetcher<T>, keep?: (item: T) => boolean) {
		this.stop();
		const generation = ++this.#generation;
		const current = () => generation === this.#generation;

		if (keep) this.items = this.items.filter(keep);
		const streaming = this.items.length === 0;
		let fresh: T[] = [];
		let swapped = streaming;
		this.loading = true;
		this.error = null;

		let queued: Change<T>[] = [];
		let timer: ReturnType<typeof setTimeout> | undefined;
		const flush = () => {
			timer = undefined;
			if (!current()) return;
			const changes = queued;
			queued = [];
			if (changes.some((c) => c.type === 'reset')) {
				this.load(fetch);
				return;
			}
			if (swapped) this.items = apply(this.items, changes);
			else fresh = apply(fresh, changes);
		};

		try {
			await fetch({
				page: (page) => {
					if (!current()) return;
					if (streaming) this.items = [...this.items, ...page];
					else fresh = fresh.concat(page);
				},
				change: (change) => {
					if (!current()) return;
					queued.push(change);
					timer ??= setTimeout(flush, BATCH_MS);
				},
				watching: (id) => {
					if (current()) this.#watches.push(id);
					else stopStream(id);
				}
			});
			if (current() && !swapped) {
				if (timer !== undefined) {
					clearTimeout(timer);
					flush();
				}
				this.items = fresh;
				swapped = true;
			}
		} catch (e) {
			if (current()) this.error = String(e);
		} finally {
			if (current()) this.loading = false;
		}
	}

	/** Stops following changes; the items stay. */
	stop() {
		for (const id of this.#watches) stopStream(id);
		this.#watches = [];
	}

	/** Ignores any load in progress, stops watching and empties the list. */
	reset() {
		this.stop();
		this.#generation++;
		this.items = [];
		this.loading = false;
		this.error = null;
	}
}

/** Applies changes in order. New items go where they sort by namespace and name, as lists do. */
function apply<T extends { name: string; namespace?: string | null }>(
	items: T[],
	changes: Change<T>[]
): T[] {
	const byKey = new Map(items.map((item) => [itemKey(item), item]));
	const added: string[] = [];
	for (const change of changes) {
		if (change.type === 'reset') continue;
		const key = itemKey(change.item);
		if (change.type === 'deleted') byKey.delete(key);
		else {
			if (!byKey.has(key)) added.push(key);
			byKey.set(key, change.item);
		}
	}
	// Existing items keep their place (with their new state); deleted ones drop out.
	const next: T[] = [];
	for (const item of items) {
		const now = byKey.get(itemKey(item));
		if (now) next.push(now);
	}
	for (const key of added) {
		const item = byKey.get(key);
		if (!item || next.includes(item)) continue;
		const at = next.findIndex((other) => itemKey(other) > key);
		next.splice(at === -1 ? next.length : at, 0, item);
	}
	return next;
}
