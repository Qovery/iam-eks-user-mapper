use crate::aws::iam::IamError;
use aws_config::environment::EnvironmentVariableCredentialsProvider;
use aws_config::SdkConfig;
use aws_sdk_iam::config::Region;
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

pub mod iam;

#[derive(Error, Debug)]
pub enum AwsError {
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
        let ar_provider = aws_config::sts::AssumeRoleProvider::builder(role_name)
            .session_name(String::from("iam-eks-user-mapper-assume-role-session"))
            .region(region.clone())
            .build(Arc::new(EnvironmentVariableCredentialsProvider::new()) as Arc<_>);
        let config = aws_config::from_env()
            .credentials_provider(ar_provider)
            .load()
            .await;

        Ok(AwsSdkConfig {
            config,
            _verbose: verbose,
        })
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
