import Container from "@lucide/svelte/icons/container";
import Network from "@lucide/svelte/icons/network";
import Server from "@lucide/svelte/icons/server";
import type { Component } from "svelte";
import type { AwsKind } from "./api.js";

/** An AWS service the account page can list, with the names `:` accepts for it. */
export interface Service {
  kind: AwsKind;
  label: string;
  icon: Component;
  /** Lower case; the first is shown as the short name. */
  names: string[];
  description: string;
}

export const SERVICES: Service[] = [
  {
    kind: "ec2",
    label: "EC2 instances",
    icon: Server,
    names: ["ec2", "instances", "instance", "vm", "servers"],
    description: "Virtual machines",
  },
  {
    kind: "eks",
    label: "EKS clusters",
    icon: Container,
    names: ["eks", "clusters", "cluster", "kubernetes", "k8s"],
    description: "Managed Kubernetes",
  },
  {
    kind: "elb",
    label: "Load balancers",
    icon: Network,
    names: [
      "elb",
      "lb",
      "alb",
      "nlb",
      "loadbalancers",
      "load-balancers",
      "elbv2",
    ],
    description: "ALB, NLB, Gateway and Classic",
  },
];

/** The sidebar. Number keys follow this order. */
export const GROUPS: { label: string; kinds: AwsKind[] }[] = [
  { label: "Compute", kinds: ["ec2"] },
  { label: "Containers", kinds: ["eks"] },
  { label: "Networking", kinds: ["elb"] },
];

export const ORDER: AwsKind[] = GROUPS.flatMap((g) => g.kinds);

export const serviceOf = (kind: AwsKind) =>
  SERVICES.find((s) => s.kind === kind)!;

/** Lower is better; null when nothing matches. Exact name, then prefix, then substring. */
function score(token: string, names: string[]): number | null {
  if (names.includes(token)) return 0;
  const prefix = names.filter((n) => n.startsWith(token));
  if (prefix.length) return 1 + Math.min(...prefix.map((n) => n.length)) / 1000;
  if (names.some((n) => n.includes(token))) return 2;
  return null;
}

/** A `:` suggestion: a service, or a region to switch to. */
export type Suggestion =
  { type: "service"; service: Service } | { type: "region"; region: string };

/** Services matching `token` (by name or label), then regions starting with or containing it. */
export function suggest(
  token: string,
  regions: string[],
  limit = 8,
): Suggestion[] {
  const t = token.toLowerCase();
  if (!t) return SERVICES.map((service) => ({ type: "service", service }));
  const services = SERVICES.map((s) => ({
    s,
    score: score(t, [...s.names, s.label.toLowerCase()]),
  }))
    .filter((x): x is { s: Service; score: number } => x.score !== null)
    .sort((a, b) => a.score - b.score)
    .map(({ s }): Suggestion => ({ type: "service", service: s }));
  return [
    ...services,
    ...suggestRegions(t, regions).map((region): Suggestion => ({
      type: "region",
      region,
    })),
  ].slice(0, limit);
}

/** Regions starting with, then containing, `token`. */
export function suggestRegions(
  token: string,
  regions: string[],
  limit = 8,
): string[] {
  const t = token.toLowerCase();
  const starts = regions.filter((r) => r.startsWith(t));
  const contains = regions.filter((r) => !r.startsWith(t) && r.includes(t));
  return [...starts, ...contains].slice(0, limit);
}
