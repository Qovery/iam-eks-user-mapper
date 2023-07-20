use crate::aws::AwsError;
use crate::kubernetes::KubernetesError;
use thiserror::Error;
use tracing::subscriber::SetGlobalDefaultError;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Initialization error, cannot setup tracing: {underlying_error}")]
    InitializationErrorCannotSetupTracing {
        underlying_error: SetGlobalDefaultError,
    },
    #[error("Configuration error: invalid inputs")]
    ConfigurationErrorInvalidInputs,
    #[error("Aws error: {underlying_error}")]
    Aws { underlying_error: AwsError },
    #[error("Kubernetes error: {underlying_error}")]
    Kubernetes { underlying_error: KubernetesError },
}
