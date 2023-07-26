use crate::aws::iam::IamError;
use aws_config::meta::region::RegionProviderChain;
use aws_config::SdkConfig;
use aws_sdk_iam::config::timeout::TimeoutConfig;
use aws_sdk_iam::config::Region;
use aws_sdk_sts::Client;
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info};

pub mod iam;

#[derive(Error, Debug)]
pub enum AwsError {
    #[error("AWS error: cannot get login configuration")]
    ErrorCannotGetLoginConfiguration,
    #[error("AWS error: error with IAM: {underlying_error}")]
    IamError { underlying_error: IamError },
}

pub struct AwsSdkConfig {
    config: SdkConfig,
    _verbose: bool,
}

impl AwsSdkConfig {
    pub async fn new(
        region: Region,
        role_name: &str,
        verbose: bool,
    ) -> Result<AwsSdkConfig, AwsError> {
        let config = aws_config::from_env().region(region.clone()).load().await;

        match config.credentials_provider() {
            Some(credential) => {
                let provider = aws_config::sts::AssumeRoleProvider::builder(role_name)
                    .session_name(String::from("iam-eks-user-mapper-assume-role"))
                    .region(region.clone())
                    .build(credential.clone());
                let local_config = aws_config::from_env()
                    .credentials_provider(provider)
                    .region(region)
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
            None => Err(AwsError::ErrorCannotGetLoginConfiguration),
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
