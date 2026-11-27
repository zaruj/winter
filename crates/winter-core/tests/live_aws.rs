//! Talks to a real AWS account (read-only calls). Run with:
//! `WINTER_TEST_AWS_PROFILE=<profile> cargo test -p winter-core --test live_aws -- --ignored --nocapture`

use std::ops::ControlFlow;

use winter_core::aws::{Aws, FALLBACK_REGION, list_profiles};

fn profile() -> String {
    std::env::var("WINTER_TEST_AWS_PROFILE").expect("set WINTER_TEST_AWS_PROFILE")
}

#[tokio::test]
#[ignore = "needs AWS credentials"]
async fn lists_ec2_eks_and_load_balancers() {
    let name = profile();
    let profiles = list_profiles().unwrap();
    println!("{profiles:?}");
    let info = profiles
        .iter()
        .find(|p| p.name == name)
        .expect("profile in ~/.aws");
    let region = info
        .region
        .clone()
        .unwrap_or_else(|| FALLBACK_REGION.into());

    let aws = Aws::new();
    let id = aws.connect(&name, &region).await.unwrap();
    assert!(!id.account.is_empty() && id.arn.starts_with("arn:aws"));
    let regions = aws.regions(&name, &region).await.unwrap();
    assert!(regions.contains(&region), "{regions:?}");

    let mut instances = Vec::new();
    let total = aws
        .ec2_instances(&name, &region, |p| {
            instances.extend(p);
            ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(total, instances.len());
    for i in &instances {
        println!(
            "EC2 {} {:?} {} {} {:?}",
            i.id, i.name, i.state, i.type_, i.private_ip
        );
    }
    if let Some(first) = instances.first() {
        let detail = aws.ec2_instance(&name, &region, &first.id).await.unwrap();
        assert_eq!(detail.summary.id, first.id);
        println!("{detail:#?}");
    }

    let mut clusters = Vec::new();
    aws.eks_clusters(&name, &region, |p| {
        clusters.extend(p);
        ControlFlow::Continue(())
    })
    .await
    .unwrap();
    let mut lbs = Vec::new();
    aws.load_balancers(&name, &region, |p| {
        lbs.extend(p);
        ControlFlow::Continue(())
    })
    .await
    .unwrap();
    println!(
        "{} regions, {} instances, {} EKS clusters, {} load balancers in {region}",
        regions.len(),
        instances.len(),
        clusters.len(),
        lbs.len()
    );
}

/// Every other list, in the profile's region. Each must succeed (an empty list is fine).
#[tokio::test]
#[ignore = "needs AWS credentials"]
async fn lists_storage_databases_and_networking() {
    let name = profile();
    let region = list_profiles()
        .unwrap()
        .into_iter()
        .find(|p| p.name == name)
        .and_then(|p| p.region)
        .unwrap_or_else(|| FALLBACK_REGION.into());
    let aws = Aws::new();

    macro_rules! count {
        ($method:ident) => {{
            let mut items = Vec::new();
            match aws
                .$method(&name, &region, |p| {
                    items.extend(p);
                    ControlFlow::Continue(())
                })
                .await
            {
                Ok(total) => assert_eq!(total, items.len()),
                // The profile may not be allowed every service; that is not a failure here.
                Err(e)
                    if e.to_string().contains("AccessDenied")
                        || e.to_string().contains("not authorized") =>
                {
                    println!("{}: skipped, no permission", stringify!($method));
                }
                Err(e) => panic!("{}: {e}", stringify!($method)),
            }
            println!("{}: {}", stringify!($method), items.len());
            if let Some(first) = items.first() {
                println!("  {first:?}");
            }
            items
        }};
    }
    /// Details of the first item of a list must load and say something.
    macro_rules! detail {
        ($kind:literal, $items:expr, $id:expr) => {
            detail!($kind, $items, $id, |_| region.clone())
        };
        ($kind:literal, $items:expr, $id:expr, $region:expr) => {
            if let Some(item) = $items.first() {
                let sections = aws
                    .detail(&name, &$region(item), $kind, &$id(item))
                    .await
                    .unwrap_or_else(|e| panic!("{} detail: {e}", $kind));
                assert!(!sections.is_empty(), "{} detail is empty", $kind);
                let titles: Vec<_> = sections.iter().map(|s| s.title.as_str()).collect();
                println!("  {} detail: {titles:?}", $kind);
            }
        };
    }
    let vpcs = count!(vpcs);
    detail!("vpc", vpcs, |v: &winter_core::aws::Vpc| v.id.clone());
    let groups = count!(security_groups);
    detail!("sg", groups, |g: &winter_core::aws::SecurityGroup| g
        .id
        .clone());
    let volumes = count!(ebs_volumes);
    detail!("ebs", volumes, |v: &winter_core::aws::EbsVolume| v
        .id
        .clone());
    let buckets = count!(s3_buckets);
    detail!(
        "s3",
        buckets,
        |b: &winter_core::aws::Bucket| b.name.clone(),
        |b: &winter_core::aws::Bucket| b.region.clone().unwrap_or_else(|| region.clone())
    );
    let zones = count!(hosted_zones);
    detail!("route53", zones, |z: &winter_core::aws::HostedZone| z
        .id
        .clone());
    let msk = count!(msk_clusters);
    detail!("msk", msk, |c: &winter_core::aws::MskCluster| c
        .arn
        .clone()
        .unwrap_or_default());
    let dbs = count!(rds_instances);
    detail!("rds", dbs, |d: &winter_core::aws::DbInstance| d
        .name
        .clone());
    let instances = count!(ec2_instances);
    detail!("ec2", instances, |i: &winter_core::aws::Ec2Instance| i
        .id
        .clone());
    let clusters = count!(eks_clusters);
    detail!("eks", clusters, |c: &winter_core::aws::EksCluster| c
        .name
        .clone());
    let lbs = count!(load_balancers);
    detail!("elb", lbs, |l: &winter_core::aws::LoadBalancer| l
        .arn
        .clone()
        .unwrap_or_else(|| format!("classic/{}", l.name)));
    // Every region has a default VPC unless someone deleted it, so expect at least one.
    assert!(!vpcs.is_empty() || std::env::var_os("WINTER_TEST_NO_VPC").is_some());
}

/// Costs about USD 0.03 (three Cost Explorer requests), so it runs only when asked for.
#[tokio::test]
#[ignore = "needs AWS credentials; billed"]
async fn cost_overview() {
    if std::env::var_os("WINTER_TEST_AWS_COST").is_none() {
        println!("set WINTER_TEST_AWS_COST=1 to run (billed by AWS)");
        return;
    }
    let overview = Aws::new().cost_overview(&profile()).await.unwrap();
    println!("{overview:#?}");
    assert!(!overview.daily.is_empty());
}

#[tokio::test]
#[ignore = "needs ~/.aws"]
async fn unknown_profile_is_an_error() {
    let err = Aws::new()
        .connect("winter-no-such-profile", FALLBACK_REGION)
        .await
        .unwrap_err();
    println!("{err}");
}
