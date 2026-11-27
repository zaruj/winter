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

export interface AwsKinds {
  ec2: Ec2Instance;
  eks: EksCluster;
  elb: LoadBalancer;
}
export type AwsKind = keyof AwsKinds;

const COMMANDS: Record<AwsKind, string> = {
  ec2: "aws_ec2_instances",
  eks: "aws_eks_clusters",
  elb: "aws_load_balancers",
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

export interface Ec2InstanceDetail extends Ec2Instance {
  ami: string | null;
  amiName: string | null;
  subnet: string | null;
  privateDns: string | null;
  publicDns: string | null;
  securityGroups: { id: string; name: string }[];
  iamProfile: string | null;
  rootDevice: string | null;
  rootDeviceType: string | null;
  volumes: {
    device: string;
    id: string;
    sizeGib: number | null;
    type: string | null;
    iops: number | null;
    encrypted: boolean | null;
    deleteOnTermination: boolean | null;
  }[];
  interfaces: {
    id: string;
    privateIp: string | null;
    publicIp: string | null;
    subnet: string | null;
    mac: string | null;
  }[];
  tags: [string, string][];
  monitoring: string | null;
  tenancy: string | null;
  ebsOptimized: boolean | null;
  stateReason: string | null;
  cpuCores: number | null;
  threadsPerCore: number | null;
  /** `required` means IMDSv2 only. */
  imdsTokens: string | null;
  virtualization: string | null;
  terminationProtection: boolean | null;
  systemStatus: string | null;
  instanceStatus: string | null;
}

export function getEc2Instance(
  profile: string,
  region: string,
  id: string,
): Promise<Ec2InstanceDetail> {
  return invoke("aws_ec2_instance", { profile, region, id });
}
