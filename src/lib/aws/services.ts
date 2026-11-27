import Archive from "@lucide/svelte/icons/archive";
import Cloud from "@lucide/svelte/icons/cloud";
import Container from "@lucide/svelte/icons/container";
import Database from "@lucide/svelte/icons/database";
import Globe from "@lucide/svelte/icons/globe";
import HardDrive from "@lucide/svelte/icons/hard-drive";
import Network from "@lucide/svelte/icons/network";
import Server from "@lucide/svelte/icons/server";
import Shield from "@lucide/svelte/icons/shield";
import Wallet from "@lucide/svelte/icons/wallet";
import Workflow from "@lucide/svelte/icons/workflow";
import type { Component } from "svelte";
import type { AwsKind } from "./api.js";

/** What the account page can show: a resource list, or the cost overview. */
export type View = AwsKind | "cost";

/** An AWS service the account page can show, with the names `:` accepts for it. */
export interface Service {
  kind: View;
  label: string;
  icon: Component;
  /** Lower case; the first is shown as the short name. */
  names: string[];
  description: string;
  /** Not tied to a region (S3 lists every bucket, Route 53 and billing are global). */
  global?: boolean;
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
    kind: "s3",
    label: "S3 buckets",
    icon: Archive,
    names: ["s3", "buckets", "bucket", "storage"],
    description: "Object storage, every region",
    global: true,
  },
  {
    kind: "ebs",
    label: "EBS volumes",
    icon: HardDrive,
    names: ["ebs", "volumes", "volume", "disks", "disk"],
    description: "Block storage for instances",
  },
  {
    kind: "rds",
    label: "RDS databases",
    icon: Database,
    names: ["rds", "databases", "database", "db", "aurora", "postgres", "mysql"],
    description: "Managed SQL databases",
  },
  {
    kind: "msk",
    label: "MSK clusters",
    icon: Workflow,
    names: ["msk", "kafka", "streams"],
    description: "Managed Kafka",
  },
  {
    kind: "vpc",
    label: "VPCs",
    icon: Cloud,
    names: ["vpc", "vpcs", "networks"],
    description: "Virtual private clouds",
  },
  {
    kind: "sg",
    label: "Security groups",
    icon: Shield,
    names: ["sg", "securitygroups", "security-groups", "firewall"],
    description: "Instance firewalls",
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
  {
    kind: "route53",
    label: "Route 53 zones",
    icon: Globe,
    names: ["route53", "r53", "dns", "zones", "hostedzones"],
    description: "DNS hosted zones",
    global: true,
  },
  {
    kind: "cost",
    label: "Cost overview",
    icon: Wallet,
    names: ["cost", "costs", "billing", "spend", "bill"],
    description: "Spend, forecast and top services",
    global: true,
  },
];

/** The sidebar. Number keys 1–9 follow this order. */
export const GROUPS: { label: string; kinds: View[] }[] = [
  { label: "Compute", kinds: ["ec2"] },
  { label: "Containers", kinds: ["eks"] },
  { label: "Storage", kinds: ["s3", "ebs"] },
  { label: "Databases", kinds: ["rds"] },
  { label: "Analytics", kinds: ["msk"] },
  { label: "Networking", kinds: ["vpc", "sg", "elb", "route53"] },
  { label: "Billing", kinds: ["cost"] },
];

export const ORDER: View[] = GROUPS.flatMap((g) => g.kinds);

/** The resource lists, in sidebar order. */
export const LISTS = ORDER.filter((k): k is AwsKind => k !== "cost");

export const serviceOf = (kind: View) =>
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
