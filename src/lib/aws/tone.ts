/** Colour groups for AWS states, as dots next to the state text. */
export type Tone = "ok" | "pending" | "off" | "bad";

export const TONE_DOT: Record<Tone, string> = {
  ok: "bg-emerald-500",
  pending: "bg-amber-500",
  off: "bg-muted-foreground/50",
  bad: "bg-red-500",
};

export const instanceTone = (s: string): Tone =>
  s === "running"
    ? "ok"
    : s === "stopped"
      ? "off"
      : s === "terminated"
        ? "bad"
        : "pending";

export const clusterTone = (s: string): Tone =>
  s === "ACTIVE"
    ? "ok"
    : s === "FAILED"
      ? "bad"
      : s === "DELETING"
        ? "off"
        : "pending";

/** Classic load balancers have no state; they are shown as fine. */
export const balancerTone = (s: string | null): Tone =>
  s === "active" || s === null
    ? "ok"
    : s === "provisioning"
      ? "pending"
      : "bad";
