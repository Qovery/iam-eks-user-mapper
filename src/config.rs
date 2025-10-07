use crate::kubernetes::{IamArn, KubernetesGroupName, KubernetesRole, SyncedBy};
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
    #[error(
        "Invalid IAM K8S group mapping `{raw_iam_k8s_group_mapping}`, should be: `iam_group_name->k8s_group_name`"
    )]
    InvalidIamK8sGroupMapping { raw_iam_k8s_group_mapping: Arc<str> },
    #[error("K8s group name nor IAM group name cannot be empty: `{raw_iam_k8s_group_mapping}`")]
    EmptyGroupName { raw_iam_k8s_group_mapping: Arc<str> },
    #[error("SSO role ARN cannot be empty if you want to activate it")]
    EmptySSORoleArn,
    #[error("Malformed SSO role ARN")]
    MalformedSSORoleArn,
    #[error("Invalid ARN, {iam_arn}")]
    InvalidArn { iam_arn: Arc<str> },
}

#[derive(Clone)]
pub struct Credentials {
    pub region: Region,
    pub _service_account_name: String,
    pub _credentials_mode: CredentialsMode,
}

#[derive(Clone)]
pub enum CredentialsMode {
    RoleBased {
        _aws_role_arn: RoleArn,
    },
    AccessKeyBased {
        _aws_access_key_id: String,
        _aws_secret_access_key: String,
    },
}

impl Credentials {
    pub fn new(region: Region, service_account_name: String, credentials_mode: CredentialsMode) -> Credentials {
        Credentials {
            region,
            _service_account_name: service_account_name,
            _credentials_mode: credentials_mode,
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

#[derive(Clone)]
pub enum GroupUserSyncConfig {
    Disabled,
    Enabled { iam_k8s_groups: Vec<IamK8sGroup> },
}

#[derive(Clone)]
pub enum SSORoleConfig {
    Disabled,
    Enabled { sso_role: KubernetesRole },
}
#[derive(Clone)]
pub enum KarpenterRoleConfig {
    Disabled,
    Enabled { karpenter_role: KubernetesRole },
}

#[derive(Clone)]
pub struct Config {
    pub credentials: Credentials,
    pub refresh_interval: Duration,
    pub admins_users: HashSet<IamArn>,
    pub group_user_sync_config: GroupUserSyncConfig,
    pub sso_role_config: SSORoleConfig,
    pub karpenter_config: KarpenterRoleConfig,
    pub verbose: bool,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        credentials: Credentials,
        refresh_interval: Duration,
        enable_group_sync: bool,
        iam_k8s_groups_mapping_raw: Vec<IamK8sGroupMappingsRaw>,
        admins_iam_users: Option<String>,
        enable_sso: bool,
        iam_sso_role_arn: Option<String>,
        karpenter_role_arn: Option<String>,
        verbose: bool,
    ) -> Result<Config, ConfigurationError> {
        // static admins IAM users
        let mut admins_users = HashSet::new();
        if let Some(users) = admins_iam_users {
            let users_list = users.split(',').collect::<Vec<&str>>();

            for u in users_list {
                admins_users.insert(IamArn::new(u));
            }
        }

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
                let iam_sso_role_arn = match iam_sso_role_arn {
                    Some(iam_sso_role_arn) => iam_sso_role_arn,
                    None => return Err(ConfigurationError::EmptySSORoleArn),
                };

                // Sanitize IAM ARN for the role, removing the part before the role name
                // E.g: arn:aws:iam::8432375466567:role/aws-reserved/sso.amazonaws.com/us-east-2/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac
                // becomes => arn:aws:iam::8432375466567:role/AWSReservedSSO_AdministratorAccess_53b82e109c5e2cac
                let sanitized_role_arn = match (iam_sso_role_arn.find(":role/"), iam_sso_role_arn.rfind('/')) {
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
                    sso_role: KubernetesRole::new(
                        sanitized_role_arn,
                        Some("cluster-admin-sso".to_string()), // TODO(benjaminch): can be a parameter at some point
                        None,
                        HashSet::from_iter(vec![KubernetesGroupName::new("system:masters")]),
                        Some(SyncedBy::IamEksUserMapper), // <- managed by the tool
                    ),
                }
            }
            false => SSORoleConfig::Disabled,
        };

        let config = match karpenter_role_arn {
            Some(x) => {
                KarpenterRoleConfig::Enabled {
                    karpenter_role: KubernetesRole::new(
                        IamArn::new(x.as_str()),
                        None,
                        Some("system:node:{{EC2PrivateDNSName}}".to_string()),
                        HashSet::from_iter(vec![
                            KubernetesGroupName::new("system:bootstrappers"),
                            KubernetesGroupName::new("system:nodes"),
                        ]),
                        Some(SyncedBy::IamEksUserMapper), // <- managed by the tool
                    ),
                }
            }
            None => KarpenterRoleConfig::Disabled,
        };

        Ok(Config {
            credentials,
            refresh_interval,
            admins_users,
            group_user_sync_config,
            sso_role_config,
            karpenter_config: config,
            verbose,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::aws::iam::IamGroup;
    use crate::config::{
        Config, ConfigurationError, Credentials, CredentialsMode, IamK8sGroup, KarpenterRoleConfig, SSORoleConfig,
    };
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
                    CredentialsMode::RoleBased {
                        _aws_role_arn: "whatever".to_string(),
                    },
                ),
                Duration::from_secs(60),
                false,
                Vec::with_capacity(0),
                None,
                true,
                Some(tc.input.to_string()),
                None,
                false,
            );

            // verify:
            assert!(res.is_ok());
            let result = res.expect("config cannot be unwrap error");
            assert_eq!(
                tc.expected.to_string(),
                match result.clone().sso_role_config {
                    SSORoleConfig::Disabled => panic!("Error!"),
                    SSORoleConfig::Enabled { sso_role } => sso_role.iam_role_arn.to_string(),
                }
            );
            assert!(match result.karpenter_config {
                KarpenterRoleConfig::Disabled => true,
                #[allow(unused_variables)]
                KarpenterRoleConfig::Enabled { karpenter_role } => false,
            })
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
                    CredentialsMode::RoleBased {
                        _aws_role_arn: "whatever".to_string(),
                    },
                ),
                Duration::from_secs(60),
                false,
                Vec::with_capacity(0),
                None,
                true,
                Some(tc.to_string()),
                None,
                false,
            );

            // verify:
            assert!(res.is_err());
            assert!(matches!(res, Err(ConfigurationError::MalformedSSORoleArn)));
        }
    }

    #[test]
    fn iam_karpenter_role_test() {
        let res = Config::new(
            Credentials::new(
                "whatever".to_string(),
                "whatever".to_string(),
                CredentialsMode::RoleBased {
                    _aws_role_arn: "whatever".to_string(),
                },
            ),
            Duration::from_secs(60),
            false,
            Vec::with_capacity(0),
            None,
            false,
            None,
            Some("arn:aws:iam::account_id:role/role_id".to_string()),
            false,
        );

        // verify:
        assert!(res.is_ok());
        let x = match res.unwrap().karpenter_config {
            KarpenterRoleConfig::Disabled => panic!("Error!"),
            KarpenterRoleConfig::Enabled { karpenter_role } => karpenter_role.iam_role_arn,
        };

        assert_eq!(x, IamArn::new("arn:aws:iam::account_id:role/role_id"))
    }
}
