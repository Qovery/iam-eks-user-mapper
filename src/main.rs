mod aws;
mod config;
mod errors;
mod kubernetes;

use crate::aws::iam::IamService;
use crate::aws::AwsSdkConfig;
use crate::errors::Error;
use crate::kubernetes::{KubernetesService, KubernetesUser};
use aws_sdk_iam::config::Region;
use clap::Parser;
use std::collections::HashSet;
use std::time::Duration;
use tokio::{task, time};
use tracing::{error, info, span, Level};
use tracing_subscriber::{prelude::*, EnvFilter, FmtSubscriber};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'R', long, env)]
    pub role_arn: String,
    #[arg(short = 'r', long, env)]
    pub region: String,
    #[arg(short = 'i', long, env, default_value_t = 60)]
    pub refresh_interval_seconds: u64,
    #[clap(short = 'g', long, env, value_parser, num_args = 1.., value_delimiter = ',')]
    pub iam_k8s_groups: Vec<String>,
}

async fn sync_iam_eks_users(
    iam_client: &IamService,
    kubernetes_client: &KubernetesService,
    groups_names: Vec<&str>,
) -> Result<(), errors::Error> {
    // get users from AWS groups
    let iam_users = iam_client
        .get_users_from_groups(groups_names)
        .await
        .map_err(|e| Error::Aws {
            underlying_error: e,
        })?;

    // create kubernetes users to be added
    let kubernetes_users: HashSet<KubernetesUser> =
        HashSet::from_iter(iam_users.iter().map(|u| KubernetesUser {
            iam_user_name: u.user_name.to_string(),
            iam_arn: u.arn.to_string(),
            roles: u.groups.iter().map(|g| g.to_string()).collect(),
        }));

    // create new users config map
    kubernetes_client
        .update_user_config_map("kube-system", "aws-auth", kubernetes_users)
        .await
        .map_err(|e| Error::Kubernetes {
            underlying_error: e,
        })
}

#[tokio::main]
async fn main() -> Result<(), errors::Error> {
    // Init tracing subscriber
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_max_level(Level::INFO)
        .fmt_fields(
            tracing_subscriber::fmt::format::debug_fn(|writer, field, value| {
                write!(writer, "{field}: {value:?}")
            })
            .delimited(", "),
        )
        .with_ansi(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|e| {
        Error::InitializationErrorCannotSetupTracing {
            underlying_error: e,
        }
    })?;

    let span = span!(Level::INFO, "main_span");
    let _enter = span.enter();

    let args = Args::parse();

    let config = config::Config::new(
        args.role_arn,
        args.region,
        Duration::from_secs(args.refresh_interval_seconds),
        args.iam_k8s_groups,
    )
    .map_err(|_e| Error::ConfigurationErrorInvalidInputs)?;

    let aws_config = AwsSdkConfig::new(
        Region::from_static(Box::leak(config.region.to_string().into_boxed_str())), // TODO(benjaminch): find a better way
        config.role_arn.as_str(),
    )
    .await
    .map_err(|e| Error::Aws {
        underlying_error: e,
    })?;
    let iam_client = IamService::new(&aws_config);

    let kubernetes_client = KubernetesService::new()
        .await
        .map_err(|e| Error::Kubernetes {
            underlying_error: e,
        })?;

    let current_span = tracing::Span::current();

    let forever = task::spawn(async move {
        // making sure to pass the current span to the new thread not to lose any tracing info
        let _ = current_span.enter();

        let mut interval = time::interval(config.refresh_interval);

        loop {
            interval.tick().await;
            info!("Syncing IAM EKS users");
            if let Err(e) = sync_iam_eks_users(
                &iam_client,
                &kubernetes_client,
                config.iam_k8s_groups.iter().map(|g| g.as_str()).collect(),
            )
            .await
            {
                error!("Error while syncing IAM EKS users: {e}");
            };
            info!("Syncing of IAM EKS users is done");
        }
    });

    let _ = forever.await;

    Ok(())
}
