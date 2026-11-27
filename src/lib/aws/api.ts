import { Channel, invoke } from "@tauri-apps/api/core";

// Typed wrappers for the AWS commands in src-tauri/src/commands.rs.
// Commands reject with an error message string.

export interface ProfileInfo {
  name: string;
  region: string | null;
  /** How the profile signs in: `keys`, `sso`, `role`, `process` or `unknown`. */
  auth: string;
  /** Named by `AWS_PROFILE` (or `default` when unset). */
  current: boolean;
  /** Last opened in winter, ms since the epoch (from ~/.config/winter/state.json). */
  lastUsed: number | null;
  /** Region it was last opened in. */
  lastRegion: string | null;
}

export interface Identity {
  profile: string;
  region: string;
  account: string;
  arn: string;
}

export interface Ec2Instance {
  id: string;
  /** The `Name` tag; empty when untagged. */
  name: string;
  state: string;
  type: string;
  zone: string | null;
  privateIp: string | null;
  publicIp: string | null;
  vpc: string | null;
  platform: string | null;
  arch: string | null;
  keyName: string | null;
  launched: string | null;
}

export interface EksCluster {
  name: string;
  status: string;
  version: string | null;
  platformVersion: string | null;
  endpoint: string | null;
  arn: string | null;
  vpc: string | null;
  created: string | null;
}

export interface LoadBalancer {
  name: string;
  /** `application`, `network`, `gateway` or `classic`. */
  type: string;
  scheme: string | null;
  state: string | null;
  dnsName: string | null;
  vpc: string | null;
  zones: string[];
  created: string | null;
  /** ELBv2 only. */
  arn: string | null;
}

export interface Vpc {
  id: string;
  /** The `Name` tag; empty when untagged. */
  name: string;
  state: string;
  /** IPv4 blocks, the primary one first. */
  cidrs: string[];
  ipv6Cidrs: string[];
  isDefault: boolean;
  tenancy: string | null;
  owner: string | null;
}

export interface SecurityGroup {
  id: string;
  name: string;
  description: string | null;
  vpc: string | null;
  inbound: number;
  outbound: number;
}

export interface EbsVolume {
  id: string;
  /** The `Name` tag; empty when untagged. */
  name: string;
  state: string;
  sizeGib: number | null;
  type: string | null;
  iops: number | null;
  throughput: number | null;
  encrypted: boolean;
  zone: string | null;
  instance: string | null;
  device: string | null;
  created: string | null;
}

export interface Bucket {
  name: string;
  region: string | null;
  created: string | null;
}

export interface HostedZone {
  id: string;
  name: string;
  private: boolean;
  records: number | null;
  comment: string | null;
}

export interface MskCluster {
  name: string;
  arn: string | null;
  /** `PROVISIONED` or `SERVERLESS`. */
  type: string;
  state: string;
  kafkaVersion: string | null;
  brokers: number | null;
  instanceType: string | null;
  created: string | null;
}

export interface DbInstance {
  /** The DB instance identifier. */
  name: string;
  engine: string | null;
  version: string | null;
  class: string | null;
  status: string;
  /** `host:port`. */
  endpoint: string | null;
  multiAz: boolean;
  storageGib: number | null;
  storageType: string | null;
  zone: string | null;
  /** Aurora cluster it belongs to. */
  cluster: string | null;
  public: boolean;
  created: string | null;
}

/** Used when a profile names no region (matches the Rust side). */
export const FALLBACK_REGION = "us-east-1";

export function listProfiles(): Promise<ProfileInfo[]> {
  return invoke("aws_profiles");
}

/** Checks the credentials; `fresh` reloads them first (e.g. after `aws sso login`). */
export function connect(
  profile: string,
  region: string,
  fresh = false,
): Promise<Identity> {
  return invoke("aws_connect", { profile, region, fresh });
}

export function listRegions(
  profile: string,
  region: string,
): Promise<string[]> {
  return invoke("aws_regions", { profile, region });
}

/** Resources the account page lists, by service. */
export interface AwsKinds {
  ec2: Ec2Instance;
  eks: EksCluster;
  s3: Bucket;
  ebs: EbsVolume;
  rds: DbInstance;
  msk: MskCluster;
  vpc: Vpc;
  sg: SecurityGroup;
  elb: LoadBalancer;
  route53: HostedZone;
}
export type AwsKind = keyof AwsKinds;

const COMMANDS: Record<AwsKind, string> = {
  ec2: "aws_ec2_instances",
  eks: "aws_eks_clusters",
  s3: "aws_s3_buckets",
  ebs: "aws_ebs_volumes",
  rds: "aws_rds_instances",
  msk: "aws_msk_clusters",
  vpc: "aws_vpcs",
  sg: "aws_security_groups",
  elb: "aws_load_balancers",
  route53: "aws_hosted_zones",
};

/** Streams a resource list page by page; resolves with the total. */
export function streamAws<K extends AwsKind>(
  kind: K,
  profile: string,
  region: string,
  onPage: (items: AwsKinds[K][]) => void,
): Promise<number> {
  return invoke(COMMANDS[kind], {
    profile,
    region,
    onPage: new Channel<AwsKinds[K][]>(onPage),
  });
}


export interface CostOverview {
  currency: string;
  /** `YYYY-MM-DD`. */
  monthStart: string;
  /** Today in UTC, `YYYY-MM-DD`. */
  today: string;
  monthToDate: number;
  /** Last month up to the same day of the month. */
  lastMonthToDate: number;
  lastMonth: number;
  /** Month-end estimate; null when Cost Explorer cannot forecast yet. */
  forecast: number | null;
  /** From the start of last month to today. */
  daily: { date: string; amount: number; estimated: boolean }[];
  /** Most expensive this month first. */
  services: { name: string; thisMonth: number; lastMonth: number }[];
}

/** Cost Explorer bills each call (three requests), so call it only when asked. */
export function getCostOverview(profile: string): Promise<CostOverview> {
  return invoke("aws_cost_overview", { profile });
}

/** A details view: titled facts, tables and tags (rendered by Sections.svelte). */
export interface DetailSection {
  title: string;
  facts: [string, string][];
  table: { columns: string[]; rows: string[][] } | null;
  chips: [string, string][];
}

/**
 * One resource in detail. `id` identifies it in its list (see `detailId` in Columns.svelte);
 * for S3, `region` is the bucket's.
 */
export function getDetail(
  profile: string,
  region: string,
  kind: AwsKind,
  id: string,
): Promise<DetailSection[]> {
  return invoke("aws_detail", { profile, region, kind, id });
}
