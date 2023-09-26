use crate::kubernetes::{IamArn, KubernetesGroupName, KubernetesRole};
use crate::IamGroup;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

type Region = String;
type RoleArn = String;
type IamK8sGroupMappingsRaw = String;

#[derive(Error, Debug, PartialEq)]
pub enum ConfigurationError {
    #[error("Invalid IAM K8S group mapping `{raw_iam_k8s_group_mapping}`, should be: `iam_group_name->k8s_group_name`")]
    InvalidIamK8sGroupMapping { raw_iam_k8s_group_mapping: Arc<str> },
    #[error("K8s group name nor IAM group name cannot be empty: `{raw_iam_k8s_group_mapping}`")]
    EmptyGroupName { raw_iam_k8s_group_mapping: Arc<str> },
    #[error("SSO role ARN cannot be empty if you want to activate it")]
    EmptySSORoleArn,
    #[error("Malformed SSO role ARN")]
    MalformedSSORoleArn,
}

pub struct Credentials {
    pub region: Region,
    pub service_account_name: String,
    pub role_arn: RoleArn,
}

impl Credentials {
    pub fn new(region: Region, service_account_name: String, role_arn: RoleArn) -> Self {
        Self {
            region,
            service_account_name,
            role_arn,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IamK8sGroup {
    pub iam_group: IamGroup,
    pub k8s_group: KubernetesGroupName,
}

impl FromStr for IamK8sGroup {
    type Err = ConfigurationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const DELIMITER: &str = "->";
        match (s.match_indices(DELIMITER).count(), s.split_once(DELIMITER)) {
            (1, Some((iam_group, k8s_group))) => {
                if iam_group.is_empty() || k8s_group.is_empty() {
                    return Err(ConfigurationError::EmptyGroupName {
                        raw_iam_k8s_group_mapping: Arc::from(s.to_string()),
                    });
                }

                Ok(IamK8sGroup {
                    iam_group: IamGroup::new(iam_group.trim()),
                    k8s_group: KubernetesGroupName::new(k8s_group.trim()),
                })
            }
            (_, _) => Err(ConfigurationError::InvalidIamK8sGroupMapping {
                raw_iam_k8s_group_mapping: Arc::from(s.to_string()),
            }),
        }
    }
}

pub enum GroupUserSyncConfig {
    Disabled,
    Enabled { iam_k8s_groups: Vec<IamK8sGroup> },
}

pub enum SSORoleConfig {
    Disabled,
    Enabled { sso_role: KubernetesRole },
}

pub struct Config {
    pub credentials: Credentials,
    pub refresh_interval: Duration,
    pub group_user_sync_config: GroupUserSyncConfig,
    pub sso_role_config: SSORoleConfig,
    pub verbose: bool,
}

impl Config {
    pub fn new(
        credentials: Credentials,
        refresh_interval: Duration,
        enable_group_sync: bool,
        iam_k8s_groups_mapping_raw: Vec<IamK8sGroupMappingsRaw>,
        enable_sso: bool,
        iam_sso_role_arn: String,
        verbose: bool,
    ) -> Result<Config, ConfigurationError> {
        // group user sync configuration
        let group_user_sync_config = match enable_group_sync {
            true => {
                let mut iam_k8s_groups = Vec::with_capacity(iam_k8s_groups_mapping_raw.len());
                for mapping in iam_k8s_groups_mapping_raw {
                    match IamK8sGroup::from_str(&mapping) {
                        Ok(g) => iam_k8s_groups.push(g),
                        Err(e) => return Err(e),
                    }
                }
                GroupUserSyncConfig::Enabled { iam_k8s_groups }
            }
            false => GroupUserSyncConfig::Disabled,
        };

        // sso configuration
        let sso_role_config = match enable_sso {
            true => {
                if iam_sso_role_arn.is_empty() {
                    return Err(ConfigurationError::EmptySSORoleArn);
                }

                // Sanitize IAM ARN for the role, removing the part before the role name
                // E.g: arn:aws:iam::8432375466567:role/aws-reserved/sso.amazonaws.com/us-east-2/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac
                // becomes => arn:aws:iam::8432375466567:role/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac
                let sanitized_role_arn =
                    match (iam_sso_role_arn.find(":role/"), iam_sso_role_arn.rfind('/')) {
                        (Some(start_index), Some(stop_index)) => IamArn::new(
                            &iam_sso_role_arn
                                .chars()
                                .take(start_index + ":role/".len())
                                .chain(iam_sso_role_arn.chars().skip(stop_index + 1))
                                .collect::<String>(),
                        ),
                        _ => return Err(ConfigurationError::MalformedSSORoleArn),
                    };

                SSORoleConfig::Enabled {
                    sso_role: KubernetesRole {
                        iam_role_arn: sanitized_role_arn,
                        role_name: Some("cluster-admin-sso".to_string()), // TODO(benjaminch): can be a parameter at some point
                        user_name: None,
                        groups: HashSet::from_iter(vec![KubernetesGroupName::new(
                            "system:masters",
                        )]), // TODO(benjaminch): can be a parameter at some point
                    },
                }
            }
            false => SSORoleConfig::Disabled,
        };

        Ok(Config {
            credentials,
            refresh_interval,
            group_user_sync_config,
            sso_role_config,
            verbose,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::aws::iam::IamGroup;
    use crate::config::{Config, ConfigurationError, Credentials, IamK8sGroup, SSORoleConfig};
    use crate::kubernetes::{IamArn, KubernetesGroupName};
    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn iam_k8s_group_from_str_test() {
        // setup:
        struct TestCase<'a> {
            input: &'a str,
            expected: Result<IamK8sGroup, ConfigurationError>,
            _description: &'a str,
        }

        let test_cases = vec![
            TestCase {
                input: "iam_group->k8s_group",
                expected: Ok(IamK8sGroup {
                    iam_group: IamGroup::new("iam_group"),
                    k8s_group: KubernetesGroupName::new("k8s_group"),
                }),
                _description: "case 1 - nominal case",
            },
            TestCase {
                input: "iam_group->k8s_group->",
                expected: Err(ConfigurationError::InvalidIamK8sGroupMapping {
                    raw_iam_k8s_group_mapping: Arc::from("iam_group->k8s_group->"),
                }),
                _description: "case 2 - there is more than one mapping delimiter",
            },
            TestCase {
                input: "iam_groupk8s_group",
                expected: Err(ConfigurationError::InvalidIamK8sGroupMapping {
                    raw_iam_k8s_group_mapping: Arc::from("iam_groupk8s_group"),
                }),
                _description: "case 3 - there is no mapping delimiter",
            },
            TestCase {
                input: "->k8s_group",
                expected: Err(ConfigurationError::EmptyGroupName {
                    raw_iam_k8s_group_mapping: Arc::from("->k8s_group"),
                }),
                _description: "case 4 - iam group is empty",
            },
            TestCase {
                input: "iam_group->",
                expected: Err(ConfigurationError::EmptyGroupName {
                    raw_iam_k8s_group_mapping: Arc::from("iam_group->"),
                }),
                _description: "case 5 - k8s group is empty",
            },
            TestCase {
                input: " iam_group -> k8s_group ",
                expected: Ok(IamK8sGroup {
                    iam_group: IamGroup::new("iam_group"),
                    k8s_group: KubernetesGroupName::new("k8s_group"),
                }),
                _description: "case 6 - some trailing spaces presents around groups names",
            },
        ];

        for tc in test_cases {
            // execute:
            let res = IamK8sGroup::from_str(tc.input);

            // verify:
            assert_eq!(tc.expected, res);
        }
    }

    #[test]
    fn iam_sso_role_arn_sanitize_ok_test() {
        // setup:
        struct TestCase<'a> {
            input: &'a str,
            expected: IamArn,
        }

        let test_cases = vec![
            TestCase {
                input: "arn:aws:iam::843237586875:role/aws-reserved/sso.amazonaws.com/us-east-2/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac",
                expected: IamArn::new("arn:aws:iam::843237586875:role/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac"),
            },
            TestCase {
                input: "arn:aws:iam::843237586875:role/whatever_here/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac",
                expected: IamArn::new("arn:aws:iam::843237586875:role/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac"),
            },
            TestCase {
                input: "arn:aws:iam::843237586875:role/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac",
                expected: IamArn::new("arn:aws:iam::843237586875:role/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac"),
            },
        ];

        for tc in test_cases {
            // execute:
            let res = Config::new(
                Credentials::new(
                    "whatever".to_string(),
                    "whatever".to_string(),
                    "whatever".to_string(),
                ),
                Duration::from_secs(60),
                false,
                Vec::with_capacity(0),
                true,
                tc.input.to_string(),
                false,
            );

            // verify:
            assert!(res.is_ok());
            assert_eq!(
                tc.expected.to_string(),
                match res.expect("config cannot be unwrap error").sso_role_config {
                    SSORoleConfig::Disabled => panic!("Error!"),
                    SSORoleConfig::Enabled { sso_role } => sso_role.iam_role_arn.to_string(),
                }
            );
        }
    }

    #[test]
    fn iam_sso_role_arn_sanitize_malformed_test() {
        // setup:
        let test_cases = vec!["AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac", "abc"];

        for tc in test_cases {
            // execute:
            let res = Config::new(
                Credentials::new(
                    "whatever".to_string(),
                    "whatever".to_string(),
                    "whatever".to_string(),
                ),
                Duration::from_secs(60),
                false,
                Vec::with_capacity(0),
                true,
                tc.to_string(),
                false,
            );

            // verify:
            assert!(res.is_err());
            assert!(matches!(res, Err(ConfigurationError::MalformedSSORoleArn)));
        }
    }
}
