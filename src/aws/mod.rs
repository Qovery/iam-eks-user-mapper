use crate::aws::iam::IamError;
use aws_config::SdkConfig;
use aws_sdk_iam::config::Region;
use std::env;
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
        let config = aws_config::load_from_env().await;

        // TODO(benjaminch): hack migration code to be removed
        if env::var("AWS_ACCESS_KEY_ID").is_err() {
            match config.credentials_provider() {
                Some(credential) => {
                    let provider = aws_config::sts::AssumeRoleProvider::builder(role_name)
                        .region(region)
                        .build(credential.clone());
                    let local_config = aws_config::from_env()
                        .credentials_provider(provider)
                        .load()
                        .await;
                    Ok(AwsSdkConfig {
                        config: local_config,
                    })
                }
                None => Err(AwsError::ErrorCannotGetLoginConfiguration),
            }
        } else {
            Ok(AwsSdkConfig { config })
        }
    }
}

impl From<SdkConfig> for AwsSdkConfig {
    fn from(value: SdkConfig) -> Self {
        AwsSdkConfig { config: value }
    }
}
