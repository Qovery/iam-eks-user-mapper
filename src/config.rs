use crate::errors;
use std::time::Duration;

type Region = String;
type RoleArn = String;
type IamK8sGroup = String;

pub struct Config {
    pub refresh_interval: Duration,
    pub region: Region,
    pub role_arn: RoleArn,
    pub iam_k8s_groups: Vec<IamK8sGroup>,
    pub verbose: bool,
}

impl Config {
    pub fn new(
        role_arn: RoleArn,
        region: Region,
        refresh_interval: Duration,
        iam_k8s_groups: Vec<IamK8sGroup>,
        verbose: bool,
    ) -> Result<Config, errors::Error> {
        Ok(Config {
            role_arn,
            region,
            refresh_interval,
            iam_k8s_groups,
            verbose,
        })
    }
}
