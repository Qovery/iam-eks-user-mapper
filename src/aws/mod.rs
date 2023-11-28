use crate::aws::iam::IamError;
use aws_config::meta::region::RegionProviderChain;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_iam::config::Region;
use aws_sdk_sts::Client;
use thiserror::Error;
use tracing::{error, info};

pub mod iam;

#[derive(Error, Debug)]
pub enum AwsError {
    #[error("AWS error: cannot get login configuration")]
    CannotGetLoginConfiguration,
    #[error("AWS error: cannot get region")]
    CannotGetAwsRegion,
    #[error("AWS error: error with IAM: {underlying_error}")]
    IamError { underlying_error: IamError },
}

impl From<IamError> for AwsError {
    fn from(e: IamError) -> Self {
        AwsError::IamError {
            underlying_error: e,
        }
    }
}

pub struct AwsSdkConfig {
    config: SdkConfig,
    _verbose: bool,
}

impl AwsSdkConfig {
    pub async fn new(
        region: String,
        role_arn: &str,
        verbose: bool,
    ) -> Result<AwsSdkConfig, AwsError> {
        let region_provider =
            RegionProviderChain::first_try(Region::new(region)).or_default_provider();
        let region = region_provider
            .region()
            .await
            .ok_or_else(|| AwsError::CannotGetAwsRegion)?;

        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider)
            .load()
            .await;

        match config.credentials_provider() {
            Some(_credential) => {
                let provider = aws_config::sts::AssumeRoleProvider::builder(role_arn)
                    .session_name(String::from("iam-eks-user-mapper-assume-role-session"))
                    .region(region)
                    .build()
                    .await;
                let local_config = aws_config::defaults(BehaviorVersion::latest())
                    .credentials_provider(provider)
                    .load()
                    .await;

                if verbose {
                    let client = Client::new(&local_config);
                    let req = client.get_caller_identity();
                    let resp = req.send().await;
                    match resp {
                        Ok(e) => {
                            info!(
                                "UserID: {}, Account: {}, Arn: {}",
                                e.user_id().unwrap_or_default(),
                                e.account().unwrap_or_default(),
                                e.arn().unwrap_or_default()
                            );
                        }
                        Err(e) => error!("Cannot get caller identity: {:?}", e),
                    }
                }

                Ok(AwsSdkConfig {
                    config: local_config,
                    _verbose: verbose,
                })
            }
            None => Err(AwsError::CannotGetLoginConfiguration),
        }
    }
}

impl From<SdkConfig> for AwsSdkConfig {
    fn from(value: SdkConfig) -> Self {
        AwsSdkConfig {
            config: value,
            _verbose: false,
        }
    }
}
