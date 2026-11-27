//! List rows for VPCs, security groups, EBS volumes, S3 buckets, Route 53 hosted zones, MSK
//! clusters and RDS databases: the columns the console shows first.

use serde::Serialize;

use super::resources::{owned, time};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vpc {
    pub id: String,
    /// The `Name` tag; empty when untagged.
    pub name: String,
    /// `pending` or `available`.
    pub state: String,
    /// IPv4 blocks, the primary one first.
    pub cidrs: Vec<String>,
    pub ipv6_cidrs: Vec<String>,
    pub is_default: bool,
    pub tenancy: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub vpc: Option<String>,
    /// Rules as the console counts them: one per source or destination.
    pub inbound: usize,
    pub outbound: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EbsVolume {
    pub id: String,
    /// The `Name` tag; empty when untagged.
    pub name: String,
    /// `creating`, `available`, `in-use`, `deleting`, `deleted` or `error`.
    pub state: String,
    pub size_gib: Option<i32>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub iops: Option<i32>,
    /// MiB/s, for gp3.
    pub throughput: Option<i32>,
    pub encrypted: bool,
    pub zone: Option<String>,
    /// Instance it is attached to (the first, for multi-attach volumes).
    pub instance: Option<String>,
    pub device: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub name: String,
    pub region: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedZone {
    /// Without the `/hostedzone/` prefix.
    pub id: String,
    /// Domain name without the trailing dot.
    pub name: String,
    pub private: bool,
    pub records: Option<i64>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MskCluster {
    pub name: String,
    pub arn: Option<String>,
    /// `PROVISIONED` or `SERVERLESS`.
    #[serde(rename = "type")]
    pub type_: String,
    /// `ACTIVE`, `CREATING`, `UPDATING`, `DELETING`, `FAILED`, `HEALING`, ...
    pub state: String,
    pub kafka_version: Option<String>,
    pub brokers: Option<i32>,
    pub instance_type: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbInstance {
    /// The DB instance identifier.
    pub name: String,
    pub engine: Option<String>,
    pub version: Option<String>,
    pub class: Option<String>,
    /// `available`, `backing-up`, `creating`, `stopped`, `modifying`, ...
    pub status: String,
    /// `host:port`.
    pub endpoint: Option<String>,
    pub multi_az: bool,
    pub storage_gib: Option<i32>,
    pub storage_type: Option<String>,
    pub zone: Option<String>,
    /// Aurora cluster it belongs to.
    pub cluster: Option<String>,
    pub public: bool,
    pub created: Option<String>,
}

fn name_tag(tags: &[aws_sdk_ec2::types::Tag]) -> String {
    tags.iter()
        .find(|t| t.key() == Some("Name"))
        .and_then(|t| t.value())
        .unwrap_or_default()
        .to_owned()
}

impl From<&aws_sdk_ec2::types::Vpc> for Vpc {
    fn from(v: &aws_sdk_ec2::types::Vpc) -> Self {
        let mut cidrs: Vec<String> = owned(v.cidr_block()).into_iter().collect();
        for block in v.cidr_block_association_set() {
            let associated = block
                .cidr_block_state()
                .and_then(|s| s.state())
                .is_none_or(|s| s.as_str() == "associated");
            if let Some(cidr) = block.cidr_block()
                && associated
                && !cidrs.iter().any(|c| c == cidr)
            {
                cidrs.push(cidr.to_owned());
            }
        }
        Self {
            id: v.vpc_id().unwrap_or_default().to_owned(),
            name: name_tag(v.tags()),
            state: v.state().map_or("unknown", |s| s.as_str()).to_owned(),
            cidrs,
            ipv6_cidrs: v
                .ipv6_cidr_block_association_set()
                .iter()
                .filter_map(|b| owned(b.ipv6_cidr_block()))
                .collect(),
            is_default: v.is_default().unwrap_or(false),
            tenancy: v.instance_tenancy().map(|t| t.as_str().to_owned()),
            owner: owned(v.owner_id()),
        }
    }
}

/// One rule per source or destination, as the console lists them.
fn rule_count(permissions: &[aws_sdk_ec2::types::IpPermission]) -> usize {
    permissions
        .iter()
        .map(|p| {
            (p.ip_ranges().len()
                + p.ipv6_ranges().len()
                + p.prefix_list_ids().len()
                + p.user_id_group_pairs().len())
            .max(1)
        })
        .sum()
}

impl From<&aws_sdk_ec2::types::SecurityGroup> for SecurityGroup {
    fn from(g: &aws_sdk_ec2::types::SecurityGroup) -> Self {
        Self {
            id: g.group_id().unwrap_or_default().to_owned(),
            name: g.group_name().unwrap_or_default().to_owned(),
            description: owned(g.description()).filter(|d| !d.is_empty()),
            vpc: owned(g.vpc_id()),
            inbound: rule_count(g.ip_permissions()),
            outbound: rule_count(g.ip_permissions_egress()),
        }
    }
}

impl From<&aws_sdk_ec2::types::Volume> for EbsVolume {
    fn from(v: &aws_sdk_ec2::types::Volume) -> Self {
        let attachment = v.attachments().first();
        Self {
            id: v.volume_id().unwrap_or_default().to_owned(),
            name: name_tag(v.tags()),
            state: v.state().map_or("unknown", |s| s.as_str()).to_owned(),
            size_gib: v.size(),
            type_: v.volume_type().map(|t| t.as_str().to_owned()),
            iops: v.iops(),
            throughput: v.throughput(),
            encrypted: v.encrypted().unwrap_or(false),
            zone: owned(v.availability_zone()),
            instance: owned(attachment.and_then(|a| a.instance_id())),
            device: owned(attachment.and_then(|a| a.device())),
            created: time(v.create_time()),
        }
    }
}

impl From<&aws_sdk_s3::types::Bucket> for Bucket {
    fn from(b: &aws_sdk_s3::types::Bucket) -> Self {
        Self {
            name: b.name().unwrap_or_default().to_owned(),
            region: owned(b.bucket_region()),
            created: time(b.creation_date()),
        }
    }
}

impl From<&aws_sdk_route53::types::HostedZone> for HostedZone {
    fn from(z: &aws_sdk_route53::types::HostedZone) -> Self {
        Self {
            id: z.id().trim_start_matches("/hostedzone/").to_owned(),
            name: z.name().trim_end_matches('.').to_owned(),
            private: z.config().is_some_and(|c| c.private_zone()),
            records: z.resource_record_set_count(),
            comment: owned(z.config().and_then(|c| c.comment())).filter(|c| !c.is_empty()),
        }
    }
}

impl From<&aws_sdk_kafka::types::Cluster> for MskCluster {
    fn from(c: &aws_sdk_kafka::types::Cluster) -> Self {
        let provisioned = c.provisioned();
        Self {
            name: c.cluster_name().unwrap_or_default().to_owned(),
            arn: owned(c.cluster_arn()),
            type_: c
                .cluster_type()
                .map_or("UNKNOWN", |t| t.as_str())
                .to_owned(),
            state: c.state().map_or("UNKNOWN", |s| s.as_str()).to_owned(),
            kafka_version: owned(
                provisioned
                    .and_then(|p| p.current_broker_software_info())
                    .and_then(|s| s.kafka_version()),
            ),
            brokers: provisioned.and_then(|p| p.number_of_broker_nodes()),
            instance_type: owned(
                provisioned
                    .and_then(|p| p.broker_node_group_info())
                    .and_then(|b| b.instance_type()),
            ),
            created: time(c.creation_time()),
        }
    }
}

impl From<&aws_sdk_rds::types::DbInstance> for DbInstance {
    fn from(d: &aws_sdk_rds::types::DbInstance) -> Self {
        Self {
            name: d.db_instance_identifier().unwrap_or_default().to_owned(),
            engine: owned(d.engine()),
            version: owned(d.engine_version()),
            class: owned(d.db_instance_class()),
            status: d.db_instance_status().unwrap_or("unknown").to_owned(),
            endpoint: d.endpoint().and_then(|e| {
                let host = e.address()?;
                Some(match e.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host.to_owned(),
                })
            }),
            multi_az: d.multi_az().unwrap_or(false),
            storage_gib: d.allocated_storage().filter(|&s| s > 0),
            storage_type: owned(d.storage_type()),
            zone: owned(d.availability_zone()),
            cluster: owned(d.db_cluster_identifier()),
            public: d.publicly_accessible().unwrap_or(false),
            created: time(d.instance_create_time()),
        }
    }
}

#[cfg(test)]
mod tests {
    use aws_sdk_ec2::types::{
        IpPermission, IpRange, Tag, UserIdGroupPair, VpcCidrBlockAssociation, VpcCidrBlockState,
        VpcCidrBlockStateCode,
    };

    use super::*;

    #[test]
    fn vpc_lists_associated_blocks_once() {
        let block = |cidr: &str, state| {
            VpcCidrBlockAssociation::builder()
                .cidr_block(cidr)
                .cidr_block_state(VpcCidrBlockState::builder().state(state).build())
                .build()
        };
        let vpc = aws_sdk_ec2::types::Vpc::builder()
            .vpc_id("vpc-1")
            .cidr_block("10.0.0.0/16")
            .cidr_block_association_set(block("10.0.0.0/16", VpcCidrBlockStateCode::Associated))
            .cidr_block_association_set(block("10.1.0.0/16", VpcCidrBlockStateCode::Associated))
            .cidr_block_association_set(block("10.2.0.0/16", VpcCidrBlockStateCode::Disassociated))
            .tags(Tag::builder().key("Name").value("main").build())
            .is_default(true)
            .build();
        let v = Vpc::from(&vpc);
        assert_eq!(v.cidrs, ["10.0.0.0/16", "10.1.0.0/16"]);
        assert_eq!((v.name.as_str(), v.is_default), ("main", true));
    }

    #[test]
    fn security_group_rules_count_each_source() {
        let group = aws_sdk_ec2::types::SecurityGroup::builder()
            .group_id("sg-1")
            .group_name("web")
            .description("")
            .ip_permissions(
                IpPermission::builder()
                    .ip_ranges(IpRange::builder().cidr_ip("0.0.0.0/0").build())
                    .ip_ranges(IpRange::builder().cidr_ip("10.0.0.0/8").build())
                    .build(),
            )
            .ip_permissions(
                IpPermission::builder()
                    .user_id_group_pairs(UserIdGroupPair::builder().group_id("sg-2").build())
                    .build(),
            )
            .ip_permissions_egress(IpPermission::builder().ip_protocol("-1").build())
            .build();
        let g = SecurityGroup::from(&group);
        assert_eq!((g.inbound, g.outbound), (3, 1));
        assert_eq!(g.description, None);
    }

    #[test]
    fn hosted_zone_names_are_tidied() {
        let zone = aws_sdk_route53::types::HostedZone::builder()
            .id("/hostedzone/Z123")
            .name("example.com.")
            .caller_reference("x")
            .config(
                aws_sdk_route53::types::HostedZoneConfig::builder()
                    .private_zone(true)
                    .build(),
            )
            .build()
            .unwrap();
        let z = HostedZone::from(&zone);
        assert_eq!(
            (z.id.as_str(), z.name.as_str(), z.private),
            ("Z123", "example.com", true)
        );
    }

    #[test]
    fn rds_endpoint_joins_host_and_port() {
        let db = aws_sdk_rds::types::DbInstance::builder()
            .db_instance_identifier("orders")
            .db_instance_status("available")
            .endpoint(
                aws_sdk_rds::types::Endpoint::builder()
                    .address("orders.abc.rds.amazonaws.com")
                    .port(5432)
                    .build(),
            )
            .build();
        let d = DbInstance::from(&db);
        assert_eq!(
            d.endpoint.as_deref(),
            Some("orders.abc.rds.amazonaws.com:5432")
        );
        assert!(!d.multi_az);
    }
}
