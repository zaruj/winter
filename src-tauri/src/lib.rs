mod commands;
mod error;
#[cfg(target_os = "macos")]
mod menu;
mod shell_env;

use winter_core::k8s::Clusters;
use winter_core::terminal::Terminals;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    shell_env::inherit_login_path();
    winter_core::tls::init();

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build);

    builder
        .manage(Clusters::new())
        .manage(Terminals::new())
        .manage(winter_core::aws::Aws::new())
        .manage(winter_core::github::GitHub::new())
        .invoke_handler(tauri::generate_handler![
            commands::k8s_contexts,
            commands::k8s_connect,
            commands::k8s_namespaces,
            commands::k8s_nodes,
            commands::k8s_node,
            commands::k8s_node_usage,
            commands::k8s_deployments,
            commands::k8s_pods,
            commands::k8s_services,
            commands::k8s_ingresses,
            commands::k8s_pvcs,
            commands::k8s_configmaps,
            commands::k8s_secrets,
            commands::k8s_resource_kinds,
            commands::k8s_table,
            commands::k8s_object,
            commands::k8s_logs,
            commands::k8s_shell_open,
            commands::k8s_shell_write,
            commands::k8s_shell_resize,
            commands::k8s_stream_stop,
            commands::aws_profiles,
            commands::aws_connect,
            commands::aws_regions,
            commands::aws_ec2_instances,
            commands::aws_eks_clusters,
            commands::aws_load_balancers,
            commands::aws_vpcs,
            commands::aws_security_groups,
            commands::aws_ebs_volumes,
            commands::aws_s3_buckets,
            commands::aws_hosted_zones,
            commands::aws_msk_clusters,
            commands::aws_rds_instances,
            commands::aws_cost_overview,
            commands::aws_detail,
            commands::github_connect,
            commands::github_sign_in,
            commands::github_sign_in_with_git,
            commands::github_sign_out,
            commands::github_repos,
            commands::github_repo,
            commands::github_workflows,
            commands::github_runs,
            commands::github_run,
            commands::github_jobs,
            commands::github_job_log,
            commands::open_github_url,
            commands::terminal_profile,
            commands::terminal_open,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
