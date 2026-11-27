//! AWS: profiles from the shared config files (as the AWS CLI reads them), and paged listing of
//! EC2 instances, EKS clusters and load balancers.
//!
//! Credentials come from the SDK's default chain for the chosen profile, so static keys, SSO,
//! `credential_process` and assumed roles all work as they do for the CLI. Lists are fetched page
//! by page and handed to a callback as each page arrives.

mod profiles;
mod resources;

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use aws_config::timeout::TimeoutConfig;
use aws_config::{BehaviorVersion, Region, SdkConfig};
use futures::future::join_all;
use serde::Serialize;
use tokio::sync::{Mutex, OnceCell};

pub use profiles::{ProfileInfo, list_profiles};
pub use resources::{
    Ec2Instance, Ec2InstanceDetail, Ec2Interface, Ec2Volume, EksCluster, LoadBalancer,
    SecurityGroupRef,
};

use crate::{Error, Result};

/// Items requested per API call.
pub const PAGE_SIZE: i32 = 100;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Covers retries too, so a call that cannot succeed gives up instead of hanging.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Used when a profile names no region.
pub const FALLBACK_REGION: &str = "us-east-1";

/// Who the credentials belong to (`aws sts get-caller-identity`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub profile: String,
    pub region: String,
    pub account: String,
    pub arn: String,
}

/// Profile name and region.
type ConfigKey = (String, String);

/// SDK configurations, one per profile and region, shared by all callers.
#[derive(Default)]
pub struct Aws {
    configs: Mutex<HashMap<ConfigKey, Arc<OnceCell<SdkConfig>>>>,
}

impl Aws {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks the profile's credentials work in `region`.
    pub async fn connect(&self, profile: &str, region: &str) -> Result<Identity> {
        let sts = aws_sdk_sts::Client::new(&self.config(profile, region).await);
        let id = sts.get_caller_identity().send().await.map_err(aws_error)?;
        Ok(Identity {
            profile: profile.to_owned(),
            region: region.to_owned(),
            account: id.account().unwrap_or_default().to_owned(),
            arn: id.arn().unwrap_or_default().to_owned(),
        })
    }

    /// Forgets cached configurations for `profile`, e.g. after its credentials were refreshed.
    pub async fn disconnect(&self, profile: &str) {
        self.configs.lock().await.retain(|(p, _), _| p != profile);
    }

    /// Regions enabled for the account, sorted.
    pub async fn regions(&self, profile: &str, region: &str) -> Result<Vec<String>> {
        let ec2 = aws_sdk_ec2::Client::new(&self.config(profile, region).await);
        let out = ec2.describe_regions().send().await.map_err(aws_error)?;
        let mut names: Vec<String> = out
            .regions()
            .iter()
            .filter_map(|r| r.region_name().map(str::to_owned))
            .collect();
        names.sort();
        Ok(names)
    }

    /// EC2 instances, like `aws ec2 describe-instances`.
    pub async fn ec2_instances(
        &self,
        profile: &str,
        region: &str,
        mut on_page: impl FnMut(Vec<Ec2Instance>) -> ControlFlow<()>,
    ) -> Result<usize> {
        let ec2 = aws_sdk_ec2::Client::new(&self.config(profile, region).await);
        let mut pages = ec2
            .describe_instances()
            .max_results(PAGE_SIZE)
            .into_paginator()
            .send();
        let mut total = 0;
        while let Some(page) = pages.next().await {
            let page = page.map_err(aws_error)?;
            let items: Vec<Ec2Instance> = page
                .reservations()
                .iter()
                .flat_map(|r| r.instances())
                .map(Ec2Instance::from)
                .collect();
            total += items.len();
            if on_page(items).is_break() {
                break;
            }
        }
        Ok(total)
    }

    /// One instance in detail. The instance itself must be readable; the AMI name, volume sizes,
    /// status checks and termination protection are added when the account may read them.
    pub async fn ec2_instance(
        &self,
        profile: &str,
        region: &str,
        id: &str,
    ) -> Result<Ec2InstanceDetail> {
        let ec2 = aws_sdk_ec2::Client::new(&self.config(profile, region).await);
        let out = ec2
            .describe_instances()
            .instance_ids(id)
            .send()
            .await
            .map_err(aws_error)?;
        let instance = out
            .reservations()
            .iter()
            .flat_map(|r| r.instances())
            .next()
            .ok_or_else(|| Error::Aws(format!("instance {id} not found in {region}")))?;

        let volume_ids: Vec<String> = instance
            .block_device_mappings()
            .iter()
            .filter_map(|m| m.ebs()?.volume_id().map(str::to_owned))
            .collect();
        let image = async {
            let ami = instance.image_id()?;
            let out = ec2.describe_images().image_ids(ami).send().await.ok()?;
            out.images().first()?.name().map(str::to_owned)
        };
        let volumes = async {
            if volume_ids.is_empty() {
                return Vec::new();
            }
            ec2.describe_volumes()
                .set_volume_ids(Some(volume_ids.clone()))
                .send()
                .await
                .map(|o| o.volumes().to_vec())
                .unwrap_or_default()
        };
        let protection = async {
            ec2.describe_instance_attribute()
                .instance_id(id)
                .attribute(aws_sdk_ec2::types::InstanceAttributeName::DisableApiTermination)
                .send()
                .await
                .ok()?
                .disable_api_termination()?
                .value()
        };
        let status = async {
            ec2.describe_instance_status()
                .instance_ids(id)
                .include_all_instances(true)
                .send()
                .await
                .ok()?
                .instance_statuses()
                .first()
                .cloned()
        };
        let (ami_name, volumes, termination_protection, status) =
            tokio::join!(image, volumes, protection, status);
        Ok(Ec2InstanceDetail::new(
            instance,
            resources::Extras {
                ami_name,
                volumes,
                termination_protection,
                status,
            },
        ))
    }

    /// EKS clusters with their details; each page is described in parallel.
    pub async fn eks_clusters(
        &self,
        profile: &str,
        region: &str,
        mut on_page: impl FnMut(Vec<EksCluster>) -> ControlFlow<()>,
    ) -> Result<usize> {
        let eks = aws_sdk_eks::Client::new(&self.config(profile, region).await);
        let mut pages = eks
            .list_clusters()
            .max_results(PAGE_SIZE)
            .into_paginator()
            .send();
        let mut total = 0;
        while let Some(page) = pages.next().await {
            let page = page.map_err(aws_error)?;
            let described = join_all(
                page.clusters()
                    .iter()
                    .map(|name| eks.describe_cluster().name(name).send()),
            )
            .await;
            let mut items = Vec::with_capacity(described.len());
            for (name, result) in page.clusters().iter().zip(described) {
                let cluster = result.map_err(aws_error)?;
                items.push(match cluster.cluster() {
                    Some(c) => EksCluster::from(c),
                    None => EksCluster::named(name),
                });
            }
            total += items.len();
            if on_page(items).is_break() {
                break;
            }
        }
        Ok(total)
    }

    /// Application, network and gateway load balancers (ELBv2), then classic ones.
    pub async fn load_balancers(
        &self,
        profile: &str,
        region: &str,
        mut on_page: impl FnMut(Vec<LoadBalancer>) -> ControlFlow<()>,
    ) -> Result<usize> {
        let config = self.config(profile, region).await;
        let mut total = 0;

        let v2 = aws_sdk_elasticloadbalancingv2::Client::new(&config);
        let mut pages = v2
            .describe_load_balancers()
            .page_size(PAGE_SIZE)
            .into_paginator()
            .send();
        while let Some(page) = pages.next().await {
            let page = page.map_err(aws_error)?;
            let items: Vec<LoadBalancer> = page
                .load_balancers()
                .iter()
                .map(LoadBalancer::from)
                .collect();
            total += items.len();
            if on_page(items).is_break() {
                return Ok(total);
            }
        }

        let classic = aws_sdk_elasticloadbalancing::Client::new(&config);
        let mut pages = classic
            .describe_load_balancers()
            .page_size(PAGE_SIZE)
            .into_paginator()
            .send();
        while let Some(page) = pages.next().await {
            let page = page.map_err(aws_error)?;
            let items: Vec<LoadBalancer> = page
                .load_balancer_descriptions()
                .iter()
                .map(LoadBalancer::from)
                .collect();
            total += items.len();
            if on_page(items).is_break() {
                break;
            }
        }
        Ok(total)
    }

    /// Loads the SDK configuration once per profile and region.
    async fn config(&self, profile: &str, region: &str) -> SdkConfig {
        crate::tls::init();
        let cell = self
            .configs
            .lock()
            .await
            .entry((profile.to_owned(), region.to_owned()))
            .or_default()
            .clone();
        cell.get_or_init(|| async {
            aws_config::defaults(BehaviorVersion::latest())
                .profile_name(profile)
                .region(Region::new(region.to_owned()))
                .timeout_config(
                    TimeoutConfig::builder()
                        .connect_timeout(CONNECT_TIMEOUT)
                        .operation_timeout(OPERATION_TIMEOUT)
                        .build(),
                )
                .load()
                .await
        })
        .await
        .clone()
    }
}

/// The SDK's message with its causes ("dispatch failure: ... no credentials found in chain"),
/// without the debug dump the SDK appends to its own rendering.
fn aws_error(error: impl std::error::Error) -> Error {
    Error::Aws(crate::error::describe(&error))
}
