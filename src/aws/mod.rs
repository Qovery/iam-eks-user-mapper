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
    #[error("AWS error: error with IAM: {underlying_error}")]
    IamError { underlying_error: IamError },
}

impl From<IamError> for AwsError {
    fn from(e: IamError) -> Self {
        AwsError::IamError { underlying_error: e }
    }
}

pub struct AwsSdkConfig {
    config: SdkConfig,
    _verbose: bool,
}

impl AwsSdkConfig {
    pub async fn new(region: String, verbose: bool) -> Result<AwsSdkConfig, AwsError> {
        let region_provider = RegionProviderChain::first_try(Region::new(region)).or_default_provider();

        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider)
            .load()
            .await;

        if verbose {
            let client = Client::new(&config);
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
