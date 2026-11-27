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

#[tokio::test]
#[ignore = "needs ~/.aws"]
async fn unknown_profile_is_an_error() {
    let err = Aws::new()
        .connect("winter-no-such-profile", FALLBACK_REGION)
        .await
        .unwrap_err();
    println!("{err}");
}
