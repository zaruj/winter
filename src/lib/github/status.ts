import { TONE_DOT, type Tone } from "#lib/aws/tone.js";

export { TONE_DOT, type Tone };

/** A run's, job's or step's state in one word, as GitHub shows it. */
export function statusLabel(status: string, conclusion: string | null): string {
  if (status !== "completed") return status.replace("_", " ");
  return (conclusion ?? "completed").replace("_", " ");
}

export function statusTone(status: string, conclusion: string | null): Tone {
  if (status !== "completed") return "pending";
  switch (conclusion) {
    case "success":
      return "ok";
    case "failure":
    case "timed_out":
    case "startup_failure":
    case "action_required":
      return "bad";
    default:
      // cancelled, skipped, neutral, stale
      return "off";
  }
}

/** `1h 2m`, `3m 4s`, `5s`; from start to end, or to `now` while running. */
export function duration(start: string | null, end: string | null, now: number): string {
  if (!start) return "-";
  const from = Date.parse(start);
  const to = end ? Date.parse(end) : now;
  const total = Math.max(0, Math.round((to - from) / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h) return `${h}h ${m}m`;
  if (m) return `${m}m ${s}s`;
  return `${s}s`;
}
