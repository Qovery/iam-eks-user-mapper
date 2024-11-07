mod aws;
mod config;
mod errors;
mod kubernetes;

use crate::aws::iam::{IamGroup, IamService};
use crate::aws::AwsSdkConfig;
use crate::config::{Credentials, GroupUserSyncConfig, IamK8sGroup, SSORoleConfig};
use crate::errors::Error;
use crate::kubernetes::{
    IamArn, IamUserName, KubernetesGroupName, KubernetesRole, KubernetesService, KubernetesUser,
    SyncedBy,
};
use clap::{ArgGroup, Parser};
use config::CredentialsMode;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::{task, time};
use tracing::{error, info, span, Level};
use tracing_subscriber::{prelude::*, EnvFilter, FmtSubscriber};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(group(
    ArgGroup::new("aws_credentials")
        .args(&["aws_role_arn", "aws_access_key_id"])
        .required(true)
))]
struct Args {
    /// Service account name to be used, e.q: my-service-account
    #[arg(short = 's', long, env, required = true)]
    pub service_account_name: String,
    /// AWS role ARN to be used, e.q: arn:aws:iam::12345678910:role/my-role
    #[arg(short = 'R', long, env, conflicts_with_all = &["aws_access_key_id", "aws_secret_access_key"])]
    pub aws_role_arn: Option<String>,
    /// AWS access key ID to be used
    #[arg(short = 'a', long, env, requires = "aws_secret_access_key")]
    pub aws_access_key_id: Option<String>,
    /// AWS secret access key to be used
    #[arg(short = 'k', long, env, requires = "aws_access_key_id")]
    pub aws_secret_access_key: Option<String>,
    /// AWS default region to be used, e.q: eu-west-3
    #[arg(short = 'r', long, env, required = true)]
    pub aws_default_region: String,
    /// Refresh interval in seconds between two user synchronization, e.q: 30
    #[arg(short = 'i', long, env, default_value_t = 60)]
    pub refresh_interval_seconds: u64,
    /// Activate group user sync (requires `iam_k8s_groups` to be set)
    #[clap(long, env, required = false, default_value_t = false)]
    pub enable_group_user_sync: bool,
    /// IAM groups to be mapped into Kubernetes, e.q: Admins->system:masters
    ///
    /// Several mappings can be provided using comma separator, e.q: Admins->system:masters,Devops->system:devops
    ///
    /// Syntax is <IAM_GROUP>-><KUBERNETES_GROUP>,<IAM_GROUP_2>-><KUBERNETES_GROUP_2>,
    #[clap(short = 'g', long, env, value_parser, num_args = 1.., value_delimiter = ',', required = false)]
    pub iam_k8s_groups: Vec<String>,
    /// Activate SSO on the cluster (requires `iam_sso_role_arn` to be set)
    #[clap(long, env, default_value_t = false, required = false)]
    pub enable_sso: bool,
    /// IAM SSO role arn
    #[clap(long, env, value_delimiter = ',', required = false)]
    pub iam_sso_role_arn: Option<String>,
    /// Enable Karpenter by defining its role ARN
    #[clap(long, env, required = false)]
    pub karpenter_role_arn: Option<String>,
    /// Activate verbose mode
    #[clap(short = 'v', long, env, default_value_t = false)]
    pub verbose: bool,
}

struct GroupsMappings {
    raw: HashMap<IamGroup, KubernetesGroupName>,
}

impl GroupsMappings {
    fn new(iam_k8s_groups: Vec<IamK8sGroup>) -> GroupsMappings {
        GroupsMappings {
            raw: HashMap::from_iter(
                iam_k8s_groups
                    .into_iter()
                    .map(|m| (m.iam_group, m.k8s_group)),
            ),
        }
    }

    fn iam_groups(&self) -> HashSet<IamGroup> {
        HashSet::from_iter(self.raw.keys().cloned())
    }

    fn k8s_group_for(&self, iam_groups: HashSet<IamGroup>) -> HashSet<KubernetesGroupName> {
        let mut k8s_groups = HashSet::new();

        for iam_group in iam_groups {
            k8s_groups.insert(
                self.raw
                    .get(&iam_group)
                    .unwrap_or_else(|| {
                        panic!("K8s group mapping is not found for IAM group `{iam_group}`")
                    })
                    .clone(),
            );
            // should never fails by design
        }

        k8s_groups
    }
}

async fn sync_iam_eks_users_and_roles(
    iam_client: &IamService,
    kubernetes_client: &KubernetesService,
    groups_mappings: Option<&GroupsMappings>,
    sso_role: Option<KubernetesRole>,
    karpenter_config: Option<KubernetesRole>,
) -> Result<(), errors::Error> {
    // create kubernetes users to be added
    let kubernetes_users = match groups_mappings {
        Some(gm) => {
            // get users from AWS groups
            let iam_users = iam_client
                .get_users_from_groups(gm.iam_groups())
                .await
                .map_err(|e| Error::Aws {
                    underlying_error: e.into(),
                })?;

            info!("Found {} users in IAM groups", iam_users.len());

            Some(HashSet::from_iter(iam_users.iter().map(|u| {
                KubernetesUser::new(
                    IamUserName::new(&u.user_name.to_string()),
                    IamArn::new(&u.arn.to_string()),
                    gm.k8s_group_for(u.groups.clone()),
                    Some(SyncedBy::IamEksUserMapper), // <- those users are managed by the tool
                )
            })))
        }
        None => None,
    };

    // create new users & roles config map
    kubernetes_client
        .update_user_and_role_config_map(
            "kube-system",
            "aws-auth",
            kubernetes_users,
            sso_role,
            karpenter_config,
        )
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

    let credentials_mode = if let Some(aws_role_arn) = &args.aws_role_arn {
        CredentialsMode::RoleBased {
            _aws_role_arn: aws_role_arn.clone(),
        }
    } else if let (Some(aws_access_key_id), Some(aws_secret_access_key)) =
        (&args.aws_access_key_id, &args.aws_secret_access_key)
    {
        CredentialsMode::AccessKeyBased {
            _aws_access_key_id: aws_access_key_id.clone(),
            _aws_secret_access_key: aws_secret_access_key.clone(),
        }
    } else {
        panic!("Bad configuration");
    };

    let credentials = Credentials::new(
        args.aws_default_region,
        args.service_account_name,
        credentials_mode,
    );

    let config = config::Config::new(
        credentials,
        Duration::from_secs(args.refresh_interval_seconds),
        args.enable_group_user_sync,
        args.iam_k8s_groups,
        args.enable_sso,
        args.iam_sso_role_arn,
        args.karpenter_role_arn,
        args.verbose,
    )
    .map_err(|e| Error::Configuration {
        underlying_error: e,
    })?;

    let aws_config = AwsSdkConfig::new(config.credentials.region, config.verbose)
        .await
        .map_err(|e| Error::Aws {
            underlying_error: e,
        })?;

    let iam_client = IamService::new(&aws_config, config.verbose);

    let kubernetes_client = KubernetesService::new()
        .await
        .map_err(|e| Error::Kubernetes {
            underlying_error: e,
        })?;

    let current_span = tracing::Span::current();
    let forever = task::spawn(async move {
        // making sure to pass the current span to the new thread not to lose any tracing info
        let _ = current_span.enter();
        let mut tick_interval = time::interval(config.refresh_interval);

        let groups_mappings = match config.group_user_sync_config {
            GroupUserSyncConfig::Disabled => None,
            GroupUserSyncConfig::Enabled { iam_k8s_groups } => {
                Some(GroupsMappings::new(iam_k8s_groups))
            }
        };

        let sso_role = match config.sso_role_config {
            SSORoleConfig::Disabled => None,
            SSORoleConfig::Enabled { sso_role } => Some(sso_role),
        };

        let karpenter_config = match config.karpenter_config {
            config::KarpenterRoleConfig::Disabled => None,
            config::KarpenterRoleConfig::Enabled { karpenter_role } => Some(karpenter_role),
        };

        loop {
            tick_interval.tick().await;
            info!("Syncing IAM EKS users & roles");
            if let Err(e) = sync_iam_eks_users_and_roles(
                &iam_client,
                &kubernetes_client,
                groups_mappings.as_ref(),
                sso_role.clone(),
                karpenter_config.clone(),
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
