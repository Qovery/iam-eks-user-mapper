use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::PostParams;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum KubernetesError {
    #[error("Cluster not reachable: {raw_message}")]
    ClusterUnreachable { raw_message: Arc<str> },
    #[error("Error while trying to serialize users maps to YAML: {raw_message}")]
    CannotSerializeUsersMap { raw_message: Arc<str> },
    #[error("Cannot find config map `{config_map_name}` in namespace `{config_map_namespace}`: {raw_message}")]
    ConfigMapNotFound {
        config_map_name: Arc<str>,
        config_map_namespace: Arc<str>,
        raw_message: Arc<str>,
    },
    #[error("Cannot patch config map `{config_map_name}` in namespace `{config_map_namespace}`: {raw_message}")]
    ConfigMapCannotBePatched {
        config_map_name: Arc<str>,
        config_map_namespace: Arc<str>,
        raw_message: Arc<str>,
    },
}

type IamArn = String;
type KubernetesRole = String;

#[derive(Eq, PartialEq)]
pub struct KubernetesUser {
    pub iam_user_name: String,
    pub iam_arn: IamArn,
    pub roles: HashSet<KubernetesRole>,
}

impl Hash for KubernetesUser {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iam_user_name.to_lowercase().hash(state);
        self.iam_arn.to_lowercase().hash(state);
    }
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MapUserConfig<'a> {
    user_arn: &'a str,
    username: &'a str,
    groups: HashSet<&'a str>,
}
impl<'a> Hash for MapUserConfig<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.user_arn.to_lowercase().hash(state);
        self.username.to_lowercase().hash(state);
    }
}

pub struct KubernetesService {
    client: Client,
}

impl KubernetesService {
    pub async fn new() -> Result<KubernetesService, KubernetesError> {
        let kube_client =
            Client::try_default()
                .await
                .map_err(|e| KubernetesError::ClusterUnreachable {
                    raw_message: Arc::from(e.to_string()),
                })?;

        Ok(KubernetesService {
            client: kube_client,
        })
    }

    fn generate_users_config_map_yaml_string(
        kubernetes_users: HashSet<KubernetesUser>,
    ) -> Result<String, KubernetesError> {
        let user_config_map: HashSet<MapUserConfig> =
            HashSet::from_iter(kubernetes_users.iter().map(|u| MapUserConfig {
                user_arn: u.iam_arn.as_str(),
                username: u.iam_user_name.as_str(),
                groups: HashSet::from_iter(u.roles.iter().map(|g| g.as_str())),
            }));

        match serde_yaml::to_string(&user_config_map) {
            Ok(s) => Ok(s),
            Err(e) => Err(KubernetesError::CannotSerializeUsersMap {
                raw_message: Arc::from(e.to_string()),
            }),
        }
    }

    pub async fn update_user_config_map(
        &self,
        config_map_namespace: &str,
        config_map_name: &str,
        kubernetes_users: HashSet<KubernetesUser>,
    ) -> Result<(), KubernetesError> {
        let config_maps_api: Api<ConfigMap> =
            Api::namespaced(self.client.clone(), config_map_namespace); // TODO(benjaminch): avoid clone()

        // get config map
        let mut users_config_map = config_maps_api.get(config_map_name).await.map_err(|e| {
            KubernetesError::ConfigMapNotFound {
                config_map_name: Arc::from(config_map_name),
                config_map_namespace: Arc::from(config_map_namespace),
                raw_message: Arc::from(e.to_string()),
            }
        })?;

        // update config map
        users_config_map.data.replace(BTreeMap::from_iter(vec![(
            "mapUsers".to_string(),
            Self::generate_users_config_map_yaml_string(kubernetes_users)?,
        )]));

        match config_maps_api
            .replace(config_map_name, &PostParams::default(), &users_config_map)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(KubernetesError::ConfigMapCannotBePatched {
                config_map_name: Arc::from(config_map_name),
                config_map_namespace: Arc::from(config_map_namespace),
                raw_message: Arc::from(e.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::kubernetes::{KubernetesError, KubernetesService, KubernetesUser, MapUserConfig};
    use std::collections::HashSet;

    #[test]
    fn generate_users_config_map_yaml_string_test() {
        // setup:
        struct TestCase<'a> {
            input: HashSet<KubernetesUser>,
            expected_output: Result<String, KubernetesError>,
            description: &'a str,
        }

        let test_cases = vec![
            TestCase {
                input: HashSet::from_iter(vec![
                    KubernetesUser {
                        iam_user_name: "user_1".to_string(),
                        iam_arn: "arn:test:user_1".to_string(),
                        roles: HashSet::from_iter(vec![
                            "group_1".to_string(),
                            "group_2".to_string(),
                        ]),
                    },
                    KubernetesUser {
                        iam_user_name: "user_2".to_string(),
                        iam_arn: "arn:test:user_2".to_string(),
                        roles: HashSet::from_iter(vec![
                            "group_2".to_string(),
                            "group_3".to_string(),
                        ]),
                    },
                ]),
                expected_output: Ok(r"
- user_arn: arn:test:user_1
  username: user_1
  groups:
    - group_1
    - group_2
- user_arn: arn:test:user_2
  username: user_2
  groups:
    - group_2
    - group_3"
                    .trim_start()
                    .to_string()),

                description: "case 1 - nominal case",
            },
            TestCase {
                input: HashSet::from_iter(vec![KubernetesUser {
                    iam_user_name: "user_1".to_string(),
                    iam_arn: "arn:test:user_1".to_string(),
                    roles: HashSet::from_iter(vec!["group_1".to_string(), "group_2".to_string()]),
                }]),
                expected_output: Ok(r"
- user_arn: arn:test:user_1
  username: user_1
  groups:
    - group_1
    - group_2"
                    .trim_start()
                    .to_string()),

                description: "case 2 - one user",
            },
            TestCase {
                input: HashSet::from_iter(vec![]),
                expected_output: Ok(r"".to_string()),
                description: "case 3 - no users",
            },
        ];

        for tc in test_cases {
            // execute:
            let result = KubernetesService::generate_users_config_map_yaml_string(tc.input);

            // verify:
            match tc.expected_output {
                Ok(expected_yaml) => {
                    assert!(result.is_ok());

                    // YAML serializer is not preserving orders
                    let parsed_yaml_expected_result: Result<HashSet<MapUserConfig>, _> =
                        serde_yaml::from_str(&expected_yaml);
                    assert!(parsed_yaml_expected_result.is_ok());

                    let result_yaml_string = result.unwrap_or_default();
                    let parsed_yaml_result: Result<HashSet<MapUserConfig>, _> =
                        serde_yaml::from_str(&result_yaml_string);
                    assert!(parsed_yaml_result.is_ok());

                    assert_eq!(
                        parsed_yaml_expected_result.unwrap_or_default(),
                        parsed_yaml_result.unwrap_or_default()
                    );
                }
                Err(e) => {
                    assert!(result.is_err());
                    assert_eq!(e, result.unwrap_err());
                }
            }
        }
    }
}
