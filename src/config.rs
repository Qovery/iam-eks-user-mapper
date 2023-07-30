use crate::kubernetes::KubernetesGroup;
use crate::IamGroup;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

type Region = String;
type RoleArn = String;
type IamK8sGroupMappingsRaw = String;

#[derive(Error, Debug, PartialEq)]
pub enum ConfigurationError {
    #[error("Invalid IAM K8S group mapping `{raw_iam_k8s_group_mapping}`, should be: `iam_groupe_name->k8s_groupe_name`")]
    InvalidIamK8sGroupMapping { raw_iam_k8s_group_mapping: Arc<str> },
    #[error("K8s group name nor IAM group name cannot be empty: `{raw_iam_k8s_group_mapping}`")]
    EmptyGroupName { raw_iam_k8s_group_mapping: Arc<str> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IamK8sGroup {
    pub iam_group: IamGroup,
    pub k8s_group: KubernetesGroup,
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
                    k8s_group: KubernetesGroup::new(k8s_group.trim()),
                })
            }
            (_, _) => Err(ConfigurationError::InvalidIamK8sGroupMapping {
                raw_iam_k8s_group_mapping: Arc::from(s.to_string()),
            }),
        }
    }
}

pub struct Config {
    pub refresh_interval: Duration,
    pub region: Region,
    pub service_account_name: String,
    pub role_arn: RoleArn,
    pub iam_k8s_groups: Vec<IamK8sGroup>,
    pub verbose: bool,
}

impl Config {
    pub fn new(
        role_arn: RoleArn,
        region: String,
        service_account_name: String,
        refresh_interval: Duration,
        iam_k8s_groups_mapping_raw: Vec<IamK8sGroupMappingsRaw>,
        verbose: bool,
    ) -> Result<Config, ConfigurationError> {
        let mut iam_k8s_groups = Vec::with_capacity(iam_k8s_groups_mapping_raw.len());
        for mapping in iam_k8s_groups_mapping_raw {
            match IamK8sGroup::from_str(&mapping) {
                Ok(g) => iam_k8s_groups.push(g),
                Err(e) => return Err(e),
            }
        }

        Ok(Config {
            role_arn,
            region,
            service_account_name,
            refresh_interval,
            iam_k8s_groups,
            verbose,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::aws::iam::IamGroup;
    use crate::config::{ConfigurationError, IamK8sGroup};
    use crate::kubernetes::KubernetesGroup;
    use std::str::FromStr;
    use std::sync::Arc;

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
                    k8s_group: KubernetesGroup::new("k8s_group"),
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
        ];

        for tc in test_cases {
            // execute:
            let res = IamK8sGroup::from_str(tc.input);

            // verify:
            assert_eq!(tc.expected, res);
        }
    }
}
