//! Compact, GUI-friendly views of AWS resources, with the columns the console lists first.

use aws_sdk_ec2::primitives::{DateTime, DateTimeFormat};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ec2Instance {
    pub id: String,
    /// The `Name` tag; empty when untagged.
    pub name: String,
    /// `pending`, `running`, `stopping`, `stopped`, `shutting-down` or `terminated`.
    pub state: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub zone: Option<String>,
    pub private_ip: Option<String>,
    pub public_ip: Option<String>,
    pub vpc: Option<String>,
    /// e.g. `Linux/UNIX`, `Windows`.
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub key_name: Option<String>,
    /// RFC 3339.
    pub launched: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EksCluster {
    pub name: String,
    /// `CREATING`, `ACTIVE`, `UPDATING`, `DELETING` or `FAILED`.
    pub status: String,
    /// Kubernetes version, e.g. `1.31`.
    pub version: Option<String>,
    pub platform_version: Option<String>,
    pub endpoint: Option<String>,
    pub arn: Option<String>,
    pub vpc: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancer {
    pub name: String,
    /// `application`, `network`, `gateway` or `classic`.
    #[serde(rename = "type")]
    pub type_: String,
    /// `internet-facing` or `internal`.
    pub scheme: Option<String>,
    /// `active`, `provisioning`, `active_impaired` or `failed`; none for classic ones.
    pub state: Option<String>,
    pub dns_name: Option<String>,
    pub vpc: Option<String>,
    pub zones: Vec<String>,
    pub created: Option<String>,
    /// ELBv2 only; classic load balancers are known by name.
    pub arn: Option<String>,
}

pub(super) fn time(t: Option<&DateTime>) -> Option<String> {
    t.and_then(|t| t.fmt(DateTimeFormat::DateTime).ok())
}

pub(super) fn owned(s: Option<&str>) -> Option<String> {
    s.map(str::to_owned)
}

impl From<&aws_sdk_ec2::types::Instance> for Ec2Instance {
    fn from(i: &aws_sdk_ec2::types::Instance) -> Self {
        Self {
            id: i.instance_id().unwrap_or_default().to_owned(),
            name: i
                .tags()
                .iter()
                .find(|t| t.key() == Some("Name"))
                .and_then(|t| t.value())
                .unwrap_or_default()
                .to_owned(),
            state: i
                .state()
                .and_then(|s| s.name())
                .map_or("unknown", |n| n.as_str())
                .to_owned(),
            type_: i
                .instance_type()
                .map(|t| t.as_str())
                .unwrap_or_default()
                .to_owned(),
            zone: owned(i.placement().and_then(|p| p.availability_zone())),
            private_ip: owned(i.private_ip_address()),
            public_ip: owned(i.public_ip_address()),
            vpc: owned(i.vpc_id()),
            platform: owned(i.platform_details()),
            arch: i.architecture().map(|a| a.as_str().to_owned()),
            key_name: owned(i.key_name()),
            launched: time(i.launch_time()),
        }
    }
}

impl EksCluster {
    /// A cluster that was listed but could not be described.
    pub(super) fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            status: "UNKNOWN".into(),
            version: None,
            platform_version: None,
            endpoint: None,
            arn: None,
            vpc: None,
            created: None,
        }
    }
}

impl From<&aws_sdk_eks::types::Cluster> for EksCluster {
    fn from(c: &aws_sdk_eks::types::Cluster) -> Self {
        Self {
            name: c.name().unwrap_or_default().to_owned(),
            status: c.status().map_or("UNKNOWN", |s| s.as_str()).to_owned(),
            version: owned(c.version()),
            platform_version: owned(c.platform_version()),
            endpoint: owned(c.endpoint()),
            arn: owned(c.arn()),
            vpc: owned(c.resources_vpc_config().and_then(|v| v.vpc_id())),
            created: c.created_at().and_then(|t| {
                t.fmt(aws_sdk_eks::primitives::DateTimeFormat::DateTime)
                    .ok()
            }),
        }
    }
}

impl From<&aws_sdk_elasticloadbalancingv2::types::LoadBalancer> for LoadBalancer {
    fn from(lb: &aws_sdk_elasticloadbalancingv2::types::LoadBalancer) -> Self {
        Self {
            name: lb.load_balancer_name().unwrap_or_default().to_owned(),
            type_: lb.r#type().map_or("unknown", |t| t.as_str()).to_owned(),
            scheme: lb.scheme().map(|s| s.as_str().to_owned()),
            state: lb
                .state()
                .and_then(|s| s.code())
                .map(|c| c.as_str().to_owned()),
            dns_name: owned(lb.dns_name()),
            vpc: owned(lb.vpc_id()),
            zones: lb
                .availability_zones()
                .iter()
                .filter_map(|z| z.zone_name().map(str::to_owned))
                .collect(),
            created: lb.created_time().and_then(|t| {
                t.fmt(aws_sdk_elasticloadbalancingv2::primitives::DateTimeFormat::DateTime)
                    .ok()
            }),
            arn: owned(lb.load_balancer_arn()),
        }
    }
}

impl From<&aws_sdk_elasticloadbalancing::types::LoadBalancerDescription> for LoadBalancer {
    fn from(lb: &aws_sdk_elasticloadbalancing::types::LoadBalancerDescription) -> Self {
        Self {
            name: lb.load_balancer_name().unwrap_or_default().to_owned(),
            type_: "classic".into(),
            scheme: owned(lb.scheme()),
            state: None,
            dns_name: owned(lb.dns_name()),
            vpc: owned(lb.vpc_id()),
            zones: lb.availability_zones().to_vec(),
            created: lb.created_time().and_then(|t| {
                t.fmt(aws_sdk_elasticloadbalancing::primitives::DateTimeFormat::DateTime)
                    .ok()
            }),
            arn: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use aws_sdk_ec2::types::{Instance, InstanceState, InstanceStateName, InstanceType, Tag};

    use super::*;

    #[test]
    fn summarizes_an_instance() {
        let instance = Instance::builder()
            .instance_id("i-123")
            .tags(Tag::builder().key("env").value("prod").build())
            .tags(Tag::builder().key("Name").value("web-1").build())
            .state(
                InstanceState::builder()
                    .name(InstanceStateName::Running)
                    .build(),
            )
            .instance_type(InstanceType::T3Micro)
            .private_ip_address("10.0.0.5")
            .launch_time(DateTime::from_secs(1_767_225_600))
            .build();
        let s = Ec2Instance::from(&instance);
        assert_eq!(
            (
                s.id.as_str(),
                s.name.as_str(),
                s.state.as_str(),
                s.type_.as_str()
            ),
            ("i-123", "web-1", "running", "t3.micro")
        );
        assert_eq!(s.private_ip.as_deref(), Some("10.0.0.5"));
        assert_eq!(s.public_ip, None);
        assert_eq!(s.launched.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn untagged_instance_has_no_name() {
        let s = Ec2Instance::from(&Instance::builder().instance_id("i-1").build());
        assert_eq!((s.name.as_str(), s.state.as_str()), ("", "unknown"));
    }

    #[test]
    fn classic_load_balancers_are_marked() {
        let lb = aws_sdk_elasticloadbalancing::types::LoadBalancerDescription::builder()
            .load_balancer_name("old")
            .scheme("internal")
            .availability_zones("ap-southeast-7a")
            .build();
        let s = LoadBalancer::from(&lb);
        assert_eq!((s.type_.as_str(), s.state), ("classic", None));
        assert_eq!(s.zones, ["ap-southeast-7a"]);
    }
}

/// Everything the console's instance page shows first, gathered from several calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ec2InstanceDetail {
    #[serde(flatten)]
    pub summary: Ec2Instance,
    pub ami: Option<String>,
    /// AMI name, when the image is still visible to the account.
    pub ami_name: Option<String>,
    pub subnet: Option<String>,
    pub private_dns: Option<String>,
    pub public_dns: Option<String>,
    pub security_groups: Vec<SecurityGroupRef>,
    /// Name of the instance profile (the IAM role's container).
    pub iam_profile: Option<String>,
    pub root_device: Option<String>,
    pub root_device_type: Option<String>,
    pub volumes: Vec<Ec2Volume>,
    pub interfaces: Vec<Ec2Interface>,
    pub tags: Vec<(String, String)>,
    /// `enabled` (detailed) or `disabled` (basic).
    pub monitoring: Option<String>,
    pub tenancy: Option<String>,
    pub ebs_optimized: Option<bool>,
    /// Why the instance is in its state, e.g. `Client.UserInitiatedShutdown: ...`.
    pub state_reason: Option<String>,
    pub cpu_cores: Option<i32>,
    pub threads_per_core: Option<i32>,
    /// `required` means IMDSv2 only.
    pub imds_tokens: Option<String>,
    pub virtualization: Option<String>,
    /// Termination protection; none when the account may not read it.
    pub termination_protection: Option<bool>,
    /// Status checks: `ok`, `impaired`, `initializing`, `insufficient-data`, `not-applicable`.
    pub system_status: Option<String>,
    pub instance_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityGroupRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ec2Volume {
    pub device: String,
    pub id: String,
    pub size_gib: Option<i32>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub iops: Option<i32>,
    pub encrypted: Option<bool>,
    pub delete_on_termination: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ec2Interface {
    pub id: String,
    pub private_ip: Option<String>,
    pub public_ip: Option<String>,
    pub subnet: Option<String>,
    pub mac: Option<String>,
}

/// What other calls add to an instance's own description.
#[derive(Default)]
pub(super) struct Extras {
    pub ami_name: Option<String>,
    pub volumes: Vec<aws_sdk_ec2::types::Volume>,
    pub termination_protection: Option<bool>,
    pub status: Option<aws_sdk_ec2::types::InstanceStatus>,
}

impl Ec2InstanceDetail {
    pub(super) fn new(i: &aws_sdk_ec2::types::Instance, extras: Extras) -> Self {
        let volume = |id: &str| extras.volumes.iter().find(|v| v.volume_id() == Some(id));
        let check = |s: Option<&aws_sdk_ec2::types::InstanceStatusSummary>| {
            s.and_then(|s| s.status()).map(|s| s.as_str().to_owned())
        };
        let status = extras.status.as_ref();
        let mut tags: Vec<(String, String)> = i
            .tags()
            .iter()
            .map(|t| {
                (
                    t.key().unwrap_or_default().to_owned(),
                    t.value().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        tags.sort();
        Self {
            summary: Ec2Instance::from(i),
            ami: owned(i.image_id()),
            ami_name: extras.ami_name,
            subnet: owned(i.subnet_id()),
            private_dns: owned(i.private_dns_name()).filter(|s| !s.is_empty()),
            public_dns: owned(i.public_dns_name()).filter(|s| !s.is_empty()),
            security_groups: i
                .security_groups()
                .iter()
                .map(|g| SecurityGroupRef {
                    id: g.group_id().unwrap_or_default().to_owned(),
                    name: g.group_name().unwrap_or_default().to_owned(),
                })
                .collect(),
            iam_profile: i
                .iam_instance_profile()
                .and_then(|p| p.arn())
                .and_then(|arn| arn.rsplit('/').next())
                .map(str::to_owned),
            root_device: owned(i.root_device_name()),
            root_device_type: i.root_device_type().map(|t| t.as_str().to_owned()),
            volumes: i
                .block_device_mappings()
                .iter()
                .filter_map(|m| {
                    let ebs = m.ebs()?;
                    let id = ebs.volume_id()?;
                    let v = volume(id);
                    Some(Ec2Volume {
                        device: m.device_name().unwrap_or_default().to_owned(),
                        id: id.to_owned(),
                        size_gib: v.and_then(|v| v.size()),
                        type_: v
                            .and_then(|v| v.volume_type())
                            .map(|t| t.as_str().to_owned()),
                        iops: v.and_then(|v| v.iops()),
                        encrypted: v.and_then(|v| v.encrypted()),
                        delete_on_termination: ebs.delete_on_termination(),
                    })
                })
                .collect(),
            interfaces: i
                .network_interfaces()
                .iter()
                .map(|n| Ec2Interface {
                    id: n.network_interface_id().unwrap_or_default().to_owned(),
                    private_ip: owned(n.private_ip_address()),
                    public_ip: owned(n.association().and_then(|a| a.public_ip())),
                    subnet: owned(n.subnet_id()),
                    mac: owned(n.mac_address()),
                })
                .collect(),
            tags,
            monitoring: i
                .monitoring()
                .and_then(|m| m.state())
                .map(|s| s.as_str().to_owned()),
            tenancy: i
                .placement()
                .and_then(|p| p.tenancy())
                .map(|t| t.as_str().to_owned()),
            ebs_optimized: i.ebs_optimized(),
            state_reason: owned(i.state_reason().and_then(|r| r.message())),
            cpu_cores: i.cpu_options().and_then(|c| c.core_count()),
            threads_per_core: i.cpu_options().and_then(|c| c.threads_per_core()),
            imds_tokens: i
                .metadata_options()
                .and_then(|m| m.http_tokens())
                .map(|t| t.as_str().to_owned()),
            virtualization: i.virtualization_type().map(|v| v.as_str().to_owned()),
            termination_protection: extras.termination_protection,
            system_status: check(status.and_then(|s| s.system_status())),
            instance_status: check(status.and_then(|s| s.instance_status())),
        }
    }
}

#[cfg(test)]
mod detail_tests {
    use aws_sdk_ec2::types::{
        EbsInstanceBlockDevice, GroupIdentifier, IamInstanceProfile, Instance,
        InstanceBlockDeviceMapping, Tag, Volume, VolumeType,
    };

    use super::*;

    #[test]
    fn joins_volumes_and_names_the_profile() {
        let instance = Instance::builder()
            .instance_id("i-1")
            .image_id("ami-1")
            .security_groups(
                GroupIdentifier::builder()
                    .group_id("sg-1")
                    .group_name("web")
                    .build(),
            )
            .iam_instance_profile(
                IamInstanceProfile::builder()
                    .arn("arn:aws:iam::1:instance-profile/app-role")
                    .build(),
            )
            .block_device_mappings(
                InstanceBlockDeviceMapping::builder()
                    .device_name("/dev/xvda")
                    .ebs(
                        EbsInstanceBlockDevice::builder()
                            .volume_id("vol-1")
                            .delete_on_termination(true)
                            .build(),
                    )
                    .build(),
            )
            .tags(Tag::builder().key("z").value("1").build())
            .tags(Tag::builder().key("Name").value("web-1").build())
            .private_dns_name("")
            .build();
        let extras = Extras {
            ami_name: Some("al2023".into()),
            volumes: vec![
                Volume::builder()
                    .volume_id("vol-1")
                    .size(30)
                    .volume_type(VolumeType::Gp3)
                    .encrypted(true)
                    .build(),
            ],
            ..Default::default()
        };
        let d = Ec2InstanceDetail::new(&instance, extras);
        assert_eq!(d.iam_profile.as_deref(), Some("app-role"));
        assert_eq!(d.security_groups[0].name, "web");
        assert_eq!(
            (d.volumes[0].size_gib, d.volumes[0].type_.as_deref()),
            (Some(30), Some("gp3"))
        );
        assert_eq!(d.volumes[0].delete_on_termination, Some(true));
        assert_eq!(d.tags[0].0, "Name");
        assert_eq!(d.private_dns, None);
        assert_eq!(d.summary.name, "web-1");
    }
}
