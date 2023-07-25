use crate::aws::iam::IamError;
use aws_config::meta::region::RegionProviderChain;
use aws_config::SdkConfig;
use aws_sdk_iam::config::timeout::TimeoutConfig;
use aws_sdk_iam::config::Region;
use std::time::Duration;
use thiserror::Error;

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
}

impl AwsSdkConfig {
    pub async fn new(region: Region, role_name: &str) -> Result<AwsSdkConfig, AwsError> {
        let region_provider = RegionProviderChain::first_try(region.clone())
            .or_default_provider()
            .or_else(region);
        let config = aws_config::from_env().region(region_provider).load().await;

        match config.credentials_provider() {
            Some(credential) => {
                let provider = aws_config::sts::AssumeRoleProvider::builder(role_name)
                    .session_name(String::from("iam-eks-user-mapper-assume-role"))
                    .build(credential.clone());
                let local_config = aws_config::from_env()
                    .credentials_provider(provider)
                    .timeout_config(
                        TimeoutConfig::builder()
                            .operation_attempt_timeout(Duration::from_millis(10000))
                            .build(),
                    )
                    .load()
                    .await;
                Ok(AwsSdkConfig {
                    config: local_config,
                })
            }
            None => Err(AwsError::ErrorCannotGetLoginConfiguration),
        }
    }
}

impl From<SdkConfig> for AwsSdkConfig {
    fn from(value: SdkConfig) -> Self {
        AwsSdkConfig { config: value }
    }
}
