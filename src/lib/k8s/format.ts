/** Age like `kubectl get`: 45s, 12m, 5h, 3d, 2y. */
export function age(created: string | null, now: number): string {
	if (!created) return '-';
	const seconds = Math.max(0, Math.floor((now - Date.parse(created)) / 1000));
	if (seconds < 120) return `${seconds}s`;
	const minutes = Math.floor(seconds / 60);
	if (minutes < 120) return `${minutes}m`;
	const hours = Math.floor(minutes / 60);
	if (hours < 48) return `${hours}h`;
	const days = Math.floor(hours / 24);
	if (days < 730) return `${days}d`;
	return `${Math.floor(days / 365)}y`;
}

export type Health = 'ok' | 'done' | 'pending' | 'bad';

const PENDING = /^(Pending|ContainerCreating|PodInitializing|Init:\d+\/\d+|Terminating|NotReady)$/;

/** Colour group for a pod STATUS value. */
export function podHealth(status: string): Health {
	if (status === 'Running') return 'ok';
	if (status === 'Completed' || status === 'Succeeded') return 'done';
	if (PENDING.test(status)) return 'pending';
	return 'bad';
}

export const healthDot: Record<Health, string> = {
	ok: 'bg-emerald-500',
	done: 'bg-muted-foreground/50',
	pending: 'bg-amber-500',
	bad: 'bg-red-500'
};

/** Case-insensitive match on any of the given fields. */
export function matches(query: string, ...fields: (string | null | undefined)[]): boolean {
	const q = query.trim().toLowerCase();
	return !q || fields.some((f) => f?.toLowerCase().includes(q));
}

/** Colour group for a node STATUS value (`Ready`, `NotReady`, `Ready,SchedulingDisabled`). */
export function nodeHealth(status: string): Health {
	if (status === 'Ready') return 'ok';
	if (status.startsWith('Ready,')) return 'pending';
	return 'bad';
}

/** CPU like kubectl: `250m`, or whole cores as `2`. */
export function cpu(millis: number | null | undefined): string {
	if (millis == null) return '-';
	return millis % 1000 === 0 ? String(millis / 1000) : `${Math.round(millis)}m`;
}

/** Memory in binary units: `512Mi`, `7.8Gi`. */
export function bytes(value: number | null | undefined): string {
	if (value == null) return '-';
	const units = ['', 'Ki', 'Mi', 'Gi', 'Ti', 'Pi'];
	let n = value;
	let unit = 0;
	while (n >= 1024 && unit < units.length - 1) {
		n /= 1024;
		unit++;
	}
	const digits = n >= 100 || unit === 0 ? 0 : 1;
	return `${n.toFixed(digits).replace(/\.0$/, '')}${units[unit]}`;
}
