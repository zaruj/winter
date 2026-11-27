//! One resource of any listed service in detail: what its console page shows first, gathered from
//! the calls that service needs (an ELB's listeners and target health, a VPC's subnets, a
//! security group's rules, ...). The result is a list of sections — facts, a table, tags — that
//! the GUI renders the same way for every service, as it does for Kubernetes objects.
//!
//! The resource's own description must succeed; the extra calls are best effort, so a missing
//! permission leaves a section out instead of failing the whole view.

use std::collections::HashMap;

use aws_sdk_ec2::types::Filter;
use futures::future::join_all;
use serde::Serialize;

use super::resources::time;
use super::{Aws, Ec2InstanceDetail, aws_error};
use crate::{Error, Result};

/// Route 53 records shown; big zones are cut here.
const MAX_RECORDS: usize = 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub title: String,
    /// Label and value; empty values are left out.
    pub facts: Vec<(String, String)>,
    pub table: Option<Table>,
    /// key=value chips, e.g. tags.
    pub chips: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// A value that may be shown as a fact.
pub trait Fact {
    fn text(self) -> Option<String>;
}
impl Fact for &str {
    fn text(self) -> Option<String> {
        (!self.is_empty()).then(|| self.to_owned())
    }
}
impl Fact for String {
    fn text(self) -> Option<String> {
        (!self.is_empty()).then_some(self)
    }
}
impl Fact for Option<&str> {
    fn text(self) -> Option<String> {
        self.and_then(Fact::text)
    }
}
impl Fact for Option<String> {
    fn text(self) -> Option<String> {
        self.and_then(Fact::text)
    }
}
impl Fact for Option<i32> {
    fn text(self) -> Option<String> {
        self.map(|n| n.to_string())
    }
}
impl Fact for Option<i64> {
    fn text(self) -> Option<String> {
        self.map(|n| n.to_string())
    }
}
impl Fact for Option<bool> {
    fn text(self) -> Option<String> {
        self.map(|b| if b { "yes" } else { "no" }.to_owned())
    }
}
impl Fact for bool {
    fn text(self) -> Option<String> {
        Some(self).text()
    }
}

impl Section {
    fn new(title: &str) -> Self {
        Self {
            title: title.to_owned(),
            ..Self::default()
        }
    }

    fn fact(mut self, label: &str, value: impl Fact) -> Self {
        if let Some(value) = value.text() {
            self.facts.push((label.to_owned(), value));
        }
        self
    }

    fn table(mut self, columns: &[&str], rows: Vec<Vec<String>>) -> Self {
        self.table = Some(Table {
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            rows,
        });
        self
    }

    fn chips(mut self, mut chips: Vec<(String, String)>) -> Self {
        chips.sort();
        self.chips = chips;
        self
    }
}

fn tags<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<Section> {
    let chips: Vec<(String, String)> = pairs
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    (!chips.is_empty()).then(|| Section::new("Tags").chips(chips))
}

fn ec2_tags(t: &[aws_sdk_ec2::types::Tag]) -> Option<Section> {
    tags(
        t.iter()
            .map(|t| (t.key().unwrap_or_default(), t.value().unwrap_or_default())),
    )
}

fn map_tags(t: Option<&HashMap<String, String>>) -> Option<Section> {
    tags(
        t.into_iter()
            .flatten()
            .map(|(k, v)| (k.as_str(), v.as_str())),
    )
}

fn joined(items: &[String]) -> String {
    items.join(", ")
}

fn s(v: Option<&str>) -> String {
    v.unwrap_or_default().to_owned()
}

fn n<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

fn yes(v: Option<bool>) -> String {
    v.text().unwrap_or_default()
}

fn not_found(what: &str, id: &str) -> Error {
    Error::Aws(format!("{what} {id} not found"))
}

/// Ports of a security group rule: `443`, `8000-8080` or `all`.
fn ports(p: &aws_sdk_ec2::types::IpPermission) -> String {
    match (p.from_port(), p.to_port()) {
        (Some(from), Some(to)) if from == to && from >= 0 => from.to_string(),
        (Some(from), Some(to)) if from >= 0 && to >= 0 => format!("{from}-{to}"),
        _ => "all".into(),
    }
}

/// One row per source (or destination) of each rule, as the console lists them.
fn rules(permissions: &[aws_sdk_ec2::types::IpPermission]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for p in permissions {
        let protocol = match p.ip_protocol().unwrap_or("-1") {
            "-1" => "all".to_owned(),
            other => other.to_owned(),
        };
        let mut push = |peer: String, description: Option<&str>| {
            rows.push(vec![protocol.clone(), ports(p), peer, s(description)]);
        };
        for r in p.ip_ranges() {
            push(s(r.cidr_ip()), r.description());
        }
        for r in p.ipv6_ranges() {
            push(s(r.cidr_ipv6()), r.description());
        }
        for r in p.prefix_list_ids() {
            push(s(r.prefix_list_id()), r.description());
        }
        for g in p.user_id_group_pairs() {
            let name = g
                .group_name()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default();
            push(format!("{}{name}", s(g.group_id())), g.description());
        }
    }
    rows
}

impl Aws {
    /// Details of one resource. `kind` is the list it came from (`ec2`, `eks`, `s3`, `ebs`, `rds`,
    /// `msk`, `vpc`, `sg`, `elb`, `route53`); `id` is what identifies it there (instance ID,
    /// cluster name, bucket, ARN for MSK and ELBv2, `classic/<name>` for a classic ELB, ...).
    /// For S3, `region` is the bucket's own.
    pub async fn detail(
        &self,
        profile: &str,
        region: &str,
        kind: &str,
        id: &str,
    ) -> Result<Vec<Section>> {
        let sections = match kind {
            "ec2" => ec2(self.ec2_instance(profile, region, id).await?),
            "eks" => self.eks_detail(profile, region, id).await?,
            "s3" => self.s3_detail(profile, region, id).await?,
            "ebs" => self.ebs_detail(profile, region, id).await?,
            "rds" => self.rds_detail(profile, region, id).await?,
            "msk" => self.msk_detail(profile, region, id).await?,
            "vpc" => self.vpc_detail(profile, region, id).await?,
            "sg" => self.sg_detail(profile, region, id).await?,
            "elb" => self.elb_detail(profile, region, id).await?,
            "route53" => self.zone_detail(profile, region, id).await?,
            other => return Err(Error::Aws(format!("no details for {other}"))),
        };
        Ok(sections
            .into_iter()
            .filter(|s| !s.facts.is_empty() || s.table.is_some() || !s.chips.is_empty())
            .collect())
    }

    async fn eks_detail(&self, profile: &str, region: &str, name: &str) -> Result<Vec<Section>> {
        let eks = aws_sdk_eks::Client::new(&self.config(profile, region).await);
        let out = eks
            .describe_cluster()
            .name(name)
            .send()
            .await
            .map_err(aws_error)?;
        let c = out.cluster().ok_or_else(|| not_found("cluster", name))?;
        let vpc = c.resources_vpc_config();
        let logs: Vec<String> = c
            .logging()
            .map(|l| l.cluster_logging())
            .unwrap_or_default()
            .iter()
            .filter(|l| l.enabled() == Some(true))
            .flat_map(|l| l.types().iter().map(|t| t.as_str().to_owned()))
            .collect();

        // Node groups and add-ons, each described in parallel; best effort.
        let groups = async {
            let names = eks
                .list_nodegroups()
                .cluster_name(name)
                .send()
                .await
                .map(|o| o.nodegroups().to_vec())
                .unwrap_or_default();
            join_all(names.iter().map(|g| {
                eks.describe_nodegroup()
                    .cluster_name(name)
                    .nodegroup_name(g)
                    .send()
            }))
            .await
            .into_iter()
            .filter_map(|r| r.ok()?.nodegroup().cloned())
            .collect::<Vec<_>>()
        };
        let addons = async {
            let names = eks
                .list_addons()
                .cluster_name(name)
                .send()
                .await
                .map(|o| o.addons().to_vec())
                .unwrap_or_default();
            join_all(
                names
                    .iter()
                    .map(|a| eks.describe_addon().cluster_name(name).addon_name(a).send()),
            )
            .await
            .into_iter()
            .filter_map(|r| r.ok()?.addon().cloned())
            .collect::<Vec<_>>()
        };
        let (groups, addons) = tokio::join!(groups, addons);

        let mut sections = vec![
            Section::new("Cluster")
                .fact("Status", c.status().map(|s| s.as_str()))
                .fact("Version", c.version())
                .fact("Platform", c.platform_version())
                .fact("Endpoint", c.endpoint())
                .fact(
                    "Endpoint access",
                    vpc.map(
                        |v| match (v.endpoint_public_access(), v.endpoint_private_access()) {
                            (true, true) => "public and private",
                            (true, false) => "public",
                            (false, true) => "private",
                            (false, false) => "none",
                        },
                    ),
                )
                .fact("Public CIDRs", vpc.map(|v| joined(v.public_access_cidrs())))
                .fact(
                    "OIDC issuer",
                    c.identity().and_then(|i| i.oidc()).and_then(|o| o.issuer()),
                )
                .fact(
                    "Service CIDR",
                    c.kubernetes_network_config()
                        .and_then(|k| k.service_ipv4_cidr()),
                )
                .fact("Logging", logs.join(", "))
                .fact("Role", c.role_arn())
                .fact("Created", time(c.created_at())),
            Section::new("Network")
                .fact("VPC", vpc.and_then(|v| v.vpc_id()))
                .fact("Subnets", vpc.map(|v| joined(v.subnet_ids())))
                .fact(
                    "Security groups",
                    vpc.map(|v| joined(v.security_group_ids())),
                )
                .fact(
                    "Cluster security group",
                    vpc.and_then(|v| v.cluster_security_group_id()),
                ),
        ];
        if !groups.is_empty() {
            sections.push(
                Section::new("Node groups").table(
                    &[
                        "Name",
                        "Status",
                        "Instances",
                        "Capacity",
                        "Desired / min / max",
                        "AMI",
                        "Version",
                    ],
                    groups
                        .iter()
                        .map(|g| {
                            let sc = g.scaling_config();
                            vec![
                                s(g.nodegroup_name()),
                                g.status()
                                    .map(|s| s.as_str().to_owned())
                                    .unwrap_or_default(),
                                joined(g.instance_types()),
                                g.capacity_type()
                                    .map(|c| c.as_str().to_owned())
                                    .unwrap_or_default(),
                                format!(
                                    "{} / {} / {}",
                                    n(sc.and_then(|s| s.desired_size())),
                                    n(sc.and_then(|s| s.min_size())),
                                    n(sc.and_then(|s| s.max_size()))
                                ),
                                g.ami_type()
                                    .map(|a| a.as_str().to_owned())
                                    .unwrap_or_default(),
                                s(g.release_version().or(g.version())),
                            ]
                        })
                        .collect(),
                ),
            );
        }
        if !addons.is_empty() {
            sections.push(
                Section::new("Add-ons").table(
                    &["Name", "Version", "Status"],
                    addons
                        .iter()
                        .map(|a| {
                            vec![
                                s(a.addon_name()),
                                s(a.addon_version()),
                                a.status()
                                    .map(|s| s.as_str().to_owned())
                                    .unwrap_or_default(),
                            ]
                        })
                        .collect(),
                ),
            );
        }
        sections.extend(map_tags(c.tags()));
        Ok(sections)
    }

    /// `region` must be the bucket's, or S3 answers with a redirect.
    async fn s3_detail(&self, profile: &str, region: &str, bucket: &str) -> Result<Vec<Section>> {
        let s3 = aws_sdk_s3::Client::new(&self.config(profile, region).await);
        // The bucket must be reachable; the rest is best effort.
        s3.head_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(aws_error)?;
        let (versioning, encryption, block, policy, tagging, lifecycle) = tokio::join!(
            s3.get_bucket_versioning().bucket(bucket).send(),
            s3.get_bucket_encryption().bucket(bucket).send(),
            s3.get_public_access_block().bucket(bucket).send(),
            s3.get_bucket_policy_status().bucket(bucket).send(),
            s3.get_bucket_tagging().bucket(bucket).send(),
            s3.get_bucket_lifecycle_configuration()
                .bucket(bucket)
                .send(),
        );
        let encryption = encryption.ok().and_then(|o| {
            let rule = o
                .server_side_encryption_configuration()?
                .rules()
                .first()?
                .clone();
            let default = rule.apply_server_side_encryption_by_default()?;
            Some(match default.kms_master_key_id() {
                Some(key) => format!("{} ({key})", default.sse_algorithm().as_str()),
                None => default.sse_algorithm().as_str().to_owned(),
            })
        });
        let block = match block {
            Ok(o) => o.public_access_block_configuration().map(|b| {
                let all = [
                    b.block_public_acls(),
                    b.ignore_public_acls(),
                    b.block_public_policy(),
                    b.restrict_public_buckets(),
                ];
                if all.iter().all(|v| *v == Some(true)) {
                    "all public access blocked".to_owned()
                } else {
                    format!(
                        "ACLs {}/{}, policy {}/{}",
                        yes(b.block_public_acls()),
                        yes(b.ignore_public_acls()),
                        yes(b.block_public_policy()),
                        yes(b.restrict_public_buckets())
                    )
                }
            }),
            Err(_) => Some("not set (public access possible)".into()),
        };
        let mut sections = vec![
            Section::new("Bucket")
                .fact("Region", region)
                .fact(
                    "Versioning",
                    versioning.ok().map(|o| {
                        o.status()
                            .map_or("never enabled".to_owned(), |s| s.as_str().to_owned())
                    }),
                )
                .fact("Encryption", encryption)
                .fact("Block public access", block)
                .fact(
                    "Policy makes it public",
                    policy
                        .ok()
                        .and_then(|o| o.policy_status()?.is_public())
                        .or(Some(false)),
                )
                .fact(
                    "Lifecycle rules",
                    Some(
                        lifecycle
                            .map(|o| o.rules().len())
                            .unwrap_or_default()
                            .to_string(),
                    ),
                ),
        ];
        if let Ok(t) = tagging {
            sections.extend(tags(t.tag_set().iter().map(|t| (t.key(), t.value()))));
        }
        Ok(sections)
    }

    async fn ebs_detail(&self, profile: &str, region: &str, id: &str) -> Result<Vec<Section>> {
        let ec2 = aws_sdk_ec2::Client::new(&self.config(profile, region).await);
        let out = ec2
            .describe_volumes()
            .volume_ids(id)
            .send()
            .await
            .map_err(aws_error)?;
        let v = out
            .volumes()
            .first()
            .ok_or_else(|| not_found("volume", id))?;
        let mut sections = vec![
            Section::new("Volume")
                .fact("State", v.state().map(|s| s.as_str()))
                .fact("Size", v.size().map(|s| format!("{s} GiB")))
                .fact("Type", v.volume_type().map(|t| t.as_str()))
                .fact("IOPS", v.iops())
                .fact("Throughput", v.throughput().map(|t| format!("{t} MiB/s")))
                .fact("Encrypted", v.encrypted())
                .fact("KMS key", v.kms_key_id())
                .fact("Zone", v.availability_zone())
                .fact("Snapshot", v.snapshot_id())
                .fact("Multi-attach", v.multi_attach_enabled())
                .fact("Created", time(v.create_time())),
        ];
        if !v.attachments().is_empty() {
            sections.push(
                Section::new("Attachments").table(
                    &["Instance", "Device", "State", "Delete on termination"],
                    v.attachments()
                        .iter()
                        .map(|a| {
                            vec![
                                s(a.instance_id()),
                                s(a.device()),
                                a.state().map(|s| s.as_str().to_owned()).unwrap_or_default(),
                                yes(a.delete_on_termination()),
                            ]
                        })
                        .collect(),
                ),
            );
        }
        sections.extend(ec2_tags(v.tags()));
        Ok(sections)
    }

    async fn rds_detail(&self, profile: &str, region: &str, id: &str) -> Result<Vec<Section>> {
        let rds = aws_sdk_rds::Client::new(&self.config(profile, region).await);
        let out = rds
            .describe_db_instances()
            .db_instance_identifier(id)
            .send()
            .await
            .map_err(aws_error)?;
        let d = out
            .db_instances()
            .first()
            .ok_or_else(|| not_found("database", id))?;
        let endpoint = d.endpoint().map(|e| match e.port() {
            Some(p) => format!("{}:{p}", s(e.address())),
            None => s(e.address()),
        });
        let mut sections = vec![
            Section::new("Database")
                .fact("Status", d.db_instance_status())
                .fact(
                    "Engine",
                    format!("{} {}", s(d.engine()), s(d.engine_version()))
                        .trim()
                        .to_owned(),
                )
                .fact("Class", d.db_instance_class())
                .fact("Endpoint", endpoint)
                .fact("Database name", d.db_name())
                .fact("Master user", d.master_username())
                .fact("Aurora cluster", d.db_cluster_identifier())
                .fact("Multi-AZ", d.multi_az())
                .fact("Zone", d.availability_zone())
                .fact("Secondary zone", d.secondary_availability_zone())
                .fact("Created", time(d.instance_create_time())),
            Section::new("Storage")
                .fact(
                    "Allocated",
                    d.allocated_storage()
                        .filter(|&g| g > 0)
                        .map(|g| format!("{g} GiB")),
                )
                .fact(
                    "Autoscaling up to",
                    d.max_allocated_storage().map(|g| format!("{g} GiB")),
                )
                .fact("Type", d.storage_type())
                .fact("IOPS", d.iops())
                .fact(
                    "Throughput",
                    d.storage_throughput().map(|t| format!("{t} MiB/s")),
                )
                .fact("Encrypted", d.storage_encrypted())
                .fact("KMS key", d.kms_key_id()),
            Section::new("Network and security")
                .fact("Publicly accessible", d.publicly_accessible())
                .fact("VPC", d.db_subnet_group().and_then(|g| g.vpc_id()))
                .fact(
                    "Subnet group",
                    d.db_subnet_group().and_then(|g| g.db_subnet_group_name()),
                )
                .fact(
                    "Security groups",
                    d.vpc_security_groups()
                        .iter()
                        .filter_map(|g| g.vpc_security_group_id())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .fact(
                    "IAM authentication",
                    d.iam_database_authentication_enabled(),
                )
                .fact("CA certificate", d.ca_certificate_identifier())
                .fact("Deletion protection", d.deletion_protection()),
            Section::new("Maintenance and backups")
                .fact(
                    "Backup retention",
                    d.backup_retention_period().map(|n| format!("{n} days")),
                )
                .fact("Backup window", d.preferred_backup_window())
                .fact("Maintenance window", d.preferred_maintenance_window())
                .fact("Auto minor upgrade", d.auto_minor_version_upgrade())
                .fact(
                    "Parameter groups",
                    d.db_parameter_groups()
                        .iter()
                        .filter_map(|g| g.db_parameter_group_name())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .fact("Performance Insights", d.performance_insights_enabled())
                .fact(
                    "Enhanced monitoring",
                    d.monitoring_interval()
                        .filter(|&i| i > 0)
                        .map(|i| format!("every {i}s")),
                ),
        ];
        sections.extend(tags(
            d.tag_list()
                .iter()
                .map(|t| (t.key().unwrap_or_default(), t.value().unwrap_or_default())),
        ));
        Ok(sections)
    }

    async fn msk_detail(&self, profile: &str, region: &str, arn: &str) -> Result<Vec<Section>> {
        let kafka = aws_sdk_kafka::Client::new(&self.config(profile, region).await);
        let (cluster, brokers) = tokio::join!(
            kafka.describe_cluster_v2().cluster_arn(arn).send(),
            kafka.get_bootstrap_brokers().cluster_arn(arn).send(),
        );
        let cluster = cluster.map_err(aws_error)?;
        let c = cluster
            .cluster_info()
            .ok_or_else(|| not_found("cluster", arn))?;
        let p = c.provisioned();
        let nodes = p.and_then(|p| p.broker_node_group_info());
        let mut sections = vec![
            Section::new("Cluster")
                .fact("State", c.state().map(|s| s.as_str()))
                .fact("Type", c.cluster_type().map(|t| t.as_str()))
                .fact(
                    "Kafka version",
                    p.and_then(|p| p.current_broker_software_info())
                        .and_then(|s| s.kafka_version()),
                )
                .fact("Brokers", p.and_then(|p| p.number_of_broker_nodes()))
                .fact("Broker type", nodes.and_then(|b| b.instance_type()))
                .fact(
                    "Storage per broker",
                    nodes
                        .and_then(|b| b.storage_info())
                        .and_then(|s| s.ebs_storage_info())
                        .and_then(|e| e.volume_size())
                        .map(|g| format!("{g} GiB")),
                )
                .fact("Subnets", nodes.map(|b| joined(b.client_subnets())))
                .fact(
                    "Security groups",
                    nodes.map(|b| joined(b.security_groups())),
                )
                .fact(
                    "Client to broker",
                    p.and_then(|p| p.encryption_info())
                        .and_then(|e| e.encryption_in_transit())
                        .and_then(|t| t.client_broker())
                        .map(|c| c.as_str()),
                )
                .fact(
                    "Serverless subnets",
                    c.serverless().map(|s| {
                        s.vpc_configs()
                            .iter()
                            .flat_map(|v| v.subnet_ids().iter().cloned())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }),
                )
                .fact("ARN", c.cluster_arn())
                .fact("Created", time(c.creation_time())),
        ];
        if let Ok(b) = brokers {
            sections.push(
                Section::new("Bootstrap brokers")
                    .fact("Plaintext", b.bootstrap_broker_string())
                    .fact("TLS", b.bootstrap_broker_string_tls())
                    .fact("SASL/SCRAM", b.bootstrap_broker_string_sasl_scram())
                    .fact("IAM", b.bootstrap_broker_string_sasl_iam())
                    .fact("Public TLS", b.bootstrap_broker_string_public_tls())
                    .fact(
                        "Public SASL/SCRAM",
                        b.bootstrap_broker_string_public_sasl_scram(),
                    )
                    .fact("Public IAM", b.bootstrap_broker_string_public_sasl_iam()),
            );
        }
        sections.extend(map_tags(c.tags()));
        Ok(sections)
    }

    async fn vpc_detail(&self, profile: &str, region: &str, id: &str) -> Result<Vec<Section>> {
        let ec2 = aws_sdk_ec2::Client::new(&self.config(profile, region).await);
        let in_vpc = |name: &str| Filter::builder().name(name).values(id).build();
        let (vpcs, subnets, nats, gateways, routes) = tokio::join!(
            ec2.describe_vpcs().vpc_ids(id).send(),
            ec2.describe_subnets().filters(in_vpc("vpc-id")).send(),
            ec2.describe_nat_gateways().filter(in_vpc("vpc-id")).send(),
            ec2.describe_internet_gateways()
                .filters(in_vpc("attachment.vpc-id"))
                .send(),
            ec2.describe_route_tables().filters(in_vpc("vpc-id")).send(),
        );
        let vpcs = vpcs.map_err(aws_error)?;
        let v = vpcs.vpcs().first().ok_or_else(|| not_found("VPC", id))?;
        let summary = super::Vpc::from(v);
        let mut sections = vec![
            Section::new("VPC")
                .fact("State", summary.state.as_str())
                .fact("IPv4 CIDR", joined(&summary.cidrs))
                .fact("IPv6 CIDR", joined(&summary.ipv6_cidrs))
                .fact("Default", summary.is_default)
                .fact("Tenancy", summary.tenancy)
                .fact("DHCP options", v.dhcp_options_id())
                .fact("Owner", summary.owner)
                .fact(
                    "Internet gateway",
                    gateways.ok().map(|g| {
                        g.internet_gateways()
                            .iter()
                            .filter_map(|g| g.internet_gateway_id())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }),
                )
                .fact(
                    "Route tables",
                    routes.ok().map(|r| r.route_tables().len().to_string()),
                ),
        ];
        if let Ok(subnets) = subnets {
            let mut list = subnets.subnets().to_vec();
            list.sort_by(|a, b| a.availability_zone().cmp(&b.availability_zone()));
            sections.push(
                Section::new("Subnets").table(
                    &[
                        "Name",
                        "ID",
                        "Zone",
                        "CIDR",
                        "Free IPs",
                        "Public IP on launch",
                    ],
                    list.iter()
                        .map(|s_| {
                            vec![
                                s_.tags()
                                    .iter()
                                    .find(|t| t.key() == Some("Name"))
                                    .and_then(|t| t.value())
                                    .unwrap_or_default()
                                    .to_owned(),
                                s(s_.subnet_id()),
                                s(s_.availability_zone()),
                                s(s_.cidr_block()),
                                n(s_.available_ip_address_count()),
                                yes(s_.map_public_ip_on_launch()),
                            ]
                        })
                        .collect(),
                ),
            );
        }
        if let Ok(nats) = nats
            && !nats.nat_gateways().is_empty()
        {
            sections.push(
                Section::new("NAT gateways").table(
                    &["ID", "State", "Subnet", "Public IP", "Type"],
                    nats.nat_gateways()
                        .iter()
                        .map(|g| {
                            vec![
                                s(g.nat_gateway_id()),
                                g.state().map(|s| s.as_str().to_owned()).unwrap_or_default(),
                                s(g.subnet_id()),
                                g.nat_gateway_addresses()
                                    .iter()
                                    .filter_map(|a| a.public_ip())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                g.connectivity_type()
                                    .map(|c| c.as_str().to_owned())
                                    .unwrap_or_default(),
                            ]
                        })
                        .collect(),
                ),
            );
        }
        sections.extend(ec2_tags(v.tags()));
        Ok(sections)
    }

    async fn sg_detail(&self, profile: &str, region: &str, id: &str) -> Result<Vec<Section>> {
        let ec2 = aws_sdk_ec2::Client::new(&self.config(profile, region).await);
        let out = ec2
            .describe_security_groups()
            .group_ids(id)
            .send()
            .await
            .map_err(aws_error)?;
        let g = out
            .security_groups()
            .first()
            .ok_or_else(|| not_found("security group", id))?;
        let columns = |peer: &'static str| ["Protocol", "Ports", peer, "Description"];
        let mut sections = vec![
            Section::new("Security group")
                .fact("Name", g.group_name())
                .fact("Description", g.description())
                .fact("VPC", g.vpc_id())
                .fact("Owner", g.owner_id()),
            Section::new("Inbound rules").table(&columns("Source"), rules(g.ip_permissions())),
            Section::new("Outbound rules")
                .table(&columns("Destination"), rules(g.ip_permissions_egress())),
        ];
        sections.extend(ec2_tags(g.tags()));
        Ok(sections)
    }

    /// `id` is an ELBv2 ARN, or `classic/<name>`.
    async fn elb_detail(&self, profile: &str, region: &str, id: &str) -> Result<Vec<Section>> {
        let config = self.config(profile, region).await;
        if let Some(name) = id.strip_prefix("classic/") {
            return classic_detail(&aws_sdk_elasticloadbalancing::Client::new(&config), name).await;
        }
        let elb = aws_sdk_elasticloadbalancingv2::Client::new(&config);
        let (lbs, listeners, groups) = tokio::join!(
            elb.describe_load_balancers().load_balancer_arns(id).send(),
            elb.describe_listeners().load_balancer_arn(id).send(),
            elb.describe_target_groups().load_balancer_arn(id).send(),
        );
        let lbs = lbs.map_err(aws_error)?;
        let lb = lbs
            .load_balancers()
            .first()
            .ok_or_else(|| not_found("load balancer", id))?;
        let groups = groups
            .map(|g| g.target_groups().to_vec())
            .unwrap_or_default();
        let group_name = |arn: &str| {
            groups
                .iter()
                .find(|g| g.target_group_arn() == Some(arn))
                .and_then(|g| g.target_group_name())
                .unwrap_or(arn)
                .to_owned()
        };
        let health = join_all(groups.iter().map(|g| {
            elb.describe_target_health()
                .set_target_group_arn(g.target_group_arn().map(str::to_owned))
                .send()
        }))
        .await;

        let mut sections = vec![
            Section::new("Load balancer")
                .fact("Type", lb.r#type().map(|t| t.as_str()))
                .fact("Scheme", lb.scheme().map(|s| s.as_str()))
                .fact(
                    "State",
                    lb.state().and_then(|s| s.code()).map(|c| c.as_str()),
                )
                .fact("DNS name", lb.dns_name())
                .fact("VPC", lb.vpc_id())
                .fact(
                    "Zones",
                    lb.availability_zones()
                        .iter()
                        .map(|z| format!("{} ({})", s(z.zone_name()), s(z.subnet_id())))
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .fact("Security groups", joined(lb.security_groups()))
                .fact("IP type", lb.ip_address_type().map(|t| t.as_str()))
                .fact("Created", time(lb.created_time())),
        ];
        if let Ok(listeners) = listeners {
            sections.push(
                Section::new("Listeners").table(
                    &["Port", "Protocol", "Default action", "Certificates"],
                    listeners
                        .listeners()
                        .iter()
                        .map(|l| {
                            let actions = l
                                .default_actions()
                                .iter()
                                .map(|a| {
                                    let kind = a.r#type().map(|t| t.as_str()).unwrap_or("?");
                                    let targets: Vec<String> = a
                                        .forward_config()
                                        .map(|f| f.target_groups())
                                        .unwrap_or_default()
                                        .iter()
                                        .filter_map(|t| t.target_group_arn())
                                        .chain(a.target_group_arn())
                                        .map(group_name)
                                        .collect();
                                    if let Some(r) = a.redirect_config() {
                                        format!("redirect to {}:{}", s(r.protocol()), s(r.port()))
                                    } else if targets.is_empty() {
                                        kind.to_owned()
                                    } else {
                                        format!("{kind} → {}", targets.join(", "))
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            vec![
                                n(l.port()),
                                l.protocol()
                                    .map(|p| p.as_str().to_owned())
                                    .unwrap_or_default(),
                                actions,
                                l.certificates().len().to_string(),
                            ]
                        })
                        .collect(),
                ),
            );
        }
        if !groups.is_empty() {
            sections.push(Section::new("Target groups").table(
                &["Name", "Protocol / port", "Targets", "Health", "Health check"],
                groups
                    .iter()
                    .zip(&health)
                    .map(|(g, h)| {
                        let (healthy, total, states) = match h {
                            Ok(h) => {
                                let d = h.target_health_descriptions();
                                let state = |t: &aws_sdk_elasticloadbalancingv2::types::TargetHealthDescription| {
                                    t.target_health()
                                        .and_then(|h| h.state())
                                        .map(|s| s.as_str().to_owned())
                                        .unwrap_or_default()
                                };
                                let healthy = d.iter().filter(|t| state(t) == "healthy").count();
                                let mut other: Vec<String> = d
                                    .iter()
                                    .map(state)
                                    .filter(|s| s != "healthy")
                                    .collect();
                                other.sort();
                                other.dedup();
                                (healthy.to_string(), d.len().to_string(), other.join(", "))
                            }
                            Err(_) => ("?".into(), "?".into(), String::new()),
                        };
                        vec![
                            s(g.target_group_name()),
                            format!(
                                "{} {}",
                                g.protocol().map(|p| p.as_str()).unwrap_or_default(),
                                n(g.port())
                            ),
                            format!(
                                "{total} {}",
                                g.target_type().map(|t| t.as_str()).unwrap_or_default()
                            ),
                            if states.is_empty() {
                                format!("{healthy}/{total} healthy")
                            } else {
                                format!("{healthy}/{total} healthy ({states})")
                            },
                            format!(
                                "{} {}",
                                g.health_check_protocol()
                                    .map(|p| p.as_str())
                                    .unwrap_or_default(),
                                s(g.health_check_path())
                            )
                            .trim()
                            .to_owned(),
                        ]
                    })
                    .collect(),
            ));
        }
        Ok(sections)
    }

    async fn zone_detail(&self, profile: &str, region: &str, id: &str) -> Result<Vec<Section>> {
        let route53 = aws_sdk_route53::Client::new(&self.config(profile, region).await);
        let zone = route53
            .get_hosted_zone()
            .id(id)
            .send()
            .await
            .map_err(aws_error)?;
        let z = zone.hosted_zone().ok_or_else(|| not_found("zone", id))?;

        let mut records = Vec::new();
        let mut start: Option<(String, aws_sdk_route53::types::RrType, Option<String>)> = None;
        loop {
            let mut request = route53
                .list_resource_record_sets()
                .hosted_zone_id(id)
                .max_items(300);
            if let Some((name, kind, identifier)) = start.take() {
                request = request
                    .start_record_name(name)
                    .start_record_type(kind)
                    .set_start_record_identifier(identifier);
            }
            let page = request.send().await.map_err(aws_error)?;
            records.extend(page.resource_record_sets().iter().cloned());
            if !page.is_truncated() || records.len() >= MAX_RECORDS {
                break;
            }
            match (page.next_record_name(), page.next_record_type()) {
                (Some(name), Some(kind)) => {
                    start = Some((
                        name.to_owned(),
                        kind.clone(),
                        page.next_record_identifier().map(str::to_owned),
                    ));
                }
                _ => break,
            }
        }
        records.truncate(MAX_RECORDS);

        let private = z.config().is_some_and(|c| c.private_zone());
        Ok(vec![
            Section::new("Zone")
                .fact("Domain", z.name().trim_end_matches('.'))
                .fact("Type", if private { "private" } else { "public" })
                .fact("Records", z.resource_record_set_count())
                .fact("Comment", z.config().and_then(|c| c.comment()))
                .fact(
                    "Name servers",
                    zone.delegation_set().map(|d| joined(d.name_servers())),
                )
                .fact(
                    "VPCs",
                    zone.vpcs()
                        .iter()
                        .map(|v| {
                            format!(
                                "{} ({})",
                                s(v.vpc_id()),
                                v.vpc_region().map(|r| r.as_str()).unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            Section::new("Records").table(
                &["Name", "Type", "TTL", "Value"],
                records
                    .iter()
                    .map(|r| {
                        let value = match r.alias_target() {
                            Some(a) => format!("alias → {}", a.dns_name().trim_end_matches('.')),
                            None => r
                                .resource_records()
                                .iter()
                                .map(|v| v.value())
                                .collect::<Vec<_>>()
                                .join(", "),
                        };
                        let routing = r
                            .set_identifier()
                            .map(|i| format!(" [{i}]"))
                            .unwrap_or_default();
                        vec![
                            format!("{}{routing}", r.name().trim_end_matches('.')),
                            r.r#type().as_str().to_owned(),
                            n(r.ttl()),
                            value,
                        ]
                    })
                    .collect(),
            ),
        ])
    }
}

async fn classic_detail(
    elb: &aws_sdk_elasticloadbalancing::Client,
    name: &str,
) -> Result<Vec<Section>> {
    let (lbs, health) = tokio::join!(
        elb.describe_load_balancers()
            .load_balancer_names(name)
            .send(),
        elb.describe_instance_health()
            .load_balancer_name(name)
            .send(),
    );
    let lbs = lbs.map_err(aws_error)?;
    let lb = lbs
        .load_balancer_descriptions()
        .first()
        .ok_or_else(|| not_found("load balancer", name))?;
    let mut sections = vec![
        Section::new("Load balancer")
            .fact("Type", "classic")
            .fact("Scheme", lb.scheme())
            .fact("DNS name", lb.dns_name())
            .fact("VPC", lb.vpc_id())
            .fact("Zones", joined(lb.availability_zones()))
            .fact("Subnets", joined(lb.subnets()))
            .fact("Security groups", joined(lb.security_groups()))
            .fact("Health check", lb.health_check().map(|h| h.target()))
            .fact("Created", time(lb.created_time())),
        Section::new("Listeners").table(
            &["Port", "Protocol", "Instance port", "Instance protocol"],
            lb.listener_descriptions()
                .iter()
                .filter_map(|d| d.listener())
                .map(|l| {
                    vec![
                        l.load_balancer_port().to_string(),
                        l.protocol().to_owned(),
                        l.instance_port().to_string(),
                        s(l.instance_protocol()),
                    ]
                })
                .collect(),
        ),
    ];
    if let Ok(health) = health {
        sections.push(
            Section::new("Instances").table(
                &["Instance", "State", "Reason"],
                health
                    .instance_states()
                    .iter()
                    .map(|i| {
                        vec![
                            s(i.instance_id()),
                            s(i.state()),
                            s(i.description().or(i.reason_code())),
                        ]
                    })
                    .collect(),
            ),
        );
    }
    Ok(sections)
}

/// The instance page as sections, like every other service.
fn ec2(d: Ec2InstanceDetail) -> Vec<Section> {
    let i = &d.summary;
    let mut sections = vec![
        Section::new("Instance")
            .fact("State", i.state.as_str())
            .fact("Type", i.type_.as_str())
            .fact(
                "Status checks",
                match (&d.system_status, &d.instance_status) {
                    (None, None) => String::new(),
                    (a, b) => format!(
                        "system {}, instance {}",
                        a.as_deref().unwrap_or("-"),
                        b.as_deref().unwrap_or("-")
                    ),
                },
            )
            .fact("State reason", d.state_reason.clone())
            .fact(
                "AMI",
                match (&d.ami, &d.ami_name) {
                    (Some(id), Some(name)) => format!("{id} ({name})"),
                    (id, _) => id.clone().unwrap_or_default(),
                },
            )
            .fact("Platform", i.platform.clone())
            .fact("Architecture", i.arch.clone())
            .fact(
                "vCPU",
                d.cpu_cores
                    .map(|c| format!("{c} cores × {} threads", d.threads_per_core.unwrap_or(1))),
            )
            .fact("Key pair", i.key_name.clone())
            .fact("IAM profile", d.iam_profile.clone())
            .fact("Zone", i.zone.clone())
            .fact("Tenancy", d.tenancy.clone())
            .fact("Launched", i.launched.clone())
            .fact(
                "Monitoring",
                d.monitoring
                    .as_deref()
                    .map(|m| if m == "enabled" { "detailed" } else { "basic" }),
            )
            .fact(
                "IMDS",
                d.imds_tokens.as_deref().map(|t| {
                    if t == "required" {
                        "v2 only"
                    } else {
                        "v1 and v2"
                    }
                }),
            )
            .fact("Termination protection", d.termination_protection)
            .fact("EBS optimized", d.ebs_optimized),
        Section::new("Network")
            .fact("Public IP", i.public_ip.clone())
            .fact("Public DNS", d.public_dns.clone())
            .fact("Private IP", i.private_ip.clone())
            .fact("Private DNS", d.private_dns.clone())
            .fact("VPC", i.vpc.clone())
            .fact("Subnet", d.subnet.clone())
            .fact(
                "Security groups",
                d.security_groups
                    .iter()
                    .map(|g| format!("{} ({})", g.name, g.id))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
    ];
    if !d.volumes.is_empty() {
        sections.push(
            Section::new("Storage").table(
                &[
                    "Device",
                    "Volume",
                    "Size",
                    "Type",
                    "IOPS",
                    "Encrypted",
                    "Delete on termination",
                ],
                d.volumes
                    .iter()
                    .map(|v| {
                        vec![
                            v.device.clone(),
                            v.id.clone(),
                            v.size_gib.map(|g| format!("{g} GiB")).unwrap_or_default(),
                            v.type_.clone().unwrap_or_default(),
                            n(v.iops),
                            yes(v.encrypted),
                            yes(v.delete_on_termination),
                        ]
                    })
                    .collect(),
            ),
        );
    }
    if !d.interfaces.is_empty() {
        sections.push(
            Section::new("Network interfaces").table(
                &["ID", "Private IP", "Public IP", "Subnet", "MAC"],
                d.interfaces
                    .iter()
                    .map(|n_| {
                        vec![
                            n_.id.clone(),
                            n_.private_ip.clone().unwrap_or_default(),
                            n_.public_ip.clone().unwrap_or_default(),
                            n_.subnet.clone().unwrap_or_default(),
                            n_.mac.clone().unwrap_or_default(),
                        ]
                    })
                    .collect(),
            ),
        );
    }
    sections.extend(tags(d.tags.iter().map(|(k, v)| (k.as_str(), v.as_str()))));
    sections
}

#[cfg(test)]
mod tests {
    use aws_sdk_ec2::types::{IpPermission, IpRange, UserIdGroupPair};

    use super::*;

    #[test]
    fn empty_facts_are_left_out() {
        let s = Section::new("x")
            .fact("a", Some("1"))
            .fact("b", None::<&str>)
            .fact("c", "")
            .fact("d", Some(true));
        assert_eq!(
            s.facts,
            [("a".into(), "1".into()), ("d".into(), "yes".into())]
        );
    }

    #[test]
    fn security_group_rules_one_row_per_peer() {
        let rows = rules(&[
            IpPermission::builder()
                .ip_protocol("tcp")
                .from_port(443)
                .to_port(443)
                .ip_ranges(
                    IpRange::builder()
                        .cidr_ip("0.0.0.0/0")
                        .description("web")
                        .build(),
                )
                .user_id_group_pairs(
                    UserIdGroupPair::builder()
                        .group_id("sg-1")
                        .group_name("lb")
                        .build(),
                )
                .build(),
            IpPermission::builder()
                .ip_protocol("-1")
                .ip_ranges(IpRange::builder().cidr_ip("10.0.0.0/8").build())
                .build(),
            IpPermission::builder()
                .ip_protocol("udp")
                .from_port(1000)
                .to_port(2000)
                .ip_ranges(IpRange::builder().cidr_ip("1.2.3.4/32").build())
                .build(),
        ]);
        assert_eq!(rows[0], ["tcp", "443", "0.0.0.0/0", "web"]);
        assert_eq!(rows[1], ["tcp", "443", "sg-1 (lb)", ""]);
        assert_eq!(rows[2], ["all", "all", "10.0.0.0/8", ""]);
        assert_eq!(rows[3][1], "1000-2000");
    }
}
